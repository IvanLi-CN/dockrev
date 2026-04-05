use std::{
    collections::{HashMap, HashSet},
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
    api::types::{JobScope, StackRecord, UpdateServiceTarget},
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

fn is_comparable_prerelease_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }

    let normalized = token.to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "alpha" | "beta" | "rc" | "pre" | "preview"
    ) {
        return true;
    }

    ["alpha", "beta", "rc", "pre", "preview"]
        .into_iter()
        .any(|prefix| {
            normalized.strip_prefix(prefix).is_some_and(|suffix| {
                let digits = suffix.strip_prefix('-').unwrap_or(suffix);
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

fn parse_comparable_strict_semver_tag(tag: &str) -> Option<Version> {
    let version = parse_strict_semver_tag(tag)?;
    let comparable = version.pre.is_empty()
        || version
            .pre
            .as_str()
            .split('.')
            .all(is_comparable_prerelease_token);
    comparable.then_some(version)
}

fn comparable_semver_baseline(preferred_tag: Option<&str>, fallback_tag: &str) -> Option<Version> {
    match preferred_tag.map(str::trim).filter(|tag| !tag.is_empty()) {
        Some(tag) => parse_comparable_strict_semver_tag(tag),
        None => parse_comparable_strict_semver_tag(fallback_tag),
    }
}

fn semver_baseline_for_current(svc: &crate::api::types::Service) -> Option<Version> {
    comparable_semver_baseline(svc.image.resolved_tag.as_deref(), &svc.image.tag)
}

fn semver_baseline_for_candidate(svc: &crate::api::types::Service) -> Option<Version> {
    let candidate = svc.candidate.as_ref()?;
    comparable_semver_baseline(candidate.resolved_tag.as_deref(), &candidate.tag)
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

fn insert_legacy_semver_compat_fields(summary: &mut serde_json::Map<String, serde_json::Value>) {
    summary.insert("semverPulled".to_string(), json!(Vec::<String>::new()));
    summary.insert(
        "semverPullWarnings".to_string(),
        serde_json::Value::Object(serde_json::Map::<String, serde_json::Value>::new()),
    );
}

fn insert_tag_pull_summary_fields(
    summary: &mut serde_json::Map<String, serde_json::Value>,
    target_tags_pulled: &[String],
    pull_tags_pulled: &[String],
    pull_tag_warnings: &[serde_json::Value],
) {
    summary.insert("targetTagsPulled".to_string(), json!(target_tags_pulled));
    summary.insert("pullTagsPulled".to_string(), json!(pull_tags_pulled));
    summary.insert("pullTagWarnings".to_string(), json!(pull_tag_warnings));
    insert_legacy_semver_compat_fields(summary);
}

struct UpdateSummaryInput<'a> {
    changed: u32,
    old_images: &'a serde_json::Map<String, serde_json::Value>,
    new_images: &'a serde_json::Map<String, serde_json::Value>,
    final_images: &'a serde_json::Map<String, serde_json::Value>,
    target_tags_pulled: &'a [String],
    pull_tags_pulled: &'a [String],
    pull_tag_warnings: &'a [serde_json::Value],
    rollback_trigger: Option<&'a str>,
    skipped_version_anomaly: &'a [serde_json::Value],
}

fn build_update_summary(
    input: UpdateSummaryInput<'_>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut summary = serde_json::Map::new();
    summary.insert("changedServices".to_string(), json!(input.changed));
    summary.insert(
        "oldDigests".to_string(),
        serde_json::Value::Object(input.old_images.clone()),
    );
    summary.insert(
        "newDigests".to_string(),
        serde_json::Value::Object(input.new_images.clone()),
    );
    summary.insert(
        "finalDigests".to_string(),
        serde_json::Value::Object(input.final_images.clone()),
    );
    insert_tag_pull_summary_fields(
        &mut summary,
        input.target_tags_pulled,
        input.pull_tags_pulled,
        input.pull_tag_warnings,
    );
    summary.insert(
        "skippedVersionAnomaly".to_string(),
        json!(input.skipped_version_anomaly),
    );
    if let Some(trigger) = input.rollback_trigger {
        summary.insert("failureStep".to_string(), json!(trigger));
        summary.insert(
            "rollback".to_string(),
            json!({
                "trigger": trigger,
                "toDigests": input.final_images,
            }),
        );
    }
    summary
}

fn record_unique_tag_ref(tag_ref: String, refs: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(tag_ref.clone()) {
        refs.push(tag_ref);
    }
}

async fn ensure_explicit_tag_ref_pulled(
    runner: &dyn CommandRunner,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    retry_policy: IdempotentRetryPolicy,
    step: &str,
    tag_ref: &str,
    successful_tag_refs: &mut HashSet<String>,
) -> anyhow::Result<()> {
    if successful_tag_refs.contains(tag_ref) {
        return Ok(());
    }
    run_checked_with_retry(
        runner,
        docker_runner::pull_image(docker_cfg, tag_ref),
        Duration::from_secs(300),
        step,
        retry_policy,
    )
    .await?;
    successful_tag_refs.insert(tag_ref.to_string());
    Ok(())
}

fn tag_pull_warning_value(
    service_id: &str,
    tag_ref: &str,
    err: &anyhow::Error,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("serviceId".to_string(), json!(service_id));
    obj.insert("tagRef".to_string(), json!(tag_ref));
    if let Some(step_failure) = err.downcast_ref::<UpdateStepFailure>() {
        obj.insert("step".to_string(), json!(step_failure.step.clone()));
        obj.insert("retry".to_string(), json!(step_failure.retry.clone()));
        obj.insert(
            "lastError".to_string(),
            json!(step_failure.last_error.clone()),
        );
    } else {
        obj.insert("error".to_string(), json!(err.to_string()));
    }
    serde_json::Value::Object(obj)
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
    for (service_index, svc) in services.into_iter().enumerate() {
        let service_index = service_index as u32;
        let target = explicit_targets_by_service.get(svc.id.as_str());

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

        let active_container_id = post_update_container_id;
        let mut active_container_id = active_container_id;
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
                        message: format!("compatibility tags settled for {}", svc.name),
                    },
                );
            }
        }

        new_images.insert(svc.id.clone(), json!(&attempted_image_id));
        final_images.insert(svc.id.clone(), json!(&final_image_id));
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
            let summary = build_update_summary(UpdateSummaryInput {
                changed,
                old_images: &old_images,
                new_images: &new_images,
                final_images: &final_images,
                target_tags_pulled: &target_tags_pulled,
                pull_tags_pulled: &pull_tags_pulled,
                pull_tag_warnings: &pull_tag_warnings,
                rollback_trigger: rollback_failure_step,
                skipped_version_anomaly: &skipped_version_anomaly,
            });
            return Ok(UpdateOutcome {
                status: "rolled_back".to_string(),
                summary_json: serde_json::Value::Object(summary),
            });
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
                new_version_discovery_count: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                    repo_url: None,
                },
                archived: None,
            }],
        }
    }

    fn explicit_targets(
        service_id: &str,
        target_tag: &str,
        target_digest: &str,
        pull_tags: &[&str],
    ) -> Vec<UpdateServiceTarget> {
        vec![UpdateServiceTarget {
            service_id: service_id.to_string(),
            target_tag: target_tag.to_string(),
            target_digest: target_digest.to_string(),
            pull_tags: Some(pull_tags.iter().map(|tag| (*tag).to_string()).collect()),
            skip_tag_followups: false,
        }]
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
            new_version_discovery_count: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
                repo_url: None,
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

    #[test]
    fn detect_semver_downgrade_ignores_opaque_hash_like_prerelease_versions() {
        let mut service =
            selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
        service.image.tag = "latest".to_string();
        service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
        }

        assert_eq!(detect_semver_downgrade(&service), None);
    }

    #[test]
    fn detect_semver_downgrade_does_not_fall_back_to_raw_tag_after_opaque_resolved_tag() {
        let mut service = selection_test_service(
            "svc-hash-tagged",
            "hash-build",
            "ghcr.io/acme/web:2026.3.28",
        );
        service.image.tag = "2026.3.28".to_string();
        service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.tag = "2026.3.27".to_string();
            candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
        }

        assert_eq!(detect_semver_downgrade(&service), None);
    }

    #[test]
    fn select_update_services_keeps_hash_like_prerelease_candidates_for_non_ui_runs() {
        let mut service =
            selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
        service.image.tag = "latest".to_string();
        service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
        }
        let stack = StackRecord {
            id: "stk_hash".to_string(),
            name: "hash-build".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![service],
        };

        let selection =
            select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

        assert_eq!(selection.services.len(), 1);
        assert!(selection.skipped_version_anomaly.is_empty());
    }

    #[test]
    fn select_update_services_keeps_opaque_resolved_tags_even_when_raw_tags_look_semver_like() {
        let mut service = selection_test_service(
            "svc-hash-tagged",
            "hash-build",
            "ghcr.io/acme/web:2026.3.28",
        );
        service.image.tag = "2026.3.28".to_string();
        service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.tag = "2026.3.27".to_string();
            candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
        }
        let stack = StackRecord {
            id: "stk_hash_tagged".to_string(),
            name: "hash-build".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![service],
        };

        let selection =
            select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

        assert_eq!(selection.services.len(), 1);
        assert!(selection.skipped_version_anomaly.is_empty());
    }

    #[test]
    fn select_update_services_still_skips_ordered_prerelease_downgrades() {
        let mut service = selection_test_service("svc-rc", "rc-build", "ghcr.io/acme/web:latest");
        service.image.tag = "latest".to_string();
        service.image.resolved_tag = Some("v1.0.0-rc.2".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.resolved_tag = Some("v1.0.0-rc.1".to_string());
        }
        let stack = StackRecord {
            id: "stk_rc".to_string(),
            name: "rc-build".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![service],
        };

        let selection =
            select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

        assert!(selection.services.is_empty());
        assert_eq!(selection.skipped_version_anomaly.len(), 1);
        assert_eq!(
            selection.skipped_version_anomaly[0]["reason"].as_str(),
            Some("semver_downgrade")
        );
    }

    #[test]
    fn select_update_services_still_skips_single_token_prerelease_downgrades() {
        let mut service = selection_test_service("svc-rc1", "rc-build", "ghcr.io/acme/web:latest");
        service.image.tag = "latest".to_string();
        service.image.resolved_tag = Some("v1.0.0-rc2".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.resolved_tag = Some("v1.0.0-rc1".to_string());
        }
        let stack = StackRecord {
            id: "stk_rc1".to_string(),
            name: "rc-build".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![service],
        };

        let selection =
            select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

        assert!(selection.services.is_empty());
        assert_eq!(selection.skipped_version_anomaly.len(), 1);
        assert_eq!(
            selection.skipped_version_anomaly[0]["reason"].as_str(),
            Some("semver_downgrade")
        );
    }

    #[test]
    fn select_update_services_still_skips_hyphenated_prerelease_downgrades() {
        let mut service =
            selection_test_service("svc-rc-hyphen", "rc-build", "ghcr.io/acme/web:latest");
        service.image.tag = "latest".to_string();
        service.image.resolved_tag = Some("v1.0.0-rc-2".to_string());
        if let Some(candidate) = service.candidate.as_mut() {
            candidate.resolved_tag = Some("v1.0.0-rc-1".to_string());
        }
        let stack = StackRecord {
            id: "stk_rc_hyphen".to_string(),
            name: "rc-build".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![service],
        };

        let selection =
            select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

        assert!(selection.services.is_empty());
        assert_eq!(selection.skipped_version_anomaly.len(), 1);
        assert_eq!(
            selection.skipped_version_anomaly[0]["reason"].as_str(),
            Some("semver_downgrade")
        );
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
                new_version_discovery_count: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                    repo_url: None,
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
                new_version_discovery_count: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                    repo_url: None,
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
                new_version_discovery_count: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                    repo_url: None,
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
    struct HealthRollbackRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for HealthRollbackRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: "old_container\n".to_string(),
                        stderr: String::new(),
                    }
                }
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
                2 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["pull", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                3 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["up", "-d", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    }
                }
                4 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: "new_container\n".to_string(),
                        stderr: String::new(),
                    }
                }
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
                        stdout: "1\n".to_string(),
                        stderr: String::new(),
                    }
                }
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
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec![
                            "inspect",
                            "--format",
                            "{{.State.Health.Status}}",
                            "new_container"
                        ]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "unhealthy\n".to_string(),
                        stderr: String::new(),
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
                10 => {
                    assert_eq!(spec.program, "docker-compose");
                    assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                    CommandOutput {
                        status: 0,
                        stdout: "rollback_container\n".to_string(),
                        stderr: String::new(),
                    }
                }
                11 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec![
                            "inspect",
                            "--format",
                            "{{.State.Health.Status}}",
                            "rollback_container"
                        ]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "healthy\n".to_string(),
                        stderr: String::new(),
                    }
                }
                12 => {
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

    #[tokio::test]
    async fn healthcheck_failure_rolls_back_with_attempted_and_final_digests() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = HealthRollbackRunner::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UpdateProgressEvent>();

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
            false,
            "ui",
            None,
            Some(tx),
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "rolled_back");
        assert_eq!(
            outcome.summary_json["newDigests"]["svc_1"],
            json!("sha256:new")
        );
        assert_eq!(
            outcome.summary_json["finalDigests"]["svc_1"],
            json!("sha256:old")
        );
        assert_eq!(
            outcome.summary_json["failureStep"].as_str(),
            Some("healthcheck")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["trigger"],
            json!("healthcheck")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["toDigests"]["svc_1"],
            json!("sha256:old")
        );

        let mut steps = Vec::new();
        let mut messages = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            steps.push(evt.step);
            messages.push(evt.message);
        }
        assert!(steps.contains(&UpdateProgressStep::HealthStart));
        assert!(steps.contains(&UpdateProgressStep::HealthFailed));
        assert!(!steps.contains(&UpdateProgressStep::HealthDone));
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("healthcheck failed"))
        );
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("rolled back after healthcheck failure"))
        );
        assert_eq!(*runner.step.lock().unwrap(), 13);
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
                        vec!["pull", "ghcr.io/org/web:1.0"]
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
                8 => {
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
            json!("sha256:new")
        );
        assert_eq!(
            outcome.summary_json["finalDigests"]["svc_1"],
            json!("sha256:old")
        );
        assert_eq!(
            outcome.summary_json["failureStep"].as_str(),
            Some("sync_configured_tag")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["trigger"],
            json!("sync_configured_tag")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["toDigests"]["svc_1"],
            json!("sha256:old")
        );
        assert_eq!(*runner.step.lock().unwrap(), 12);
    }

    #[tokio::test]
    async fn explicit_target_digest_still_syncs_tag_based_service() {
        let stack = single_service_stack("ghcr.io/org/web:1.0", None);
        let runner = ExplicitTargetDigestSyncRunner::default();
        let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:explicit", &[]);

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            Some(explicit_targets.as_slice()),
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(
            outcome.summary_json["targetTagsPulled"],
            json!(["ghcr.io/org/web:1.0"])
        );
        assert_eq!(*runner.step.lock().unwrap(), 9);
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
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
        assert_eq!(*runner.step.lock().unwrap(), 7);
    }

    #[tokio::test]
    async fn explicit_targets_must_cover_selected_services_at_execution_time() {
        let mut stack = single_service_stack(
            "ghcr.io/org/web:1.0",
            Some(Candidate {
                tag: "1.0".to_string(),
                resolved_tag: Some("1.0".to_string()),
                digest: "sha256:new1".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
        );
        let mut worker = stack.services[0].clone();
        worker.id = "svc_2".to_string();
        worker.name = "worker".to_string();
        worker.image.reference = "ghcr.io/org/worker:2.0".to_string();
        worker.image.tag = "2.0".to_string();
        worker.candidate = Some(Candidate {
            tag: "2.0".to_string(),
            resolved_tag: Some("2.0".to_string()),
            digest: "sha256:new2".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        });
        stack.services.push(worker);

        let runner = FakeRunner::default();
        let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new1", &[]);

        let err = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "live",
            Some(explicit_targets.as_slice()),
            false,
            "ui",
            None,
            None,
        )
        .await
        .expect_err("missing explicit target should fail before executing update commands");

        assert!(
            err.to_string()
                .contains("explicit update targets no longer cover selected services: svc_2")
        );
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stack_update_pulls_target_tag_before_sync_and_compatibility_tags_afterwards() {
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
        let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:candidate", &["v0.7.7"]);

        let outcome = run_update_job(
            &runner,
            "docker-compose",
            None,
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Stack,
            None,
            "live",
            Some(explicit_targets.as_slice()),
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(
            outcome.summary_json["targetTagsPulled"],
            json!(["ghcr.io/org/web:1.0"])
        );
        assert_eq!(
            outcome.summary_json["pullTagsPulled"],
            json!(["ghcr.io/org/web:v0.7.7"])
        );
        let calls = runner.calls.lock().unwrap();
        let target_idx = calls
            .iter()
            .position(|(program, args)| {
                program == "docker"
                    && args == &vec!["pull".to_string(), "ghcr.io/org/web:1.0".to_string()]
            })
            .expect("target tag pull should exist");
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
        let compat_idx = calls
            .iter()
            .position(|(program, args)| {
                program == "docker"
                    && args == &vec!["pull".to_string(), "ghcr.io/org/web:v0.7.7".to_string()]
            })
            .expect("compatibility tag pull should exist");
        assert!(target_idx < sync_idx);
        assert!(sync_idx < compat_idx);
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
                new_version_discovery_count: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                    repo_url: None,
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

    #[derive(Default)]
    struct CompatibilityTagWarningRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for CompatibilityTagWarningRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container
"
                    .to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old
"
                    .to_string(),
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
                    stdout: "new_container
"
                    .to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0
"
                    .to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new
"
                    .to_string(),
                    stderr: String::new(),
                },
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["pull", "ghcr.io/org/web:1.0"]
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
                8 => {
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
                9 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["pull", "ghcr.io/org/web:v0.7.7"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: "compat tag missing".to_string(),
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
    async fn compatibility_tag_pull_failures_only_record_warnings() {
        let stack = single_service_stack(
            "ghcr.io/org/web:1.0",
            Some(Candidate {
                tag: "1.0".to_string(),
                resolved_tag: Some("1.0".to_string()),
                digest: "sha256:new".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
        );
        let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &["v0.7.7"]);
        let runner = CompatibilityTagWarningRunner::default();

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
            Some(explicit_targets.as_slice()),
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "success");
        assert_eq!(
            outcome.summary_json["targetTagsPulled"],
            json!(["ghcr.io/org/web:1.0"])
        );
        assert_eq!(outcome.summary_json["pullTagsPulled"], json!([]));
        assert_eq!(outcome.summary_json["semverPulled"], json!([]));
        let warnings = outcome.summary_json["pullTagWarnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0]["tagRef"], json!("ghcr.io/org/web:v0.7.7"));
        assert_eq!(warnings[0]["step"], json!("pull_tag"));
    }

    #[derive(Default)]
    struct TargetTagPullRollbackRunner {
        step: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for TargetTagPullRollbackRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            let mut step = self.step.lock().unwrap();
            let out = match *step {
                0 => CommandOutput {
                    status: 0,
                    stdout: "old_container
"
                    .to_string(),
                    stderr: String::new(),
                },
                1 => CommandOutput {
                    status: 0,
                    stdout: "sha256:old
"
                    .to_string(),
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
                    stdout: "new_container
"
                    .to_string(),
                    stderr: String::new(),
                },
                5 => CommandOutput {
                    status: 0,
                    stdout: "0
"
                    .to_string(),
                    stderr: String::new(),
                },
                6 => CommandOutput {
                    status: 0,
                    stdout: "sha256:new
"
                    .to_string(),
                    stderr: String::new(),
                },
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec!["pull", "ghcr.io/org/web:1.0"]
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 1,
                        stdout: String::new(),
                        stderr: "target tag missing".to_string(),
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
                    stdout: "rollback_container
"
                    .to_string(),
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
                        stdout: "sha256:old
"
                        .to_string(),
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
    async fn target_tag_pull_failure_rolls_back_with_explicit_failure_step() {
        let stack = single_service_stack(
            "ghcr.io/org/web:1.0",
            Some(Candidate {
                tag: "1.0".to_string(),
                resolved_tag: Some("1.0".to_string()),
                digest: "sha256:new".to_string(),
                arch_match: ArchMatch::Match,
                arch: vec!["linux/amd64".to_string()],
            }),
        );
        let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &[]);
        let runner = TargetTagPullRollbackRunner::default();

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
            Some(explicit_targets.as_slice()),
            false,
            "ui",
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.status, "rolled_back");
        assert_eq!(
            outcome.summary_json["failureStep"],
            json!("pull_target_tag")
        );
        assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
        assert_eq!(
            outcome.summary_json["newDigests"]["svc_1"],
            json!("sha256:new")
        );
        assert_eq!(
            outcome.summary_json["finalDigests"]["svc_1"],
            json!("sha256:old")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["trigger"],
            json!("pull_target_tag")
        );
        assert_eq!(
            outcome.summary_json["rollback"]["toDigests"]["svc_1"],
            json!("sha256:old")
        );
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
