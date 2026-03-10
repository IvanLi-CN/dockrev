use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use semver::Version;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;
use ulid::Ulid;

use crate::{
    api::types::{JobScope, StackRecord},
    compose_runner::{ComposeRunnerConfig, ComposeStack},
    docker_runner,
    runner::{CommandRunner, CommandSpec},
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

    fn with_partial_summary(mut self, partial_summary: serde_json::Value) -> Self {
        self.partial_summary = Some(partial_summary);
        self
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
    HealthDone,
    SyncTagStart,
    SyncTagDone,
    ServiceDone,
}

#[derive(Clone, Debug)]
pub struct UpdateProgressEvent {
    pub step: UpdateProgressStep,
    pub service_name: String,
    pub service_index: u32,
    pub service_total: u32,
    pub pull_fraction: Option<f64>,
    pub message: String,
}

fn emit_update_progress(
    progress_events: Option<&UnboundedSender<UpdateProgressEvent>>,
    event: UpdateProgressEvent,
) {
    if let Some(tx) = progress_events {
        let _ = tx.send(event);
    }
}

fn retry_backoff_delay(retry_policy: IdempotentRetryPolicy, attempt: usize) -> Duration {
    let exponent = (attempt.saturating_sub(1)).min(16);
    let scale = 1u64 << exponent;
    let base = retry_policy
        .base_ms
        .saturating_mul(scale)
        .min(retry_policy.max_ms);
    let jitter_span = (base / 3).max(1);
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter = now_nanos % (jitter_span + 1);
    Duration::from_millis(base.saturating_add(jitter).min(retry_policy.max_ms))
}

fn parse_strict_semver_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(normalized).ok()
}

fn semver_baseline_for_current(svc: &crate::api::types::Service) -> Option<Version> {
    svc.image
        .resolved_tag
        .as_deref()
        .and_then(parse_strict_semver_tag)
        .or_else(|| parse_strict_semver_tag(&svc.image.tag))
}

fn semver_baseline_for_candidate(svc: &crate::api::types::Service) -> Option<Version> {
    let candidate = svc.candidate.as_ref()?;
    candidate
        .resolved_tag
        .as_deref()
        .and_then(parse_strict_semver_tag)
        .or_else(|| parse_strict_semver_tag(&candidate.tag))
}

fn detect_semver_downgrade(svc: &crate::api::types::Service) -> Option<(String, String)> {
    let current = semver_baseline_for_current(svc)?;
    let candidate = semver_baseline_for_candidate(svc)?;
    if candidate < current {
        return Some((current.to_string(), candidate.to_string()));
    }
    None
}

pub fn is_dockrev_image_ref(image_ref: &str, dockrev_image_repo: Option<&str>) -> bool {
    let Some(repo) = dockrev_image_repo
        .map(str::trim)
        .filter(|repo| !repo.is_empty())
    else {
        return false;
    };
    image_ref == repo
        || image_ref.starts_with(&format!("{repo}:"))
        || image_ref.starts_with(&format!("{repo}@"))
}

#[derive(Clone, Debug, Default)]
pub struct UpdateServiceSelection<'a> {
    pub services: Vec<&'a crate::api::types::Service>,
    pub skipped_version_anomaly: Vec<serde_json::Value>,
}

pub fn select_update_services<'a>(
    stack: &'a StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    dockrev_image_repo: Option<&str>,
) -> UpdateServiceSelection<'a> {
    let mut services = match scope {
        JobScope::All => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Stack => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Service => stack
            .services
            .iter()
            .filter(|s| service_id.is_some_and(|id| id == s.id))
            .collect::<Vec<_>>(),
    };

    // For stack/all updates, only apply to actionable candidates (UI shows others as skipped).
    if !matches!(scope, JobScope::Service) {
        services.retain(|svc| {
            if svc.archived.unwrap_or(false) {
                return false;
            }
            if svc.ignore.as_ref().is_some_and(|i| i.matched) {
                return false;
            }
            let Some(candidate) = svc.candidate.as_ref() else {
                return false;
            };
            if !allow_arch_mismatch
                && matches!(candidate.arch_match, crate::api::types::ArchMatch::Mismatch)
            {
                return false;
            }
            if is_dockrev_image_ref(&svc.image.reference, dockrev_image_repo) {
                return false;
            }
            true
        });
    }

    let mut skipped_version_anomaly: Vec<serde_json::Value> = Vec::new();
    if !update_reason.eq_ignore_ascii_case("ui") {
        services.retain(|svc| {
            if let Some((current_semver, candidate_semver)) = detect_semver_downgrade(svc) {
                skipped_version_anomaly.push(json!({
                    "serviceId": svc.id,
                    "serviceName": svc.name,
                    "current": current_semver,
                    "candidate": candidate_semver,
                    "reason": "semver_downgrade",
                }));
                return false;
            }
            true
        });
    }

    UpdateServiceSelection {
        services,
        skipped_version_anomaly,
    }
}

fn failed_summary_with_skipped_anomaly(
    reason: &str,
    skipped_version_anomaly: &[serde_json::Value],
) -> serde_json::Value {
    failed_summary_with_failure_step(reason, None, skipped_version_anomaly)
}

fn failed_summary_with_failure_step(
    reason: &str,
    failure_step: Option<&str>,
    skipped_version_anomaly: &[serde_json::Value],
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("reason".to_string(), json!(reason));
    if let Some(step) = failure_step {
        obj.insert("failureStep".to_string(), json!(step));
    }
    obj.insert(
        "skippedVersionAnomaly".to_string(),
        serde_json::Value::Array(skipped_version_anomaly.to_vec()),
    );
    serde_json::Value::Object(obj)
}

