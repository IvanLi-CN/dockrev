use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    phase: &'a str,
    processed_bytes: u64,
    total_bytes: u64,
    compressed_bytes: u64,
    percent: u32,
    throughput_bps: u64,
    eta_seconds: Option<u64>,
}

pub async fn maybe_run_from_args() -> anyhow::Result<bool> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) != Some("backup-helper") {
        return Ok(false);
    }
    let source = required_arg(&args, "--source")?;
    let output_part = PathBuf::from(required_arg(&args, "--output-part")?);
    let output_final = PathBuf::from(required_arg(&args, "--output-final")?);
    let total_bytes = required_arg(&args, "--total-bytes")?.parse::<u64>()?;
    run(
        PathBuf::from(source),
        output_part,
        output_final,
        total_bytes,
    )
    .await?;
    Ok(true)
}

fn required_arg(args: &[String], name: &str) -> anyhow::Result<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))
}

async fn run(
    source: PathBuf,
    output_part: PathBuf,
    output_final: PathBuf,
    total_bytes: u64,
) -> anyhow::Result<()> {
    let result = run_inner(source, output_part.clone(), output_final, total_bytes).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&output_part).await;
    }
    result
}

pub(crate) async fn archive_directory(
    source: &std::path::Path,
    output_part: &std::path::Path,
    output_final: &std::path::Path,
    total_bytes: u64,
) -> anyhow::Result<()> {
    run(
        source.to_path_buf(),
        output_part.to_path_buf(),
        output_final.to_path_buf(),
        total_bytes,
    )
    .await
}

async fn run_inner(
    source: PathBuf,
    output_part: PathBuf,
    output_final: PathBuf,
    total_bytes: u64,
) -> anyhow::Result<()> {
    if let Some(parent) = output_part.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _ = tokio::fs::remove_file(&output_part).await;

    let mut tar = Command::new("tar")
        .args(["-cf", "-", "-C"])
        .arg(&source)
        .arg(".")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut zstd = Command::new("zstd")
        .args(["-T0", "--fast=1", "--check", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let mut tar_stdout = tar
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("tar stdout unavailable"))?;
    let mut zstd_stdin = zstd
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("zstd stdin unavailable"))?;
    let mut zstd_stdout = zstd
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("zstd stdout unavailable"))?;
    let mut tar_stderr = tar
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("tar stderr unavailable"))?;
    let mut zstd_stderr = zstd
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("zstd stderr unavailable"))?;
    let tar_stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        tar_stderr.read_to_end(&mut bytes).await?;
        anyhow::Ok(bytes)
    });
    let zstd_stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        zstd_stderr.read_to_end(&mut bytes).await?;
        anyhow::Ok(bytes)
    });
    let compressed_bytes = Arc::new(AtomicU64::new(0));
    let output_counter = compressed_bytes.clone();
    let output_path = output_part.clone();
    let output_task = tokio::spawn(async move {
        let mut file = tokio::fs::File::create(output_path).await?;
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let read = zstd_stdout.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).await?;
            output_counter.fetch_add(read as u64, Ordering::Relaxed);
        }
        file.sync_all().await?;
        anyhow::Ok(())
    });

    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut processed = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    emit_progress("archive", 0, total_bytes, 0, started);
    let pump_result = async {
        loop {
            let read = tar_stdout.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            zstd_stdin.write_all(&buffer[..read]).await?;
            processed = processed.saturating_add(read as u64);
            if last_emit.elapsed() >= Duration::from_millis(500) {
                emit_progress(
                    "archive",
                    processed,
                    total_bytes,
                    compressed_bytes.load(Ordering::Relaxed),
                    started,
                );
                last_emit = Instant::now();
            }
        }
        zstd_stdin.shutdown().await?;
        anyhow::Ok(())
    }
    .await;
    drop(zstd_stdin);
    if let Err(error) = pump_result {
        let _ = tar.kill().await;
        let _ = zstd.kill().await;
        output_task.abort();
        tar_stderr_task.abort();
        zstd_stderr_task.abort();
        let _ = output_task.await;
        let _ = tar_stderr_task.await;
        let _ = zstd_stderr_task.await;
        return Err(error.context("stream tar output into zstd"));
    }

    let tar_status = tar.wait().await?;
    let tar_stderr = tar_stderr_task.await??;
    let zstd_status = zstd.wait().await?;
    let zstd_stderr = zstd_stderr_task.await??;
    output_task.await??;
    if !tar_status.success() {
        return Err(anyhow::anyhow!(
            "tar failed: {}",
            String::from_utf8_lossy(&tar_stderr)
        ));
    }
    if !zstd_status.success() {
        return Err(anyhow::anyhow!(
            "zstd failed: {}",
            String::from_utf8_lossy(&zstd_stderr)
        ));
    }
    tokio::fs::rename(&output_part, &output_final).await?;
    emit_progress(
        "complete",
        total_bytes.max(processed),
        total_bytes,
        compressed_bytes.load(Ordering::Relaxed),
        started,
    );
    Ok(())
}

fn emit_progress(
    phase: &str,
    processed_bytes: u64,
    total_bytes: u64,
    compressed_bytes: u64,
    started: Instant,
) {
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let throughput_bps = (processed_bytes as f64 / elapsed) as u64;
    let raw_percent = if total_bytes == 0 {
        0
    } else {
        ((processed_bytes.saturating_mul(100)) / total_bytes).min(100) as u32
    };
    let percent = if phase == "complete" {
        100
    } else {
        raw_percent.min(99)
    };
    let remaining = total_bytes.saturating_sub(processed_bytes);
    let eta_seconds = (throughput_bps > 0).then(|| remaining / throughput_bps);
    let progress = Progress {
        phase,
        processed_bytes,
        total_bytes,
        compressed_bytes,
        percent,
        throughput_bps,
        eta_seconds,
    };
    println!(
        "{}",
        serde_json::to_string(&progress).expect("serialize backup progress")
    );
}
