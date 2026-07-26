use std::time::Duration;

use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, spec: CommandSpec, timeout: Duration) -> anyhow::Result<CommandOutput>;

    async fn run_stream(
        &self,
        spec: CommandSpec,
        timeout: Duration,
        on_stdout: &mut (dyn FnMut(String) + Send),
        on_stderr: &mut (dyn FnMut(String) + Send),
    ) -> anyhow::Result<CommandOutput> {
        let out = self.run(spec, timeout).await?;
        if !out.stdout.is_empty() {
            on_stdout(out.stdout.clone());
        }
        if !out.stderr.is_empty() {
            on_stderr(out.stderr.clone());
        }
        Ok(out)
    }
}

#[derive(Clone, Default)]
pub struct TokioCommandRunner;

#[async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(&self, spec: CommandSpec, timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.kill_on_drop(true);

        let output = tokio::time::timeout(timeout, cmd.output()).await??;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    async fn run_stream(
        &self,
        spec: CommandSpec,
        timeout: Duration,
        on_stdout: &mut (dyn FnMut(String) + Send),
        on_stderr: &mut (dyn FnMut(String) + Send),
    ) -> anyhow::Result<CommandOutput> {
        enum StreamEvent {
            Stdout(String),
            Stderr(String),
            StdoutDone,
            StderrDone,
        }

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let fut = async {
            let mut child = cmd.spawn()?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("failed to capture stderr"))?;

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();

            let tx_out = tx.clone();
            let out_task = tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    let n = reader.read_line(&mut line).await?;
                    if n == 0 {
                        break;
                    }
                    let _ = tx_out.send(StreamEvent::Stdout(line.clone()));
                }
                let _ = tx_out.send(StreamEvent::StdoutDone);
                anyhow::Ok(())
            });

            let tx_err = tx.clone();
            let err_task = tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    let n = reader.read_line(&mut line).await?;
                    if n == 0 {
                        break;
                    }
                    let _ = tx_err.send(StreamEvent::Stderr(line.clone()));
                }
                let _ = tx_err.send(StreamEvent::StderrDone);
                anyhow::Ok(())
            });

            drop(tx);

            let mut stdout_all = String::new();
            let mut stderr_all = String::new();
            let mut stdout_done = false;
            let mut stderr_done = false;

            while !(stdout_done && stderr_done) {
                match rx.recv().await {
                    Some(StreamEvent::Stdout(chunk)) => {
                        stdout_all.push_str(&chunk);
                        on_stdout(chunk);
                    }
                    Some(StreamEvent::Stderr(chunk)) => {
                        stderr_all.push_str(&chunk);
                        on_stderr(chunk);
                    }
                    Some(StreamEvent::StdoutDone) => {
                        stdout_done = true;
                    }
                    Some(StreamEvent::StderrDone) => {
                        stderr_done = true;
                    }
                    None => break,
                }
            }

            let wait_status = child.wait().await?;

            out_task.await??;
            err_task.await??;

            Ok(CommandOutput {
                status: wait_status.code().unwrap_or(-1),
                stdout: stdout_all,
                stderr: stderr_all,
            })
        };

        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| anyhow::anyhow!("command timed out after {:?}", timeout))?
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_kills_timed_out_process_before_it_can_run_delayed_side_effect() {
        let marker = std::env::temp_dir().join(format!(
            "dockrev-runner-timeout-{}-{}.marker",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let command = format!("sleep 0.2; touch {}", marker.display());

        let result = TokioCommandRunner
            .run(
                CommandSpec {
                    program: "sh".to_string(),
                    args: vec!["-c".to_string(), command],
                    env: Vec::new(),
                },
                Duration::from_millis(20),
            )
            .await;

        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !marker.exists(),
            "timed-out command still ran its delayed side effect"
        );
    }
}
