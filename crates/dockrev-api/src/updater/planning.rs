use semver::Version;

use super::*;

pub(super) fn emit_update_progress(
    progress_events: Option<&UnboundedSender<UpdateProgressEvent>>,
    event: UpdateProgressEvent,
) {
    if let Some(tx) = progress_events {
        let _ = tx.send(event);
    }
}

pub(super) fn retry_backoff_delay(retry_policy: IdempotentRetryPolicy, attempt: usize) -> Duration {
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

pub(super) fn detect_semver_downgrade(
    svc: &crate::api::types::Service,
) -> Option<(String, String)> {
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

pub(super) fn failed_summary_with_failure_step(
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

pub(super) fn should_sync_local_tag(image_ref: &str) -> bool {
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

pub(super) fn insert_tag_pull_summary_fields(
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

pub(super) struct UpdateSummaryInput<'a> {
    pub(super) changed: u32,
    pub(super) old_images: &'a serde_json::Map<String, serde_json::Value>,
    pub(super) new_images: &'a serde_json::Map<String, serde_json::Value>,
    pub(super) final_images: &'a serde_json::Map<String, serde_json::Value>,
    pub(super) target_tags_pulled: &'a [String],
    pub(super) pull_tags_pulled: &'a [String],
    pub(super) pull_tag_warnings: &'a [serde_json::Value],
    pub(super) rollback_trigger: Option<&'a str>,
    pub(super) skipped_version_anomaly: &'a [serde_json::Value],
}

pub(super) fn build_update_summary(
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

pub(super) fn record_unique_tag_ref(
    tag_ref: String,
    refs: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if seen.insert(tag_ref.clone()) {
        refs.push(tag_ref);
    }
}

pub(super) async fn ensure_explicit_tag_ref_pulled(
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

pub(super) fn tag_pull_warning_value(
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
