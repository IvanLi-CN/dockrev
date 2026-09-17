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
fn auto_update_projection_serializes_auditable_context() {
    let projection = api::types::AutoUpdateProjection {
        policy_status: "running".to_string(),
        reason: Some("update_job_running".to_string()),
        rule_id: Some("stable".to_string()),
        evaluated_at: Some("2026-04-30T00:00:00Z".to_string()),
        policy_scope: Some(api::types::AutoUpdatePolicyScope {
            scope_type: "stack".to_string(),
            scope_id: "stack".to_string(),
        }),
        update_job_id: Some("job-1".to_string()),
    };
    let json = serde_json::to_value(projection).unwrap();
    assert_eq!(json["policyScope"]["scopeType"], "stack");
    assert_eq!(json["policyScope"]["scopeId"], "stack");
    assert_eq!(json["updateJobId"], "job-1");
    assert!(
        serde_json::from_value::<api::types::AutoUpdateProjection>(serde_json::json!({
            "policyStatus": "waiting_inference"
        }))
        .is_ok()
    );
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
fn candidate_settlement_rejects_strict_tags_without_digest_evidence() {
    let candidate = notify::NewVersionDiscoveredService {
        stack_id: "stack".to_string(),
        service_id: "service".to_string(),
        image_ref: "ghcr.io/acme/app".to_string(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "1.4.0".to_string(),
        candidate_display_tag: "1.4.0".to_string(),
        candidate_digest: "".to_string(),
    };
    assert_eq!(candidate_settlement_state(&candidate).0, "unresolved");
    assert_eq!(candidate_settlement_state(&candidate).1, None);
}

#[test]
fn update_request_can_be_rebuilt_from_a_persisted_auto_policy_job() {
    let request = update_request_from_job(&api::types::JobListItem {
        id: "job".to_string(),
        r#type: api::types::JobType::Update,
        scope: JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "queued".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: json!({
            "mode": "apply",
            "targets": [{
                "serviceId": "service",
                "targetTag": "latest",
                "targetDigest": "sha256:new"
            }]
        }),
    })
    .unwrap();
    assert!(matches!(request.mode, UpdateMode::Apply));
    assert_eq!(request.targets.unwrap()[0].target_digest, "sha256:new");
    assert!(matches!(request.reason, UpdateReason::AutoPolicy));
}

#[test]
fn persisted_auto_policy_job_requires_explicit_valid_targets() {
    let mut job = api::types::JobListItem {
        id: "job".to_string(),
        r#type: api::types::JobType::Update,
        scope: JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "queued".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: json!({ "mode": "apply" }),
    };
    assert!(update_request_from_job(&job).is_err());

    job.summary_json["targets"] = json!([]);
    assert!(update_request_from_job(&job).is_err());

    job.summary_json["targets"] = json!([{
        "serviceId": "service",
        "targetTag": "latest",
        "targetDigest": ""
    }]);
    assert!(update_request_from_job(&job).is_err());
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
fn permanent_inference_errors_do_not_use_transient_retry_budget() {
    assert!(permanent_inference_error(&anyhow::anyhow!(
        "registry request failed: 404 Not Found"
    )));
    assert!(permanent_inference_error(&anyhow::anyhow!(
        "parse manifest json"
    )));
    assert!(!permanent_inference_error(&anyhow::anyhow!(
        "registry request failed: 503 Service Unavailable"
    )));
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

    let stack_conflict = ApiError::conflict("service operation in progress").with_details(json!({
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

include!("auto_update_reconciliation_tests.rs");
