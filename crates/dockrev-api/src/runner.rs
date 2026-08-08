use std::time::Duration;

use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const STREAM_PTY_ENV: &str = "DOCKREV_STREAM_PTY";

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
        apply_command_env(&mut cmd, &spec.env);
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

        let (program, args) = stream_command(&spec);
        let mut cmd = Command::new(program);
        cmd.args(args);
        apply_command_env(&mut cmd, &spec.env);
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

fn apply_command_env(cmd: &mut Command, env: &[(String, String)]) {
    // This is an internal routing marker, never a child-process setting.
    cmd.env_remove(STREAM_PTY_ENV);
    for (key, value) in env {
        if key != STREAM_PTY_ENV {
            cmd.env(key, value);
        }
    }
}

fn stream_command(spec: &CommandSpec) -> (String, Vec<String>) {
    if !requires_stream_pty(spec) {
        return (spec.program.clone(), spec.args.clone());
    }

    #[cfg(target_os = "macos")]
    {
        // BSD script accepts a command argv after the transcript file instead
        // of util-linux's `-c` form.
        let mut args = vec![
            "-q".to_string(),
            "-e".to_string(),
            "-F".to_string(),
            "/dev/null".to_string(),
        ];
        args.push(spec.program.clone());
        args.extend(spec.args.iter().cloned());
        ("script".to_string(), args)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // `script` is supplied by the runtime image and gives Compose V1 a real
        // terminal while retaining the runner's normal streamed byte capture.
        let command = std::iter::once(shell_quote(&spec.program))
            .chain(spec.args.iter().map(|arg| shell_quote(arg)))
            .collect::<Vec<_>>()
            .join(" ");
        (
            "script".to_string(),
            vec![
                "-q".to_string(),
                "-e".to_string(),
                "-f".to_string(),
                "-c".to_string(),
                format!("exec {command}"),
                "/dev/null".to_string(),
            ],
        )
    }
}

fn requires_stream_pty(spec: &CommandSpec) -> bool {
    spec.env
        .iter()
        .any(|(key, value)| key == STREAM_PTY_ENV && value == "1")
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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

    #[test]
    fn stream_command_uses_a_pty_without_forwarding_its_routing_marker() {
        let spec = CommandSpec {
            program: "docker-compose".to_string(),
            args: vec!["pull".to_string(), "web service".to_string()],
            env: vec![
                ("DOCKER_CONFIG".to_string(), "/tmp/config".to_string()),
                (STREAM_PTY_ENV.to_string(), "1".to_string()),
            ],
        };

        let (program, args) = stream_command(&spec);

        assert_eq!(program, "script");
        if cfg!(target_os = "macos") {
            assert_eq!(
                args,
                [
                    "-q",
                    "-e",
                    "-F",
                    "/dev/null",
                    "docker-compose",
                    "pull",
                    "web service"
                ]
            );
        } else {
            assert_eq!(args[..4], ["-q", "-e", "-f", "-c"]);
            assert_eq!(args[5], "/dev/null");
            assert_eq!(args[4], "exec 'docker-compose' 'pull' 'web service'");
        }
    }

    #[test]
    fn shell_quote_preserves_embedded_single_quotes() {
        assert_eq!(shell_quote("it's ready"), "'it'\"'\"'s ready'");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn run_stream_pty_marker_is_not_forwarded_to_the_command() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut on_stdout = |chunk: Vec<u8>| stdout.extend(chunk);
        let mut on_stderr = |chunk: Vec<u8>| stderr.extend(chunk);

        let output = TokioCommandRunner
            .run_stream(
                CommandSpec {
                    program: "sh".to_string(),
                    args: vec![
                        "-c".to_string(),
                        "test -z \"${DOCKREV_STREAM_PTY+x}\"; printf '\\033[1Aprogress\\r'"
                            .to_string(),
                    ],
                    env: vec![(STREAM_PTY_ENV.to_string(), "1".to_string())],
                },
                Duration::from_secs(1),
                &mut on_stdout,
                &mut on_stderr,
            )
            .await
            .unwrap();

        assert_eq!(output.status, 0);
        let terminal_stream = [stdout.as_slice(), stderr.as_slice()].concat();
        assert!(
            terminal_stream.windows(4).any(|bytes| bytes == b"\x1b[1A"),
            "PTY stream lost its terminal control sequence: stdout={stdout:?}, stderr={stderr:?}"
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn run_stream_pty_propagates_child_exit_status() {
        let mut discard_stdout = |_chunk: Vec<u8>| {};
        let mut discard_stderr = |_chunk: Vec<u8>| {};

        let output = TokioCommandRunner
            .run_stream(
                CommandSpec {
                    program: "sh".to_string(),
                    args: vec!["-c".to_string(), "exit 7".to_string()],
                    env: vec![(STREAM_PTY_ENV.to_string(), "1".to_string())],
                },
                Duration::from_secs(1),
                &mut discard_stdout,
                &mut discard_stderr,
            )
            .await
            .unwrap();

        assert_eq!(output.status, 7);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[tokio::test]
    async fn run_stream_pty_kills_timed_out_child_before_delayed_side_effect() {
        let marker = std::env::temp_dir().join(format!(
            "dockrev-stream-pty-timeout-{}-{}.marker",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let command = format!("sleep 0.2; touch {}", marker.display());
        let mut discard_stdout = |_chunk: Vec<u8>| {};
        let mut discard_stderr = |_chunk: Vec<u8>| {};

        let result = TokioCommandRunner
            .run_stream(
                CommandSpec {
                    program: "sh".to_string(),
                    args: vec!["-c".to_string(), command],
                    env: vec![(STREAM_PTY_ENV.to_string(), "1".to_string())],
                },
                Duration::from_millis(20),
                &mut discard_stdout,
                &mut discard_stderr,
            )
            .await;

        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !marker.exists(),
            "timed-out PTY command still ran its delayed side effect"
        );
    }
}
