use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use regex::Regex;
use semver::{Version, VersionReq};
use serde_json::json;

use crate::{
    api,
    api::types::{
        AutoUpdateMatcherType, AutoUpdatePolicy, AutoUpdatePolicyMode, AutoUpdateRule,
        AutoUpdateRuleAction, BackupMode, JobScope, TriggerUpdateRequest, UpdateMode, UpdateReason,
        UpdateServiceTarget,
    },
    db::{
        AutoUpdateCandidateInput, AutoUpdateCandidateRow, AutoUpdatePendingInput,
        AutoUpdatePendingRow, NewVersionDiscoveryRow,
    },
    error::ApiError,
    ids, ignore, notify,
    state::AppState,
};

pub const TIME_DELAY_PRESETS_SECONDS: &[u32] = &[
    0, 900, 3_600, 10_800, 21_600, 43_200, 86_400, 259_200, 604_800,
];
pub const VERSION_LAG_PRESETS: &[u32] = &[0, 1, 2, 3, 5, 8];

const PENDING_POLL_INTERVAL_SECONDS: u64 = 60;

#[derive(Clone, Debug)]
struct EffectivePolicy {
    scope_type: &'static str,
    scope_id: String,
    policy: AutoUpdatePolicy,
}

#[derive(Clone, Debug)]
struct MatchedRule {
    rule: AutoUpdateRule,
}

pub fn validate_policy_for_scope(
    policy: &AutoUpdatePolicy,
    scope_type: &str,
) -> Result<(), ApiError> {
    match scope_type {
        "stack" => {
            if policy.mode != AutoUpdatePolicyMode::Override {
                return Err(ApiError::invalid_argument(
                    "stack autoUpdatePolicy.mode must be override",
                ));
            }
        }
        "service" => {}
        _ => {
            return Err(ApiError::invalid_argument(
                "invalid auto update policy scope",
            ));
        }
    }

    if policy.mode == AutoUpdatePolicyMode::Inherit || policy.mode == AutoUpdatePolicyMode::Disabled
    {
        return Ok(());
    }

    if policy.enabled && policy.rules.is_empty() {
        return Err(ApiError::invalid_argument(
            "autoUpdatePolicy.rules must not be empty when enabled",
        ));
    }

    let mut ids = BTreeSet::new();
    for (idx, rule) in policy.rules.iter().enumerate() {
        let prefix = format!("autoUpdatePolicy.rules[{idx}]");
        if rule.id.trim().is_empty() {
            return Err(ApiError::invalid_argument(format!(
                "{prefix}.id is required"
            )));
        }
        if !ids.insert(rule.id.trim().to_string()) {
            return Err(ApiError::invalid_argument(format!(
                "{prefix}.id must be unique"
            )));
        }
        if rule.name.trim().is_empty() {
            return Err(ApiError::invalid_argument(format!(
                "{prefix}.name is required"
            )));
        }
        validate_matcher(&rule.matcher.kind, &rule.matcher.pattern, &prefix)?;
        match rule.action {
            AutoUpdateRuleAction::Immediate => {}
            AutoUpdateRuleAction::Delayed => {
                validate_delay_value(
                    rule.delay.min_age_seconds,
                    TIME_DELAY_PRESETS_SECONDS,
                    &format!("{prefix}.delay.minAgeSeconds"),
                )?;
                validate_delay_value(
                    rule.delay.min_version_lag,
                    VERSION_LAG_PRESETS,
                    &format!("{prefix}.delay.minVersionLag"),
                )?;
            }
        }
    }

    Ok(())
}

fn validate_delay_value(value: u32, presets: &[u32], field: &str) -> Result<(), ApiError> {
    if presets.contains(&value) {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(format!(
            "{field} must use a supported slider preset"
        )))
    }
}

fn validate_matcher(
    kind: &AutoUpdateMatcherType,
    pattern: &str,
    prefix: &str,
) -> Result<(), ApiError> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(ApiError::invalid_argument(format!(
            "{prefix}.matcher.pattern is required"
        )));
    }
    match kind {
        AutoUpdateMatcherType::Semver => VersionReq::parse(pattern).map(|_| ()).map_err(|_| {
            ApiError::invalid_argument(format!("{prefix}.matcher.pattern invalid semver"))
        }),
        AutoUpdateMatcherType::Regex => Regex::new(pattern).map(|_| ()).map_err(|_| {
            ApiError::invalid_argument(format!("{prefix}.matcher.pattern invalid regex"))
        }),
        AutoUpdateMatcherType::Glob => glob_to_regex(pattern)
            .and_then(|regex| Regex::new(&regex).map(|_| ()).map_err(anyhow::Error::from))
            .map_err(|_| {
                ApiError::invalid_argument(format!("{prefix}.matcher.pattern invalid glob"))
            }),
    }
}

fn glob_to_regex(pattern: &str) -> anyhow::Result<String> {
    let mut out = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' | ']' | '(' | ')' | '{' | '}' | '.' | '+' | '^' | '$' | '|' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out.push('$');
    Ok(out)
}

fn rule_delay(rule: &AutoUpdateRule) -> (u32, u32) {
    match rule.action {
        AutoUpdateRuleAction::Immediate => (0, 0),
        AutoUpdateRuleAction::Delayed => (rule.delay.min_age_seconds, rule.delay.min_version_lag),
    }
}

fn candidate_match_values(candidate: &notify::NewVersionDiscoveredService) -> Vec<&str> {
    if candidate.candidate_display_tag.trim().is_empty()
        || candidate.candidate_display_tag.trim() == candidate.candidate_tag.trim()
    {
        vec![candidate.candidate_tag.trim()]
    } else {
        vec![
            candidate.candidate_display_tag.trim(),
            candidate.candidate_tag.trim(),
        ]
    }
}

fn resolved_candidate_version(candidate: &notify::NewVersionDiscoveredService) -> Option<String> {
    dockrev_common::normalized_semver_from_oci_version(&candidate.candidate_tag)
}