fn should_sync_local_tag(image_ref: &str) -> bool {
    let trimmed = image_ref.trim();
    !trimmed.is_empty() && !trimmed.contains('@')
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
    target_tag: Option<&str>,
    target_digest: Option<&str>,
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
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: json!({
                "mode": "dry-run",
                "changedServices": services.len(),
                "skippedVersionAnomaly": skipped_version_anomaly,
            }),
        });
    }

    if services.is_empty() {
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: json!({
                "changedServices": 0,
                "oldDigests": serde_json::Map::<String, serde_json::Value>::new(),
                "newDigests": serde_json::Map::<String, serde_json::Value>::new(),
                "semverPulled": Vec::<String>::new(),
                "semverPullWarnings": serde_json::Map::<String, serde_json::Value>::new(),
                "skippedVersionAnomaly": skipped_version_anomaly,
            }),
        });
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

    let override_path = build_override_file(stack, &services, target_tag, target_digest)?;
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
    let mut semver_pulled: Vec<String> = Vec::new();
    let mut semver_pulled_set: HashSet<String> = HashSet::new();
    let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
        serde_json::Map::new();

    let compose_for_update = override_stack.as_ref().unwrap_or(&compose_stack);

    let service_total = services.len() as u32;
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
                    message: format!("skipped service {} (container not running)", svc.name),
                },
            );
            continue;
        }

        let sync_local_tag = should_sync_local_tag(&svc.image.reference);

        let old_image_id = run_to_string_with_retry(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &pre_update_container_id),
            Duration::from_secs(10),
            "inspect_image_id",
            idempotent_retry_policy,
        )
        .await?;
        let old_image_id = old_image_id.trim().to_string();
        old_images.insert(svc.id.clone(), json!(old_image_id));

        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullStart,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                message: format!("pulling image for {}", svc.name),
            },
        );
        if let Some(progress_events) = progress_events.as_ref() {
            run_checked_with_pull_progress(
                runner,
                compose_for_update.pull_service_with_progress(&compose_cfg, &svc.name),
                Duration::from_secs(300),
                "pull_service",
                idempotent_retry_policy,
                |fraction| {
                    emit_update_progress(
                        Some(progress_events),
                        UpdateProgressEvent {
                            step: UpdateProgressStep::PullProgress,
                            service_name: svc.name.clone(),
                            service_index,
                            service_total,
                            pull_fraction: Some(fraction),
                            message: format!(
                                "pulling image for {} ({:.0}%)",
                                svc.name,
                                fraction * 100.0
                            ),
                        },
                    );
                },
            )
            .await?;
        } else {
            run_checked_with_retry(
                runner,
                compose_for_update.pull_service_with_progress(&compose_cfg, &svc.name),
                Duration::from_secs(300),
                "pull_service",
                idempotent_retry_policy,
            )
            .await?;
        }
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullDone,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: Some(1.0),
                message: format!("pull completed for {}", svc.name),
            },
        );

        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::UpStart,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                message: format!("recreating service {}", svc.name),
            },
        );
        run_checked(
            runner,
            compose_for_update.up_service(&compose_cfg, &svc.name),
            Duration::from_secs(300),
        )
        .await?;
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::UpDone,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                message: format!("service {} updated", svc.name),
            },
        );

        // `up -d` may recreate the container, so refresh the container id before any inspect/health checks.
        let post_update_container_id = run_to_string(
            runner,
            compose_for_update.ps_q_service(&compose_cfg, &svc.name),
            Duration::from_secs(30),
        )
        .await?;
        let post_update_container_id = post_update_container_id.trim().to_string();
        if post_update_container_id.is_empty() {
            return Ok(UpdateOutcome {
                status: "failed".to_string(),
                summary_json: failed_summary_with_skipped_anomaly(
                    "container_missing_after_update",
                    &skipped_version_anomaly,
                ),
            });
        }

        let has_health = run_to_string_with_retry(
            runner,
            docker_runner::inspect_has_healthcheck(&docker_cfg, &post_update_container_id),
            Duration::from_secs(10),
            "inspect_has_healthcheck",
            idempotent_retry_policy,
        )
        .await?;

        let has_health = has_health.trim() == "1";
        let mut rolled_back = false;
        let mut rollback_failure_step: Option<&'static str> = None;
        let mut active_container_id = post_update_container_id;
        if has_health {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::HealthStart,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    message: format!("waiting healthcheck for {}", svc.name),
                },
            );
            let ok = wait_healthy(
                runner,
                &docker_cfg,
                &active_container_id,
                Duration::from_secs(90),
                idempotent_retry_policy,
            )
            .await?;
            if !ok {
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
                    }
                    Err(err) => {
                        return Ok(UpdateOutcome {
                            status: "failed".to_string(),
                            summary_json: failed_summary_with_skipped_anomaly(
                                err.to_string().as_str(),
                                &skipped_version_anomaly,
                            ),
                        });
                    }
                }
            }
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::HealthDone,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    message: format!("healthcheck passed for {}", svc.name),
                },
            );
        }

        let mut new_image_id = run_to_string_with_retry(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
            Duration::from_secs(10),
            "inspect_image_id",
            idempotent_retry_policy,
        )
        .await?;
        new_image_id = new_image_id.trim().to_string();

        if !rolled_back && sync_local_tag {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::SyncTagStart,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    message: format!("syncing compose tag for {}", svc.name),
                },
            );
            if let Err(_sync_err) = run_checked_with_retry(
                runner,
                docker_runner::tag_image(&docker_cfg, &new_image_id, &svc.image.reference),
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
                        let final_image_id = run_to_string_with_retry(
                            runner,
                            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
                            Duration::from_secs(10),
                            "inspect_image_id",
                            idempotent_retry_policy,
                        )
                        .await?;
                        new_image_id = final_image_id.trim().to_string();
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
                        message: format!("compose tag synced for {}", svc.name),
                    },
                );
            }
        }

        new_images.insert(svc.id.clone(), json!(&new_image_id));
        changed += 1;

        if rolled_back {
            emit_update_progress(
                progress_events.as_ref(),
                UpdateProgressEvent {
                    step: UpdateProgressStep::ServiceDone,
                    service_name: svc.name.clone(),
                    service_index,
                    service_total,
                    pull_fraction: None,
                    message: format!("service {} rolled back", svc.name),
                },
            );
            let mut summary = serde_json::Map::new();
            summary.insert("changedServices".to_string(), json!(changed));
            summary.insert(
                "oldDigests".to_string(),
                serde_json::Value::Object(old_images),
            );
            summary.insert(
                "newDigests".to_string(),
                serde_json::Value::Object(new_images),
            );
            summary.insert("semverPulled".to_string(), json!(semver_pulled));
            summary.insert(
                "semverPullWarnings".to_string(),
                serde_json::Value::Object(semver_pull_warnings),
            );
            summary.insert(
                "skippedVersionAnomaly".to_string(),
                json!(skipped_version_anomaly),
            );
            if let Some(step) = rollback_failure_step {
                summary.insert("failureStep".to_string(), json!(step));
            }
            return Ok(UpdateOutcome {
                status: "rolled_back".to_string(),
                summary_json: serde_json::Value::Object(summary),
            });
        }

        if !matches!(scope, JobScope::Service) {
            let repo = strip_tag_and_digest(&svc.image.reference)
                .unwrap_or_else(|| svc.image.reference.clone());
            maybe_pull_semver_tag_for_image(
                runner,
                &docker_cfg,
                idempotent_retry_policy,
                &svc.id,
                &repo,
                &new_image_id,
                &mut semver_pulled,
                &mut semver_pulled_set,
                &mut semver_pull_warnings,
            )
            .await
            .map_err(|err| {
                let partial_summary = json!({
                    "changedServices": changed,
                    "oldDigests": old_images.clone(),
                    "newDigests": new_images.clone(),
                    "semverPulled": semver_pulled.clone(),
                    "semverPullWarnings": semver_pull_warnings.clone(),
                    "skippedVersionAnomaly": skipped_version_anomaly.clone(),
                });
                match err.downcast::<UpdateStepFailure>() {
                    Ok(step_failure) => {
                        anyhow::Error::new(step_failure.with_partial_summary(partial_summary))
                    }
                    Err(err) => err,
                }
            })?;
        }

        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::ServiceDone,
                service_name: svc.name.clone(),
                service_index,
                service_total,
                pull_fraction: None,
                message: format!("service {} done", svc.name),
            },
        );
    }

    Ok(UpdateOutcome {
        status: "success".to_string(),
        summary_json: json!({
            "changedServices": changed,
            "oldDigests": old_images,
            "newDigests": new_images,
            "semverPulled": semver_pulled,
            "semverPullWarnings": semver_pull_warnings,
            "skippedVersionAnomaly": skipped_version_anomaly,
        }),
    })
}

