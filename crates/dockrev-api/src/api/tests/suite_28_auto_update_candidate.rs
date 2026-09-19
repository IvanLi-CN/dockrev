fn immediate_stack_auto_update_policy() -> crate::api::types::AutoUpdatePolicy {
    crate::api::types::AutoUpdatePolicy {
        mode: crate::api::types::AutoUpdatePolicyMode::Override,
        enabled: true,
        rules: vec![crate::api::types::AutoUpdateRule {
            id: "stable".to_string(),
            name: "Stable".to_string(),
            enabled: true,
            matcher: crate::api::types::AutoUpdateMatcher {
                kind: crate::api::types::AutoUpdateMatcherType::Semver,
                pattern: ">=1, <2".to_string(),
            },
            action: crate::api::types::AutoUpdateRuleAction::Immediate,
            delay: crate::api::types::AutoUpdateDelay {
                min_age_seconds: 0,
                min_version_lag: 0,
            },
        }],
        updated_at: None,
    }
}

fn delayed_stack_auto_update_policy(
    min_age_seconds: u32,
    min_version_lag: u32,
) -> crate::api::types::AutoUpdatePolicy {
    let mut policy = immediate_stack_auto_update_policy();
    policy.rules[0].action = crate::api::types::AutoUpdateRuleAction::Delayed;
    policy.rules[0].delay = crate::api::types::AutoUpdateDelay {
        min_age_seconds,
        min_version_lag,
    };
    policy
}

fn auto_update_discovery_summary(
    stack_id: &str,
    service_id: &str,
    candidate_digest: &str,
) -> serde_json::Value {
    json!({
        "newVersions": {
            "count": 1,
            "services": [{
                "stackId": stack_id,
                "serviceId": service_id,
                "serviceName": "web",
                "imageRef": "ghcr.io/acme/web",
                "currentTag": "latest",
                "currentDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000001",
                "currentDisplayTag": "1.0.0",
                "candidateTag": "1.1.0",
                "candidateDisplayTag": "1.1.0",
                "candidateDigest": candidate_digest
            }]
        }
    })
}

#[test]
fn candidate_provenance_serializes_in_service_stack_and_overview_contracts() {
    let settlement = crate::api::types::CandidateSettlement {
        status: "ready".to_string(),
        raw_tag: Some("latest".to_string()),
        candidate_digest: Some("sha256:new".to_string()),
        resolved_version: Some("1.4.0".to_string()),
        resolved_tags: Some(vec!["1.4.0".to_string()]),
        reason: Some("digest_bound_version".to_string()),
        attempts: 1,
        retry_at: None,
        discovered_at: Some("2026-04-30T00:00:00Z".to_string()),
        last_error: None,
        superseded_at: None,
        superseded_by_candidate_id: None,
        source: Some("github_webhook".to_string()),
        source_job_id: Some("check-webhook".to_string()),
        hydration_origin: Some("discovery_history".to_string()),
    };
    let hydration = crate::api::types::CandidateHydrationDiagnostic {
        status: "hydrated".to_string(),
        reason: None,
        candidate_digest: Some("sha256:new".to_string()),
        source: Some("github_webhook".to_string()),
        source_job_id: Some("check-webhook".to_string()),
        hydration_origin: Some("discovery_history".to_string()),
        discovered_at: Some("2026-04-30T00:00:00Z".to_string()),
    };
    let settings = crate::api::types::ServiceSettings {
        auto_rollback: false,
        backup_targets: crate::api::types::BackupTargetOverrides {
            bind_paths: BTreeMap::new(),
            volume_names: BTreeMap::new(),
        },
        repo_url: None,
    };
    let service = crate::api::types::Service {
        id: "service".to_string(),
        name: "web".to_string(),
        image: crate::api::types::ComposeRef {
            reference: "ghcr.io/acme/web".to_string(),
            tag: "latest".to_string(),
            digest: Some("sha256:current".to_string()),
            resolved_tag: None,
            resolved_tags: None,
        },
        homepage: None,
        update_guard: None,
        candidate: None,
        ignore: None,
        version_inference: None,
        candidate_settlement: Some(settlement.clone()),
        candidate_hydration: Some(hydration.clone()),
        auto_update: None,
        new_version_discovery_count: None,
        settings: settings.clone(),
        archived: None,
    };

    let service_json = serde_json::to_value(&service).unwrap();
    assert_eq!(
        service_json["candidateSettlement"]["sourceJobId"],
        "check-webhook"
    );
    assert_eq!(
        service_json["candidateHydration"]["hydrationOrigin"],
        "discovery_history"
    );
    assert!(service_json["candidateSettlement"].get("source_job_id").is_none());

    let stack_json = serde_json::to_value(crate::api::types::GetStackResponse {
        stack: crate::api::types::StackResponse {
            id: "stack".to_string(),
            name: "stack".to_string(),
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec![],
                env_file: None,
            },
            services: vec![crate::api::types::StackServiceResponse {
                service,
                lifecycle_state: crate::api::types::ServiceLifecycleState::Unknown,
            }],
            archived: Some(false),
        },
    })
    .unwrap();
    assert_eq!(
        stack_json["stack"]["services"][0]["candidateSettlement"]["source"],
        "github_webhook"
    );

    let resource = crate::api::types::ServiceResourceOverviewItem {
        service_id: "service".to_string(),
        sampled_at: None,
        cpu_percent: None,
        mem_used_bytes: None,
        mem_limit_bytes: None,
        net_rx_rate_bps: None,
        net_tx_rate_bps: None,
        stale: true,
        sample_count: 0,
    };
    let overview_json = serde_json::to_value(crate::api::types::HomepageNavResponse {
        generated_at: "2026-04-30T00:01:00Z".to_string(),
        last_check_at: None,
        resource_summary: crate::api::types::ServiceResourceOverviewResponse {
            enabled: false,
            window: "1h".to_string(),
            generated_at: "2026-04-30T00:01:00Z".to_string(),
            stale_after_seconds: 300,
            services: vec![resource.clone()],
        },
        items: vec![crate::api::types::HomepageNavItem {
            stack_id: "stack".to_string(),
            stack_name: "stack".to_string(),
            service_id: "service".to_string(),
            service_name: "web".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: "latest".to_string(),
            image_digest: Some("sha256:current".to_string()),
            image_resolved_tag: None,
            image_resolved_tags: None,
            is_dockrev: false,
            homepage: crate::api::types::ServiceHomepage::default(),
            candidate: None,
            ignore: None,
            version_inference: None,
            candidate_settlement: Some(settlement),
            candidate_hydration: Some(hydration),
            auto_update: None,
            new_version_discovery_count: None,
            settings,
            archived: None,
            resource,
        }],
    })
    .unwrap();
    assert_eq!(
        overview_json["items"][0]["candidateHydration"]["sourceJobId"],
        "check-webhook"
    );

    let sparse = serde_json::to_value(crate::api::types::CandidateSettlement {
        status: "unresolved".to_string(),
        raw_tag: None,
        candidate_digest: None,
        resolved_version: None,
        resolved_tags: None,
        reason: Some("migration_ambiguous_history".to_string()),
        attempts: 0,
        retry_at: None,
        discovered_at: None,
        last_error: None,
        superseded_at: None,
        superseded_by_candidate_id: None,
        source: None,
        source_job_id: None,
        hydration_origin: None,
    })
    .unwrap();
    assert!(sparse.get("source").is_none());
    assert!(sparse.get("sourceJobId").is_none());
    assert!(sparse.get("hydrationOrigin").is_none());
}