fn rule_matches_text(rule: &AutoUpdateRule, value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    match rule.matcher.kind {
        AutoUpdateMatcherType::Semver => {
            let Some(version) = ignore::parse_version(value) else {
                return false;
            };
            VersionReq::parse(rule.matcher.pattern.trim()).is_ok_and(|req| req.matches(&version))
        }
        AutoUpdateMatcherType::Regex => Regex::new(rule.matcher.pattern.trim()).is_ok_and(|re| {
            re.is_match(value) && re.find(value).is_some_and(|m| m.as_str() == value)
        }),
        AutoUpdateMatcherType::Glob => glob_to_regex(rule.matcher.pattern.trim())
            .ok()
            .and_then(|regex| Regex::new(&regex).ok())
            .is_some_and(|re| re.is_match(value)),
    }
}

fn rule_matches_candidate(
    rule: &AutoUpdateRule,
    candidate: &notify::NewVersionDiscoveredService,
    resolved_version: Option<&str>,
) -> bool {
    match rule.matcher.kind {
        AutoUpdateMatcherType::Semver => {
            resolved_version.is_some_and(|version| rule_matches_text(rule, version))
        }
        AutoUpdateMatcherType::Regex | AutoUpdateMatcherType::Glob => {
            candidate_match_values(candidate)
                .into_iter()
                .any(|value| rule_matches_text(rule, value))
        }
    }
}

fn has_waiting_semver_rule(policy: &AutoUpdatePolicy) -> bool {
    policy.rules.iter().any(|rule| {
        rule.enabled
            && matches!(rule.matcher.kind, AutoUpdateMatcherType::Semver)
            && VersionReq::parse(rule.matcher.pattern.trim()).is_ok()
    })
}

async fn effective_policy_for_service(
    state: &AppState,
    stack_id: &str,
    service_id: &str,
) -> anyhow::Result<Option<EffectivePolicy>> {
    let service_policy = state
        .db
        .get_auto_update_policy("service", service_id, AutoUpdatePolicyMode::Inherit)
        .await?;
    match service_policy.mode {
        AutoUpdatePolicyMode::Override => {
            return Ok(Some(EffectivePolicy {
                scope_type: "service",
                scope_id: service_id.to_string(),
                policy: service_policy,
            }));
        }
        AutoUpdatePolicyMode::Disabled => return Ok(None),
        AutoUpdatePolicyMode::Inherit => {}
    }

    let stack_policy = state
        .db
        .get_auto_update_policy("stack", stack_id, AutoUpdatePolicyMode::Override)
        .await?;
    if !stack_policy.enabled {
        return Ok(None);
    }
    Ok(Some(EffectivePolicy {
        scope_type: "stack",
        scope_id: stack_id.to_string(),
        policy: stack_policy,
    }))
}

fn parse_rfc3339(input: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339).ok()
}

