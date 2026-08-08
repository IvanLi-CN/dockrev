use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::runner::{CommandRunner, CommandSpec};

use super::{
    IdempotentRetryPolicy, UpdateStepFailure, is_registry_rate_limit_failure_text,
    pull_progress::{
        PullProgressFractionSource, PullProgressSnapshot, PullProgressTracker,
        parse_pull_fraction_from_line, pull_progress_signature,
    },
    retry_backoff_delay,
};

struct PullProgressStreamObserver<'a, F>
where
    F: FnMut(PullProgressSnapshot) + Send,
{
    tracker: PullProgressTracker,
    line_buffer: Vec<u8>,
    last_fraction: f64,
    last_signature: String,
    last_status_emit: std::time::Instant,
    on_progress: &'a mut F,
}

impl<'a, F> PullProgressStreamObserver<'a, F>
where
    F: FnMut(PullProgressSnapshot) + Send,
{
    fn new(on_progress: &'a mut F, last_fraction: f64, last_signature: String) -> Self {
        Self {
            tracker: PullProgressTracker::default(),
            line_buffer: Vec::new(),
            last_fraction,
            last_signature,
            last_status_emit: std::time::Instant::now()
                .checked_sub(Duration::from_secs(5))
                .unwrap_or_else(std::time::Instant::now),
            on_progress,
        }
    }

    fn observe_chunk(&mut self, chunk: &[u8]) {
        self.line_buffer.extend_from_slice(chunk);
        while let Some(delimiter) = self
            .line_buffer
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')
        {
            let delimiter_byte = self.line_buffer[delimiter];
            let mut line = self.line_buffer.drain(..=delimiter).collect::<Vec<_>>();
            line.pop();
            if delimiter_byte == b'\r' && self.line_buffer.first() == Some(&b'\n') {
                self.line_buffer.remove(0);
            }
            self.observe_line(&String::from_utf8_lossy(&line));
        }
    }

    fn finish(&mut self) {
        if self.line_buffer.is_empty() {
            return;
        }
        if self.line_buffer.last() == Some(&b'\r') {
            self.line_buffer.pop();
        }
        let line = std::mem::take(&mut self.line_buffer);
        self.observe_line(&String::from_utf8_lossy(&line));
    }

    fn observe_line(&mut self, line: &str) {
        let snapshot = self.tracker.observe_line(line).or_else(|| {
            parse_pull_fraction_from_line(line).map(|fraction| PullProgressSnapshot {
                fraction: Some(fraction.clamp(0.0, 1.0)),
                fraction_source: Some(PullProgressFractionSource::Bytes),
                download: None,
            })
        });
        let Some(mut snapshot) = snapshot else {
            return;
        };
        if let Some(fraction) = snapshot.fraction {
            snapshot.fraction = Some(fraction.clamp(0.0, 0.99));
        }
        let fraction_changed = snapshot
            .fraction
            .is_some_and(|fraction| fraction > self.last_fraction + 0.01);
        let signature = pull_progress_signature(&snapshot);
        let status_changed = signature != self.last_signature
            && self.last_status_emit.elapsed() >= Duration::from_millis(600);
        if fraction_changed || status_changed {
            if let Some(fraction) = snapshot.fraction {
                self.last_fraction = fraction;
            }
            self.last_signature = signature;
            self.last_status_emit = std::time::Instant::now();
            (self.on_progress)(snapshot);
        }
    }
}

pub(super) async fn run_checked_with_pull_progress<F>(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
    step: &str,
    retry_policy: IdempotentRetryPolicy,
    mut on_progress: F,
) -> anyhow::Result<()>
where
    F: FnMut(PullProgressSnapshot) + Send,
{
    let mut last_fraction = 0.0f64;
    let mut last_signature = String::new();
    for attempt in 1..=retry_policy.max_attempts {
        let observer = Arc::new(Mutex::new(PullProgressStreamObserver::new(
            &mut on_progress,
            last_fraction,
            last_signature,
        )));
        let run_result = {
            let stdout_observer = Arc::clone(&observer);
            let mut on_stdout = move |chunk: Vec<u8>| {
                stdout_observer
                    .lock()
                    .expect("pull progress observer mutex poisoned")
                    .observe_chunk(&chunk);
            };
            let stderr_observer = Arc::clone(&observer);
            let mut on_stderr = move |chunk: Vec<u8>| {
                stderr_observer
                    .lock()
                    .expect("pull progress observer mutex poisoned")
                    .observe_chunk(&chunk);
            };
            runner
                .run_stream(spec.clone(), timeout, &mut on_stdout, &mut on_stderr)
                .await
        };

        {
            let mut observer = observer
                .lock()
                .expect("pull progress observer mutex poisoned");
            observer.finish();
            last_fraction = observer.last_fraction;
            last_signature = observer.last_signature.clone();
        }

        let out = match run_result {
            Ok(out) => out,
            Err(err) => {
                if is_registry_rate_limit_failure_text(&err.to_string()) {
                    return Err(anyhow::Error::new(UpdateStepFailure::new(
                        step,
                        retry_policy,
                        attempt,
                        format!("registry rate limited: {err}"),
                    )));
                }
                if attempt >= retry_policy.max_attempts {
                    return Err(anyhow::Error::new(UpdateStepFailure::new(
                        step,
                        retry_policy,
                        attempt,
                        err.to_string(),
                    )));
                }
                tokio::time::sleep(retry_backoff_delay(retry_policy, attempt)).await;
                continue;
            }
        };
        if out.status == 0 {
            return Ok(());
        }

        let failure_message = format!(
            "command failed: status={} stdout={} stderr={}",
            out.status, out.stdout, out.stderr
        );
        if is_registry_rate_limit_failure_text(&failure_message) {
            return Err(anyhow::Error::new(UpdateStepFailure::new(
                step,
                retry_policy,
                attempt,
                format!("registry rate limited: {failure_message}"),
            )));
        }

        if attempt >= retry_policy.max_attempts {
            return Err(anyhow::Error::new(UpdateStepFailure::new(
                step,
                retry_policy,
                attempt,
                failure_message,
            )));
        }
        tokio::time::sleep(retry_backoff_delay(retry_policy, attempt)).await;
    }

    Err(anyhow::Error::new(UpdateStepFailure::new(
        step,
        retry_policy,
        retry_policy.max_attempts,
        "retry loop exhausted unexpectedly",
    )))
}
