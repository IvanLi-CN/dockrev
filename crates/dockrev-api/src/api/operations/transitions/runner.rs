use super::*;

pub(crate) struct DbUpdateApplyGate {
    pub(crate) db: crate::db::Db,
    pub(crate) job_id: String,
}

pub(crate) struct DbBackupRecoveryStore {
    pub(crate) db: crate::db::Db,
    pub(crate) job_id: String,
}

#[async_trait::async_trait]
impl backup::BackupRecoveryStore for DbBackupRecoveryStore {
    async fn save(&self, snapshot: &backup::BackupRecoverySnapshot) -> anyhow::Result<()> {
        self.db
            .save_update_stop_recovery_snapshot(&self.job_id, snapshot, &now_rfc3339()?)
            .await
    }

    async fn clear(&self) -> anyhow::Result<()> {
        self.db
            .clear_update_stop_recovery_snapshot(&self.job_id, &now_rfc3339()?)
            .await
    }
}

/// Restores services interrupted during a pre-apply backup after the API has bound.
pub(crate) async fn recover_interrupted_update_backups(state: Arc<AppState>) {
    let now = match now_rfc3339() {
        Ok(now) => now,
        Err(error) => {
            tracing::error!(error = %error, "cannot start interrupted backup recovery");
            return;
        }
    };
    let pending = match state.db.claim_pending_update_stop_recoveries(&now).await {
        Ok(pending) => pending,
        Err(error) => {
            tracing::error!(error = %error, "cannot claim interrupted backup recoveries");
            return;
        }
    };

    for pending in pending {
        let finished_at = now_rfc3339().unwrap_or_else(|_| now.clone());
        let result = async {
            let snapshot: backup::BackupRecoverySnapshot =
                serde_json::from_str(&pending.snapshot_json)?;
            let stack = state
                .db
                .get_stack(&snapshot.stack_id)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("backup recovery stack missing: {}", snapshot.stack_id)
                })?;
            backup::restore_backup_recovery_snapshot(
                &*state.runner,
                &state.config.compose_bin,
                state.config.docker_config_path.as_deref(),
                &stack,
                &state.config.managed_override_dir,
                &snapshot,
            )
            .await
        }
        .await;

        match result {
            Ok(()) => {
                let _ = state
                    .db
                    .clear_update_stop_recovery_snapshot(&pending.job_id, &finished_at)
                    .await;
                let was_stopped = state
                    .db
                    .get_update_stop_control(&pending.job_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|control| control.stop_requested_at)
                    .is_some();
                let status = if was_stopped { "cancelled" } else { "failed" };
                let summary = json!({"mode": "apply", "stopRequested": was_stopped, "recoveredOnStartup": true});
                let _ = state
                    .db
                    .finish_job(&pending.job_id, status, &finished_at, &summary)
                    .await;
            }
            Err(error) => {
                let error_message = error.to_string();
                let _ = state
                    .db
                    .record_update_stop_recovery_error(
                        &pending.job_id,
                        &error_message,
                        &finished_at,
                    )
                    .await;
                let summary =
                    json!({"mode": "apply", "stopRequested": true, "recoveryError": error_message});
                let _ = state
                    .db
                    .finish_job(&pending.job_id, "failed", &finished_at, &summary)
                    .await;
                tracing::error!(job_id = %pending.job_id, error = %error, "interrupted backup recovery failed");
            }
        }
    }

    // The controlled recovery path finalizes jobs after the initial startup scan. Retry evidence
    // recovery now so a spool belonging to one of those jobs is not stranded until next restart.
    crate::rollback_evidence::recover_orphaned_evidence(&state.db, &state.config.db_path).await;
}

#[async_trait::async_trait]
impl updater::UpdateApplyGate for DbUpdateApplyGate {
    async fn commit(&self) -> anyhow::Result<bool> {
        if self
            .db
            .commit_update_job_apply(&self.job_id, &now_rfc3339()?)
            .await?
        {
            return Ok(true);
        }
        Ok(self
            .db
            .get_update_stop_control(&self.job_id)
            .await?
            .is_some_and(|control| control.apply_committed_at.is_some()))
    }
}

pub(crate) struct DbLoggingRunner {
    pub(crate) db: crate::db::Db,
    pub(crate) inner: Arc<dyn crate::runner::CommandRunner>,
    pub(crate) job_id: String,
    pub(crate) live_log_hub: Arc<crate::job_live_logs::JobLiveLogHub>,
    pub(crate) stop_signal: Option<tokio::sync::watch::Receiver<bool>>,
}

#[async_trait::async_trait]
impl crate::runner::CommandRunner for DbLoggingRunner {
    async fn run(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let mut on_stdout = |_chunk: Vec<u8>| {};
        let mut on_stderr = |_chunk: Vec<u8>| {};
        self.run_stream(spec, timeout, &mut on_stdout, &mut on_stderr)
            .await
    }

