use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::types::{BackupSettings, BackupTarget, BackupTargetPolicy, JobScope, StackRecord};
use crate::compose_runner::{ComposeRunnerConfig, ComposeStack};
use crate::runner::{CommandRunner, CommandSpec};

#[derive(Clone, Debug)]
pub struct BackupRunResult {
    pub status: String,
    pub artifact_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub summary_json: serde_json::Value,
    pub log_lines: Vec<String>,
    pub services_kept_stopped: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupProgressEvent {
    pub phase: String,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub compressed_bytes: u64,
    pub percent: u32,
    pub throughput_bps: u64,
    pub eta_seconds: Option<u64>,
}

const RECOVERY_CHECKPOINT_PREFIX: &str = "backup-recovery-checkpoint: ";

#[derive(Debug)]
struct BackupSafetyError(String);

impl std::fmt::Display for BackupSafetyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BackupSafetyError {}

pub fn is_safety_failure(error: &anyhow::Error) -> bool {
    error.downcast_ref::<BackupSafetyError>().is_some()
}

fn safety_failure(message: String) -> anyhow::Error {
    BackupSafetyError(message).into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRecoveryCheckpoint {
    backup_id: String,
    stack_id: String,
    artifact_key: String,
    services: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackupRecoverySnapshot {
    pub stack_id: String,
    pub services: Vec<String>,
}

#[async_trait::async_trait]
pub trait BackupRecoveryStore: Send + Sync {
    async fn save(&self, snapshot: &BackupRecoverySnapshot) -> anyhow::Result<()>;
    async fn clear(&self) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
struct IncludedBackupTarget {
    target: BackupTarget,
    policy: BackupTargetPolicy,
    related_services: Vec<String>,
    size_bytes: u64,
}

pub fn should_run_backup(settings: &BackupSettings, backup_mode: &str) -> bool {
    match backup_mode {
        "skip" => false,
        "force" => true,
        _ => settings.enabled,
    }
}

pub fn requires_service_stop(
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
) -> bool {
    let services = match scope {
        JobScope::All | JobScope::Stack => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Service => stack
            .services
            .iter()
            .filter(|service| Some(service.id.as_str()) == service_id)
            .collect::<Vec<_>>(),
    };
    stack.backup.targets.iter().any(|target| {
        matches!(
            effective_policy_for_target(target, &services),
            BackupTargetPolicy::StopRelatedServices
        )
    })
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

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn run_pre_update_backup(
    runner: &dyn CommandRunner,
    helper_runner: &dyn CommandRunner,
    recovery_runner: &dyn CommandRunner,
    recovery_store: &dyn BackupRecoveryStore,
    settings: &BackupSettings,
    db_path: &Path,
    helper_image_fallback: &str,
    backup_id: &str,
    job_id: &str,
    compose_bin: &str,
    docker_config_path: Option<&std::path::Path>,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    keep_stopped_services: &[String],
    now_rfc3339: &str,
    recovery_db: Option<&crate::db::Db>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<BackupProgressEvent>>,
) -> anyhow::Result<BackupRunResult> {
    let _managed_override_operation_guard = crate::managed_override::operation_lock().await;
    run_pre_update_backup_unlocked(
        runner,
        helper_runner,
        recovery_runner,
        recovery_store,
        settings,
        db_path,
        helper_image_fallback,
        backup_id,
        job_id,
        compose_bin,
        docker_config_path,
        stack,
        scope,
        service_id,
        keep_stopped_services,
        now_rfc3339,
        recovery_db,
        progress_tx,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_pre_update_backup_unlocked(
    runner: &dyn CommandRunner,
    helper_runner: &dyn CommandRunner,
    recovery_runner: &dyn CommandRunner,
    recovery_store: &dyn BackupRecoveryStore,
    settings: &BackupSettings,
    db_path: &Path,
    helper_image_fallback: &str,
    backup_id: &str,
    job_id: &str,
    compose_bin: &str,
    docker_config_path: Option<&std::path::Path>,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    keep_stopped_services: &[String],
    now_rfc3339: &str,
    recovery_db: Option<&crate::db::Db>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<BackupProgressEvent>>,
) -> anyhow::Result<BackupRunResult> {
    if stack.backup.targets.is_empty() {
        return Ok(BackupRunResult {
            status: "skipped".to_string(),
            artifact_path: None,
            size_bytes: None,
            summary_json: json!({ "status": "skipped", "reason": "no_targets" }),
            log_lines: vec!["backup: skipped (no targets)".to_string()],
            services_kept_stopped: Vec::new(),
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

    let storage = crate::backup_storage::resolve_backup_storage(runner, db_path).await?;

    let ts_slug = timestamp_slug(now_rfc3339);
    let file_name = format!("{ts_slug}.tar.zst");
    let artifact_key = storage.artifact_key(&stack.id, &file_name);
    let artifact_path = storage
        .logical_artifact_path(&artifact_key)
        .to_string_lossy()
        .to_string();

    let mut included = Vec::<IncludedBackupTarget>::new();
    let mut decisions = Vec::new();

    for target in &stack.backup.targets {
        let effective = effective_policy_for_target(target, &services);
        if matches!(effective, BackupTargetPolicy::Disabled) {
            decisions
                .push(json!({"target": target, "status":"skipped", "reason":"skipped_by_user"}));
            continue;
        }

        let probe =
            probe_size_bytes(runner, target, storage.helper_image(helper_image_fallback)).await;
        let size_bytes = match probe {
            Ok(bytes) => bytes,
            Err(e) => {
                decisions.push(json!({"target": target, "status":"skipped", "reason":"skipped_by_probe_error", "error": e.to_string()}));
                continue;
            }
        };

        let over_threshold = size_bytes > settings.skip_targets_over_bytes;
        if matches!(effective, BackupTargetPolicy::LiveBackup) && over_threshold {
            decisions.push(json!({"target": target, "status":"skipped", "reason":"skipped_by_size", "sizeBytes": size_bytes}));
            continue;
        }

        let related_services = declared_related_service_names(target, stack);
        included.push(IncludedBackupTarget {
            target: target.clone(),
            policy: effective,
            related_services: related_services.clone(),
            size_bytes,
        });
        decisions.push(json!({
            "target": target,
            "status":"included",
            "sizeBytes": size_bytes,
            "policy": effective.as_str(),
            "relatedServices": related_services
        }));
    }

    if included.is_empty() {
        return Ok(BackupRunResult {
            status: "skipped".to_string(),
            artifact_path: None,
            size_bytes: None,
            summary_json: json!({ "status": "skipped", "reason": "no_included_targets", "targets": decisions }),
            log_lines: vec!["backup: skipped (no included targets)".to_string()],
            services_kept_stopped: Vec::new(),
        });
    }

    let compose_cfg = compose_runner_config(docker_config_path, compose_bin)?;
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };
    let services_to_restart =
        running_related_services_for_backup(runner, &compose_stack, &compose_cfg, &included)
            .await?;
    recovery_store
        .save(&BackupRecoverySnapshot {
            stack_id: stack.id.clone(),
            services: services_to_restart.clone(),
        })
        .await?;
    if !services_to_restart.is_empty()
        && let Some(db) = recovery_db
    {
        let checkpoint = BackupRecoveryCheckpoint {
            backup_id: backup_id.to_string(),
            stack_id: stack.id.clone(),
            artifact_key: artifact_key.to_string_lossy().to_string(),
            services: services_to_restart.clone(),
        };
        if let Err(error) = db
            .insert_job_log(
                job_id,
                &crate::api::types::JobLogLine {
                    ts: now_rfc3339.to_string(),
                    level: "info".to_string(),
                    msg: format!(
                        "{RECOVERY_CHECKPOINT_PREFIX}{}",
                        serde_json::to_string(&checkpoint)?
                    ),
                },
            )
            .await
        {
            return Err(error.context("persist backup recovery checkpoint"));
        }
    }
    if let Err(error) =
        stop_services_for_backup(runner, &compose_stack, &compose_cfg, &services_to_restart).await
    {
        if let Err(restore_error) = restart_related_services_after_backup(
            recovery_runner,
            &compose_stack,
            &compose_cfg,
            &services_to_restart,
        )
        .await
        {
            return Err(safety_failure(format!(
                "stopping services failed ({error}); restoring services failed ({restore_error})"
            )));
        }
        recovery_store.clear().await?;
        return Err(error);
    }
    let total_bytes = included.iter().map(|item| item.size_bytes).sum::<u64>();
    let backup_result = run_backup_container(
        helper_runner,
        &storage,
        helper_image_fallback,
        &artifact_key,
        &included,
        total_bytes,
        backup_id,
        job_id,
        progress_tx,
    )
    .await;
    let services_kept_stopped = services_to_restart
        .iter()
        .filter(|service| keep_stopped_services.contains(service))
        .cloned()
        .collect::<Vec<_>>();
    match backup_result {
        Ok(()) => {
            let services_to_resume = services_to_restart
                .iter()
                .filter(|service| !keep_stopped_services.contains(service))
                .cloned()
                .collect::<Vec<_>>();
            if let Err(resume_error) = restart_related_services_after_backup(
                recovery_runner,
                &compose_stack,
                &compose_cfg,
                &services_to_resume,
            )
            .await
            {
                if let Err(restore_error) = restart_related_services_after_backup(
                    recovery_runner,
                    &compose_stack,
                    &compose_cfg,
                    &services_to_restart,
                )
                .await
                {
                    return Err(safety_failure(format!(
                        "resuming non-update services failed ({resume_error}); restoring all stopped services failed ({restore_error})"
                    )));
                }
                recovery_store.clear().await?;
                return Err(resume_error.context("resume non-update services after backup"));
            }
        }
        Err(error) => {
            if let Err(restore_error) = restart_related_services_after_backup(
                recovery_runner,
                &compose_stack,
                &compose_cfg,
                &services_to_restart,
            )
            .await
            {
                return Err(safety_failure(format!(
                    "backup failed ({error}); restoring previous services failed ({restore_error})"
                )));
            }
            recovery_store.clear().await?;
            return Err(error);
        }
    }

    let size_bytes = match artifact_size_bytes(
        runner,
        &storage,
        helper_image_fallback,
        &artifact_key,
    )
    .await
    {
        Ok(size) => size,
        Err(error) => {
            if let Err(restore_error) = restart_related_services_after_backup(
                recovery_runner,
                &compose_stack,
                &compose_cfg,
                &services_kept_stopped,
            )
            .await
            {
                return Err(safety_failure(format!(
                    "reading backup artifact size failed ({error}); restoring services failed ({restore_error})"
                )));
            }
            recovery_store.clear().await?;
            return Err(error.context("read backup artifact size"));
        }
    };

    let mut log_lines = Vec::new();
    log_lines.push(format!(
        "backup: artifact={artifact_path} size_bytes={size_bytes}"
    ));
    for d in &decisions {
        log_lines.push(format!("backup: target={}", d));
    }

    if services_kept_stopped.is_empty() {
        recovery_store.clear().await?;
    }

    Ok(BackupRunResult {
        status: "success".to_string(),
        artifact_path: Some(artifact_path.clone()),
        size_bytes: Some(size_bytes),
        summary_json: json!({
            "status": "success",
            "artifactPath": artifact_path,
            "artifactKey": artifact_key.to_string_lossy(),
            "archiveFormat": "tar",
            "compression": "zstd",
            "sizeBytes": size_bytes,
            "targets": decisions,
        }),
        log_lines,
        services_kept_stopped,
    })
}

pub async fn recover_interrupted_backups(
    state: &crate::state::AppState,
    recovered_job_ids: &[String],
) -> anyhow::Result<()> {
    for job_id in recovered_job_ids {
        if state.db.get_update_stop_control(job_id).await?.is_some() {
            // Controlled update recovery runs only after the API has bound.
            continue;
        }
        let logs = state.db.list_job_logs(job_id).await?;
        let checkpoint = logs.iter().rev().find_map(|line| {
            line.msg
                .strip_prefix(RECOVERY_CHECKPOINT_PREFIX)
                .and_then(|raw| serde_json::from_str::<BackupRecoveryCheckpoint>(raw).ok())
        });
        let Some(checkpoint) = checkpoint else {
            continue;
        };

        stop_interrupted_helper(&*state.runner, job_id).await?;
        let storage =
            crate::backup_storage::resolve_backup_storage(&*state.runner, &state.config.db_path)
                .await?;
        let part_key = PathBuf::from(format!("{}.part", checkpoint.artifact_key));
        delete_artifact_if_present(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &part_key,
        )
        .await?;

        let stack = state
            .db
            .get_stack(&checkpoint.stack_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("recovery stack not found: {}", checkpoint.stack_id))?;
        restore_services_after_failed_apply(
            &*state.runner,
            &state.config.compose_bin,
            state.config.docker_config_path.as_deref(),
            &stack,
            &state.config.managed_override_dir,
            &checkpoint.services,
        )
        .await?;
        state
            .db
            .insert_job_log(
                job_id,
                &crate::api::types::JobLogLine {
                    ts: time::OffsetDateTime::now_utc()
                        .format(&time::format_description::well_known::Rfc3339)?,
                    level: "warn".to_string(),
                    msg: format!(
                        "backup recovery restored services: {}",
                        checkpoint.services.join(",")
                    ),
                },
            )
            .await?;
    }
    Ok(())
}

async fn stop_interrupted_helper(runner: &dyn CommandRunner, job_id: &str) -> anyhow::Result<()> {
    let out = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=cc.ivanli.dockrev.job-id={job_id}"),
                    "--filter".to_string(),
                    "label=cc.ivanli.dockrev.stop-mode=stop".to_string(),
                ],
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "list interrupted backup helper failed: {}",
            out.stderr
        ));
    }
    let ids = out.stdout.split_whitespace().collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let mut args = vec!["stop".to_string(), "--time".to_string(), "2".to_string()];
    args.extend(ids.into_iter().map(str::to_string));
    let stopped = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args,
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;
    if stopped.status != 0 {
        return Err(anyhow::anyhow!(
            "stop interrupted backup helper failed: {}",
            stopped.stderr
        ));
    }
    Ok(())
}

pub(crate) async fn cleanup_once(state: &crate::state::AppState) -> anyhow::Result<()> {
    let now_dt = time::OffsetDateTime::now_utc();
    let now = now_dt.format(&time::format_description::well_known::Rfc3339)?;

    let due = state.db.list_due_backup_cleanups(&now).await?;
    if due.is_empty() {
        return Ok(());
    }

    for item in due {
        let Some(stack) = state.db.get_stack(&item.stack_id).await? else {
            let _ = state
                .db
                .mark_backup_cleanup_failed(&item.id, &now, "stack not found")
                .await;
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

        if let Err(error) = state.db.mark_backup_cleanup_attempt(&item.id, &now).await {
            tracing::warn!(backup_id = %item.id, error = %error, "backup cleanup attempt state update failed");
            continue;
        }

        let storage = match crate::backup_storage::resolve_backup_storage(
            &*state.runner,
            &state.config.db_path,
        )
        .await
        {
            Ok(storage) => storage,
            Err(error) => {
                state
                    .db
                    .mark_backup_cleanup_failed(&item.id, &now, &error.to_string())
                    .await?;
                tracing::warn!(backup_id = %item.id, error = %error, "backup cleanup storage unresolved");
                continue;
            }
        };
        let Some(key) = legacy_artifact_key(&storage, &item.artifact_path) else {
            let error = format!(
                "backup cleanup path is outside managed storage: {}",
                item.artifact_path
            );
            state
                .db
                .mark_backup_cleanup_failed(&item.id, &now, &error)
                .await?;
            tracing::warn!(backup_id = %item.id, path = %item.artifact_path, "backup cleanup path is outside managed storage");
            continue;
        };
        match reconcile_artifact(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &key,
        )
        .await
        {
            Ok(ArtifactCleanupOutcome::Deleted) => {
                state.db.mark_backup_deleted(&item.id, &now).await?;
            }
            Ok(ArtifactCleanupOutcome::Missing) => {
                state.db.mark_backup_missing(&item.id, &now).await?;
                if let Some(job_id) = item.job_id.as_deref() {
                    let _ = state
                        .db
                        .insert_job_log(
                            job_id,
                            &crate::api::types::JobLogLine {
                                ts: now.clone(),
                                level: "info".to_string(),
                                msg: format!("backup missing (verified): {}", item.artifact_path),
                            },
                        )
                        .await;
                }
                continue;
            }
            Err(error) => {
                state
                    .db
                    .mark_backup_cleanup_failed(&item.id, &now, &error.to_string())
                    .await?;
                tracing::warn!(backup_id = %item.id, error = %error, "backup cleanup delete failed");
                continue;
            }
        }
        if let Some(job_id) = item.job_id.as_deref() {
            let _ = state
                .db
                .insert_job_log(
                    job_id,
                    &crate::api::types::JobLogLine {
                        ts: now.clone(),
                        level: "info".to_string(),
                        msg: format!("backup deleted: {}", item.artifact_path),
                    },
                )
                .await;
        }
    }

    Ok(())
}

fn compose_runner_config(
    docker_config_path: Option<&std::path::Path>,
    compose_bin: &str,
) -> anyhow::Result<ComposeRunnerConfig> {
    let env = docker_config_path
        .map(|path| {
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_string_lossy()
                .to_string()
        })
        .map(|dir| vec![("DOCKER_CONFIG".to_string(), dir)])
        .unwrap_or_default();
    Ok(ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
        env,
    })
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

fn effective_policy_for_target(
    target: &BackupTarget,
    services: &[&crate::api::types::Service],
) -> BackupTargetPolicy {
    let mut choices = Vec::new();
    for svc in services {
        let choice = match target {
            BackupTarget::DockerVolume { name } => svc
                .settings
                .backup_targets
                .volume_names
                .get(name)
                .map(choice_to_policy)
                .unwrap_or(BackupTargetPolicy::Disabled),
            BackupTarget::BindMount { path } => svc
                .settings
                .backup_targets
                .bind_paths
                .get(path)
                .map(choice_to_policy)
                .unwrap_or(BackupTargetPolicy::Disabled),
        };
        choices.push(choice);
    }
    coalesce_policy(&choices)
}

fn choice_to_policy(choice: &crate::api::types::TernaryChoice) -> BackupTargetPolicy {
    match choice {
        crate::api::types::TernaryChoice::Force => BackupTargetPolicy::StopRelatedServices,
        crate::api::types::TernaryChoice::Inherit => BackupTargetPolicy::LiveBackup,
        crate::api::types::TernaryChoice::Skip => BackupTargetPolicy::Disabled,
    }
}

fn coalesce_policy(choices: &[BackupTargetPolicy]) -> BackupTargetPolicy {
    if choices
        .iter()
        .any(|c| matches!(c, BackupTargetPolicy::StopRelatedServices))
    {
        return BackupTargetPolicy::StopRelatedServices;
    }
    if choices
        .iter()
        .any(|c| matches!(c, BackupTargetPolicy::LiveBackup))
    {
        return BackupTargetPolicy::LiveBackup;
    }
    BackupTargetPolicy::Disabled
}

fn declared_related_service_names(target: &BackupTarget, stack: &StackRecord) -> Vec<String> {
    stack
        .services
        .iter()
        .filter_map(|service| {
            let matched = match target {
                BackupTarget::DockerVolume { name } => service
                    .settings
                    .backup_targets
                    .volume_names
                    .contains_key(name),
                BackupTarget::BindMount { path } => service
                    .settings
                    .backup_targets
                    .bind_paths
                    .contains_key(path),
            };
            matched.then(|| service.name.clone())
        })
        .collect()
}

async fn running_related_services_for_backup(
    runner: &dyn CommandRunner,
    compose_stack: &ComposeStack,
    compose_cfg: &ComposeRunnerConfig,
    included: &[IncludedBackupTarget],
) -> anyhow::Result<Vec<String>> {
    let mut services_to_stop = std::collections::BTreeSet::<String>::new();
    for item in included {
        if !matches!(item.policy, BackupTargetPolicy::StopRelatedServices) {
            continue;
        }
        services_to_stop.extend(item.related_services.iter().cloned());
    }
    if services_to_stop.is_empty() {
        return Ok(Vec::new());
    }
    let services = services_to_stop.into_iter().collect::<Vec<_>>();
    let mut running_services = Vec::new();
    for service in services {
        let container_id = run_to_string(
            runner,
            compose_stack.ps_q_service(compose_cfg, &service),
            Duration::from_secs(20),
        )
        .await?;
        if !container_id.trim().is_empty() {
            running_services.push(service);
        }
    }
    Ok(running_services)
}

async fn stop_services_for_backup(
    runner: &dyn CommandRunner,
    compose_stack: &ComposeStack,
    compose_cfg: &ComposeRunnerConfig,
    running_services: &[String],
) -> anyhow::Result<()> {
    if running_services.is_empty() {
        return Ok(());
    }
    let out = runner
        .run(
            compose_stack.stop_services(compose_cfg, running_services),
            Duration::from_secs(120),
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "stop services failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

async fn restart_related_services_after_backup(
    runner: &dyn CommandRunner,
    compose_stack: &ComposeStack,
    compose_cfg: &ComposeRunnerConfig,
    services: &[String],
) -> anyhow::Result<()> {
    if services.is_empty() {
        return Ok(());
    }
    let out = runner
        .run(
            compose_stack.up_services(compose_cfg, services),
            Duration::from_secs(180),
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "restart services failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

pub async fn restore_services_after_failed_apply(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    stack: &StackRecord,
    managed_override_dir: &Path,
    services: &[String],
) -> anyhow::Result<()> {
    let _managed_override_operation_guard = crate::managed_override::operation_lock().await;
    restore_services_after_failed_apply_unlocked(
        runner,
        compose_bin,
        docker_config_path,
        stack,
        managed_override_dir,
        services,
    )
    .await
}

pub(crate) async fn retain_running_services(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    stack: &StackRecord,
    services: &[String],
) -> anyhow::Result<Vec<String>> {
    let compose_cfg = compose_runner_config(docker_config_path, compose_bin)?;
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };
    let mut running = Vec::new();
    for service in services {
        let container_id = run_to_string(
            runner,
            compose_stack.ps_q_service(&compose_cfg, service),
            Duration::from_secs(30),
        )
        .await?;
        if !container_id.trim().is_empty() {
            running.push(service.clone());
        }
    }
    Ok(running)
}

pub(crate) async fn restore_services_after_failed_apply_unlocked(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    stack: &StackRecord,
    managed_override_dir: &Path,
    services: &[String],
) -> anyhow::Result<()> {
    let compose_cfg = compose_runner_config(docker_config_path, compose_bin)?;
    let managed_override_path =
        crate::managed_override::managed_override_path(managed_override_dir, &stack.id);
    let pending_override_snapshot =
        crate::managed_override::has_pending_snapshot(&managed_override_path);
    let pending_override_applied = pending_override_snapshot
        && crate::managed_override::pending_snapshot_is_applied(&managed_override_path)?;
    if pending_override_applied && !managed_override_path.is_file() {
        anyhow::bail!(
            "applied managed override transaction has no active override: {}",
            managed_override_path.display()
        );
    }
    if pending_override_snapshot && !pending_override_applied {
        let _override_guard = crate::managed_override::lock();
        let snapshot = format!("{}.previous", managed_override_path.display());
        if Path::new(&snapshot).is_file() {
            crate::managed_override::restore_snapshot(&managed_override_path, Some(&snapshot))
                .context("restore managed override before service recovery")?;
        } else {
            anyhow::bail!(
                "managed override pending marker has no previous snapshot: {}",
                managed_override_path.display()
            );
        }
    }
    let mut compose = stack.compose.clone();
    if managed_override_path.is_file() {
        compose
            .compose_files
            .push(managed_override_path.to_string_lossy().to_string());
    }
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose,
    };
    if services.is_empty() {
        if pending_override_snapshot {
            crate::managed_override::discard_snapshot(&managed_override_path)
                .context("discard managed override snapshot after empty service recovery")?;
        }
        return Ok(());
    }
    let out = runner
        .run(
            compose_stack.up_services_no_pull_no_deps_force_recreate(&compose_cfg, services),
            Duration::from_secs(180),
        )
        .await?;
    if out.status != 0 {
        anyhow::bail!(
            "restart services failed: status={} stderr={}",
            out.status,
            out.stderr
        );
    }
    if pending_override_snapshot {
        crate::managed_override::discard_snapshot(&managed_override_path)
            .context("discard managed override snapshot after service recovery")?;
    }
    Ok(())
}

pub async fn restore_backup_recovery_snapshot(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    stack: &StackRecord,
    managed_override_dir: &Path,
    snapshot: &BackupRecoverySnapshot,
) -> anyhow::Result<()> {
    let _managed_override_operation_guard = crate::managed_override::operation_lock().await;
    restore_backup_recovery_snapshot_unlocked(
        runner,
        compose_bin,
        docker_config_path,
        stack,
        managed_override_dir,
        snapshot,
    )
    .await
}

pub(crate) async fn restore_backup_recovery_snapshot_unlocked(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    stack: &StackRecord,
    managed_override_dir: &Path,
    snapshot: &BackupRecoverySnapshot,
) -> anyhow::Result<()> {
    if snapshot.stack_id != stack.id {
        return Err(anyhow::anyhow!(
            "backup recovery snapshot belongs to another stack"
        ));
    }
    restore_services_after_failed_apply_unlocked(
        runner,
        compose_bin,
        docker_config_path,
        stack,
        managed_override_dir,
        &snapshot.services,
    )
    .await
}

async fn probe_size_bytes(
    runner: &dyn CommandRunner,
    target: &BackupTarget,
    helper_image: &str,
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
            helper_image.to_string(),
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

#[allow(clippy::too_many_arguments)]
async fn run_backup_container(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
    included: &[IncludedBackupTarget],
    total_bytes: u64,
    backup_id: &str,
    job_id: &str,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<BackupProgressEvent>>,
) -> anyhow::Result<()> {
    let (output_source, output_relative) = storage.helper_output_mount();
    let mut args = Vec::new();
    args.push("run".to_string());
    args.push("--rm".to_string());
    args.push("--name".to_string());
    args.push(format!("dockrev-backup-{backup_id}"));
    args.push("--label".to_string());
    args.push(format!("cc.ivanli.dockrev.backup-id={backup_id}"));
    args.push("--label".to_string());
    args.push(format!("cc.ivanli.dockrev.job-id={job_id}"));
    args.push("--label".to_string());
    args.push(format!(
        "cc.ivanli.dockrev.stop-mode={}",
        if included
            .iter()
            .any(|item| matches!(item.policy, BackupTargetPolicy::StopRelatedServices))
        {
            "stop"
        } else {
            "live"
        }
    ));
    args.push("-v".to_string());
    args.push(format!("{output_source}:/out-root"));

    let mut binds = 0usize;
    for item in included {
        match &item.target {
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

    let final_relative = output_relative.join(artifact_key);
    let part_relative = PathBuf::from(format!("{}.part", final_relative.to_string_lossy()));
    args.push(storage.helper_image(helper_image_fallback).to_string());
    args.push("/usr/local/bin/dockrev".to_string());
    args.push("backup-helper".to_string());
    args.push("--source".to_string());
    args.push("/backup".to_string());
    args.push("--output-part".to_string());
    args.push(format!("/out-root/{}", part_relative.to_string_lossy()));
    args.push("--output-final".to_string());
    args.push(format!("/out-root/{}", final_relative.to_string_lossy()));
    args.push("--total-bytes".to_string());
    args.push(total_bytes.to_string());

    let spec = CommandSpec {
        program: "docker".to_string(),
        args,
        env: Vec::new(),
    };

    let mut pending = String::new();
    let mut on_stdout = |chunk: Vec<u8>| {
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(newline) = pending.find('\n') {
            let line = pending[..newline].trim().to_string();
            pending.drain(..=newline);
            if let Ok(progress) = serde_json::from_str::<BackupProgressEvent>(&line)
                && let Some(tx) = progress_tx.as_ref()
            {
                let _ = tx.send(progress);
            }
        }
    };
    let mut stderr_bytes = Vec::new();
    let mut on_stderr = |chunk: Vec<u8>| stderr_bytes.extend_from_slice(&chunk);
    let out = runner
        .run_stream(
            spec,
            Duration::from_secs(600),
            &mut on_stdout,
            &mut on_stderr,
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "backup failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

async fn artifact_size_bytes(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<u64> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            Ok(tokio::fs::metadata(logical_root.join(artifact_key))
                .await?
                .len())
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root:ro"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "sh".to_string(),
                            "-lc".to_string(),
                            format!("stat -c %s '/out-root/{}'", path.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            if out.status != 0 {
                return Err(anyhow::anyhow!(
                    "backup artifact stat failed: {}",
                    out.stderr
                ));
            }
            Ok(out.stdout.trim().parse::<u64>()?)
        }
    }
}

fn legacy_artifact_key(
    storage: &crate::backup_storage::BackupStorage,
    artifact_path: &str,
) -> Option<PathBuf> {
    let key = Path::new(artifact_path)
        .strip_prefix(storage.logical_root())
        .ok()
        .filter(|key| !key.as_os_str().is_empty())?;
    key.components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
        .then(|| key.to_path_buf())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactCleanupOutcome {
    Deleted,
    Missing,
}

async fn reconcile_artifact(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<ArtifactCleanupOutcome> {
    if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
        return Ok(ArtifactCleanupOutcome::Missing);
    }

    match delete_artifact(runner, storage, helper_image_fallback, artifact_key).await {
        Ok(()) => Ok(ArtifactCleanupOutcome::Deleted),
        Err(error) => {
            // A concurrent actor may have removed the file after the existence check.
            if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
                Ok(ArtifactCleanupOutcome::Missing)
            } else {
                Err(error)
            }
        }
    }
}

async fn artifact_exists(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<bool> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let root = tokio::fs::canonicalize(logical_root).await?;
            let artifact_path = logical_root.join(artifact_key);
            let resolved_artifact = match tokio::fs::canonicalize(&artifact_path).await {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error.into()),
            };
            if !resolved_artifact.starts_with(&root) {
                return Err(anyhow::anyhow!(
                    "backup artifact resolves outside managed storage: {}",
                    artifact_path.display()
                ));
            }
            Ok(true)
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root:ro"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "test".to_string(),
                            "-e".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            match out.status {
                0 => Ok(true),
                1 => Ok(false),
                status => Err(anyhow::anyhow!(
                    "backup artifact existence check failed: status={} stderr={}",
                    status,
                    out.stderr.trim()
                )),
            }
        }
    }
}

async fn delete_artifact(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<()> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let root = tokio::fs::canonicalize(logical_root).await?;
            let artifact_path = logical_root.join(artifact_key);
            let resolved_artifact = tokio::fs::canonicalize(&artifact_path).await?;
            if !resolved_artifact.starts_with(&root) {
                return Err(anyhow::anyhow!(
                    "backup artifact resolves outside managed storage: {}",
                    artifact_path.display()
                ));
            }
            tokio::fs::remove_file(artifact_path).await?;
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "rm".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            if out.status != 0 {
                return Err(anyhow::anyhow!(
                    "backup artifact delete failed: {}",
                    out.stderr
                ));
            }
        }
    }
    Ok(())
}

async fn delete_artifact_if_present(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<()> {
    if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
        return Ok(());
    }
    delete_artifact(runner, storage, helper_image_fallback, artifact_key).await
}

#[cfg(test)]
#[path = "backup/tests.rs"]
mod tests;
