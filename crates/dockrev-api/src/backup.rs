use std::{path::PathBuf, time::Duration};

use serde_json::json;

use crate::api::types::{BackupSettings, BackupTarget, JobScope, StackRecord, TernaryChoice};
use crate::compose_runner::{ComposeRunnerConfig, ComposeStack};
use crate::docker_runner;
use crate::runner::{CommandRunner, CommandSpec};

#[derive(Clone, Debug)]
pub struct BackupRunResult {
    pub status: String,
    pub artifact_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub summary_json: serde_json::Value,
    pub log_lines: Vec<String>,
}

pub fn should_run_backup(settings: &BackupSettings, backup_mode: &str) -> bool {
    match backup_mode {
        "skip" => false,
        "force" => true,
        _ => settings.enabled,
    }
}

pub fn spawn_cleanup_task(state: std::sync::Arc<crate::state::AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = cleanup_once(&state).await {
                tracing::warn!(error = %e, "backup cleanup tick failed");
            }
        }
    });
}

pub async fn run_pre_update_backup(
    runner: &dyn CommandRunner,
    settings: &BackupSettings,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    now_rfc3339: &str,
) -> anyhow::Result<BackupRunResult> {
    if stack.backup.targets.is_empty() {
        return Ok(BackupRunResult {
            status: "skipped".to_string(),
            artifact_path: None,
            size_bytes: None,
            summary_json: json!({ "status": "skipped", "reason": "no_targets" }),
            log_lines: vec!["backup: skipped (no targets)".to_string()],
        });
    }

    let services = match scope {
        JobScope::All => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Stack => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Service => stack
            .services
            .iter()
            .filter(|s| service_id.is_some_and(|id| id == s.id))
            .collect::<Vec<_>>(),
    };

    let stack_dir = PathBuf::from(&settings.base_dir).join(&stack.id);
    tokio::fs::create_dir_all(&stack_dir).await?;

    let ts_slug = timestamp_slug(now_rfc3339);
    let artifact_path = stack_dir
        .join(format!("{ts_slug}.tar.gz"))
        .to_string_lossy()
        .to_string();

    let mut included = Vec::new();
    let mut decisions = Vec::new();

    for target in &stack.backup.targets {
        let effective = effective_choice_for_target(target, &services);
        if matches!(effective, TernaryChoice::Skip) {
            decisions
                .push(json!({"target": target, "status":"skipped", "reason":"skipped_by_user"}));
            continue;
        }

        let probe = probe_size_bytes(runner, target).await;
        let size_bytes = match probe {
            Ok(bytes) => bytes,
            Err(e) => {
                decisions.push(json!({"target": target, "status":"skipped", "reason":"skipped_by_probe_error", "error": e.to_string()}));
                continue;
            }
        };

        let over_threshold = size_bytes > settings.skip_targets_over_bytes;
        if matches!(effective, TernaryChoice::Inherit) && over_threshold {
            decisions.push(json!({"target": target, "status":"skipped", "reason":"skipped_by_size", "sizeBytes": size_bytes}));
            continue;
        }

        included.push((target.clone(), size_bytes));
        decisions.push(json!({"target": target, "status":"included", "sizeBytes": size_bytes, "effective": ternary_str(&effective)}));
    }

    if included.is_empty() {
        return Ok(BackupRunResult {
            status: "skipped".to_string(),
            artifact_path: None,
            size_bytes: None,
            summary_json: json!({ "status": "skipped", "reason": "no_included_targets", "targets": decisions }),
            log_lines: vec!["backup: skipped (no included targets)".to_string()],
        });
    }

    run_backup_container(runner, &stack_dir, &included, &ts_slug).await?;

    let size_bytes = tokio::fs::metadata(&artifact_path).await?.len();

    let mut log_lines = Vec::new();
    log_lines.push(format!(
        "backup: artifact={artifact_path} size_bytes={size_bytes}"
    ));
    for d in &decisions {
        log_lines.push(format!("backup: target={}", d));
    }

    Ok(BackupRunResult {
        status: "success".to_string(),
        artifact_path: Some(artifact_path.clone()),
        size_bytes: Some(size_bytes),
        summary_json: json!({
            "status": "success",
            "artifactPath": artifact_path,
            "sizeBytes": size_bytes,
            "targets": decisions,
        }),
        log_lines,
    })
}

async fn cleanup_once(state: &crate::state::AppState) -> anyhow::Result<()> {
    let now_dt = time::OffsetDateTime::now_utc();
    let now = now_dt.format(&time::format_description::well_known::Rfc3339)?;

    let due = state.db.list_due_backup_cleanups(&now).await?;
    if due.is_empty() {
        return Ok(());
    }

    for item in due {
        let Some(stack) = state.db.get_stack(&item.stack_id).await? else {
            continue;
        };

        let keep_last = stack.backup.retention.keep_last as usize;
        if keep_last > 0 {
            let ids = state
                .db
                .list_success_backup_ids_for_stack(&item.stack_id)
                .await?;
            if ids.iter().take(keep_last).any(|id| id == &item.id) {
                continue;
            }
        }

        let healthy = stack_is_healthy_now(&*state.runner, &state.config.compose_bin, &stack)
            .await
            .unwrap_or(false);
        if !healthy {
            continue;
        }

        let _ = tokio::fs::remove_file(&item.artifact_path).await;
        state.db.mark_backup_deleted(&item.id, &now).await?;
        let _ = state
            .db
            .insert_job_log(
                &item.job_id,
                &crate::api::types::JobLogLine {
                    ts: now.clone(),
                    level: "info".to_string(),
                    msg: format!("backup deleted: {}", item.artifact_path),
                },
            )
            .await;
    }

    Ok(())
}