    async fn run_raw(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
    ) -> anyhow::Result<crate::runner::RawCommandOutput> {
        self.inner.run_raw(spec, timeout).await
    }

    async fn run_raw_bounded(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
        max_stdout_bytes: usize,
    ) -> anyhow::Result<crate::runner::RawCommandOutput> {
        self.inner
            .run_raw_bounded(spec, timeout, max_stdout_bytes)
            .await
    }

    async fn run_stream(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
        on_stdout: &mut (dyn FnMut(Vec<u8>) + Send),
        on_stderr: &mut (dyn FnMut(Vec<u8>) + Send),
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let start = now_rfc3339()?;
        let msg = format!("$ {} {}", spec.program, spec.args.join(" "));
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts: start,
                    level: "info".to_string(),
                    msg,
                },
            )
            .await;

        let mut captured_stdout = Vec::new();
        let mut captured_stderr = Vec::new();
        let compose_pull = is_compose_pull(&spec);
        let command_seq = self.live_log_hub.begin_command(&self.job_id);
        let terminal =
            TerminalEmitter::new(self.live_log_hub.clone(), self.job_id.clone(), command_seq);
        let stdout_terminal = terminal.clone();
        let stderr_terminal = terminal.clone();
        let mut tap_stdout = |chunk: Vec<u8>| {
            captured_stdout.extend_from_slice(&chunk);
            stdout_terminal.push(&chunk);
            on_stdout(chunk);
        };
        let mut tap_stderr = |chunk: Vec<u8>| {
            captured_stderr.extend_from_slice(&chunk);
            stderr_terminal.push(&chunk);
            on_stderr(chunk);
        };

        let result = if let Some(mut stop_signal) = self.stop_signal.clone() {
            if *stop_signal.borrow() {
                Err(crate::update_stop::requested_error())
            } else {
                tokio::select! {
                    result = self.inner.run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr) => result,
                    _ = wait_for_update_stop(&mut stop_signal) => Err(crate::update_stop::requested_error()),
                }
            }
        } else {
            self.inner
                .run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr)
                .await
        };
        let out = match result {
            Ok(out) => out,
            Err(error) => {
                self.live_log_hub.publish_command_complete(
                    &self.job_id,
                    command_seq,
                    terminal.finish(),
                    false,
                );
                return Err(error);
            }
        };
        if captured_stdout.is_empty() && !out.stdout.is_empty() {
            captured_stdout.extend_from_slice(out.stdout.as_bytes());
            if !out.stdout.is_empty() {
                terminal.push(out.stdout.as_bytes());
                on_stdout(out.stdout.as_bytes().to_vec());
            }
        }
        if captured_stderr.is_empty() && !out.stderr.is_empty() {
            captured_stderr.extend_from_slice(out.stderr.as_bytes());
            if !out.stderr.is_empty() {
                terminal.push(out.stderr.as_bytes());
                on_stderr(out.stderr.as_bytes().to_vec());
            }
        }

        let had_live_output = terminal.finish();
        let captured_stdout = String::from_utf8_lossy(&captured_stdout).to_string();
        let captured_stderr = String::from_utf8_lossy(&captured_stderr).to_string();
        let (summary_stdout, summary_stderr) =
            command_summary_output(compose_pull, out.status, &captured_stdout, &captured_stderr);

        let ts = now_rfc3339()?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status, summary_stdout, summary_stderr
        );
        let summary_persisted = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts,
                    level: if out.status == 0 {
                        "info".to_string()
                    } else {
                        "warn".to_string()
                    },
                    msg,
                },
            )
            .await
            .is_ok();

        self.live_log_hub.publish_command_complete(
            &self.job_id,
            command_seq,
            had_live_output,
            summary_persisted,
        );

        Ok(crate::runner::CommandOutput {
            status: out.status,
            stdout: captured_stdout,
            stderr: captured_stderr,
        })
    }
}

