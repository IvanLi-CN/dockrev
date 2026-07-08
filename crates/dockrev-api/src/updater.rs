use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;
use ulid::Ulid;

use crate::{
    api::types::{JobProgressDownload, JobScope, StackRecord, UpdateServiceTarget},
    compose_runner::{ComposeRunnerConfig, ComposeStack},
    docker_runner,
    runner::{CommandRunner, CommandSpec},
};

mod planning;
mod pull_progress;

#[allow(unused_imports)]
pub use planning::UpdateServiceSelection;
#[cfg(test)]
use planning::detect_semver_downgrade;
use planning::{
    UpdateSummaryInput, build_update_summary, emit_update_progress, ensure_explicit_tag_ref_pulled,
    failed_summary_with_failure_step, insert_tag_pull_summary_fields, record_unique_tag_ref,
    retry_backoff_delay, should_sync_local_tag, tag_pull_warning_value,
};
pub use planning::{is_dockrev_image_ref, select_update_services};
use pull_progress::{
    PullProgressFractionSource, PullProgressSnapshot, PullProgressTracker,
    parse_pull_fraction_from_line, pull_progress_message, pull_progress_signature,
};

#[derive(Clone, Debug)]
struct TempFileCleanup(std::path::PathBuf);

impl Drop for TempFileCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Clone, Debug)]
struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Debug)]
struct DockerCliAuthBridge {
    docker_config_dir: PathBuf,
    _cleanup: TempDirCleanup,
}

impl DockerCliAuthBridge {
    fn stage(docker_config_path: &Path) -> anyhow::Result<Self> {
        let temp_root = std::env::temp_dir().join(format!("dockrev-auth-config-{}", Ulid::new()));
        let docker_config_dir = temp_root.join(".docker");
        let source_dir = docker_config_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let source_file_name = docker_config_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        std::fs::create_dir_all(&docker_config_dir).with_context(|| {
            format!(
                "create docker auth workspace {}",
                docker_config_dir.display()
            )
        })?;

        if source_file_name == "config.json" {
            copy_selected_docker_config_metadata(source_dir, &docker_config_dir)?;
        }

        let staged_config_path = docker_config_dir.join("config.json");
        std::fs::copy(docker_config_path, &staged_config_path).with_context(|| {
            format!(
                "stage docker config {} -> {}",
                docker_config_path.display(),
                staged_config_path.display()
            )
        })?;

        Ok(Self {
            docker_config_dir,
            _cleanup: TempDirCleanup(temp_root),
        })
    }

    fn env(&self) -> Vec<(String, String)> {
        // Keep compose `${HOME}` interpolation untouched; only point Docker CLI tools at the staged config.
        vec![(
            "DOCKER_CONFIG".to_string(),
            self.docker_config_dir.to_string_lossy().to_string(),
        )]
    }
}

fn copy_selected_docker_config_metadata(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let contexts_src = src.join("contexts");
    if contexts_src.is_dir() {
        copy_dir_recursively(&contexts_src, &dest.join("contexts")).with_context(|| {
            format!(
                "stage docker config contexts {} -> {}",
                contexts_src.display(),
                dest.join("contexts").display()
            )
        })?;
    }
    Ok(())
}

fn copy_dir_recursively(src: &Path, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursively(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub status: String,
    pub summary_json: serde_json::Value,
}

#[derive(Clone, Copy, Debug)]
pub struct IdempotentRetryPolicy {
    pub max_attempts: usize,
    pub base_ms: u64,
    pub max_ms: u64,
}

impl Default for IdempotentRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_ms: 300,
            max_ms: 3000,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrySummary {
    pub attempts: u32,
    pub max_attempts: u32,
    pub base_ms: u64,
    pub max_ms: u64,
}

#[derive(Clone, Debug)]
pub struct UpdateStepFailure {
    pub step: String,
    pub retry: RetrySummary,
    pub last_error: String,
    pub partial_summary: Option<serde_json::Value>,
}

impl UpdateStepFailure {
    fn new(
        step: impl Into<String>,
        retry_policy: IdempotentRetryPolicy,
        attempts: usize,
        last_error: impl Into<String>,
    ) -> Self {
        Self {
            step: step.into(),
            retry: RetrySummary {
                attempts: attempts as u32,
                max_attempts: retry_policy.max_attempts as u32,
                base_ms: retry_policy.base_ms,
                max_ms: retry_policy.max_ms,
            },
            last_error: last_error.into(),
            partial_summary: None,
        }
    }
}

impl std::fmt::Display for UpdateStepFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "update step '{}' failed after {}/{} attempts: {}",
            self.step, self.retry.attempts, self.retry.max_attempts, self.last_error
        )
    }
}