fn add_seconds(ts: &str, seconds: u32) -> String {
    parse_rfc3339(ts)
        .map(|value| value + time::Duration::seconds(seconds as i64))
        .and_then(|value| {
            value
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| ts.to_string())
}

fn version_lag_met(
    min_version_lag: u32,
    current_display_tag: &str,
    candidate: &notify::NewVersionDiscoveredService,
    rule: &AutoUpdateRule,
    history: &[NewVersionDiscoveryRow],
) -> bool {
    if min_version_lag == 0 {
        return true;
    }
    let Some(current_version) = ignore::parse_version(current_display_tag) else {
        return false;
    };

    let mut versions = BTreeSet::<semver::Version>::new();
    for value in candidate_match_values(candidate) {
        if let Some(version) = ignore::parse_version(value)
            && version > current_version
            && rule_matches_text(rule, value)
        {
            versions.insert(version);
        }
    }

    let current_digest = candidate.current_digest.as_deref().unwrap_or_default();
    for row in history {
        if row.service_id != candidate.service_id {
            continue;
        }
        if !current_digest.is_empty() && row.current_digest != current_digest {
            continue;
        }
        if !row.current_display_tag.is_empty() && row.current_display_tag != current_display_tag {
            continue;
        }
        let values = if row.candidate_display_tag.trim().is_empty()
            || row.candidate_display_tag.trim() == row.candidate_tag.trim()
        {
            vec![row.candidate_tag.as_str()]
        } else {
            vec![
                row.candidate_display_tag.as_str(),
                row.candidate_tag.as_str(),
            ]
        };
        for value in values {
            if let Some(version) = ignore::parse_version(value)
                && version > current_version
                && rule_matches_text(rule, value)
            {
                versions.insert(version);
            }
        }
    }

    versions.len() >= min_version_lag as usize
}

fn summary_string(summary: &serde_json::Value, key: &str) -> Option<String> {
    summary
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn pending_candidate(pending: &AutoUpdatePendingRow) -> notify::NewVersionDiscoveredService {
    let current_tag = summary_string(&pending.summary_json, "currentTag")
        .unwrap_or_else(|| pending.current_display_tag.clone());
    let current_digest = summary_string(&pending.summary_json, "currentDigest");
    notify::NewVersionDiscoveredService {
        stack_id: pending.stack_id.clone(),
        service_id: pending.service_id.clone(),
        image_ref: summary_string(&pending.summary_json, "imageRef").unwrap_or_default(),
        current_tag,
        current_digest,
        current_display_tag: pending.current_display_tag.clone(),
        candidate_tag: pending.candidate_tag.clone(),
        candidate_display_tag: pending.candidate_display_tag.clone(),
        candidate_digest: pending.candidate_digest.clone(),
    }
}

fn candidate_settlement_state(
    candidate: &notify::NewVersionDiscoveredService,
) -> (&'static str, Option<String>, Option<String>) {
    if let Some(version) = resolved_candidate_version(candidate) {
        return (
            "ready",
            Some(version),
            Some("digest_bound_version".to_string()),
        );
    }
    if candidate.candidate_tag.trim().is_empty() || candidate.candidate_digest.trim().is_empty() {
        return (
            "unresolved",
            None,
            Some("missing_candidate_evidence".to_string()),
        );
    }
    (
        "awaiting_inference",
        None,
        Some("version_inference_pending".to_string()),
    )
}

fn candidate_id_for(service_id: &str, digest: &str) -> String {
    format!("{service_id}:{digest}")
}

fn candidate_from_row(row: &AutoUpdateCandidateRow) -> notify::NewVersionDiscoveredService {
    notify::NewVersionDiscoveredService {
        stack_id: row.stack_id.clone(),
        service_id: row.service_id.clone(),
        image_ref: row.image_ref.clone(),
        current_tag: row.current_tag.clone(),
        current_digest: row.current_digest.clone(),
        current_display_tag: row.current_display_tag.clone(),
        candidate_tag: row.raw_tag.clone(),
        candidate_display_tag: row
            .resolved_version
            .clone()
            .unwrap_or_else(|| row.raw_tag.clone()),
        candidate_digest: row.candidate_digest.clone(),
    }
}

fn resolved_version_from_snapshot(
    snapshot: &crate::api::types::ServiceDigestTagsSnapshotResponse,
    raw_tag: &str,
) -> Option<String> {
    let versions = snapshot
        .tags
        .iter()
        .filter(|tag| tag.trim() != raw_tag.trim())
        .filter_map(|tag| {
            dockrev_common::normalized_semver_from_oci_version(tag).and_then(|version| {
                Version::parse(&version)
                    .ok()
                    .map(|parsed| (parsed, version))
            })
        });
    versions
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(_, version)| version)
}

fn snapshot_is_authoritative(
    snapshot: &crate::api::types::ServiceDigestTagsSnapshotResponse,
) -> bool {
    snapshot.scan.repo_tags_considered >= snapshot.scan.repo_tags_total
        && snapshot.scan.manifests_timeout == 0
        && snapshot.scan.manifests_error == 0
}

fn retry_at_for_attempt(now: &str, attempts: u32) -> Option<String> {
    let seconds = match attempts {
        1 => 60,
        2 => 300,
        3 => 600,
        _ => return None,
    };
    let retry_at = add_seconds(now, seconds);
    (retry_at != now).then_some(retry_at)
}

fn inference_attempt_is_terminal(attempts: u32) -> bool {
    attempts > 3
}

pub async fn reconcile_inference_for_digest(
    state: &Arc<AppState>,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    now: &str,
) -> anyhow::Result<usize> {
    let candidates = state
        .db
        .list_auto_update_candidates_for_digest(image_repo, digest)
        .await?;
    if candidates.is_empty() {
        return Ok(0);
    }
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await?
        .and_then(|(snapshot_json, _checked_at, _updated_at)| {
            serde_json::from_str::<crate::api::types::ServiceDigestTagsSnapshotResponse>(
                &snapshot_json,
            )
            .ok()
        });

    let mut reconciled = 0;
    for candidate in candidates {
        let snapshot_ready = snapshot.as_ref().is_some_and(|snapshot| {
            snapshot_is_authoritative(snapshot)
                && crate::snapshot_worker::normalize_digest(&snapshot.digest)
                    == crate::snapshot_worker::normalize_digest(&candidate.candidate_digest)
        });
        let mut resolved = snapshot_ready
            .then(|| {
                snapshot.as_ref().and_then(|snapshot| {
                    resolved_version_from_snapshot(snapshot, &candidate.raw_tag)
                })
            })
            .flatten();
        if snapshot_ready
            && resolved.is_none()
            && let Ok(image) = crate::registry::ImageRef::parse(&format!(
                "{}@{}",
                image_repo.trim(),
                candidate.candidate_digest.trim()
            ))
        {
            resolved = match state
                .registry
                .get_oci_version(&image, &candidate.candidate_digest, host_platform)
                .await
            {
                Ok(raw) => {
                    raw.and_then(|raw| dockrev_common::normalized_semver_from_oci_version(&raw))
                }
                Err(error) => {
                    tracing::debug!(
                        image_repo,
                        digest = %candidate.candidate_digest,
                        error = %error,
                        "auto update candidate OCI version lookup failed"
                    );
                    None
                }
            };
        }
        let attempts = if resolved.is_some() {
            candidate.attempts
        } else {
            candidate.attempts.saturating_add(1)
        };
        let terminal = resolved.is_none() && inference_attempt_is_terminal(attempts);
        let status = if resolved.is_some() {
            "ready"
        } else if terminal {
            "unresolved"
        } else {
            "awaiting_inference"
        };
        let reason = resolved
            .as_ref()
            .map(|_| "digest_bound_version")
            .or_else(|| terminal.then_some("version_inference_unresolved"));
        let retry_at = (!terminal && resolved.is_none())
            .then(|| retry_at_for_attempt(now, attempts))
            .flatten();
        let settled = state
            .db
            .settle_auto_update_candidate(
                &candidate.service_id,
                &candidate.candidate_digest,
                status,
                resolved.as_deref(),
                reason,
                attempts,
                retry_at.as_deref(),
                (resolved.is_some() || terminal).then_some(now),
                now,
            )
            .await?;
        let Some(settled) = settled else { continue };
        state
            .db
            .management_events()
            .publish_change(
                "auto_update",
                "candidate",
                settled.id.clone(),
                json!({
                    "phase": "settled",
                    "status": settled.status,
                    "serviceId": settled.service_id,
                    "candidateDigest": settled.candidate_digest,
                }),
            )
            .await;
        if status == "ready" {
            evaluate_candidate(
                state,
                &candidate.source_job_id,
                now,
                &candidate_from_row(&settled),
            )
            .await?;
        }
        reconciled += 1;
    }
    Ok(reconciled)
}

pub async fn reconcile_pending_inference(
    state: &Arc<AppState>,
    now: &str,
) -> anyhow::Result<usize> {
    let candidates = state
        .db
        .list_auto_update_candidates_for_retry(now, 50)
        .await?;
    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    let mut reconciled = 0;
    for candidate in candidates {
        let Some(image_repo) =
            crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)
        else {
            continue;
        };
        reconciled += reconcile_inference_for_digest(
            state,
            &image_repo,
            &candidate.candidate_digest,
            &host_platform,
            now,
        )
        .await?;
    }
    Ok(reconciled)
}

