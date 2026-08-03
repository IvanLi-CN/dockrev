use super::*;

pub(crate) struct DbLoggingRunner {
    pub(crate) db: crate::db::Db,
    pub(crate) inner: Arc<dyn crate::runner::CommandRunner>,
    pub(crate) job_id: String,
    pub(crate) live_log_hub: Arc<crate::job_live_logs::JobLiveLogHub>,
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

        let result = self
            .inner
            .run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr)
            .await;
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

        let ts = now_rfc3339()?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status,
            truncate(&captured_stdout, 2000),
            truncate(&captured_stderr, 2000)
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

#[derive(Clone)]
struct TerminalEmitter {
    state: Arc<std::sync::Mutex<TerminalEmitterState>>,
}

struct TerminalEmitterState {
    parser: vt100::Parser,
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
        state.parser.process(chunk);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandRunner;
    use std::{path::Path, time::Duration};

    struct StaticRunner;

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
