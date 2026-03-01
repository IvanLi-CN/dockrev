use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use semver::Version;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

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
    HealthDone,
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
    json!({
        "reason": reason,
        "skippedVersionAnomaly": skipped_version_anomaly,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run_update_job(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    idempotent_retry_policy: IdempotentRetryPolicy,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    mode: &str,
    target_tag: Option<&str>,
    target_digest: Option<&str>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    progress_events: Option<UnboundedSender<UpdateProgressEvent>>,
) -> anyhow::Result<UpdateOutcome> {
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
    };
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };

    let selection =
        select_update_services(stack, scope, service_id, allow_arch_mismatch, update_reason);
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

    let docker_cfg = docker_runner::DockerRunnerConfig::default();

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
                run_checked_with_retry(
                    runner,
                    docker_runner::tag_image(&docker_cfg, &old_image_id, &svc.image.reference),
                    Duration::from_secs(30),
                    "tag_image",
                    idempotent_retry_policy,
                )
                .await?;
                run_checked(
                    runner,
                    compose_stack.up_service_no_pull(&compose_cfg, &svc.name),
                    Duration::from_secs(300),
                )
                .await?;

                // Rollback `up -d` can also recreate the container.
                let rollback_container_id = run_to_string(
                    runner,
                    compose_stack.ps_q_service(&compose_cfg, &svc.name),
                    Duration::from_secs(30),
                )
                .await?;
                let rollback_container_id = rollback_container_id.trim().to_string();
                if rollback_container_id.is_empty() {
                    return Ok(UpdateOutcome {
                        status: "failed".to_string(),
                        summary_json: failed_summary_with_skipped_anomaly(
                            "container_missing_after_rollback",
                            &skipped_version_anomaly,
                        ),
                    });
                }
                active_container_id = rollback_container_id;

                let ok2 = wait_healthy(
                    runner,
                    &docker_cfg,
                    &active_container_id,
                    Duration::from_secs(90),
                    idempotent_retry_policy,
                )
                .await?;
                if !ok2 {
                    return Ok(UpdateOutcome {
                        status: "failed".to_string(),
                        summary_json: failed_summary_with_skipped_anomaly(
                            "rollback_failed",
                            &skipped_version_anomaly,
                        ),
                    });
                }
                rolled_back = true;
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

        let new_image_id = run_to_string_with_retry(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &active_container_id),
            Duration::from_secs(10),
            "inspect_image_id",
            idempotent_retry_policy,
        )
        .await?;
        let new_image_id = new_image_id.trim().to_string();
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
            return Ok(UpdateOutcome {
                status: "rolled_back".to_string(),
                summary_json: json!({
                    "changedServices": changed,
                    "oldDigests": old_images,
                    "newDigests": new_images,
                    "semverPulled": semver_pulled,
                    "semverPullWarnings": semver_pull_warnings,
                    "skippedVersionAnomaly": skipped_version_anomaly,
                }),
            });
        }

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
        .await?;

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

fn semver_tag_from_oci_version(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() || t == "<no value>" {
        return None;
    }
    let t = t
        .strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .unwrap_or(t);
    let v = Version::parse(t).ok()?;
    if !v.build.is_empty() {
        return None;
    }
    Some(v.to_string())
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

    let Some(tag) = semver_tag_from_oci_version(&raw_version) else {
        return Ok(());
    };
    let tag_ref = format!("{repo}:{tag}");

    if semver_pulled_set.contains(&tag_ref) {
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
    if let Ok(parsed) = serde_json::from_str::<Option<Vec<String>>>(repo_tags.trim())
        && parsed.unwrap_or_default().iter().any(|t| t == &tag_ref)
    {
        semver_pulled_set.insert(tag_ref);
        return Ok(());
    }

    run_checked_with_retry(
        runner,
        docker_runner::pull_image(docker_cfg, &tag_ref),
        Duration::from_secs(300),
        "semver_pull",
        idempotent_retry_policy,
    )
    .await?;
    if semver_pulled_set.insert(tag_ref.clone()) {
        semver_pulled.push(tag_ref);
    }

    Ok(())
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
        api::types::{BackupTargetOverrides, ComposeRef, Service, ServiceSettings, TernaryChoice},
        runner::{CommandOutput, CommandRunner},
    };
    use std::{collections::BTreeMap, sync::Mutex};

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
                // docker image inspect version label (best-effort semver tag pull; empty => skip)
                7 => {
                    assert_eq!(spec.program, "docker");
                    assert_eq!(
                        spec.args,
                        vec![
                            "image",
                            "inspect",
                            "--format",
                            r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                            "sha256:new"
                        ]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                    );
                    CommandOutput {
                        status: 0,
                        stdout: "\n".to_string(),
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
            IdempotentRetryPolicy::default(),
            &stack,
            &JobScope::Service,
            Some("svc_1"),
            "live",
            None,
            None,
            false,
            "ui",
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
        assert!(steps.contains(&UpdateProgressStep::ServiceDone));
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
        )
        .await
        .expect_err("up -d failure should abort immediately without retries");
        assert!(err.to_string().contains("command failed"));
        assert_eq!(*runner.up_calls.lock().unwrap(), 1);
    }

    #[derive(Default)]
    struct SemverPullFailRunner {
        pull_calls: Mutex<usize>,
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
                    stdout: r#"["ghcr.io/org/web:latest"]"#.to_string(),
                    stderr: String::new(),
                });
            }
            if args == vec!["pull", "ghcr.io/org/web:0.7.7"] {
                *self.pull_calls.lock().unwrap() += 1;
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
        assert_eq!(detail.retry.attempts, 3);
        assert_eq!(detail.retry.max_attempts, 3);
        assert!(detail.last_error.contains("status=1"));
        assert!(semver_pulled.is_empty());
        assert_eq!(*runner.pull_calls.lock().unwrap(), 3);
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