fn build_override_file(
    stack: &StackRecord,
    services: &[&crate::api::types::Service],
    target_tag: Option<&str>,
    target_digest: Option<&str>,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    if services.is_empty() {
        return Ok(None);
    }

    let has_explicit_target = target_tag.is_some() || target_digest.is_some();

    let mut lines: Vec<String> = Vec::new();
    lines.push("services:".to_string());

    let mut any = false;
    for svc in services {
        let override_image = if has_explicit_target {
            let base = strip_tag_and_digest(&svc.image.reference)
                .unwrap_or_else(|| svc.image.reference.clone());
            if let Some(d) = target_digest {
                format!("{base}@{}", normalize_digest(d))
            } else if let Some(t) = target_tag {
                replace_tag(&svc.image.reference, t).unwrap_or_else(|| svc.image.reference.clone())
            } else {
                svc.image.reference.clone()
            }
        } else if let Some(candidate) = svc.candidate.as_ref() {
            let base = strip_tag_and_digest(&svc.image.reference)
                .unwrap_or_else(|| svc.image.reference.clone());
            format!("{base}@{}", normalize_digest(&candidate.digest))
        } else {
            continue;
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

fn replace_tag(image_ref: &str, tag: &str) -> Option<String> {
    let (without_digest, digest) = image_ref.split_once('@').unwrap_or((image_ref, ""));
    let (left, right) = without_digest.rsplit_once(':')?;
    if right.is_empty() || right.contains('/') || left.is_empty() {
        return None;
    }
    if digest.is_empty() {
        Some(format!("{left}:{tag}"))
    } else {
        Some(format!("{left}:{tag}@{digest}"))
    }
}

fn semver_tag_candidates_from_oci_version(raw: &str) -> Vec<String> {
    let raw_tag = raw.trim();
    if raw_tag.is_empty() || raw_tag == "<no value>" {
        return Vec::new();
    }

    let normalized = raw_tag
        .strip_prefix('v')
        .or_else(|| raw_tag.strip_prefix('V'))
        .unwrap_or(raw_tag);
    let version = match Version::parse(normalized) {
        Ok(version) if version.build.is_empty() => version,
        _ => return Vec::new(),
    };

    let normalized_tag = version.to_string();
    let mut candidates = vec![raw_tag.to_string()];
    if normalized_tag != raw_tag {
        candidates.push(normalized_tag);
    }
    candidates
}

fn record_semver_pull_success(
    tag_ref: String,
    semver_pulled: &mut Vec<String>,
    semver_pulled_set: &mut HashSet<String>,
) {
    if semver_pulled_set.insert(tag_ref.clone()) {
        semver_pulled.push(tag_ref);
    }
}

#[allow(clippy::too_many_arguments)]
async fn maybe_pull_semver_tag_for_image(
    runner: &dyn CommandRunner,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    idempotent_retry_policy: IdempotentRetryPolicy,
    _service_id: &str,
    repo: &str,
    image_id: &str,
    semver_pulled: &mut Vec<String>,
    semver_pulled_set: &mut HashSet<String>,
    _semver_pull_warnings: &mut serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<()> {
    let raw_version = run_to_string_with_retry(
        runner,
        docker_runner::image_inspect_oci_version(docker_cfg, image_id),
        Duration::from_secs(10),
        "semver_inspect_version",
        idempotent_retry_policy,
    )
    .await?;

    let candidate_refs = semver_tag_candidates_from_oci_version(&raw_version)
        .into_iter()
        .map(|tag| format!("{repo}:{tag}"))
        .collect::<Vec<_>>();
    if candidate_refs.is_empty() {
        return Ok(());
    }

    // Skip if the tag already exists locally for this image id.
    let repo_tags = run_to_string_with_retry(
        runner,
        docker_runner::image_inspect_repo_tags(docker_cfg, image_id),
        Duration::from_secs(10),
        "semver_inspect_repo_tags",
        idempotent_retry_policy,
    )
    .await?;
    let parsed_repo_tags = serde_json::from_str::<Option<Vec<String>>>(repo_tags.trim())
        .ok()
        .flatten()
        .unwrap_or_default();

    for tag_ref in &candidate_refs {
        if parsed_repo_tags.iter().any(|t| t == tag_ref) {
            record_semver_pull_success(tag_ref.clone(), semver_pulled, semver_pulled_set);
            return Ok(());
        }
    }

    let mut pull_failures: Vec<String> = Vec::new();
    let mut total_pull_attempts = 0usize;
    for tag_ref in candidate_refs {
        if semver_pulled_set.contains(&tag_ref) {
            return Ok(());
        }

        match run_checked_with_retry(
            runner,
            docker_runner::pull_image(docker_cfg, &tag_ref),
            Duration::from_secs(300),
            "semver_pull",
            idempotent_retry_policy,
        )
        .await
        {
            Ok(()) => {
                record_semver_pull_success(tag_ref, semver_pulled, semver_pulled_set);
                return Ok(());
            }
            Err(err) => match err.downcast::<UpdateStepFailure>() {
                Ok(step_failure) => {
                    total_pull_attempts += step_failure.retry.attempts as usize;
                    pull_failures.push(format!("{tag_ref} => {}", step_failure.last_error));
                }
                Err(err) => {
                    total_pull_attempts += idempotent_retry_policy.max_attempts;
                    pull_failures.push(format!("{tag_ref} => {err}"));
                }
            },
        }
    }

    Err(anyhow::Error::new(UpdateStepFailure {
        step: "semver_pull".to_string(),
        retry: RetrySummary {
            attempts: total_pull_attempts as u32,
            max_attempts: (idempotent_retry_policy.max_attempts * pull_failures.len()) as u32,
            base_ms: idempotent_retry_policy.base_ms,
            max_ms: idempotent_retry_policy.max_ms,
        },
        last_error: format!("all semver candidates failed: {}", pull_failures.join("; ")),
        partial_summary: None,
    }))
}

fn parse_size_to_bytes(input: &str) -> Option<f64> {
    let trimmed = input
        .trim()
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | ','));
    if trimmed.is_empty() {
        return None;
    }

    let mut split_idx = None;
    for (idx, ch) in trimmed.char_indices() {
        if !(ch.is_ascii_digit() || ch == '.') {
            split_idx = Some(idx);
            break;
        }
    }
    let idx = split_idx.unwrap_or(trimmed.len());
    if idx == 0 {
        return None;
    }
    let num = trimmed[..idx].parse::<f64>().ok()?;
    let unit = trimmed[idx..].trim().to_ascii_uppercase();
    let factor = match unit.as_str() {
        "" | "B" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024.0 * 1024.0,
        "G" | "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(num * factor)
}

fn parse_pull_fraction_from_line(line: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    for token in line.split_whitespace() {
        let clean = token
            .trim()
            .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | ','));
        let Some((current, total)) = clean.split_once('/') else {
            continue;
        };
        let Some(current_bytes) = parse_size_to_bytes(current) else {
            continue;
        };
        let Some(total_bytes) = parse_size_to_bytes(total) else {
            continue;
        };
        if total_bytes <= 0.0 {
            continue;
        }
        let ratio = (current_bytes / total_bytes).clamp(0.0, 1.0);
        if best.is_none_or(|v| ratio > v) {
            best = Some(ratio);
        }
    }
    best
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
    F: FnMut(f64) + Send,
{
    let mut last_fraction = 0.0f64;
    for attempt in 1..=retry_policy.max_attempts {
        let mut on_stdout = |_chunk: String| {};
        let mut on_stderr = |chunk: String| {
            if let Some(frac) = parse_pull_fraction_from_line(&chunk) {
                let capped = frac.clamp(0.0, 0.99);
                if capped > last_fraction + 0.01 {
                    last_fraction = capped;
                    on_progress(capped);
                }
            }
        };

        let out = match runner
            .run_stream(spec.clone(), timeout, &mut on_stdout, &mut on_stderr)
            .await
        {
            Ok(out) => out,
            Err(err) => {
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

        if attempt >= retry_policy.max_attempts {
            return Err(anyhow::Error::new(UpdateStepFailure::new(
                step,
                retry_policy,
                attempt,
                format!(
                    "command failed: status={} stderr={}",
                    out.status, out.stderr
                ),
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
        if attempt >= retry_policy.max_attempts {
            return Err(anyhow::Error::new(UpdateStepFailure::new(
                step,
                retry_policy,
                attempt,
                format!(
                    "command failed: status={} stderr={}",
                    out.status, out.stderr
                ),
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
        if attempt >= retry_policy.max_attempts {
            return Err(anyhow::Error::new(UpdateStepFailure::new(
                step,
                retry_policy,
                attempt,
                format!(
                    "command failed: status={} stderr={}",
                    out.status, out.stderr
                ),
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
mod tests {
    use super::*;
    use crate::{
        api::types::{
            ArchMatch, BackupTargetOverrides, Candidate, ComposeRef, Service, ServiceSettings,
            TernaryChoice,
        },
        runner::{CommandOutput, CommandRunner},
    };
    use std::{collections::BTreeMap, fs, sync::Mutex};

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for FakeRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((spec.program, spec.args.clone()));
            Ok(CommandOutput {
                status: 0,
                stdout: "\n".to_string(),
                stderr: String::new(),
            })
        }
    }

    fn args_end_with(args: &[String], suffix: &[&str]) -> bool {
        if args.len() < suffix.len() {
            return false;
        }
        let start = args.len() - suffix.len();
        suffix
            .iter()
            .enumerate()
            .all(|(i, s)| args[start + i] == *s)
    }

    fn single_service_stack(image_reference: &str, candidate: Option<Candidate>) -> StackRecord {
        StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: image_reference.to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate,
                ignore: None,
                version_inference: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
                archived: None,
            }],
        }
    }

    fn write_test_docker_config() -> (PathBuf, TempDirCleanup) {
        let root = std::env::temp_dir().join(format!("dockrev-test-auth-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("docker-config.custom.json");
        fs::write(&path, r#"{"auths":{"ghcr.io":{"auth":"Zm9vOmJhcg=="}}}"#).unwrap();
        (path, TempDirCleanup(root))
    }

    fn write_test_default_named_docker_config() -> (PathBuf, TempDirCleanup) {
        let root =
            std::env::temp_dir().join(format!("dockrev-test-auth-default-{}", ulid::Ulid::new()));
        let contexts_dir = root.join("contexts/meta");
        let buildx_dir = root.join("buildx");
        fs::create_dir_all(&contexts_dir).unwrap();
        fs::create_dir_all(&buildx_dir).unwrap();
        let path = root.join("config.json");
        fs::write(&path, r#"{"auths":{"ghcr.io":{"auth":"Zm9vOmJhcg=="}}}"#).unwrap();
        fs::write(
            contexts_dir.join("state.json"),
            r#"{"currentContext":"desktop-linux"}"#,
        )
        .unwrap();
        fs::write(buildx_dir.join("state.json"), "cache-state").unwrap();
        fs::write(root.join("notes.txt"), "not-for-auth-bridge").unwrap();
        (path, TempDirCleanup(root))
    }

    #[test]
    fn docker_cli_auth_bridge_stages_custom_config_as_config_json() {
        let (source_path, _source_cleanup) = write_test_docker_config();

        let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");
        let staged_path = bridge.docker_config_dir.join("config.json");

        assert_eq!(
            fs::read_to_string(&staged_path).unwrap(),
            fs::read_to_string(&source_path).unwrap()
        );
        assert_eq!(
            bridge.env(),
            vec![(
                "DOCKER_CONFIG".to_string(),
                bridge.docker_config_dir.to_string_lossy().to_string(),
            )]
        );
    }

    #[test]
    fn docker_cli_auth_bridge_copies_context_metadata_for_real_config_json() {
        let (source_path, _source_cleanup) = write_test_default_named_docker_config();

        let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");
        let staged_path = bridge.docker_config_dir.join("config.json");
        let staged_context = bridge.docker_config_dir.join("contexts/meta/state.json");

        assert_eq!(
            fs::read_to_string(&staged_path).unwrap(),
            fs::read_to_string(&source_path).unwrap()
        );
        assert_eq!(
            fs::read_to_string(&staged_context).unwrap(),
            r#"{"currentContext":"desktop-linux"}"#
        );
        assert!(!bridge.docker_config_dir.join("buildx/state.json").exists());
        assert!(!bridge.docker_config_dir.join("notes.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn docker_cli_auth_bridge_handles_read_only_real_config_json() {
        use std::os::unix::fs::PermissionsExt;

        let (source_path, _source_cleanup) = write_test_default_named_docker_config();
        let mut permissions = fs::metadata(&source_path).unwrap().permissions();
        permissions.set_mode(0o444);
        fs::set_permissions(&source_path, permissions).unwrap();

        let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");

        assert_eq!(
            fs::read_to_string(bridge.docker_config_dir.join("config.json")).unwrap(),
            fs::read_to_string(&source_path).unwrap()
        );
    }

    #[derive(Default)]
    struct EnvCaptureUpdateRunner {
        step: Mutex<usize>,
        specs: Mutex<Vec<CommandSpec>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for EnvCaptureUpdateRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.specs.lock().unwrap().push(spec.clone());
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                },
                2 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                3 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                4 => CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                },
                7 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                _ => panic!(
                    "unexpected extra command: program={} args={:?}",
                    spec.program, spec.args
                ),
            };
            *step += 1;
            Ok(out)
        }
    }

    #[derive(Default)]
    struct EnvCaptureSemverRunner {
        specs: Mutex<Vec<CommandSpec>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for EnvCaptureSemverRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.specs.lock().unwrap().push(spec.clone());
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "[]\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:0.7.7"] {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: format!("unexpected args: {:?}", spec.args),
            })
        }
    }

    fn selection_test_service(id: &str, name: &str, image_reference: &str) -> Service {
        Service {
            id: id.to_string(),
            name: name.to_string(),
            image: ComposeRef {
                reference: image_reference.to_string(),
                tag: "0.29.3".to_string(),
                digest: None,
                resolved_tag: None,
                resolved_tags: None,
            },
            candidate: Some(Candidate {
                tag: "latest".to_string(),
                resolved_tag: Some("0.29.5".to_string()),
                digest: "sha256:candidate".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
            ignore: None,
            version_inference: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
            },
            archived: None,
        }
    }

    #[test]
    fn aggregate_selection_excludes_dockrev_but_keeps_supervisor() {
        let stack = StackRecord {
            id: "stk_guard".to_string(),
            name: "dockrev-mod".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![
                selection_test_service(
                    "svc-dockrev",
                    "dockrev",
                    "ghcr.io/ivanli-cn/dockrev:0.29.3",
                ),
                selection_test_service(
                    "svc-supervisor",
                    "dockrev-supervisor",
                    "ghcr.io/ivanli-cn/dockrev-supervisor:0.29.3",
                ),
            ],
        };

        let selection = select_update_services(
            &stack,
            &JobScope::Stack,
            None,
            false,
            "ui",
            Some("ghcr.io/ivanli-cn/dockrev"),
        );
        let ids = selection
            .services
            .iter()
            .map(|svc| svc.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["svc-supervisor"]);
    }

    #[test]
    fn service_scope_still_allows_dockrev_update_selection() {
        let stack = StackRecord {
            id: "stk_guard".to_string(),
            name: "dockrev-mod".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![selection_test_service(
                "svc-dockrev",
                "dockrev",
                "ghcr.io/ivanli-cn/dockrev:0.29.3",
            )],
        };

        let selection = select_update_services(
            &stack,
            &JobScope::Service,
            Some("svc-dockrev"),
            false,
            "ui",
            Some("ghcr.io/ivanli-cn/dockrev"),
        );

        assert_eq!(selection.services.len(), 1);
        assert_eq!(selection.services[0].id, "svc-dockrev");
    }

    #[tokio::test]
    async fn aggregate_dockrev_only_update_becomes_noop() {
        let stack = single_service_stack(
            "ghcr.io/ivanli-cn/dockrev:0.29.3",
            Some(Candidate {
                tag: "latest".to_string(),
                resolved_tag: Some("0.29.5".to_string()),
                digest: "sha256:candidate".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
        );
        let runner = FakeRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "live",
            None,
            None,
            false,
            "ui",
            Some("ghcr.io/ivanli-cn/dockrev"),
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.summary_json["changedServices"].as_u64(), Some(0));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_job_injects_docker_auth_env_into_compose_and_docker_commands() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = EnvCaptureUpdateRunner::default();
        let (docker_config_path, _docker_config_cleanup) = write_test_docker_config();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            Some(docker_config_path.as_path()),
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");

        let specs = runner.specs.lock().unwrap();
        let compose_pull = specs
            .iter()
            .find(|spec| args_end_with(&spec.args, &["pull", "web"]))
            .expect("compose pull command should exist");
        let compose_up = specs
            .iter()
            .find(|spec| args_end_with(&spec.args, &["up", "-d", "web"]))
            .expect("compose up command should exist");
        let docker_tag = specs
            .iter()
            .find(|spec| spec.args == vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"])
            .expect("docker tag command should exist");

        for spec in [compose_pull, compose_up, docker_tag] {
            assert_eq!(spec.env.len(), 1);
            assert!(spec.env.iter().all(|(k, _)| k == "DOCKER_CONFIG"));
            assert!(
                spec.env
                    .iter()
                    .any(|(k, v)| k == "DOCKER_CONFIG" && v.ends_with("/.docker"))
            );
        }
    }

    #[tokio::test]
    async fn noop_update_with_broken_docker_config_path_stays_noop() {
        let stack = single_service_stack("ghcr.io/ivanli-cn/dockrev:1.0", None);
        let runner = FakeRunner::default();
        let missing_path = std::env::temp_dir()
            .join(format!("dockrev-missing-config-{}", ulid::Ulid::new()))
            .join("config.json");

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            Some(missing_path.as_path()),
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "live",
            None,
            None,
            false,
            "ui",
            Some("ghcr.io/ivanli-cn/dockrev"),
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.summary_json["changedServices"].as_u64(), Some(0));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_job_without_docker_config_keeps_command_env_empty() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = EnvCaptureUpdateRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert!(
            runner
                .specs
                .lock()
                .unwrap()
                .iter()
                .all(|spec| spec.env.is_empty())
        );
    }

    #[derive(Default)]
    struct RefreshContainerIdRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for RefreshContainerIdRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                // ps -q (pre-update)
                0 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: "old_container\n".to_string(),
                        stderr: String::new(),
                    }
                }
                // docker inspect image id (pre-update)
                1 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["inspect", "--format", "{{.Image}}", "old_container"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "sha256:old\n".to_string(),
                        stderr: String::new(),
                    }
                }
                // docker-compose pull
                2 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["pull", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                // docker-compose up -d
                3 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["up", "-d", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                // ps -q (post-update; container recreated)
                4 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: "new_container\n".to_string(),
                        stderr: String::new(),
                    }
                }
                // docker inspect has healthcheck (MUST use post-update id)
                5 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec![
                            "inspect",
                            "--format",
                            "{{if .State.Health}}1{{else}}0{{end}}",
                            "new_container"
                        ]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "0\n".to_string(),
                        stderr: String::new(),
                    }
                }
                // docker inspect image id (post-update; MUST use post-update id)
                6 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["inspect", "--format", "{{.Image}}", "new_container"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "sha256:new\n".to_string(),
                        stderr: String::new(),
                    }
                }
                // docker image tag after successful update
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                _ => panic!(
                    "unexpected extra command: program={} args={:?}",
                    spec.program, spec.args
                ),
            };

            *step += 1;
            Ok(out)
        }
    }

    #[tokio::test]
    async fn dry_run_does_not_execute() {
        let stack = StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: "ghcr.io/org/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate: None,
                ignore: None,
                version_inference: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
                archived: None,
            }],
        };

        let runner = FakeRunner::default();
        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "dry-run",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome.status, "success");
        assert_eq!(runner.calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn update_job_refreshes_container_id_after_up() {
        let stack = StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: "ghcr.io/org/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate: None,
                ignore: None,
                version_inference: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
                archived: None,
            }],
        };

        let runner = RefreshContainerIdRunner::default();
        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.summary_json["changedServices"].as_u64().unwrap(), 1);
        assert_eq!(*runner.step.lock().unwrap(), 8);
    }

    #[tokio::test]
    async fn update_job_emits_service_progress_events() {
        let stack = StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: "ghcr.io/org/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate: None,
                ignore: None,
                version_inference: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
                archived: None,
            }],
        };

        let runner = RefreshContainerIdRunner::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UpdateProgressEvent>();
        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            Some(tx),
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        let mut steps = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            steps.push(evt.step);
        }
        assert!(steps.contains(&UpdateProgressStep::ServiceStart));
        assert!(steps.contains(&UpdateProgressStep::PullStart));
        assert!(steps.contains(&UpdateProgressStep::PullDone));
        assert!(steps.contains(&UpdateProgressStep::UpDone));
        assert!(steps.contains(&UpdateProgressStep::SyncTagStart));
        assert!(steps.contains(&UpdateProgressStep::SyncTagDone));
        assert!(steps.contains(&UpdateProgressStep::ServiceDone));
    }

    #[derive(Default)]
    struct SyncTagRollbackRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SyncTagRollbackRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                },
                2 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                3 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                4 => CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                },
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: "cannot sync tag".to_string(),
                    }
                }
                8 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                9 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(
                        &spec.args,
                        &["up", "-d", "--pull", "never", "web"]
                    ));
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                10 => CommandOutput {
                    status: 0,
                    stdout: "rollback_container\n".to_string(),
                    stderr: String::new(),
                },
                11 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "sha256:old\n".to_string(),
                        stderr: String::new(),
                    }
                }
                _ => panic!(
                    "unexpected extra command: program={} args={:?}",
                    spec.program, spec.args
                ),
            };
            *step += 1;
            Ok(out)
        }
    }

    #[derive(Default)]
    struct DigestPinnedRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for DigestPinnedRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                },
                2 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                3 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                4 => CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                },
                _ => panic!(
                    "unexpected extra command: program={} args={:?}",
                    spec.program, spec.args
                ),
            };
            *step += 1;
            Ok(out)
        }
    }

    #[derive(Default)]
    struct ExplicitTargetDigestSyncRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for ExplicitTargetDigestSyncRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                },
                2 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                3 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                4 => CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                },
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                _ => panic!(
                    "unexpected extra command: program={} args={:?}",
                    spec.program, spec.args
                ),
            };
            *step += 1;
            Ok(out)
        }
    }

    #[derive(Default)]
    struct SyncBeforeSemverRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SyncBeforeSemverRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((spec.program.clone(), spec.args.clone()));
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            let out = if spec.program == "docker-compose"
                && args_end_with(&spec.args, &["ps", "-q", "web"])
            {
                CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                }
            } else if spec.program == "docker"
                && args == vec!["inspect", "--format", "{{.Image}}", "new_container"]
            {
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            } else if spec.program == "docker"
                && args
                    == vec![
                        "inspect",
                        "--format",
                        "{{if .State.Health}}1{{else}}0{{end}}",
                        "new_container",
                    ]
            {
                CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                }
            } else if spec.program == "docker"
                && args
                    == vec![
                        "image",
                        "inspect",
                        "--format",
                        r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                        "sha256:new",
                    ]
            {
                CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                }
            } else if spec.program == "docker"
                && args
                    == vec![
                        "image",
                        "inspect",
                        "--format",
                        "{{json .RepoTags}}",
                        "sha256:new",
                    ]
            {
                CommandOutput {
                    status: 0,
                    stdout: "[]\n".to_string(),
                    stderr: String::new(),
                }
            } else {
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            };
            Ok(out)
        }
    }

    #[tokio::test]
    async fn sync_tag_failure_rolls_back_instead_of_reporting_success() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = SyncTagRollbackRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy {
                max_attempts: 1,
                base_ms: 1,
                max_ms: 2,
            },
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "rolled_back");
        assert_eq!(
            outcome.summary_json["newDigests"]["svc_1"],
            json!("sha256:old")
        );
        assert_eq!(
            outcome.summary_json["failureStep"].as_str(),
            Some("sync_configured_tag")
        );
        assert_eq!(*runner.step.lock().unwrap(), 12);
    }

    #[tokio::test]
    async fn explicit_target_digest_still_syncs_tag_based_service() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = ExplicitTargetDigestSyncRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            Some("sha256:explicit"),
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(*runner.step.lock().unwrap(), 8);
    }

    #[tokio::test]
    async fn digest_pinned_service_skips_local_tag_sync() {
        let stack = single_service_stack("ghcr.io/org/web@sha256:old", None);
        let runner = DigestPinnedRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(*runner.step.lock().unwrap(), 7);
    }

    #[tokio::test]
    async fn stack_update_syncs_local_tag_before_semver_pull() {
        let stack = single_service_stack(
            "ghcr.io/org/web:1.0",
            Some(Candidate {
                tag: "1.0".to_string(),
                resolved_tag: Some("0.7.7".to_string()),
                digest: "sha256:candidate".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
        );
        let runner = SyncBeforeSemverRunner::default();

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(
            outcome.summary_json["semverPulled"],
            json!(["ghcr.io/org/web:v0.7.7"])
        );
        let calls = runner.calls.lock().unwrap();
        let sync_idx = calls
            .iter()
            .position(|(program, args)| {
                program == "docker"
                    && args
                        == &vec![
                            "image".to_string(),
                            "tag".to_string(),
                            "sha256:new".to_string(),
                            "ghcr.io/org/web:1.0".to_string(),
                        ]
            })
            .expect("sync tag command should exist");
        let semver_idx = calls
            .iter()
            .position(|(program, args)| {
                program == "docker"
                    && args == &vec!["pull".to_string(), "ghcr.io/org/web:v0.7.7".to_string()]
            })
            .expect("semver pull should exist");
        assert!(sync_idx < semver_idx);
    }

    #[test]
    fn parse_pull_fraction_supports_size_ratio_tokens() {
        let line = "d2cad1f9f7c9 Downloading [==================> ] 3.146MB/5.89MB";
        let frac = parse_pull_fraction_from_line(line).unwrap();
        assert!(frac > 0.50 && frac < 0.60);

        let full = "9b4e5f7f3558 Downloading [==================================================>] 443B/443B";
        let full_frac = parse_pull_fraction_from_line(full).unwrap();
        assert!((full_frac - 1.0).abs() < f64::EPSILON);
    }

    #[derive(Default)]
    struct FlakyInspectRunner {
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for FlakyInspectRunner {
        async fn run(
            &self,
            _spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls < 3 {
                Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "transient".to_string(),
                })
            } else {
                Ok(CommandOutput {
                    status: 0,
                    stdout: "ok\n".to_string(),
                    stderr: String::new(),
                })
            }
        }
    }

    #[tokio::test]
    async fn run_to_string_with_retry_succeeds_after_transient_failures() {
        let runner = FlakyInspectRunner::default();
        let got = run_to_string_with_retry(
            &runner,
            CommandSpec {
                program: "docker".to_string(),
                args: vec!["inspect".to_string()],
                env: Vec::new(),
            },
            Duration::from_millis(100),
            "inspect_image_id",
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
        )
        .await
        .expect("third attempt should succeed");
        assert_eq!(got.trim(), "ok");
        assert_eq!(*runner.calls.lock().unwrap(), 3);
    }

    #[derive(Default)]
    struct FailUpRunner {
        up_calls: Mutex<usize>,
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for FailUpRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "c_before\n".to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                },
                2 => CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                3 => {
                    if args_end_with(&spec.args, &["up", "-d", "web"]) {
                        *self.up_calls.lock().unwrap() += 1;
                    }
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: "up failed".to_string(),
                    }
                }
                _ => CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected extra command".to_string(),
                },
            };
            *step += 1;
            Ok(out)
        }
    }

    #[tokio::test]
    async fn up_command_is_not_retried_when_it_fails() {
        let stack = StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: "ghcr.io/org/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate: None,
                ignore: None,
                version_inference: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
                archived: None,
            }],
        };

        let runner = FailUpRunner::default();
        let err = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy {
                max_attempts: 5,
                base_ms: 1,
                max_ms: 2,
            },
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
            None,
            None,
        )
        .await
        .expect_err("up -d failure should abort immediately without retries");
        assert!(err.to_string().contains("command failed"));
        assert_eq!(*runner.up_calls.lock().unwrap(), 1);
    }

    #[test]
    fn semver_tag_candidates_preserve_raw_tag_and_dedupe_normalized_variant() {
        assert_eq!(
            semver_tag_candidates_from_oci_version(" v0.7.7\n"),
            vec!["v0.7.7".to_string(), "0.7.7".to_string()]
        );
        assert_eq!(
            semver_tag_candidates_from_oci_version("0.7.7"),
            vec!["0.7.7".to_string()]
        );
        assert!(semver_tag_candidates_from_oci_version("0.7.7+build.1").is_empty());
    }

    #[derive(Default)]
    struct SemverRawTagRunner {
        pull_calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SemverRawTagRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/org/web:latest"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:v0.7.7"] {
                self.pull_calls.lock().unwrap().push(args[1].to_string());
                return Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct SemverFallbackRunner {
        pull_calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SemverFallbackRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/org/web:latest"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:v0.7.7"] {
                self.pull_calls.lock().unwrap().push(args[1].to_string());
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "raw not found".to_string(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:0.7.7"] {
                self.pull_calls.lock().unwrap().push(args[1].to_string());
                return Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct SemverNormalizedTagAlreadyPresentRunner {
        inspect_repo_tags_calls: Mutex<usize>,
        pull_calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SemverNormalizedTagAlreadyPresentRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                *self.inspect_repo_tags_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/org/web:0.7.7"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:v0.7.7"]
                || args == vec!["pull", "ghcr.io/org/web:0.7.7"]
            {
                *self.pull_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "pull should not run when a fallback tag already exists locally"
                        .to_string(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct SemverAlreadyTaggedRunner {
        inspect_repo_tags_calls: Mutex<usize>,
        pull_calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SemverAlreadyTaggedRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                *self.inspect_repo_tags_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/org/web:v0.7.7"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:v0.7.7"] {
                *self.pull_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "pull should not run when tag already exists locally".to_string(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct SemverPullFailRunner {
        pull_calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SemverPullFailRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "v0.7.7\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:new",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/org/web:latest"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:v0.7.7"]
                || args == vec!["pull", "ghcr.io/org/web:0.7.7"]
            {
                self.pull_calls.lock().unwrap().push(args[1].to_string());
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "not found".to_string(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct NoSemverOciVersionRunner {
        inspect_repo_tags_calls: Mutex<usize>,
        pull_calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for NoSemverOciVersionRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            if spec.program != "docker" {
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "unexpected program".to_string(),
                });
            }
            let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                    "sha256:no-semver",
                ]
            {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "latest\n".to_string(),
                    stderr: String::new(),
                });
            }
            if args
                == vec![
                    "image",
                    "inspect",
                    "--format",
                    "{{json .RepoTags}}",
                    "sha256:no-semver",
                ]
            {
                *self.inspect_repo_tags_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "repo tags should not be inspected without a semver label".to_string(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:latest"] {
                *self.pull_calls.lock().unwrap() += 1;
                return Ok(CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "pull should not be attempted without a semver label".to_string(),
                });
            }
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected args".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_prefers_raw_oci_tag_before_normalized_fallback() {
        let runner = SemverRawTagRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("raw OCI tag should be pulled first");

        assert_eq!(semver_pulled, vec!["ghcr.io/org/web:v0.7.7".to_string()]);
        assert!(semver_pulled_set.contains("ghcr.io/org/web:v0.7.7"));
        assert_eq!(
            *runner.pull_calls.lock().unwrap(),
            vec!["ghcr.io/org/web:v0.7.7".to_string()]
        );
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_falls_back_to_normalized_tag_after_raw_failure() {
        let runner = SemverFallbackRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 1,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("normalized tag should be used as fallback");

        assert_eq!(semver_pulled, vec!["ghcr.io/org/web:0.7.7".to_string()]);
        assert!(semver_pulled_set.contains("ghcr.io/org/web:0.7.7"));
        assert_eq!(
            *runner.pull_calls.lock().unwrap(),
            vec![
                "ghcr.io/org/web:v0.7.7".to_string(),
                "ghcr.io/org/web:0.7.7".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_still_attempts_raw_tag_when_only_normalized_tag_was_previously_pulled()
     {
        let runner = SemverRawTagRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = vec!["ghcr.io/org/web:0.7.7".to_string()];
        let mut semver_pulled_set: HashSet<String> =
            HashSet::from(["ghcr.io/org/web:0.7.7".to_string()]);
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_2",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("raw tag should still be attempted before normalized set short-circuit");

        assert_eq!(
            semver_pulled,
            vec![
                "ghcr.io/org/web:0.7.7".to_string(),
                "ghcr.io/org/web:v0.7.7".to_string(),
            ]
        );
        assert!(semver_pulled_set.contains("ghcr.io/org/web:0.7.7"));
        assert!(semver_pulled_set.contains("ghcr.io/org/web:v0.7.7"));
        assert_eq!(
            *runner.pull_calls.lock().unwrap(),
            vec!["ghcr.io/org/web:v0.7.7".to_string()]
        );
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_short_circuits_when_raw_tag_already_exists_locally() {
        let runner = SemverAlreadyTaggedRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("local raw tag should short-circuit semver pull");

        assert_eq!(semver_pulled, vec!["ghcr.io/org/web:v0.7.7".to_string()]);
        assert!(semver_pulled_set.contains("ghcr.io/org/web:v0.7.7"));
        assert_eq!(*runner.inspect_repo_tags_calls.lock().unwrap(), 1);
        assert_eq!(*runner.pull_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_short_circuits_when_normalized_tag_already_exists_locally() {
        let runner = SemverNormalizedTagAlreadyPresentRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("normalized local tag should short-circuit before remote retries");

        assert_eq!(semver_pulled, vec!["ghcr.io/org/web:0.7.7".to_string()]);
        assert!(semver_pulled_set.contains("ghcr.io/org/web:0.7.7"));
        assert_eq!(*runner.inspect_repo_tags_calls.lock().unwrap(), 1);
        assert_eq!(*runner.pull_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn semver_pull_failure_is_not_best_effort_anymore() {
        let runner = SemverPullFailRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        let err = maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect_err("semver pull failures should now fail the update job");

        let detail = err
            .downcast_ref::<UpdateStepFailure>()
            .expect("expected UpdateStepFailure");
        assert_eq!(detail.step, "semver_pull");
        assert_eq!(detail.retry.attempts, 6);
        assert_eq!(detail.retry.max_attempts, 6);
        assert!(
            detail
                .last_error
                .contains("ghcr.io/org/web:v0.7.7 => command failed: status=1 stderr=not found")
        );
        assert!(
            detail
                .last_error
                .contains("ghcr.io/org/web:0.7.7 => command failed: status=1 stderr=not found")
        );
        assert!(semver_pulled.is_empty());
        assert_eq!(
            *runner.pull_calls.lock().unwrap(),
            vec![
                "ghcr.io/org/web:v0.7.7".to_string(),
                "ghcr.io/org/web:v0.7.7".to_string(),
                "ghcr.io/org/web:v0.7.7".to_string(),
                "ghcr.io/org/web:0.7.7".to_string(),
                "ghcr.io/org/web:0.7.7".to_string(),
                "ghcr.io/org/web:0.7.7".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn maybe_pull_semver_tag_skips_when_oci_version_is_not_semver() {
        let runner = NoSemverOciVersionRunner::default();
        let docker_cfg = docker_runner::DockerRunnerConfig::default();

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 3,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:no-semver",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .expect("non-semver OCI version should skip fallback pull");

        assert!(semver_pulled.is_empty());
        assert!(semver_pulled_set.is_empty());
        assert_eq!(*runner.inspect_repo_tags_calls.lock().unwrap(), 0);
        assert_eq!(*runner.pull_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn semver_pull_uses_docker_auth_env_when_configured() {
        let runner = EnvCaptureSemverRunner::default();
        let (docker_config_path, _docker_config_cleanup) = write_test_docker_config();
        let auth_bridge = DockerCliAuthBridge::stage(&docker_config_path).unwrap();
        let docker_cfg = docker_runner::DockerRunnerConfig {
            docker_bin: "docker".to_string(),
            env: auth_bridge.env(),
        };

        let mut semver_pulled: Vec<String> = Vec::new();
        let mut semver_pulled_set: HashSet<String> = HashSet::new();
        let mut semver_pull_warnings: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();

        maybe_pull_semver_tag_for_image(
            &runner,
            &docker_cfg,
            IdempotentRetryPolicy {
                max_attempts: 1,
                base_ms: 1,
                max_ms: 2,
            },
            "svc_1",
            "ghcr.io/org/web",
            "sha256:new",
            &mut semver_pulled,
            &mut semver_pulled_set,
            &mut semver_pull_warnings,
        )
        .await
        .unwrap();

        assert_eq!(semver_pulled, vec!["ghcr.io/org/web:0.7.7".to_string()]);

        for spec in runner.specs.lock().unwrap().iter() {
            assert_eq!(spec.env.len(), 1);
            assert!(spec.env.iter().all(|(k, _)| k == "DOCKER_CONFIG"));
        }
    }

    #[test]
    fn strip_tag_and_digest_handles_digest_only_refs() {
        assert_eq!(
            strip_tag_and_digest("alpine@sha256:deadbeef"),
            Some("alpine".to_string())
        );
        assert_eq!(
            strip_tag_and_digest("ghcr.io/org/web@sha256:deadbeef"),
            Some("ghcr.io/org/web".to_string())
        );
    }
}