pub async fn reevaluate_service_policy(
    state: &Arc<AppState>,
    service_id: &str,
    now: &str,
) -> anyhow::Result<()> {
    let rows = state
        .db
        .list_latest_auto_update_candidates(&[service_id.to_string()])
        .await?;
    let Some(candidate) = rows.into_iter().next() else {
        return Ok(());
    };
    match candidate.status.as_str() {
        "ready" => {
            evaluate_candidate(
                state,
                &candidate.source_job_id,
                now,
                &candidate_from_row(&candidate),
            )
            .await?;
        }
        "awaiting_inference" => {
            state
                .db
                .set_auto_update_candidate_policy(
                    service_id,
                    &candidate.candidate_digest,
                    "waiting_inference",
                    Some("version_inference_pending"),
                    None,
                    now,
                )
                .await?;
        }
        "unresolved" => {
            state
                .db
                .set_auto_update_candidate_policy(
                    service_id,
                    &candidate.candidate_digest,
                    "skipped",
                    Some("version_unresolved"),
                    None,
                    now,
                )
                .await?;
        }
        _ => {}
    }
    Ok(())
}

async fn pending_delay_gates_met(
    state: &Arc<AppState>,
    pending: &AutoUpdatePendingRow,
    now: &str,
) -> anyhow::Result<bool> {
    let Some(effective) =
        effective_policy_for_service(state.as_ref(), &pending.stack_id, &pending.service_id)
            .await?
    else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "policy_disabled", now)
            .await?;
        return Ok(false);
    };
    if effective.scope_type != pending.policy_scope_type
        || effective.scope_id != pending.policy_scope_id
    {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "policy_changed", now)
            .await?;
        return Ok(false);
    }

    let Some(rule) = effective
        .policy
        .rules
        .iter()
        .find(|rule| rule.id == pending.rule_id && rule.enabled)
    else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "rule_disabled", now)
            .await?;
        return Ok(false);
    };

    let candidate = pending_candidate(pending);
    let Some(settlement) = state
        .db
        .get_auto_update_candidate(&pending.service_id, &pending.candidate_digest)
        .await?
    else {
        return Ok(false);
    };
    let requires_resolved_version = matches!(rule.matcher.kind, AutoUpdateMatcherType::Semver);
    if settlement.status == "superseded"
        || (requires_resolved_version && settlement.status != "ready")
        || !rule_matches_candidate(rule, &candidate, settlement.resolved_version.as_deref())
    {
        state
            .db
            .mark_auto_update_pending_skipped(
                &pending.id,
                if settlement.status == "superseded" {
                    "candidate_superseded"
                } else if settlement.status == "unresolved" {
                    "candidate_unresolved"
                } else if requires_resolved_version {
                    "waiting_inference"
                } else {
                    "candidate_rule_no_longer_matches"
                },
                now,
            )
            .await?;
        return Ok(false);
    }

    let (min_age_seconds, min_version_lag) = rule_delay(rule);
    let due_at = add_seconds(&pending.first_seen_at, min_age_seconds);
    let time_met = parse_rfc3339(&due_at)
        .zip(parse_rfc3339(now))
        .is_some_and(|(due, now)| now >= due);
    if !time_met {
        return Ok(false);
    }

    let history = state
        .db
        .list_new_version_discoveries_for_services(std::slice::from_ref(&pending.service_id))
        .await
        .context("load auto update pending version discovery history")?;
    Ok(version_lag_met(
        min_version_lag,
        &candidate.current_display_tag,
        &candidate,
        rule,
        &history,
    ))
}

fn build_auto_update_target(service: &crate::api::types::Service) -> Option<UpdateServiceTarget> {
    let candidate = service.candidate.as_ref()?;
    let mut pull_tags = Vec::new();
    if let Some(resolved) = candidate.resolved_tag.as_deref()
        && ignore::is_strict_semver(resolved)
        && resolved.trim() != service.image.tag.trim()
    {
        pull_tags.push(resolved.trim().to_string());
    }
    Some(UpdateServiceTarget {
        service_id: service.id.clone(),
        target_tag: service.image.tag.clone(),
        target_digest: candidate.digest.clone(),
        pull_tags: Some(pull_tags),
        skip_tag_followups: false,
    })
}

fn permanent_enqueue_error(error: &ApiError) -> bool {
    if error.code() == "compose_v2_required" {
        return false;
    }
    if error.code() != "conflict" {
        return matches!(error.code(), "invalid_argument" | "not_found");
    }

    !matches!(
        error.detail_str("reason"),
        Some(
            "rollback_in_progress"
                | "service_lifecycle_in_progress"
                | "stack_lifecycle_in_progress"
                | "service_update_in_progress"
                | "stack_update_in_progress"
                | "global_update_in_progress"
                | "service_operation_in_progress"
        )
    )
}