async fn stack_is_healthy_now(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    stack: &StackRecord,
) -> anyhow::Result<bool> {
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
    };
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };

    let docker_cfg = docker_runner::DockerRunnerConfig::default();

    for svc in &stack.services {
        let container_id = run_to_string(
            runner,
            compose_stack.ps_q_service(&compose_cfg, &svc.name),
            Duration::from_secs(20),
        )
        .await?;
        let container_id = container_id.trim().to_string();
        if container_id.is_empty() {
            return Ok(false);
        }

        let has_health = run_to_string(
            runner,
            docker_runner::inspect_has_healthcheck(&docker_cfg, &container_id),
            Duration::from_secs(10),
        )
        .await?;
        let has_health = has_health.trim() == "1";
        if !has_health {
            continue;
        }

        let status = run_to_string(
            runner,
            docker_runner::inspect_health_status(&docker_cfg, &container_id),
            Duration::from_secs(10),
        )
        .await?;
        if status.trim() != "healthy" {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn run_to_string(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
) -> anyhow::Result<String> {
    let out = runner.run(spec, timeout).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "command failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(out.stdout)
}

fn sanitize_project_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "dockrev".to_string()
    } else {
        out
    }
}

fn timestamp_slug(now_rfc3339: &str) -> String {
    // Expect RFC3339; best-effort fallback.
    // Example: 2026-01-19T06:15:54Z -> 20260119-061554Z
    let cleaned = now_rfc3339.replace(['-', ':'], "");
    // 20260119T061554Z
    if let Some((date, rest)) = cleaned.split_once('T') {
        let time = rest.trim_end_matches('Z');
        let time = if time.len() >= 6 { &time[..6] } else { time };
        return format!("{}-{}Z", &date[..8.min(date.len())], time);
    }
    "backup".to_string()
}

fn effective_choice_for_target(
    target: &BackupTarget,
    services: &[&crate::api::types::Service],
) -> TernaryChoice {
    let mut choices = Vec::new();
    for svc in services {
        let choice = match target {
            BackupTarget::DockerVolume { name } => svc
                .settings
                .backup_targets
                .volume_names
                .get(name)
                .cloned()
                .unwrap_or(TernaryChoice::Inherit),
            BackupTarget::BindMount { path } => svc
                .settings
                .backup_targets
                .bind_paths
                .get(path)
                .cloned()
                .unwrap_or(TernaryChoice::Inherit),
        };
        choices.push(choice);
    }
    coalesce_choice(&choices)
}

fn coalesce_choice(choices: &[TernaryChoice]) -> TernaryChoice {
    // Force > Inherit > Skip
    if choices.iter().any(|c| matches!(c, TernaryChoice::Force)) {
        return TernaryChoice::Force;
    }
    if choices.iter().any(|c| matches!(c, TernaryChoice::Inherit)) {
        return TernaryChoice::Inherit;
    }
    TernaryChoice::Skip
}

fn ternary_str(choice: &TernaryChoice) -> &'static str {
    match choice {
        TernaryChoice::Inherit => "inherit",
        TernaryChoice::Skip => "skip",
        TernaryChoice::Force => "force",
    }
}

async fn probe_size_bytes(
    runner: &dyn CommandRunner,
    target: &BackupTarget,
) -> anyhow::Result<u64> {
    let mount = match target {
        BackupTarget::DockerVolume { name } => format!("{name}:/data:ro"),
        BackupTarget::BindMount { path } => format!("{path}:/data:ro"),
    };

    let spec = CommandSpec {
        program: "docker".to_string(),
        args: vec![
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            mount,
            "alpine".to_string(),
            "sh".to_string(),
            "-lc".to_string(),
            "du -sb /data | cut -f1".to_string(),
        ],
        env: Vec::new(),
    };

    let out = runner.run(spec, Duration::from_secs(30)).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "probe failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    let raw = out.stdout.trim();
    let bytes = raw
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("invalid du output: {raw} ({e})"))?;
    Ok(bytes)
}