impl std::error::Error for UpdateStepFailure {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateProgressStep {
    ServiceStart,
    PullStart,
    PullProgress,
    PullDone,
    UpStart,
    UpDone,
    HealthStart,
    HealthFailed,
    HealthDone,
    TargetTagPullStart,
    TargetTagPullDone,
    SyncTagStart,
    SyncTagDone,
    PullTagsStart,
    PullTagsDone,
    ServiceDone,
}

#[derive(Clone, Debug)]
pub struct UpdateProgressEvent {
    pub step: UpdateProgressStep,
    pub service_name: String,
    pub service_index: u32,
    pub service_total: u32,
    pub pull_fraction: Option<f64>,
    pub download: Option<JobProgressDownload>,
    pub message: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_update_job(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    idempotent_retry_policy: IdempotentRetryPolicy,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    mode: &str,
    explicit_targets: Option<&[UpdateServiceTarget]>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    dockrev_image_repo: Option<&str>,
    progress_events: Option<UnboundedSender<UpdateProgressEvent>>,
) -> anyhow::Result<UpdateOutcome> {
    let selection = select_update_services(
        stack,
        scope,
        service_id,
        allow_arch_mismatch,
        update_reason,
        dockrev_image_repo,
    );
    let services = selection.services;
    let skipped_version_anomaly = selection.skipped_version_anomaly;

    if mode == "dry-run" {
        let mut summary = serde_json::Map::new();
        summary.insert("mode".to_string(), json!("dry-run"));
        summary.insert("changedServices".to_string(), json!(services.len()));
        insert_tag_pull_summary_fields(&mut summary, &[], &[], &[]);
        summary.insert(
            "skippedVersionAnomaly".to_string(),
            json!(skipped_version_anomaly),
        );
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: serde_json::Value::Object(summary),
        });
    }

