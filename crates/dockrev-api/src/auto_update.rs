use std::{collections::BTreeSet, sync::Arc, time::Duration};

use anyhow::Context as _;
use regex::Regex;
use semver::VersionReq;
use serde_json::json;

use crate::{
    api,
    api::types::{
        AutoUpdateMatcherType, AutoUpdatePolicy, AutoUpdatePolicyMode, AutoUpdateRule,
        AutoUpdateRuleAction, BackupMode, JobScope, TriggerUpdateRequest, UpdateMode, UpdateReason,
        UpdateServiceTarget,
    },
    db::{AutoUpdatePendingInput, AutoUpdatePendingRow, NewVersionDiscoveryRow},
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

fn match_rule(
    policy: &AutoUpdatePolicy,
    candidate: &notify::NewVersionDiscoveredService,
) -> Option<MatchedRule> {
    if !policy.enabled {
        return None;
    }
    policy.rules.iter().find_map(|rule| {
        if !rule.enabled {
            return None;
        }
        let matched = candidate_match_values(candidate)
            .into_iter()
            .any(|value| rule_matches_text(rule, value));
        matched.then(|| MatchedRule { rule: rule.clone() })
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
    if !candidate_match_values(&candidate)
        .into_iter()
        .any(|value| rule_matches_text(rule, value))
    {
        state
            .db
            .mark_auto_update_pending_skipped(&pending.id, "rule_no_longer_matches", now)
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
    matches!(error.code(), "invalid_argument" | "conflict" | "not_found")
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
    let Some(effective) =
        effective_policy_for_service(state.as_ref(), &candidate.stack_id, &candidate.service_id)
            .await?
    else {
        return Ok(());
    };
    let Some(matched) = match_rule(&effective.policy, candidate) else {
        return Ok(());
    };
    let (min_age_seconds, min_version_lag) = rule_delay(&matched.rule);
    let due_at = add_seconds(finished_at, min_age_seconds);
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
                first_seen_at: finished_at.to_string(),
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
    api::new_version_notification_reason(reason, summary).is_some()
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

    for candidate in &discovered_services {
        evaluate_candidate(state, job_id, finished_at, candidate).await?;
    }
    process_due_pending(state, finished_at, 50).await?;
    Ok(())
}

pub async fn process_due_pending(
    state: &Arc<AppState>,
    now: &str,
    limit: usize,
) -> anyhow::Result<usize> {
    let due = state.db.list_due_auto_update_pending(now, limit).await?;
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

pub fn spawn_tasks(state: Arc<AppState>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(PENDING_POLL_INTERVAL_SECONDS);
        loop {
            tokio::time::sleep(interval).await;
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
}
