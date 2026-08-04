use std::time::Duration;

use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
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
        on_stdout: &mut (dyn FnMut(Vec<u8>) + Send),
        on_stderr: &mut (dyn FnMut(Vec<u8>) + Send),
    ) -> anyhow::Result<CommandOutput> {
        let out = self.run(spec, timeout).await?;
        if !out.stdout.is_empty() {
            on_stdout(out.stdout.as_bytes().to_vec());
        }
        if !out.stderr.is_empty() {
            on_stderr(out.stderr.as_bytes().to_vec());
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
        on_stdout: &mut (dyn FnMut(Vec<u8>) + Send),
        on_stderr: &mut (dyn FnMut(Vec<u8>) + Send),
    ) -> anyhow::Result<CommandOutput> {
        enum StreamEvent {
            Stdout(Vec<u8>),
            Stderr(Vec<u8>),
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
                let mut reader = stdout;
                let mut buffer = [0_u8; 8192];
                loop {
                    let n = reader.read(&mut buffer).await?;
                    if n == 0 {
                        break;
                    }
                    let _ = tx_out.send(StreamEvent::Stdout(buffer[..n].to_vec()));
                }
                let _ = tx_out.send(StreamEvent::StdoutDone);
                anyhow::Ok(())
            });

            let tx_err = tx.clone();
            let err_task = tokio::spawn(async move {
                let mut reader = stderr;
                let mut buffer = [0_u8; 8192];
                loop {
                    let n = reader.read(&mut buffer).await?;
                    if n == 0 {
                        break;
                    }
                    let _ = tx_err.send(StreamEvent::Stderr(buffer[..n].to_vec()));
                }
                let _ = tx_err.send(StreamEvent::StderrDone);
                anyhow::Ok(())
            });

            drop(tx);

            let mut stdout_all = Vec::new();
            let mut stderr_all = Vec::new();
            let mut stdout_done = false;
            let mut stderr_done = false;

            while !(stdout_done && stderr_done) {
                match rx.recv().await {
                    Some(StreamEvent::Stdout(chunk)) => {
                        stdout_all.extend_from_slice(&chunk);
                        on_stdout(chunk);
                    }
                    Some(StreamEvent::Stderr(chunk)) => {
                        stderr_all.extend_from_slice(&chunk);
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
                stdout: String::from_utf8_lossy(&stdout_all).to_string(),
                stderr: String::from_utf8_lossy(&stderr_all).to_string(),
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

    #[tokio::test]
    async fn run_stream_emits_raw_bytes_including_carriage_returns() {
        let mut stdout_chunks = Vec::new();
        let mut stderr_chunks = Vec::new();
        let mut on_stdout = |chunk: Vec<u8>| stdout_chunks.extend(chunk);
        let mut on_stderr = |chunk: Vec<u8>| stderr_chunks.extend(chunk);

        TokioCommandRunner
            .run_stream(
                CommandSpec {
                    program: "sh".to_string(),
                    args: vec![
                        "-c".to_string(),
                        "printf 'layer 1\\r'; printf 'layer 2\\n' >&2".to_string(),
                    ],
                    env: Vec::new(),
                },
                Duration::from_secs(1),
                &mut on_stdout,
                &mut on_stderr,
            )
            .await
            .unwrap();

        assert_eq!(stdout_chunks, b"layer 1\r");
        assert_eq!(stderr_chunks, b"layer 2\n");
    }
}