async fn enqueue_pending(
    state: &Arc<AppState>,
    pending: &AutoUpdatePendingRow,
    now: &str,
) -> anyhow::Result<Option<String>> {
    let Some(stack) = state.db.get_stack(&pending.stack_id).await? else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "stack_not_found", now)
            .await?;
        return Ok(None);
    };
    if stack.archived {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "stack_archived", now)
            .await?;
        return Ok(None);
    }
    let Some(service) = stack
        .services
        .iter()
        .find(|service| service.id == pending.service_id)
    else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "service_not_found", now)
            .await?;
        return Ok(None);
    };
    if service.archived.unwrap_or(false) {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "service_archived", now)
            .await?;
        return Ok(None);
    }
    if service.ignore.as_ref().is_some_and(|ignore| ignore.matched) {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "service_ignored", now)
            .await?;
        return Ok(None);
    }
    if crate::updater::is_dockrev_image_ref(
        &service.image.reference,
        Some(&state.config.dockrev_image_repo),
    ) {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "dockrev_self_update", now)
            .await?;
        return Ok(None);
    }
    let Some(candidate) = service.candidate.as_ref() else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "candidate_missing", now)
            .await?;
        return Ok(None);
    };
    if api::normalize_digest_for_compare(&candidate.digest)
        != api::normalize_digest_for_compare(&pending.candidate_digest)
    {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "candidate_changed", now)
            .await?;
        return Ok(None);
    }

    let Some(target) = build_auto_update_target(service) else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "target_unavailable", now)
            .await?;
        return Ok(None);
    };

    if !state
        .db
        .try_claim_auto_update_pending(&pending.id, now)
        .await?
    {
        return Ok(None);
    }

    let req = TriggerUpdateRequest {
        scope: JobScope::Service,
        stack_id: Some(pending.stack_id.clone()),
        service_id: Some(pending.service_id.clone()),
        target_tag: None,
        target_digest: None,
        pull_tags: None,
        targets: Some(vec![target]),
        mode: UpdateMode::Apply,
        allow_arch_mismatch: false,
        backup_mode: BackupMode::Inherit,
        reason: UpdateReason::AutoPolicy,
    };

    match api::enqueue_update_job(
        state.clone(),
        "auto-policy".to_string(),
        "auto_policy".to_string(),
        req,
        now.to_string(),
    )
    .await
    {
        Ok(job_id) => {
            state
                .db
                .mark_auto_update_pending_enqueued(&pending.id, &job_id, now)
                .await?;
            Ok(Some(job_id))
        }
        Err(err) => {
            if permanent_enqueue_error(&err) {
                let skip_reason = format!("enqueue_rejected_{}", err.code());
                state
                    .db
                    .mark_auto_update_pending_skipped(&pending.id, &skip_reason, now)
                    .await?;
                return Ok(None);
            }
            state
                .db
                .release_auto_update_pending_claim(&pending.id, now)
                .await?;
            Err(anyhow::anyhow!(
                "enqueue auto policy update failed: {err:?}"
            ))
        }
    }
}

async fn evaluate_candidate(
    state: &Arc<AppState>,
    job_id: &str,
    finished_at: &str,
    candidate: &notify::NewVersionDiscoveredService,
) -> anyhow::Result<()> {
    let (settlement_status, resolved_version, settlement_reason) =
        candidate_settlement_state(candidate);
    let candidate_row = state
        .db
        .upsert_auto_update_candidate(
            &AutoUpdateCandidateInput {
                id: candidate_id_for(&candidate.service_id, &candidate.candidate_digest),
                stack_id: candidate.stack_id.clone(),
                service_id: candidate.service_id.clone(),
                image_ref: crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)
                    .unwrap_or_else(|| candidate.image_ref.clone()),
                raw_tag: candidate.candidate_tag.clone(),
                candidate_digest: candidate.candidate_digest.clone(),
                resolved_version: resolved_version.clone(),
                status: settlement_status.to_string(),
                reason: settlement_reason.clone(),
                attempts: 0,
                retry_at: None,
                discovered_at: finished_at.to_string(),
                source_job_id: job_id.to_string(),
                current_tag: candidate.current_tag.clone(),
                current_display_tag: candidate.current_display_tag.clone(),
                current_digest: candidate.current_digest.clone(),
            },
            finished_at,
        )
        .await?;
    state
        .db
        .supersede_auto_update_candidates(
            &candidate.service_id,
            &candidate.candidate_digest,
            &candidate_row.discovered_at,
            &candidate_row.id,
            finished_at,
        )
        .await?;
    state
        .db
        .management_events()
        .publish_change(
            "auto_update",
            "candidate",
            candidate_row.id.clone(),
            json!({
                "phase": "settled",
                "status": candidate_row.status,
                "serviceId": candidate.service_id,
                "candidateDigest": candidate.candidate_digest,
            }),
        )
        .await;

    if candidate_row.status == "superseded" {
        return Ok(());
    }

    // The database row is authoritative when a duplicate discovery carries less evidence than an
    // earlier settlement. This prevents a later `latest` observation from downgrading a ready
    // digest-bound candidate back to waiting.
    let settlement_status = candidate_row.status.as_str();
    let resolved_version = candidate_row.resolved_version.clone();
    let settlement_reason = candidate_row.reason.clone();
    // Rebuild the candidate from the settled row before evaluating policy. A later observation may
    // still carry `latest`, while the canonical row already contains the digest-bound version.
    // Delay and version-lag gates must use that same evidence as the matcher.
    let candidate = candidate_from_row(&candidate_row);
    let Some(effective) =
        effective_policy_for_service(state.as_ref(), &candidate.stack_id, &candidate.service_id)
            .await?
    else {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "skipped",
                Some("policy_disabled"),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    };
    let matched = effective
        .policy
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .find(|rule| rule_matches_candidate(rule, &candidate, resolved_version.as_deref()))
        .cloned()
        .map(|rule| MatchedRule { rule });
    let matched = if let Some(matched) = matched {
        matched
    } else if settlement_status == "awaiting_inference"
        && has_waiting_semver_rule(&effective.policy)
    {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "waiting_inference",
                settlement_reason.as_deref(),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    } else if settlement_status == "unresolved" && has_waiting_semver_rule(&effective.policy) {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "skipped",
                Some("version_unresolved"),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    } else {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "rule_not_matched",
                Some("no_enabled_rule_match"),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    };
    state
        .db
        .set_auto_update_candidate_policy(
            &candidate.service_id,
            &candidate.candidate_digest,
            "delayed",
            Some("policy_matched"),
            Some(&matched.rule.id),
            finished_at,
        )
        .await?;
    let (min_age_seconds, min_version_lag) = rule_delay(&matched.rule);
    let discovered_at = candidate_row.discovered_at.clone();
    let due_at = add_seconds(&discovered_at, min_age_seconds);
    let pending = state
        .db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: ids::new_auto_update_pending_id(),
                policy_scope_type: effective.scope_type.to_string(),
                policy_scope_id: effective.scope_id,
                rule_id: matched.rule.id.clone(),
                stack_id: candidate.stack_id.clone(),
                service_id: candidate.service_id.clone(),
                source_check_job_id: job_id.to_string(),
                candidate_tag: candidate.candidate_tag.clone(),
                candidate_display_tag: candidate.candidate_display_tag.clone(),
                candidate_digest: candidate.candidate_digest.clone(),
                current_display_tag: candidate.current_display_tag.clone(),
                first_seen_at: discovered_at,
                due_at,
                min_age_seconds,
                min_version_lag,
                summary_json: json!({
                    "imageRef": candidate.image_ref,
                    "currentTag": candidate.current_tag,
                    "currentDigest": candidate.current_digest,
                    "candidateTag": candidate.candidate_tag,
                    "candidateDisplayTag": candidate.candidate_display_tag,
                    "candidateDigest": candidate.candidate_digest,
                    "currentDisplayTag": candidate.current_display_tag,
                    "ruleId": matched.rule.id,
                    "policyScopeType": effective.scope_type,
                    "sourceCheckJobId": job_id,
                }),
                candidate_id: Some(candidate_row.id),
            },
            finished_at,
        )
        .await?;

    if pending.status == "pending" && pending_delay_gates_met(state, &pending, finished_at).await? {
        let _ = enqueue_pending(state, &pending, finished_at).await?;
    }
    Ok(())
}