async fn wait_for_update_stop(signal: &mut tokio::sync::watch::Receiver<bool>) {
    while !*signal.borrow() {
        if signal.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[derive(Clone)]
struct TerminalEmitter {
    state: Arc<std::sync::Mutex<TerminalEmitterState>>,
}

struct TerminalEmitterState {
    parser: vt100::Parser,
    line_discipline: crate::job_live_logs::TerminalLineDiscipline,
    hub: Arc<crate::job_live_logs::JobLiveLogHub>,
    job_id: String,
    command_seq: u64,
    last_emit: std::time::Instant,
    had_output: bool,
    had_visible_output: bool,
}

impl TerminalEmitter {
    fn new(
        hub: Arc<crate::job_live_logs::JobLiveLogHub>,
        job_id: String,
        command_seq: u64,
    ) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(TerminalEmitterState {
                parser: vt100::Parser::new(200, 240, 2000),
                line_discipline: crate::job_live_logs::TerminalLineDiscipline::default(),
                hub,
                job_id,
                command_seq,
                last_emit: std::time::Instant::now(),
                had_output: false,
                had_visible_output: false,
            })),
        }
    }

    fn push(&self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        let mut state = self.state.lock().expect("terminal emitter lock poisoned");
        let normalized = state.line_discipline.normalize(chunk);
        state.parser.process(&normalized);
        state.had_output = true;
        if state.last_emit.elapsed() >= std::time::Duration::from_millis(50) {
            state.publish_snapshot();
        }
    }

    fn finish(&self) -> bool {
        let mut state = self.state.lock().expect("terminal emitter lock poisoned");
        if state.had_output {
            state.publish_snapshot();
            return state.had_visible_output;
        }
        false
    }
}

impl TerminalEmitterState {
    fn publish_snapshot(&mut self) -> bool {
        let terminal = crate::job_live_logs::terminal_snapshot(
            &self.parser,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
            self.command_seq,
        );
        let has_visible_lines = terminal.lines.iter().any(|line| !line.segments.is_empty());
        self.hub.publish_terminal(&self.job_id, terminal);
        self.last_emit = std::time::Instant::now();
        self.had_visible_output |= has_visible_lines;
        has_visible_lines
    }
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

pub(crate) fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_string();
    }
    format!("{}...(truncated)", &input[..max])
}

fn is_compose_pull(spec: &crate::runner::CommandSpec) -> bool {
    let command_start = if crate::compose_capability::uses_docker_subcommand(&spec.program) {
        if spec.args.first().is_none_or(|arg| arg != "compose") {
            return false;
        }
        1
    } else if is_standalone_compose(&spec.program) {
        0
    } else {
        return false;
    };

    compose_subcommand(&spec.args[command_start..]) == Some("pull")
}

fn compose_subcommand(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if !arg.starts_with('-') {
            return Some(arg);
        }
        index += if compose_option_takes_value(arg) {
            2
        } else {
            1
        };
    }
    None
}

fn compose_option_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-f" | "--file"
            | "-p"
            | "--project-name"
            | "--project-directory"
            | "--env-file"
            | "--profile"
            | "--ansi"
            | "--parallel"
            | "--progress"
    )
}

fn is_standalone_compose(program: &str) -> bool {
    let program = program.to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program);
    program == "docker-compose"
        || program.ends_with("/docker-compose")
        || program.ends_with("\\docker-compose")
}

