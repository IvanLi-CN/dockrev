#[cfg(test)]
mod reconciliation_tests {
    use super::super::*;
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
        assert!(version_lag_met(2, &candidate.current_display_tag, &candidate, &rule, &history));
        assert!(!version_lag_met(3, &candidate.current_display_tag, &candidate, &rule, &history));
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
            source: "schedule".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
            settled_at: Some("2026-04-30T00:01:00Z".to_string()),
            updated_at: "2026-04-30T00:01:00Z".to_string(),
            policy_status: Some("delayed".to_string()),
            policy_reason: Some("policy_matched".to_string()),
            policy_rule_id: Some("r1".to_string()),
            policy_evaluated_at: Some("2026-04-30T00:01:00Z".to_string()),
            policy_scope_type: Some("stack".to_string()),
            policy_scope_id: Some("stack".to_string()),
            update_job_id: None,
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
        assert!(version_lag_met(2, &candidate.current_display_tag, &candidate, &rule, &history));
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
            source: "schedule".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
            settled_at: None,
            updated_at: "2026-04-30T00:00:00Z".to_string(),
            policy_status: Some("waiting_inference".to_string()),
            policy_reason: None,
            policy_rule_id: None,
            policy_evaluated_at: None,
            policy_scope_type: None,
            policy_scope_id: None,
            update_job_id: None,
        };
        let digest_a = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let digest_b = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let candidates = vec![
            candidate("service-a", "ghcr.io/acme/a:latest", digest_a),
            candidate("service-b", "ghcr.io/acme/b:latest", digest_b),
        ];
        let (keys, targets) = candidate_event_targets(&candidates, "linux/amd64");
        let key_a = crate::snapshot_worker::snapshot_task_key("ghcr.io/acme/a", digest_a, "linux/amd64").unwrap();
        let key_b = crate::snapshot_worker::snapshot_task_key("ghcr.io/acme/b", digest_b, "linux/amd64").unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(targets.get(&key_a), Some(&("ghcr.io/acme/a".to_string(), digest_a.to_string())));
        assert_eq!(targets.get(&key_b), Some(&("ghcr.io/acme/b".to_string(), digest_b.to_string())));
        assert_eq!(targets.get("unrelated"), None);
    }
}