fn auto_policy_source(reason: &str, summary: &serde_json::Value) -> bool {
    reason.eq_ignore_ascii_case("schedule") || api::summary_emits_new_version_notification(summary)
}

pub async fn handle_completed_check(
    state: &Arc<AppState>,
    job_id: &str,
    reason: &str,
    finished_at: &str,
    summary: &serde_json::Value,
) -> anyhow::Result<()> {
    if !auto_policy_source(reason, summary) {
        return Ok(());
    }
    let mut discovered_services = notify::extract_new_versions_discovered(summary);
    if api::summary_emits_new_version_notification(summary)
        && let Some(matched_service_ids) = api::summary_matched_service_ids(summary)
    {
        discovered_services.retain(|service| matched_service_ids.contains(&service.service_id));
    }
    if discovered_services.is_empty() {
        return Ok(());
    }

    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    for candidate in &discovered_services {
        evaluate_candidate(state, job_id, finished_at, candidate).await?;
        if let Some(image_repo) =
            crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)
        {
            reconcile_inference_for_digest(
                state,
                &image_repo,
                &candidate.candidate_digest,
                &host_platform,
                finished_at,
            )
            .await?;
        }
    }
    process_due_pending(state, finished_at, 50).await?;
    Ok(())
}

pub async fn process_due_pending(
    state: &Arc<AppState>,
    now: &str,
    limit: usize,
) -> anyhow::Result<usize> {
    let due = state
        .db
        .list_auto_update_pending_candidates(now, limit)
        .await?;
    let mut enqueued = 0usize;
    for pending in due {
        if !pending_delay_gates_met(state, &pending, now).await? {
            continue;
        }
        if enqueue_pending(state, &pending, now).await?.is_some() {
            enqueued += 1;
        }
    }
    Ok(enqueued)
}

fn candidate_event_targets(
    candidates: &[AutoUpdateCandidateRow],
    host_platform: &str,
) -> (Vec<String>, HashMap<String, (String, String)>) {
    let mut targets = HashMap::new();
    let keys = candidates
        .iter()
        .filter_map(|candidate| {
            let image_repo =
                crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)?;
            let key = crate::snapshot_worker::snapshot_task_key(
                &image_repo,
                &candidate.candidate_digest,
                host_platform,
            )?;
            targets.insert(
                key.clone(),
                (image_repo, candidate.candidate_digest.clone()),
            );
            Some(key)
        })
        .collect::<Vec<_>>();
    (keys, targets)
}

