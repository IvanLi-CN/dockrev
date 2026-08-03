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
        let mut on_stdout = |_chunk: String| {};
        let mut on_stderr = |_chunk: String| {};
        self.run_stream(spec, timeout, &mut on_stdout, &mut on_stderr)
            .await
    }

    async fn run_stream(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
        on_stdout: &mut (dyn FnMut(String) + Send),
        on_stderr: &mut (dyn FnMut(String) + Send),
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

        let mut captured_stdout = String::new();
        let mut captured_stderr = String::new();
        let mut stdout_emitter =
            LiveLineEmitter::new(self.live_log_hub.clone(), self.job_id.clone(), "stdout");
        let mut stderr_emitter =
            LiveLineEmitter::new(self.live_log_hub.clone(), self.job_id.clone(), "stderr");
        let mut tap_stdout = |chunk: String| {
            captured_stdout.push_str(&chunk);
            stdout_emitter.push(&chunk);
            on_stdout(chunk);
        };
        let mut tap_stderr = |chunk: String| {
            captured_stderr.push_str(&chunk);
            stderr_emitter.push(&chunk);
            on_stderr(chunk);
        };

        let result = self
            .inner
            .run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr)
            .await;
        let stdout_had_output = stdout_emitter.finish();
        let stderr_had_output = stderr_emitter.finish();
        let mut had_live_output = stdout_had_output || stderr_had_output;
        let out = match result {
            Ok(out) => out,
            Err(error) => {
                self.live_log_hub
                    .publish_command_complete(&self.job_id, had_live_output, false);
                return Err(error);
            }
        };
        if captured_stdout.is_empty() {
            captured_stdout = out.stdout.clone();
            if !out.stdout.is_empty() {
                stdout_emitter.push(&out.stdout);
                had_live_output |= stdout_emitter.finish();
                on_stdout(out.stdout.clone());
            }
        }
        if captured_stderr.is_empty() {
            captured_stderr = out.stderr.clone();
            if !out.stderr.is_empty() {
                stderr_emitter.push(&out.stderr);
                had_live_output |= stderr_emitter.finish();
                on_stderr(out.stderr.clone());
            }
        }

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

struct LiveLineEmitter {
    hub: Arc<crate::job_live_logs::JobLiveLogHub>,
    job_id: String,
    stream: &'static str,
    pending: String,
    had_output: bool,
}

impl LiveLineEmitter {
    fn new(
        hub: Arc<crate::job_live_logs::JobLiveLogHub>,
        job_id: String,
        stream: &'static str,
    ) -> Self {
        Self {
            hub,
            job_id,
            stream,
            pending: String::new(),
            had_output: false,
        }
    }

    fn push(&mut self, chunk: &str) {
        self.pending.push_str(chunk);
        while let Some(newline) = self.pending.find('\n') {
            let line = self.pending[..newline]
                .strip_suffix('\r')
                .unwrap_or(&self.pending[..newline])
                .to_string();
            self.pending.drain(..=newline);
            self.emit(line);
        }
    }

    fn finish(&mut self) -> bool {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.emit(line);
        }
        self.had_output
    }

    fn emit(&mut self, msg: String) {
        self.had_output = true;
        self.hub.publish_log(
            &self.job_id,
            crate::job_live_logs::JobLiveLog {
                ts: now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                stream: self.stream,
                msg,
            },
        );
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

        let mut messages = Vec::new();
        for _ in 0..4 {
            messages.push(live.recv().await.unwrap());
        }
        assert!(matches!(
            &messages[0],
            crate::job_live_logs::JobLiveEvent::Log(log) if log.msg == "first"
        ));
        assert!(matches!(
            &messages[1],
            crate::job_live_logs::JobLiveEvent::Log(log) if log.msg == "second"
        ));
        assert!(matches!(
            &messages[2],
            crate::job_live_logs::JobLiveEvent::Log(log) if log.stream == "stderr" && log.msg == "warning"
        ));
        assert!(matches!(
            &messages[3],
            crate::job_live_logs::JobLiveEvent::CommandComplete(done)
                if done.had_output && done.summary_persisted
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
}