async fn run_backup_container(
    runner: &dyn CommandRunner,
    stack_dir: &std::path::Path,
    included: &[(BackupTarget, u64)],
    ts_slug: &str,
) -> anyhow::Result<()> {
    let mut args = Vec::new();
    args.push("run".to_string());
    args.push("--rm".to_string());
    args.push("-v".to_string());
    args.push(format!("{}:/out", stack_dir.to_string_lossy()));

    let mut binds = 0usize;
    for (target, _) in included {
        match target {
            BackupTarget::DockerVolume { name } => {
                args.push("-v".to_string());
                args.push(format!("{name}:/backup/volumes/{name}:ro"));
            }
            BackupTarget::BindMount { path } => {
                let mount = format!("/backup/binds/{binds}");
                args.push("-v".to_string());
                args.push(format!("{path}:{mount}:ro"));
                binds += 1;
            }
        }
    }

    let tar_name = format!("{ts_slug}.tar");
    let sh = format!("tar -cf /out/{tar_name} -C /backup . && gzip -f /out/{tar_name}");
    args.push("alpine".to_string());
    args.push("sh".to_string());
    args.push("-lc".to_string());
    args.push(sh);

    let spec = CommandSpec {
        program: "docker".to_string(),
        args,
        env: Vec::new(),
    };

    let out = runner.run(spec, Duration::from_secs(600)).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "backup failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Clone, Default)]
    struct FakeRunner {
        sizes: BTreeMap<String, u64>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for FakeRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<crate::runner::CommandOutput> {
            if spec.program == "docker" && spec.args.get(0).is_some_and(|a| a == "run") {
                if spec.args.iter().any(|a| a.contains("du -sb /data")) {
                    let mount = spec
                        .args
                        .windows(2)
                        .find(|w| w[0] == "-v")
                        .map(|w| w[1].clone())
                        .unwrap_or_default();
                    let key = mount.split(':').next().unwrap_or_default().to_string();
                    let bytes = self.sizes.get(&key).copied().unwrap_or(0);
                    return Ok(crate::runner::CommandOutput {
                        status: 0,
                        stdout: format!("{bytes}\n"),
                        stderr: String::new(),
                    });
                }

                if let Some(out_mount) = spec
                    .args
                    .windows(2)
                    .find(|w| w[0] == "-v" && w[1].ends_with(":/out"))
                    .map(|w| w[1].clone())
                {
                    let host_dir = out_mount.split(':').next().unwrap_or_default();
                    let cmd = spec.args.last().cloned().unwrap_or_default();
                    let name = cmd
                        .split("/out/")
                        .nth(1)
                        .and_then(|s| s.split(".tar").next())
                        .unwrap_or("backup");
                    let path = PathBuf::from(host_dir).join(format!("{name}.tar.gz"));
                    tokio::fs::write(&path, vec![0u8; 10]).await?;
                    return Ok(crate::runner::CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    });
                }
            }

            Ok(crate::runner::CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn test_stack(targets: Vec<BackupTarget>) -> StackRecord {
        StackRecord {
            id: "stk_test".to_string(),
            name: "demo".to_string(),
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/tmp/compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig {
                targets,
                retention: Default::default(),
            },
            services: vec![crate::api::types::Service {
                id: "svc_test".to_string(),
                name: "web".to_string(),
                image: crate::api::types::ComposeRef {
                    reference: "ghcr.io/acme/web:5.2".to_string(),
                    tag: "5.2".to_string(),
                    digest: None,
                },
                candidate: None,
                ignore: None,
                settings: crate::api::types::ServiceSettings {
                    auto_rollback: true,
                    backup_targets: crate::api::types::BackupTargetOverrides {
                        bind_paths: BTreeMap::new(),
                        volume_names: BTreeMap::new(),
                    },
                },
            }],
        }
    }

    #[tokio::test]
    async fn backup_skips_over_threshold_for_inherit() {
        let tmp = std::env::temp_dir()
            .join(format!("dockrev-backup-test-{}", ulid::Ulid::new()))
            .to_string_lossy()
            .to_string();
        let settings = BackupSettings {
            enabled: true,
            require_success: true,
            base_dir: tmp.clone(),
            skip_targets_over_bytes: 100,
        };

        let runner = FakeRunner {
            sizes: BTreeMap::from([("big".to_string(), 1000)]),
        };

        let stack = test_stack(vec![BackupTarget::DockerVolume {
            name: "big".to_string(),
        }]);

        let out = run_pre_update_backup(
            &runner,
            &settings,
            &stack,
            &JobScope::Stack,
            None,
            "2026-01-19T00:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(out.status, "skipped");
    }

    #[tokio::test]
    async fn backup_includes_force_over_threshold() {
        let tmp = std::env::temp_dir()
            .join(format!("dockrev-backup-test-{}", ulid::Ulid::new()))
            .to_string_lossy()
            .to_string();
        let settings = BackupSettings {
            enabled: true,
            require_success: true,
            base_dir: tmp.clone(),
            skip_targets_over_bytes: 100,
        };

        let runner = FakeRunner {
            sizes: BTreeMap::from([("big".to_string(), 1000)]),
        };

        let mut stack = test_stack(vec![BackupTarget::DockerVolume {
            name: "big".to_string(),
        }]);
        stack.services[0]
            .settings
            .backup_targets
            .volume_names
            .insert("big".to_string(), TernaryChoice::Force);

        let out = run_pre_update_backup(
            &runner,
            &settings,
            &stack,
            &JobScope::Stack,
            None,
            "2026-01-19T00:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(out.status, "success");
        assert!(out.artifact_path.as_deref().unwrap().ends_with(".tar.gz"));
        assert_eq!(out.size_bytes, Some(10));
    }
}