    if services.is_empty() {
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: serde_json::Value::Object(build_update_summary(UpdateSummaryInput {
                changed: 0,
                old_images: &serde_json::Map::new(),
                new_images: &serde_json::Map::new(),
                final_images: &serde_json::Map::new(),
                target_tags_pulled: &[],
                pull_tags_pulled: &[],
                pull_tag_warnings: &[],
                rollback_trigger: None,
                skipped_version_anomaly: &skipped_version_anomaly,
            })),
        });
    }

    let explicit_targets_by_service = explicit_targets
        .unwrap_or(&[])
        .iter()
        .map(|target| (target.service_id.clone(), target.clone()))
        .collect::<HashMap<_, _>>();
    if !explicit_targets_by_service.is_empty() {
        let missing_target_service_ids = services
            .iter()
            .filter(|svc| !explicit_targets_by_service.contains_key(svc.id.as_str()))
            .map(|svc| svc.id.clone())
            .collect::<Vec<_>>();
        if !missing_target_service_ids.is_empty() {
            return Err(anyhow::anyhow!(
                "explicit update targets no longer cover selected services: {}",
                missing_target_service_ids.join(", ")
            ));
        }
    }

    let auth_bridge = docker_config_path
        .map(DockerCliAuthBridge::stage)
        .transpose()?;
    let command_env = auth_bridge
        .as_ref()
        .map(DockerCliAuthBridge::env)
        .unwrap_or_default();
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
        env: command_env.clone(),
    };
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };

    let override_path = build_override_file(stack, &services, &explicit_targets_by_service)?;
    let _override_cleanup = override_path.as_ref().map(|p| TempFileCleanup(p.clone()));
    let override_stack = override_path.as_ref().map(|p| ComposeStack {
        project_name: compose_stack.project_name.clone(),
        compose: {
            let mut c = stack.compose.clone();
            c.compose_files.push(p.to_string_lossy().to_string());
            c
        },
    });

    let docker_cfg = docker_runner::DockerRunnerConfig {
        docker_bin: "docker".to_string(),
        env: command_env,
    };

    let mut changed = 0u32;
    let mut old_images = serde_json::Map::new();
    let mut new_images = serde_json::Map::new();
    let mut final_images = serde_json::Map::new();
    let mut target_tags_pulled: Vec<String> = Vec::new();
    let mut target_tags_seen: HashSet<String> = HashSet::new();
    let mut pull_tags_pulled: Vec<String> = Vec::new();
    let mut pull_tags_seen: HashSet<String> = HashSet::new();
    let mut pull_tag_warnings: Vec<serde_json::Value> = Vec::new();
    let mut successful_tag_refs: HashSet<String> = HashSet::new();

    let compose_for_update = override_stack.as_ref().unwrap_or(&compose_stack);

    let service_total = services.len() as u32;
    let mut prepared_services = Vec::new();

    for (service_index, svc) in services.into_iter().enumerate() {
        let service_index = service_index as u32;

        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::ServiceStart,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("starting service {}", svc.name),
            },
        );

        let pre_update_container_id = run_to_string(
            runner,
            compose_for_update.ps_q_service(&compose_cfg, &svc.name),
            Duration::from_secs(30),
        )
        .await?;
        let pre_update_container_id = pre_update_container_id.trim().to_string();
        if pre_update_container_id.is_empty() {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::ServiceDone,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    download: None,
                    message: format!("skipped service {} (container not running)", svc.name),
                },
            );
            continue;
        }

        let old_image_id = run_to_string_with_retry(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &pre_update_container_id),
            Duration::from_secs(10),
            "inspect_image_id",
            idempotent_retry_policy,
        )
        .await?;
        let old_image_id = old_image_id.trim().to_string();
        old_images.insert(svc.id.clone(), json!(&old_image_id));

        prepared_services.push((
            svc,
            service_index,
            old_image_id,
            should_sync_local_tag(&svc.image.reference),
        ));
    }

    if prepared_services.is_empty() {
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: serde_json::Value::Object(build_update_summary(UpdateSummaryInput {
                changed,
                old_images: &old_images,
                new_images: &new_images,
                final_images: &final_images,
                target_tags_pulled: &target_tags_pulled,
                pull_tags_pulled: &pull_tags_pulled,
                pull_tag_warnings: &pull_tag_warnings,
                rollback_trigger: None,
                skipped_version_anomaly: &skipped_version_anomaly,
            })),
        });
    }

    let prepared_service_names = prepared_services
        .iter()
        .map(|(svc, _, _, _)| svc.name.clone())
        .collect::<Vec<_>>();

    for (svc, service_index, _, _) in &prepared_services {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullStart,
                service_name: svc.name.clone(),
                service_index: *service_index,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("pulling image for {}", svc.name),
            },
        );
    }

    if let Some(progress_events) = progress_events.as_ref() {
        run_checked_with_pull_progress(
            runner,
            compose_for_update.pull_services_with_progress(&compose_cfg, &prepared_service_names),
            Duration::from_secs(300),
            "pull_services",
            idempotent_retry_policy,
            |snapshot| {
                for (svc, service_index, _, _) in &prepared_services {
                    emit_update_progress(
                        Some(progress_events),
                        UpdateProgressEvent {
                            step: UpdateProgressStep::PullProgress,
                            service_name: svc.name.clone(),
                            service_index: *service_index,
                            service_total,
                            pull_fraction: snapshot.fraction,
                            download: snapshot.download.clone(),
                            message: pull_progress_message(&svc.name, &snapshot),
                        },
                    );
                }
            },
        )
        .await?;
    } else {
        run_checked_with_retry(
            runner,
            compose_for_update.pull_services_with_progress(&compose_cfg, &prepared_service_names),
            Duration::from_secs(300),
            "pull_services",
            idempotent_retry_policy,
        )
        .await?;
    }

    for (svc, service_index, _, _) in &prepared_services {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullDone,
                service_name: svc.name.clone(),
                service_index: *service_index,
                service_total,
                pull_fraction: Some(1.0),
                download: None,
                message: format!("pull completed for {}", svc.name),
            },
        );
    }

    for (svc, service_index, _, _) in &prepared_services {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::UpStart,
                service_name: svc.name.clone(),
                service_index: *service_index,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("recreating service {}", svc.name),
            },
        );
    }

    run_checked(
        runner,
        compose_for_update.up_services(&compose_cfg, &prepared_service_names),
        Duration::from_secs(300),
    )
    .await?;

    for (svc, service_index, _, _) in &prepared_services {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::UpDone,
                service_name: svc.name.clone(),
                service_index: *service_index,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("service {} updated", svc.name),
            },
        );
    }

    let mut rollback_trigger: Option<&str> = None;
    let mut rolled_back_any = false;

    for (svc, service_index, old_image_id, sync_local_tag) in prepared_services {
        let target = explicit_targets_by_service.get(svc.id.as_str());

        let post_update_container_id = run_to_string(
            runner,
            compose_for_update.ps_q_service(&compose_cfg, &svc.name),
            Duration::from_secs(30),
        )
        .await?;
        let post_update_container_id = post_update_container_id.trim().to_string();
        if post_update_container_id.is_empty() {
            return Err(anyhow::anyhow!("container_missing_after_update"));
        }

        let mut active_container_id = post_update_container_id;
        let has_health = has_healthcheck(
            runner,
            &docker_cfg,
            &active_container_id,
            idempotent_retry_policy,
        )
        .await?;
        let attempted_image_id = run_to_string_with_retry(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
            Duration::from_secs(10),
            "inspect_image_id",
            idempotent_retry_policy,
        )
        .await?;
        let attempted_image_id = attempted_image_id.trim().to_string();
        let mut final_image_id = attempted_image_id.clone();
        let mut rolled_back = false;
        let mut rollback_failure_step: Option<&str> = None;

        if has_health {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::HealthStart,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    download: None,
                    message: format!("waiting for healthcheck on {}", svc.name),
                },
            );
            let healthy = wait_healthy(
                runner,
                &docker_cfg,
                &active_container_id,
                Duration::from_secs(90),
                idempotent_retry_policy,
            )
            .await?;
            if !healthy {
                rollback_failure_step = Some("healthcheck");
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::HealthFailed,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("healthcheck failed for {}; rolling back", svc.name),
                    },
                );
                match rollback_service_after_failed_update(
                    runner,
                    &compose_cfg,
                    &compose_stack,
                    &docker_cfg,
                    &svc.name,
                    &svc.image.reference,
                    &old_image_id,
                    sync_local_tag,
                    has_health,
                    idempotent_retry_policy,
                )
                .await
                {
                    Ok(rollback_container_id) => {
                        active_container_id = rollback_container_id;
                        rolled_back = true;
                        let rollback_image_id = run_to_string_with_retry(
                            runner,
                            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
                            Duration::from_secs(10),
                            "inspect_image_id",
                            idempotent_retry_policy,
                        )
                        .await?;
                        final_image_id = rollback_image_id.trim().to_string();
                    }
                    Err(err) => {
                        return Ok(UpdateOutcome {
                            status: "failed".to_string(),
                            summary_json: failed_summary_with_failure_step(
                                err.to_string().as_str(),
                                Some("healthcheck"),
                                &skipped_version_anomaly,
                            ),
                        });
                    }
                }
            }
            if !rolled_back {
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::HealthDone,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("healthcheck passed for {}", svc.name),
                    },
                );
            }
        }

        if !rolled_back
            && let Some(target) = target
            && !target.skip_tag_followups
        {
            let repo = strip_tag_and_digest(&svc.image.reference)
                .unwrap_or_else(|| svc.image.reference.clone());
            let target_tag_ref = format!("{repo}:{}", target.target_tag.trim());
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::TargetTagPullStart,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    download: None,
                    message: format!("pulling target tag for {}", svc.name),
                },
            );
            if let Err(_pull_err) = ensure_explicit_tag_ref_pulled(
                runner,
                &docker_cfg,
                idempotent_retry_policy,
                "pull_target_tag",
                &target_tag_ref,
                &mut successful_tag_refs,
            )
            .await
            {
                match rollback_service_after_failed_update(
                    runner,
                    &compose_cfg,
                    &compose_stack,
                    &docker_cfg,
                    &svc.name,
                    &svc.image.reference,
                    &old_image_id,
                    sync_local_tag,
                    has_health,
                    idempotent_retry_policy,
                )
                .await
                {
                    Ok(rollback_container_id) => {
                        active_container_id = rollback_container_id;
                        rolled_back = true;
                        rollback_failure_step = Some("pull_target_tag");
                        let rollback_image_id = run_to_string_with_retry(
                            runner,
                            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
                            Duration::from_secs(10),
                            "inspect_image_id",
                            idempotent_retry_policy,
                        )
                        .await?;
                        final_image_id = rollback_image_id.trim().to_string();
                    }
                    Err(err) => {
                        return Ok(UpdateOutcome {
                            status: "failed".to_string(),
                            summary_json: failed_summary_with_failure_step(
                                err.to_string().as_str(),
                                Some("pull_target_tag"),
                                &skipped_version_anomaly,
                            ),
                        });
                    }
                }
            } else {
                record_unique_tag_ref(
                    target_tag_ref.clone(),
                    &mut target_tags_pulled,
                    &mut target_tags_seen,
                );
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::TargetTagPullDone,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("target tag pulled for {}", svc.name),
                    },
                );
            }
        }

        if !rolled_back && sync_local_tag && !target.is_some_and(|target| target.skip_tag_followups)
        {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::SyncTagStart,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    download: None,
                    message: format!("syncing compose tag for {}", svc.name),
                },
            );
            if let Err(_sync_err) = run_checked_with_retry(
                runner,
                docker_runner::tag_image(&docker_cfg, &attempted_image_id, &svc.image.reference),
                Duration::from_secs(30),
                "sync_configured_tag",
                idempotent_retry_policy,
            )
            .await
            {
                match rollback_service_after_failed_update(
                    runner,
                    &compose_cfg,
                    &compose_stack,
                    &docker_cfg,
                    &svc.name,
                    &svc.image.reference,
                    &old_image_id,
                    sync_local_tag,
                    has_health,
                    idempotent_retry_policy,
                )
                .await
                {
                    Ok(rollback_container_id) => {
                        active_container_id = rollback_container_id;
                        rolled_back = true;
                        rollback_failure_step = Some("sync_configured_tag");
                        let rollback_image_id = run_to_string_with_retry(
                            runner,
                            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
                            Duration::from_secs(10),
                            "inspect_image_id",
                            idempotent_retry_policy,
                        )
                        .await?;
                        final_image_id = rollback_image_id.trim().to_string();
                    }
                    Err(err) => {
                        return Ok(UpdateOutcome {
                            status: "failed".to_string(),
                            summary_json: failed_summary_with_failure_step(
                                err.to_string().as_str(),
                                Some("sync_configured_tag"),
                                &skipped_version_anomaly,
                            ),
                        });
                    }
                }
            } else {
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::SyncTagDone,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("compose tag synced for {}", svc.name),
                    },
                );
            }
        }

        if !rolled_back
            && let Some(target) = target
            && !target.skip_tag_followups
        {
            let repo = strip_tag_and_digest(&svc.image.reference)
                .unwrap_or_else(|| svc.image.reference.clone());
            let pull_tag_refs = target
                .pull_tags
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|tag| format!("{repo}:{tag}"))
                .collect::<Vec<_>>();
            if !pull_tag_refs.is_empty() {
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::PullTagsStart,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("pulling compatibility tags for {}", svc.name),
                    },
                );
                for tag_ref in pull_tag_refs {
                    if pull_tags_seen.contains(&tag_ref) {
                        continue;
                    }
                    match ensure_explicit_tag_ref_pulled(
                        runner,
                        &docker_cfg,
                        idempotent_retry_policy,
                        "pull_tag",
                        &tag_ref,
                        &mut successful_tag_refs,
                    )
                    .await
                    {
                        Ok(()) => {
                            record_unique_tag_ref(
                                tag_ref,
                                &mut pull_tags_pulled,
                                &mut pull_tags_seen,
                            );
                        }
                        Err(err) => {
                            pull_tags_seen.insert(tag_ref.clone());
                            pull_tag_warnings.push(tag_pull_warning_value(&svc.id, &tag_ref, &err));
                        }
                    }
                }
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::PullTagsDone,
                        service_name: svc.name.clone(),
                        service_index,
                        service_total,
                        pull_fraction: None,
                        download: None,
                        message: format!("compatibility tags settled for {}", svc.name),
                    },
                );
            }
        }

        new_images.insert(svc.id.clone(), json!(&attempted_image_id));
        final_images.insert(svc.id.clone(), json!(&final_image_id));
        changed += 1;

        if rolled_back {
            rolled_back_any = true;
            if rollback_trigger.is_none() {
                rollback_trigger = rollback_failure_step;
            }
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::ServiceDone,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    download: None,
                    message: match rollback_failure_step {
                        Some("healthcheck") => {
                            format!("service {} rolled back after healthcheck failure", svc.name)
                        }
                        Some("pull_target_tag") => {
                            format!(
                                "service {} rolled back after target tag pull failure",
                                svc.name
                            )
                        }
                        Some("sync_configured_tag") => {
                            format!(
                                "service {} rolled back after compose tag sync failure",
                                svc.name
                            )
                        }
                        _ => format!("service {} rolled back", svc.name),
                    },
                },
            );
            continue;
        }

        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::ServiceDone,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("service {} done", svc.name),
            },
        );
    }

    Ok(UpdateOutcome {
        status: if rolled_back_any {
            "rolled_back".to_string()
        } else {
            "success".to_string()
        },
        summary_json: serde_json::Value::Object(build_update_summary(UpdateSummaryInput {
            changed,
            old_images: &old_images,
            new_images: &new_images,
            final_images: &final_images,
            target_tags_pulled: &target_tags_pulled,
            pull_tags_pulled: &pull_tags_pulled,
            pull_tag_warnings: &pull_tag_warnings,
            rollback_trigger,
            skipped_version_anomaly: &skipped_version_anomaly,
        })),
    })
}