fn command_summary_output(
    compose_pull: bool,
    status: i32,
    stdout: &str,
    stderr: &str,
) -> (String, String) {
    if compose_pull && status == 0 {
        return (String::new(), String::new());
    }
    (truncate(stdout, 2000), truncate(stderr, 2000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandRunner;
    use std::{path::Path, time::Duration};

    struct StaticRunner;

    struct ComposePullProgressRunner;

    #[async_trait::async_trait]
    impl crate::runner::CommandRunner for StaticRunner {
        async fn run(
            &self,
            _spec: crate::runner::CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<crate::runner::CommandOutput> {
            Ok(crate::runner::CommandOutput {
                status: 0,
                stdout: "first\nsecond\n".to_string(),
                stderr: "warning\n".to_string(),
            })
        }
    }

    #[async_trait::async_trait]
    impl crate::runner::CommandRunner for ComposePullProgressRunner {
        async fn run(
            &self,
            _spec: crate::runner::CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<crate::runner::CommandOutput> {
            Ok(crate::runner::CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: [
                    "f99586fca4fe Downloading 1.049MB",
                    "f99586fca4fe Downloading 2.097MB",
                    "f99586fca4fe Downloading 3.146MB",
                ]
                .join("\n"),
            })
        }
    }

    #[tokio::test]
    async fn live_output_is_not_persisted_as_individual_job_logs() {
        let db = crate::db::Db::open(Path::new(":memory:")).await.unwrap();
        db.insert_job(crate::api::types::JobListItem {
            id: "job-1".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "running".to_string(),
            created_at: "2026-08-03T00:00:00Z".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            started_at: Some("2026-08-03T00:00:00Z".to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();

        let hub = Arc::new(crate::job_live_logs::JobLiveLogHub::new());
        let mut live = hub.subscribe("job-1").await;
        let runner = DbLoggingRunner {
            db: db.clone(),
            inner: Arc::new(StaticRunner),
            job_id: "job-1".to_string(),
            live_log_hub: hub,
            stop_signal: None,
        };

        runner
            .run(
                crate::runner::CommandSpec {
                    program: "docker".to_string(),
                    args: vec!["compose".to_string()],
                    env: Vec::new(),
                },
                Duration::from_secs(1),
            )
            .await
            .unwrap();

        let first = live.recv().await.unwrap();
        let second = live.recv().await.unwrap();
        assert!(matches!(
            first,
            crate::job_live_logs::JobLiveEvent::Terminal(terminal)
                if terminal.command_seq == 1
                    && terminal.lines.iter().any(|line| line.segments.iter().any(|segment| segment.text.contains("first")))
        ));
        assert!(matches!(
            second,
            crate::job_live_logs::JobLiveEvent::CommandComplete(done)
                if done.command_seq == 1 && done.had_output && done.summary_persisted
        ));

        let logs = db.list_job_logs("job-1").await.unwrap();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].msg.starts_with("$ docker compose"));
        assert!(logs[1].msg.starts_with("status=0 stdout=first"));
        assert!(
            !logs
                .iter()
                .any(|log| log.msg == "first" || log.msg == "second")
        );
    }

    #[tokio::test]
    async fn successful_compose_pull_does_not_persist_transient_progress_in_summary() {
        let db = crate::db::Db::open(Path::new(":memory:")).await.unwrap();
        db.insert_job(crate::api::types::JobListItem {
            id: "job-pull".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "running".to_string(),
            created_at: "2026-08-06T00:00:00Z".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            started_at: Some("2026-08-06T00:00:00Z".to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();

        let runner = DbLoggingRunner {
            db: db.clone(),
            inner: Arc::new(ComposePullProgressRunner),
            job_id: "job-pull".to_string(),
            live_log_hub: Arc::new(crate::job_live_logs::JobLiveLogHub::new()),
            stop_signal: None,
        };

        runner
            .run(
                crate::runner::CommandSpec {
                    program: "docker-compose".to_string(),
                    args: vec!["pull".to_string(), "web".to_string()],
                    env: Vec::new(),
                },
                Duration::from_secs(1),
            )
            .await
            .unwrap();

        let logs = db.list_job_logs("job-pull").await.unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[1].msg, "status=0 stdout= stderr=");
    }

    #[test]
    fn command_summary_keeps_failed_pull_and_non_pull_output() {
        assert_eq!(
            command_summary_output(true, 1, "", "pull failed"),
            (String::new(), "pull failed".to_string())
        );
        assert_eq!(
            command_summary_output(false, 0, "container-id", ""),
            ("container-id".to_string(), String::new())
        );
    }

    #[test]
    fn compose_pull_detection_requires_the_compose_subcommand() {
        let plugin = crate::runner::CommandSpec {
            program: "docker".to_string(),
            args: vec![
                "compose".to_string(),
                "-f".to_string(),
                "stack.yml".to_string(),
                "pull".to_string(),
            ],
            env: Vec::new(),
        };
        let standalone = crate::runner::CommandSpec {
            program: "/usr/local/bin/docker-compose".to_string(),
            args: vec![
                "-f".to_string(),
                "stack.yml".to_string(),
                "pull".to_string(),
            ],
            env: Vec::new(),
        };
        let docker_pull = crate::runner::CommandSpec {
            program: "docker".to_string(),
            args: vec!["pull".to_string(), "example/image".to_string()],
            env: Vec::new(),
        };
        let stop_pull = crate::runner::CommandSpec {
            program: "docker-compose".to_string(),
            args: vec!["stop".to_string(), "pull".to_string()],
            env: Vec::new(),
        };
        let plugin_exec_pull = crate::runner::CommandSpec {
            program: "docker".to_string(),
            args: vec![
                "compose".to_string(),
                "exec".to_string(),
                "api".to_string(),
                "pull".to_string(),
            ],
            env: Vec::new(),
        };

        assert!(is_compose_pull(&plugin));
        assert!(is_compose_pull(&standalone));
        assert!(!is_compose_pull(&docker_pull));
        assert!(!is_compose_pull(&stop_pull));
        assert!(!is_compose_pull(&plugin_exec_pull));
    }

    #[tokio::test]
    async fn control_only_terminal_output_does_not_suppress_summary() {
        let hub = Arc::new(crate::job_live_logs::JobLiveLogHub::new());
        let mut live = hub.subscribe("job-control-only").await;
        let command_seq = hub.begin_command("job-control-only");
        let terminal =
            TerminalEmitter::new(hub.clone(), "job-control-only".to_string(), command_seq);

        terminal.push(b"\x1b[2J\x1b[H");
        assert!(!terminal.finish());

        let event = live.recv().await.unwrap();
        assert!(matches!(
            event,
            crate::job_live_logs::JobLiveEvent::Terminal(snapshot)
                if snapshot.command_seq == command_seq && snapshot.lines.is_empty()
        ));
    }
}
