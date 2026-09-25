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
        AutoUpdateJobContext, AutoUpdateMatcherType, AutoUpdatePolicy, AutoUpdatePolicyMode,
        AutoUpdateRule, AutoUpdateRuleAction, BackupMode, JobScope, TriggerUpdateRequest,
        UpdateMode, UpdateReason, UpdateServiceTarget,
    },
    db::{
        AutoUpdateCandidateInput, AutoUpdateCandidateRow, AutoUpdateCandidateSettlementInput,
        AutoUpdatePendingInput, AutoUpdatePendingRow, NewVersionDiscoveryRow,
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
    resolved_tags: Option<&[String]>,
) -> bool {
    match rule.matcher.kind {
        AutoUpdateMatcherType::Semver => {
            resolved_version.is_some_and(|version| rule_matches_text(rule, version))
        }
        AutoUpdateMatcherType::Regex | AutoUpdateMatcherType::Glob => {
            candidate_match_values(candidate, resolved_tags)
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
            if !service_policy.enabled {
                return Ok(None);
            }
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

fn subtract_seconds(ts: &str, seconds: u32) -> String {
    parse_rfc3339(ts)
        .map(|value| value - time::Duration::seconds(seconds as i64))
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
    resolved_tags: Option<&[String]>,
) -> bool {
    if min_version_lag == 0 {
        return true;
    }
    let Some(current_version) = ignore::parse_version(current_display_tag) else {
        return false;
    };

    let mut versions = BTreeSet::<semver::Version>::new();
    for value in candidate_match_values(candidate, resolved_tags) {
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

fn candidate_digest_is_valid(candidate_digest: &str) -> bool {
    crate::snapshot_worker::normalize_digest_identity(candidate_digest).is_some()
}

fn candidate_settlement_state(
    candidate: &notify::NewVersionDiscoveredService,
) -> (&'static str, Option<String>, Option<String>) {
    if candidate.candidate_digest.trim().is_empty() {
        return (
            "unresolved",
            None,
            Some("missing_candidate_evidence".to_string()),
        );
    }
    if !candidate_digest_is_valid(&candidate.candidate_digest) {
        return (
            "unresolved",
            None,
            Some("invalid_candidate_digest".to_string()),
        );
    }
    if let Some(version) = resolved_candidate_version(candidate) {
        return (
            "ready",
            Some(version),
            Some("digest_bound_version".to_string()),
        );
    }
    if candidate.candidate_tag.trim().is_empty() {
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

fn update_request_from_job(job: &api::types::JobListItem) -> anyhow::Result<TriggerUpdateRequest> {
    let mode = job
        .summary_json
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("auto policy update job is missing mode"))?;
    let mode = serde_json::from_value::<UpdateMode>(serde_json::Value::String(mode.to_string()))
        .context("parse auto policy update mode")?;
    let targets = job
        .summary_json
        .get("targets")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("auto policy update job is missing targets"))
        .and_then(|value| {
            serde_json::from_value::<Vec<UpdateServiceTarget>>(value)
                .context("parse auto policy update targets")
        })?;
    if targets.is_empty() {
        anyhow::bail!("auto policy update job has no targets");
    }
    for target in &targets {
        if target.service_id.trim().is_empty()
            || target.target_tag.trim().is_empty()
            || api::normalize_digest_for_compare(&target.target_digest).is_none()
        {
            anyhow::bail!("auto policy update job has an invalid target");
        }
    }
    if job.scope != api::types::JobScope::Service
        || job.stack_id.as_deref().is_none_or(str::is_empty)
        || job.service_id.as_deref().is_none_or(str::is_empty)
        || targets.len() != 1
        || targets[0].service_id != job.service_id.as_deref().unwrap_or_default()
    {
        anyhow::bail!("auto policy update job has an invalid service scope");
    }
    let backup_mode =
        serde_json::from_value::<BackupMode>(serde_json::Value::String(job.backup_mode.clone()))
            .context("parse auto policy update backup mode")?;
    Ok(TriggerUpdateRequest {
        scope: job.scope.clone(),
        stack_id: job.stack_id.clone(),
        service_id: job.service_id.clone(),
        target_tag: None,
        target_digest: None,
        pull_tags: None,
        targets: Some(targets),
        mode,
        allow_arch_mismatch: job.allow_arch_mismatch,
        backup_mode,
        reason: UpdateReason::AutoPolicy,
    })
}

async fn recover_enqueued_auto_update_jobs(
    state: &Arc<AppState>,
    now: &str,
    limit: usize,
) -> anyhow::Result<usize> {
    let jobs = state.db.list_enqueued_auto_update_jobs(limit).await?;
    let mut recovered = 0;
    for job in jobs {
        let request = match update_request_from_job(&job) {
            Ok(request) => request,
            Err(error) => {
                state
                    .db
                    .fail_corrupt_auto_update_job(&job.id, now, "migration_ambiguous_history")
                    .await?;
                tracing::warn!(job_id = %job.id, error = %error, "auto policy update job recovery skipped invalid job summary");
                continue;
            }
        };
        if !state
            .db
            .claim_queued_job_by_id_for_recovery(&job.id, now)
            .await?
        {
            continue;
        }
        let run_state = state.clone();
        let run_job_id = job.id.clone();
        tokio::spawn(async move {
            let _ = api::run_update_job(run_state, run_job_id, request).await;
        });
        recovered += 1;
    }
    Ok(recovered)
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

fn permanent_inference_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        " 400 ",
        " 401 ",
        " 403 ",
        " 404 ",
        " 405 ",
        " 406 ",
        " 415 ",
        " 422 ",
        "bad request",
        "forbidden",
        "not found",
        "unauthorized",
        "parse manifest json",
        "parse config blob json",
    ]
    .iter()
    .any(|marker| message.contains(marker))
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
        let Some(candidate) = state
            .db
            .begin_auto_update_candidate_inference(
                &candidate.service_id,
                &candidate.candidate_digest,
                now,
            )
            .await?
        else {
            continue;
        };
        let authoritative_snapshot = snapshot.as_ref().filter(|snapshot| {
            snapshot_is_authoritative(snapshot)
                && crate::snapshot_worker::normalize_digest(&snapshot.digest)
                    == crate::snapshot_worker::normalize_digest(&candidate.candidate_digest)
        });
        let resolved_tags = authoritative_snapshot.map(|snapshot| snapshot.tags.clone());
        let mut resolved = authoritative_snapshot
            .and_then(|snapshot| resolved_version_from_snapshot(snapshot, &candidate.raw_tag));
        let mut inference_error = false;
        let mut permanent_error = false;
        let mut inference_reason = None;
        if resolved.is_none()
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
                Ok(raw) => match raw {
                    Some(raw) => match dockrev_common::normalized_semver_from_oci_version(&raw) {
                        Some(version) => Some(version),
                        None => {
                            inference_error = true;
                            permanent_error = true;
                            inference_reason = Some("invalid_oci_version".to_string());
                            None
                        }
                    },
                    None => None,
                },
                Err(error) => {
                    inference_error = true;
                    permanent_error = permanent_inference_error(&error);
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
        let authoritative_without_version =
            authoritative_snapshot.is_some() && resolved.is_none() && !inference_error;
        let attempts = if resolved.is_some() || authoritative_without_version {
            candidate.attempts
        } else {
            candidate.attempts.saturating_add(1)
        };
        let terminal = resolved.is_none()
            && (authoritative_without_version
                || permanent_error
                || inference_attempt_is_terminal(attempts));
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
            .or(inference_reason.as_deref())
            .or_else(|| terminal.then_some("version_inference_unresolved"));
        let last_error = inference_error.then(|| {
            inference_reason
                .clone()
                .unwrap_or_else(|| "version_inference_failed".into())
        });
        let retry_at = (!terminal && resolved.is_none())
            .then(|| retry_at_for_attempt(now, attempts))
            .flatten();
        let settled = state
            .db
            .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
                service_id: candidate.service_id.clone(),
                candidate_digest: candidate.candidate_digest.clone(),
                status: status.to_string(),
                resolved_version: resolved.clone(),
                resolved_tags,
                reason: reason.map(str::to_string),
                last_error,
                attempts,
                // `begin_auto_update_candidate_inference` reserves this exact CAS token.
                evidence_generation: candidate.evidence_generation,
                retry_at,
                settled_at: (resolved.is_some() || terminal).then_some(now.to_string()),
                now: now.to_string(),
            })
            .await?;
        let settled = settled.unwrap_or_else(|| candidate.clone());
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
        if matches!(settled.status.as_str(), "ready" | "unresolved") {
            evaluate_candidate(
                state,
                &candidate.source_job_id,
                &settled.discovered_at,
                now,
                &candidate_from_row(&settled),
                Some(&settled.source),
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
    if candidate.policy_status.as_deref() == Some("completed") {
        return Ok(());
    }
    if !is_qualified_auto_policy_source(Some(&candidate.source))
        || !has_valid_auto_policy_source(
            &state.db,
            &candidate.source_job_id,
            &candidate.source,
            Some(&candidate.service_id),
            Some(&candidate.stack_id),
        )
        .await?
    {
        state
            .db
            .set_auto_update_candidate_policy(
                service_id,
                &candidate.candidate_digest,
                "skipped",
                Some("unqualified_source"),
                None,
                now,
            )
            .await?;
        return Ok(());
    }
    match candidate.status.as_str() {
        "ready" | "awaiting_inference" | "unresolved" => {
            evaluate_candidate(
                state,
                &candidate.source_job_id,
                &candidate.discovered_at,
                now,
                &candidate_from_row(&candidate),
                Some(&candidate.source),
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
    if !effective
        .scope_type
        .trim()
        .eq_ignore_ascii_case(pending.policy_scope_type.trim())
        || !effective
            .scope_id
            .trim()
            .eq_ignore_ascii_case(pending.policy_scope_id.trim())
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
        || !rule_matches_candidate(
            rule,
            &candidate,
            settlement.resolved_version.as_deref(),
            settlement.resolved_tags.as_deref(),
        )
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
        settlement.resolved_tags.as_deref(),
    ))
}
fn build_auto_update_target(
    service: &crate::api::types::Service,
    settled_version: Option<&str>,
) -> Option<UpdateServiceTarget> {
    let candidate = service.candidate.as_ref()?;
    let mut pull_tags = Vec::new();
    if let Some(resolved) = settled_version.or(candidate.resolved_tag.as_deref())
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
        skip_target_tag_pull: false,
        auto_policy_context: None,
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

    let Some(settlement) = state
        .db
        .get_auto_update_candidate(&pending.service_id, &pending.candidate_digest)
        .await?
    else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "candidate_missing", now)
            .await?;
        return Ok(None);
    };
    let Some(mut target) =
        build_auto_update_target(service, settlement.resolved_version.as_deref())
    else {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "target_unavailable", now)
            .await?;
        return Ok(None);
    };
    if !has_valid_auto_policy_source(
        &state.db,
        &pending.source_check_job_id,
        &settlement.source,
        Some(&pending.service_id),
        Some(&pending.stack_id),
    )
    .await?
    {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "unqualified_source", now)
            .await?;
        return Ok(None);
    }
    target.auto_policy_context = Some(AutoUpdateJobContext {
        pending_id: pending.id.clone(),
        candidate_id: settlement.id.clone(),
        rule_id: pending.rule_id.clone(),
        policy_scope_type: pending.policy_scope_type.clone(),
        policy_scope_id: pending.policy_scope_id.clone(),
        expected_current_digest: service.image.digest.clone(),
    });

    if !state
        .db
        .try_claim_auto_update_pending_if_current(
            &pending.id,
            &pending.service_id,
            &pending.candidate_digest,
            &pending.policy_scope_type,
            &pending.policy_scope_id,
            &pending.rule_id,
            now,
        )
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

    match api::enqueue_update_job_deferred(
        state.clone(),
        "auto-policy".to_string(),
        "auto_policy".to_string(),
        req.clone(),
        now.to_string(),
    )
    .await
    {
        Ok(job_id) => {
            let enqueued = state
                .db
                .mark_auto_update_pending_enqueued(&pending.id, &job_id, now)
                .await?;
            if !enqueued {
                return Ok(None);
            }
            let started_at = crate::now_rfc3339().unwrap_or_else(|_| now.to_string());
            if !state
                .db
                .claim_queued_job_by_id(&job_id, &started_at)
                .await?
            {
                return Ok(None);
            }
            let run_state = state.clone();
            let run_job_id = job_id.clone();
            tokio::spawn(async move {
                let _ = api::run_update_job(run_state, run_job_id, req).await;
            });
            Ok(Some(job_id))
        }
        Err(err) => {
            if permanent_enqueue_error(&err) {
                let skip_reason = err
                    .detail_str("reason")
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("enqueue_rejected_{}", err.code()));
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
    discovered_at: &str,
    finished_at: &str,
    candidate: &notify::NewVersionDiscoveredService,
    source: Option<&str>,
) -> anyhow::Result<()> {
    if !candidate_digest_is_valid(&candidate.candidate_digest) {
        tracing::warn!(
            service_id = %candidate.service_id,
            candidate_digest = %candidate.candidate_digest,
            "ignoring auto update candidate with invalid digest identity"
        );
        return Ok(());
    }
    if let Some(source) = source
        && is_qualified_auto_policy_source(Some(source))
        && !has_valid_auto_policy_source(
            &state.db,
            job_id,
            source,
            Some(&candidate.service_id),
            Some(&candidate.stack_id),
        )
        .await?
    {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "skipped",
                Some("unqualified_source"),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    }
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
                discovered_at: discovered_at.to_string(),
                source_job_id: job_id.to_string(),
                source: source.unwrap_or("unknown").to_string(),
                current_tag: candidate.current_tag.clone(),
                current_display_tag: candidate.current_display_tag.clone(),
                current_digest: candidate.current_digest.clone(),
            },
            finished_at,
        )
        .await?;
    if !is_qualified_auto_policy_source(source) {
        state
            .db
            .set_auto_update_candidate_policy(
                &candidate.service_id,
                &candidate.candidate_digest,
                "skipped",
                Some("unqualified_source"),
                None,
                finished_at,
            )
            .await?;
        return Ok(());
    }
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
    let candidate_row = state
        .db
        .get_auto_update_candidate(&candidate.service_id, &candidate.candidate_digest)
        .await?
        .unwrap_or(candidate_row);
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

    let settlement_status = candidate_row.status.as_str();
    let resolved_version = candidate_row.resolved_version.clone();
    let settlement_reason = candidate_row.reason.clone();
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
        .find(|rule| {
            rule_matches_candidate(
                rule,
                &candidate,
                resolved_version.as_deref(),
                candidate_row.resolved_tags.as_deref(),
            )
        })
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
                    "policyUpdatedAt": effective.policy.updated_at,
                    "sourceCheckJobId": job_id,
                }),
                candidate_id: Some(candidate_row.id),
            },
            finished_at,
        )
        .await?;
    state
        .db
        .sync_auto_update_candidate_projection_context(
            &candidate.service_id,
            &candidate.candidate_digest,
        )
        .await?;

    if pending.status == "pending" && pending_delay_gates_met(state, &pending, finished_at).await? {
        let _ = enqueue_pending(state, &pending, finished_at).await?;
    }
    Ok(())
}

pub async fn process_due_pending(
    state: &Arc<AppState>,
    now: &str,
    limit: usize,
) -> anyhow::Result<usize> {
    state.db.hydrate_auto_update_candidates(now).await?;
    let stale_before = subtract_seconds(now, 300);
    state
        .db
        .reconcile_auto_update_pending_claims(&stale_before, now)
        .await?;
    reconcile_auto_update_policy_candidates(state, now).await?;
    let mut enqueued = recover_enqueued_auto_update_jobs(state, now, limit).await?;
    let due = state
        .db
        .list_auto_update_pending_candidates(now, limit)
        .await?;
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

include!("auto_update_check_completion.rs");
include!("auto_update_reconciliation.rs");

#[cfg(test)]
#[path = "auto_update_tests.rs"]
mod tests;