fn build_override_file(
    stack: &StackRecord,
    services: &[&crate::api::types::Service],
    explicit_targets: &HashMap<String, UpdateServiceTarget>,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    if services.is_empty() {
        return Ok(None);
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("services:".to_string());

    let mut any = false;
    for svc in services {
        let base = strip_tag_and_digest(&svc.image.reference)
            .unwrap_or_else(|| svc.image.reference.clone());
        let override_image = if explicit_targets.is_empty() {
            let Some(candidate) = svc.candidate.as_ref() else {
                continue;
            };
            format!("{base}@{}", normalize_digest(&candidate.digest))
        } else {
            let target = explicit_targets.get(svc.id.as_str()).ok_or_else(|| {
                anyhow::anyhow!("missing explicit update target for service {}", svc.id)
            })?;
            format!("{base}@{}", normalize_digest(&target.target_digest))
        };

        any = true;
        lines.push(format!("  {}:", svc.name));
        lines.push(format!("    image: {override_image}"));
    }

    if !any {
        return Ok(None);
    }

    let file_name = format!(
        "dockrev-override-{}-{}.yml",
        sanitize_project_name(&stack.name),
        ulid::Ulid::new()
    );
    let path = std::env::temp_dir().join(file_name);
    std::fs::write(&path, lines.join("\n") + "\n")?;
    Ok(Some(path))
}

fn normalize_digest(input: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return t.to_string();
    }
    if t.contains(':') {
        return t.to_string();
    }
    format!("sha256:{t}")
}