#[tokio::test]
async fn digest_bound_snapshot_settles_candidate_and_re_evaluates_policy() {
    let state = test_state(":memory:").await;
    insert_schedule_check_job(&state, "check", "2026-04-30T00:00:00Z").await;
    state
        .db
        .put_auto_update_policy(
            "stack",
            "stack",
            &delayed_stack_auto_update_policy(3600, 0),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    state
        .db
        .upsert_auto_update_candidate(
            &crate::db::AutoUpdateCandidateInput {
                id: "candidate-inference".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:new".to_string(),
                resolved_version: None,
                status: "awaiting_inference".to_string(),
                reason: Some("version_inference_pending".to_string()),
                attempts: 1,
                retry_at: Some("2026-04-30T00:01:00Z".to_string()),
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:old".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
        digest: "sha256:new".to_string(),
        tags: vec!["latest".to_string(), "1.4.0".to_string()],
        checked_at: "2026-04-30T00:00:30Z".to_string(),
        scan: crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    };
    state
        .db
        .upsert_image_digest_tags_snapshot(
            "ghcr.io/acme/web",
            "sha256:new",
            "linux/amd64",
            &serde_json::to_string(&snapshot).unwrap(),
            &snapshot.checked_at,
            &snapshot.checked_at,
        )
        .await
        .unwrap();

    crate::auto_update::reconcile_inference_for_digest(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        "2026-04-30T00:01:00Z",
    )
    .await
    .unwrap();

    let candidate = state
        .db
        .get_auto_update_candidate("service", "sha256:new")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "ready");
    assert_eq!(candidate.resolved_version.as_deref(), Some("1.4.0"));
    assert_eq!(candidate.policy_status.as_deref(), Some("delayed"));
    assert_eq!(
        state
            .db
            .list_auto_update_pending_candidates("2026-04-30T00:01:00Z", 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn terminal_unresolved_candidate_re_evaluates_policy_as_skipped() {
    let state = test_state(":memory:").await;
    insert_schedule_check_job(&state, "check", "2026-04-30T00:00:00Z").await;
    state
        .db
        .put_auto_update_policy(
            "stack",
            "stack",
            &delayed_stack_auto_update_policy(3600, 0),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    state
        .db
        .upsert_auto_update_candidate(
            &crate::db::AutoUpdateCandidateInput {
                id: "candidate-unresolved-policy".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:unresolved-policy".to_string(),
                resolved_version: None,
                status: "awaiting_inference".to_string(),
                reason: Some("version_inference_pending".to_string()),
                attempts: 3,
                retry_at: None,
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:old".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();

    crate::auto_update::reconcile_inference_for_digest(
        &state,
        "ghcr.io/acme/web",
        "sha256:unresolved-policy",
        "linux/amd64",
        "2026-04-30T00:01:00Z",
    )
    .await
    .unwrap();

    let candidate = state
        .db
        .get_auto_update_candidate("service", "sha256:unresolved-policy")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.policy_status.as_deref(), Some("skipped"));
    assert_eq!(candidate.policy_reason.as_deref(), Some("version_unresolved"));
}

#[tokio::test]
async fn authoritative_snapshot_without_semver_settles_candidate_as_unresolved() {
    let state = test_state(":memory:").await;
    state
        .db
        .upsert_auto_update_candidate(
            &crate::db::AutoUpdateCandidateInput {
                id: "candidate-authoritative-unresolved".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:authoritative-unresolved".to_string(),
                resolved_version: None,
                status: "awaiting_inference".to_string(),
                reason: Some("version_inference_pending".to_string()),
                attempts: 2,
                retry_at: Some("2026-04-30T00:01:00Z".to_string()),
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:old".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
        digest: "sha256:authoritative-unresolved".to_string(),
        tags: vec!["latest".to_string(), "stable".to_string()],
        checked_at: "2026-04-30T00:02:00Z".to_string(),
        scan: crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    };
    state
        .db
        .upsert_image_digest_tags_snapshot(
            "ghcr.io/acme/web",
            "sha256:authoritative-unresolved",
            "linux/amd64",
            &serde_json::to_string(&snapshot).unwrap(),
            &snapshot.checked_at,
            &snapshot.checked_at,
        )
        .await
        .unwrap();

    crate::auto_update::reconcile_inference_for_digest(
        &state,
        "ghcr.io/acme/web",
        "sha256:authoritative-unresolved",
        "linux/amd64",
        "2026-04-30T00:02:30Z",
    )
    .await
    .unwrap();

    let candidate = state
        .db
        .get_auto_update_candidate("service", "sha256:authoritative-unresolved")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.attempts, 2);
    assert_eq!(candidate.retry_at, None);
    assert_eq!(candidate.reason.as_deref(), Some("version_inference_unresolved"));
}

#[tokio::test]
async fn malformed_oci_version_settles_candidate_as_unresolved_without_retry() {
    let state = test_state_with(
        ":memory:",
        Arc::new(ExplicitVersionFallbackRegistry::new("not-a-semver")),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    state
        .db
        .upsert_auto_update_candidate(
            &crate::db::AutoUpdateCandidateInput {
                id: "candidate-malformed-oci".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:new".to_string(),
                resolved_version: None,
                status: "awaiting_inference".to_string(),
                reason: Some("version_inference_pending".to_string()),
                attempts: 0,
                retry_at: Some("2026-04-30T00:01:00Z".to_string()),
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:old".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();

    crate::auto_update::reconcile_inference_for_digest(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        "2026-04-30T00:00:30Z",
    )
    .await
    .unwrap();

    let candidate = state
        .db
        .get_auto_update_candidate("service", "sha256:new")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.reason.as_deref(), Some("invalid_oci_version"));
    assert_eq!(candidate.retry_at, None);
    assert_eq!(candidate.attempts, 1);
}

#[tokio::test]
async fn raw_tag_policy_can_enqueue_while_version_inference_is_pending() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-raw-tag-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000001"),
        Some("latest"),
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000002"),
    )
    .await;

    let mut policy = immediate_stack_auto_update_policy();
    policy.rules[0].matcher.kind = crate::api::types::AutoUpdateMatcherType::Glob;
    policy.rules[0].matcher.pattern = "latest".to_string();
    state
        .db
        .put_auto_update_policy("stack", &stack_id, &policy, &now)
        .await
        .unwrap();

    let mut summary = auto_update_discovery_summary(
        &stack_id,
        &service_id,
        "sha256:0000000000000000000000000000000000000000000000000000000000000002",
    );
    summary["newVersions"]["services"][0]["currentDigest"] = json!(
        "sha256:0000000000000000000000000000000000000000000000000000000000000001"
    );
    summary["newVersions"]["services"][0]["candidateTag"] = json!("latest");
    summary["newVersions"]["services"][0]["candidateDisplayTag"] = json!("latest");
    insert_schedule_check_job(&state, "chk_schedule_raw_tag", &now).await;
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_raw_tag",
        "schedule",
        &now,
        &summary,
    )
    .await
    .unwrap();

    let candidate = state
        .db
        .get_auto_update_candidate(
            &service_id,
            "sha256:0000000000000000000000000000000000000000000000000000000000000002",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "awaiting_inference");
    assert!(
        matches!(candidate.policy_status.as_deref(), Some("queued" | "running")),
        "candidate={candidate:?} pending={:?}",
        state
            .db
            .list_auto_update_pending_candidates(&now, 10)
            .await
            .unwrap()
    );
    assert_eq!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .filter(|job| job.reason == "auto_policy")
            .count(),
        1
    );
}