pub fn spawn_tasks(state: Arc<AppState>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(PENDING_POLL_INTERVAL_SECONDS);
        loop {
            let now = match time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
            {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error = %error, "auto update policy scheduler: clock unavailable");
                    continue;
                }
            };
            if let Err(error) = process_due_pending(&state, &now, 50).await {
                tracing::warn!(error = %error, "auto update policy scheduler failed");
            }
            if let Err(error) = reconcile_pending_inference(&state, &now).await {
                tracing::warn!(error = %error, "auto update candidate reconciliation failed");
            }

            let after_id = state.snapshot_worker.latest_event_id().await;
            let host_platform =
                crate::registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());
            let (candidate_keys, candidate_targets) = match state
                .db
                .list_auto_update_candidates_for_events(50)
                .await
            {
                Ok(candidates) => candidate_event_targets(&candidates, &host_platform),
                Err(error) => {
                    tracing::debug!(error = %error, "auto update candidate event wait list failed");
                    (Vec::new(), HashMap::new())
                }
            };
            if candidate_keys.is_empty() {
                tokio::time::sleep(interval).await;
            } else {
                let outcomes = state
                    .snapshot_worker
                    .wait_for_task_finished_keys_since(after_id, &candidate_keys, interval)
                    .await;
                if outcomes.keys().next().is_some() {
                    for key in outcomes.keys() {
                        let Some((image_repo, digest)) = candidate_targets.get(key) else {
                            continue;
                        };
                        if let Err(error) = reconcile_inference_for_digest(
                            &state,
                            image_repo,
                            digest,
                            &host_platform,
                            &now,
                        )
                        .await
                        {
                            tracing::warn!(
                                image_repo,
                                digest,
                                error = %error,
                                "auto update candidate settlement after snapshot event failed"
                            );
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{AutoUpdateDelay, AutoUpdateMatcher};

    fn rule(kind: AutoUpdateMatcherType, pattern: &str) -> AutoUpdateRule {
        AutoUpdateRule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            enabled: true,
            matcher: AutoUpdateMatcher {
                kind,
                pattern: pattern.to_string(),
            },
            action: AutoUpdateRuleAction::Delayed,
            delay: AutoUpdateDelay {
                min_age_seconds: 900,
                min_version_lag: 2,
            },
        }
    }

    #[test]
    fn matches_semver_regex_and_glob() {
        assert!(rule_matches_text(
            &rule(AutoUpdateMatcherType::Semver, ">=1.2, <2"),
            "1.4.0"
        ));
        assert!(!rule_matches_text(
            &rule(AutoUpdateMatcherType::Semver, ">=1.2, <2"),
            "2.0.0"
        ));
        assert!(rule_matches_text(
            &rule(AutoUpdateMatcherType::Regex, r"1\.4\.[0-9]+"),
            "1.4.7"
        ));
        assert!(rule_matches_text(
            &rule(AutoUpdateMatcherType::Glob, "1.4.*-alpine"),
            "1.4.7-alpine"
        ));
    }

    #[test]
    fn semver_is_fail_closed_until_digest_bound_version_exists() {
        let candidate = notify::NewVersionDiscoveredService {
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            current_tag: "latest".to_string(),
            current_digest: Some("sha256:old".to_string()),
            current_display_tag: "1.0.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "latest".to_string(),
            candidate_digest: "sha256:new".to_string(),
        };
        let semver = rule(AutoUpdateMatcherType::Semver, ">=1, <2");
        assert!(!rule_matches_candidate(&semver, &candidate, None));
        assert!(rule_matches_candidate(&semver, &candidate, Some("1.4.0")));

        let regex = rule(AutoUpdateMatcherType::Regex, "latest");
        assert!(rule_matches_candidate(&regex, &candidate, None));
    }

    #[test]
    fn candidate_settlement_requires_a_strict_or_digest_bound_version() {
        let mut candidate = notify::NewVersionDiscoveredService {
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            current_tag: "latest".to_string(),
            current_digest: Some("sha256:old".to_string()),
            current_display_tag: "1.0.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "latest".to_string(),
            candidate_digest: "sha256:new".to_string(),
        };
        assert_eq!(
            candidate_settlement_state(&candidate).0,
            "awaiting_inference"
        );

        candidate.candidate_display_tag = "1.4.0".to_string();
        assert_eq!(
            candidate_settlement_state(&candidate).0,
            "awaiting_inference"
        );

        candidate.candidate_tag = "1.5.0".to_string();
        candidate.candidate_display_tag = "1.5.0".to_string();
        assert_eq!(candidate_settlement_state(&candidate).0, "ready");
    }

    #[test]
    fn digest_snapshot_resolution_uses_the_highest_bound_semver_tag() {
        let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
            digest: "sha256:new".to_string(),
            tags: vec![
                "latest".to_string(),
                "1.2.0".to_string(),
                "v1.4.0".to_string(),
                "release-notes".to_string(),
            ],
            checked_at: "2026-04-30T00:00:00Z".to_string(),
            scan: crate::api::types::ServiceDigestTagsScanSummary {
                repo_tags_total: 4,
                repo_tags_considered: 4,
                manifests_ok: 4,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        };
        assert_eq!(
            resolved_version_from_snapshot(&snapshot, "latest").as_deref(),
            Some("1.4.0")
        );
    }

    #[test]
    fn inference_retry_schedule_is_one_five_and_ten_minutes() {
        let now = "2026-04-30T00:00:00Z";
        assert_eq!(
            retry_at_for_attempt(now, 1).as_deref(),
            Some("2026-04-30T00:01:00Z")
        );
        assert_eq!(
            retry_at_for_attempt(now, 2).as_deref(),
            Some("2026-04-30T00:05:00Z")
        );
        assert_eq!(
            retry_at_for_attempt(now, 3).as_deref(),
            Some("2026-04-30T00:10:00Z")
        );
        assert_eq!(retry_at_for_attempt(now, 4), None);
    }

    #[test]
    fn inference_attempts_wait_through_the_third_backoff_before_terminal_state() {
        assert!(!inference_attempt_is_terminal(1));
        assert!(!inference_attempt_is_terminal(2));
        assert!(!inference_attempt_is_terminal(3));
        assert!(inference_attempt_is_terminal(4));
    }

    #[test]
    fn non_semver_snapshot_tags_do_not_settle_a_candidate() {
        let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
            digest: "sha256:new".to_string(),
            tags: vec!["latest".to_string(), "15-alpine".to_string()],
            checked_at: "2026-04-30T00:00:00Z".to_string(),
            scan: crate::api::types::ServiceDigestTagsScanSummary {
                repo_tags_total: 2,
                repo_tags_considered: 2,
                manifests_ok: 2,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        };
        assert!(snapshot_is_authoritative(&snapshot));
        assert_eq!(resolved_version_from_snapshot(&snapshot, "latest"), None);
    }

    #[test]
    fn incomplete_snapshot_cannot_be_used_as_version_evidence() {
        let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
            digest: "sha256:new".to_string(),
            tags: vec!["latest".to_string(), "1.4.0".to_string()],
            checked_at: "2026-04-30T00:00:00Z".to_string(),
            scan: crate::api::types::ServiceDigestTagsScanSummary {
                repo_tags_total: 4,
                repo_tags_considered: 2,
                manifests_ok: 2,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        };
        assert!(!snapshot_is_authoritative(&snapshot));
    }

    #[test]
    fn rejects_non_slider_presets() {
        let mut policy = AutoUpdatePolicy {
            mode: AutoUpdatePolicyMode::Override,
            enabled: true,
            rules: vec![rule(AutoUpdateMatcherType::Semver, ">=1")],
            updated_at: None,
        };
        assert!(validate_policy_for_scope(&policy, "stack").is_ok());
        policy.rules[0].delay.min_age_seconds = 901;
        assert!(validate_policy_for_scope(&policy, "stack").is_err());
    }

    #[test]
    fn keeps_auto_update_pending_for_transient_service_operation_conflicts() {
        let conflict = ApiError::conflict("service operation in progress").with_details(json!({
            "reason": "service_lifecycle_in_progress",
            "existingJobId": "job-lifecycle-1",
        }));
        assert!(!permanent_enqueue_error(&conflict));

        let stack_conflict =
            ApiError::conflict("service operation in progress").with_details(json!({
                "reason": "stack_lifecycle_in_progress",
                "existingJobId": "job-stack-lifecycle-1",
            }));
        assert!(!permanent_enqueue_error(&stack_conflict));

        let stale_candidate = ApiError::conflict("target digest no longer matches latest scan");
        assert!(permanent_enqueue_error(&stale_candidate));
    }

    #[test]
    fn keeps_auto_update_pending_for_compose_v2_capability_failures() {
        let capability_failure = ApiError::compose_v2_required("docker-compose", "v1");
        assert!(!permanent_enqueue_error(&capability_failure));
    }

    #[test]
    fn delayed_version_lag_requires_matching_versions() {
        let candidate = notify::NewVersionDiscoveredService {
            stack_id: "stack".to_string(),
            service_id: "svc".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            current_tag: "latest".to_string(),
            current_digest: Some("sha256:old".to_string()),
            current_display_tag: "1.0.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.2.0".to_string(),
            candidate_digest: "sha256:new".to_string(),
        };
        let history = vec![NewVersionDiscoveryRow {
            service_id: "svc".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            discovered_at: "2026-04-30T00:00:00Z".to_string(),
            current_digest: "sha256:old".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_tag: "latest".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_digest: "sha256:mid".to_string(),
            candidate_display_tag: "1.1.0".to_string(),
        }];
        let rule = rule(AutoUpdateMatcherType::Semver, ">=1, <2");
        assert!(version_lag_met(
            2,
            &candidate.current_display_tag,
            &candidate,
            &rule,
            &history
        ));
        assert!(!version_lag_met(
            3,
            &candidate.current_display_tag,
            &candidate,
            &rule,
            &history
        ));
    }

    #[test]
    fn delayed_version_lag_uses_the_canonical_resolved_version_for_floating_tags() {
        let candidate = candidate_from_row(&AutoUpdateCandidateRow {
            id: "svc:sha256:new".to_string(),
            stack_id: "stack".to_string(),
            service_id: "svc".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            raw_tag: "latest".to_string(),
            candidate_digest: "sha256:new".to_string(),
            resolved_version: Some("1.2.0".to_string()),
            status: "ready".to_string(),
            reason: Some("digest_bound_version".to_string()),
            attempts: 1,
            retry_at: None,
            discovered_at: "2026-04-30T00:00:00Z".to_string(),
            source_job_id: "check".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
            settled_at: Some("2026-04-30T00:01:00Z".to_string()),
            updated_at: "2026-04-30T00:01:00Z".to_string(),
            policy_status: Some("delayed".to_string()),
            policy_reason: Some("policy_matched".to_string()),
            policy_rule_id: Some("r1".to_string()),
            policy_evaluated_at: Some("2026-04-30T00:01:00Z".to_string()),
        });
        let history = vec![NewVersionDiscoveryRow {
            service_id: "svc".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            discovered_at: "2026-04-30T00:00:00Z".to_string(),
            current_digest: "sha256:old".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_tag: "latest".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_digest: "sha256:mid".to_string(),
            candidate_display_tag: "1.1.0".to_string(),
        }];
        let rule = rule(AutoUpdateMatcherType::Semver, ">=1, <2");

        assert_eq!(candidate.candidate_display_tag, "1.2.0");
        assert!(version_lag_met(
            2,
            &candidate.current_display_tag,
            &candidate,
            &rule,
            &history
        ));
    }

    #[test]
    fn candidate_event_targets_are_keyed_by_the_finished_task() {
        let candidate = |service_id: &str, image_ref: &str, digest: &str| AutoUpdateCandidateRow {
            id: format!("{service_id}:{digest}"),
            stack_id: "stack".to_string(),
            service_id: service_id.to_string(),
            image_ref: image_ref.to_string(),
            raw_tag: "latest".to_string(),
            candidate_digest: digest.to_string(),
            resolved_version: None,
            status: "awaiting_inference".to_string(),
            reason: Some("version_inference_pending".to_string()),
            attempts: 0,
            retry_at: None,
            discovered_at: "2026-04-30T00:00:00Z".to_string(),
            source_job_id: "check".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
            settled_at: None,
            updated_at: "2026-04-30T00:00:00Z".to_string(),
            policy_status: Some("waiting_inference".to_string()),
            policy_reason: None,
            policy_rule_id: None,
            policy_evaluated_at: None,
        };
        let digest_a = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let digest_b = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let candidates = vec![
            candidate("service-a", "ghcr.io/acme/a:latest", digest_a),
            candidate("service-b", "ghcr.io/acme/b:latest", digest_b),
        ];
        let (keys, targets) = candidate_event_targets(&candidates, "linux/amd64");
        let key_a =
            crate::snapshot_worker::snapshot_task_key("ghcr.io/acme/a", digest_a, "linux/amd64")
                .unwrap();
        let key_b =
            crate::snapshot_worker::snapshot_task_key("ghcr.io/acme/b", digest_b, "linux/amd64")
                .unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(
            targets.get(&key_a),
            Some(&("ghcr.io/acme/a".to_string(), digest_a.to_string()))
        );
        assert_eq!(
            targets.get(&key_b),
            Some(&("ghcr.io/acme/b".to_string(), digest_b.to_string()))
        );
        assert_eq!(targets.get("unrelated"), None);
    }
}