fn strip_tag_and_digest(image_ref: &str) -> Option<String> {
    let (without_digest, _) = image_ref.split_once('@').unwrap_or((image_ref, ""));
    let Some((left, right)) = without_digest.rsplit_once(':') else {
        return Some(without_digest.to_string());
    };
    if right.is_empty() || right.contains('/') || left.is_empty() {
        return Some(without_digest.to_string());
    }
    Some(left.to_string())
}

async fn run_checked_with_pull_progress<F>(
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
        let mut tracker = PullProgressTracker::default();
        let mut last_status_emit = std::time::Instant::now()
            .checked_sub(Duration::from_secs(5))
            .unwrap_or_else(std::time::Instant::now);
        let mut on_stdout = |_chunk: String| {};
        let mut on_stderr = |chunk: String| {
            for line in chunk.lines() {
                let snapshot = tracker.observe_line(line).or_else(|| {
                    parse_pull_fraction_from_line(line).map(|fraction| PullProgressSnapshot {
                        fraction: Some(fraction.clamp(0.0, 1.0)),
                        fraction_source: Some(PullProgressFractionSource::Bytes),
                        download: None,
                    })
                });
                let Some(mut snapshot) = snapshot else {
                    continue;
                };
                if let Some(fraction) = snapshot.fraction {
                    snapshot.fraction = Some(fraction.clamp(0.0, 0.99));
                }
                let fraction_changed = snapshot
                    .fraction
                    .is_some_and(|fraction| fraction > last_fraction + 0.01);
                let signature = pull_progress_signature(&snapshot);
                let status_changed = signature != last_signature
                    && last_status_emit.elapsed() >= Duration::from_millis(600);
                if fraction_changed || status_changed {
                    if let Some(fraction) = snapshot.fraction {
                        last_fraction = fraction;
                    }
                    last_signature = signature;
                    last_status_emit = std::time::Instant::now();
                    on_progress(snapshot);
                }
            }
        };

        let out = match runner
            .run_stream(spec.clone(), timeout, &mut on_stdout, &mut on_stderr)
            .await
        {
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
            "command failed: status={} stderr={}",
            out.status, out.stderr
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

#[allow(clippy::too_many_arguments)]
async fn rollback_service_after_failed_update(
    runner: &dyn CommandRunner,
    compose_cfg: &ComposeRunnerConfig,
    compose_stack: &ComposeStack,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    service_name: &str,
    configured_image_ref: &str,
    old_image_id: &str,
    sync_local_tag: bool,
    has_health: bool,
    idempotent_retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<String> {
    if sync_local_tag {
        run_checked_with_retry(
            runner,
            docker_runner::tag_image(docker_cfg, old_image_id, configured_image_ref),
            Duration::from_secs(30),
            "tag_image",
            idempotent_retry_policy,
        )
        .await?;
    }

    run_checked(
        runner,
        compose_stack.up_service_no_pull(compose_cfg, service_name),
        Duration::from_secs(300),
    )
    .await?;

    let rollback_container_id = run_to_string(
        runner,
        compose_stack.ps_q_service(compose_cfg, service_name),
        Duration::from_secs(30),
    )
    .await?;
    let rollback_container_id = rollback_container_id.trim().to_string();
    if rollback_container_id.is_empty() {
        return Err(anyhow::anyhow!("container_missing_after_rollback"));
    }

    if has_health {
        let ok = wait_healthy(
            runner,
            docker_cfg,
            &rollback_container_id,
            Duration::from_secs(90),
            idempotent_retry_policy,
        )
        .await?;
        if !ok {
            return Err(anyhow::anyhow!("rollback_failed"));
        }
    }

    Ok(rollback_container_id)
}

async fn has_healthcheck(
    runner: &dyn CommandRunner,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    container_id: &str,
    retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<bool> {
    let out = run_to_string_with_retry(
        runner,
        docker_runner::inspect_has_healthcheck(docker_cfg, container_id),
        Duration::from_secs(10),
        "inspect_has_healthcheck",
        retry_policy,
    )
    .await?;
    Ok(out.trim() == "1")
}

async fn wait_healthy(
    runner: &dyn CommandRunner,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    container_id: &str,
    timeout: Duration,
    idempotent_retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = run_to_string_with_retry(
            runner,
            docker_runner::inspect_health_status(docker_cfg, container_id),
            Duration::from_secs(10),
            "inspect_health_status",
            idempotent_retry_policy,
        )
        .await?;

        match status.trim() {
            "healthy" => return Ok(true),
            "unhealthy" => return Ok(false),
            _ => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_checked(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
) -> anyhow::Result<()> {
    let out = runner.run(spec, timeout).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "command failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

async fn run_checked_with_retry(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
    step: &str,
    retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<()> {
    for attempt in 1..=retry_policy.max_attempts {
        let out = match runner.run(spec.clone(), timeout).await {
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
            "command failed: status={} stderr={}",
            out.status, out.stderr
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

async fn run_to_string_with_retry(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
    step: &str,
    retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<String> {
    for attempt in 1..=retry_policy.max_attempts {
        let out = match runner.run(spec.clone(), timeout).await {
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
            return Ok(out.stdout);
        }
        let failure_message = format!(
            "command failed: status={} stderr={}",
            out.status, out.stderr
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

fn is_registry_rate_limit_failure_text(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("pull rate limit")
        || lower.contains("toomanyrequests")
        || lower.contains("too many requests")
        || lower.contains("rate limit")
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

#[cfg(test)]
mod tests;
