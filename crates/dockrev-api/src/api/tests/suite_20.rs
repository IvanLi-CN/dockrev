#[tokio::test]
async fn stack_and_service_auto_update_policy_settings_roundtrip_and_validate() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-auto-policy-settings-{}.yml", ulid::Ulid::new());
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
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();

    let stack_policy = json!({
        "mode": "override",
        "enabled": true,
        "rules": [{
            "id": "stable-semver",
            "name": "Stable semver",
            "enabled": true,
            "matcher": { "type": "semver", "pattern": ">=1, <2" },
            "action": "delayed",
            "delay": { "minAgeSeconds": 3600, "minVersionLag": 2 }
        }]
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/stacks/{stack_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "autoUpdatePolicy": stack_policy }).to_string(),
                ))
                .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}/settings"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["autoUpdatePolicy"]["rules"][0]["delay"]["minAgeSeconds"].as_u64(),
        Some(3600)
    );
    assert_eq!(
        body["autoUpdatePolicy"]["rules"][0]["delay"]["minVersionLag"].as_u64(),
        Some(2)
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": true,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} },
                        "autoUpdatePolicy": {
                            "mode": "override",
                            "enabled": true,
                            "rules": [{
                                "id": "release-glob",
                                "name": "Release glob",
                                "enabled": true,
                                "matcher": { "type": "glob", "pattern": "1.4.*" },
                                "action": "immediate",
                                "delay": { "minAgeSeconds": 0, "minVersionLag": 0 }
                            }]
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/settings"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["autoUpdatePolicy"]["mode"].as_str(), Some("override"));
    assert_eq!(
        body["autoUpdatePolicy"]["rules"][0]["matcher"]["type"].as_str(),
        Some("glob")
    );

    let invalid_slider = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/stacks/{stack_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoUpdatePolicy": {
                            "mode": "override",
                            "enabled": true,
                            "rules": [{
                                "id": "bad-slider",
                                "name": "Bad slider",
                                "enabled": true,
                                "matcher": { "type": "semver", "pattern": ">=1" },
                                "action": "delayed",
                                "delay": { "minAgeSeconds": 901, "minVersionLag": 2 }
                            }]
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_slider.status(), 400);
    let body = response_json(invalid_slider).await;
    assert_eq!(body["error"]["code"].as_str(), Some("invalid_argument"));

    let invalid_regex = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/stacks/{stack_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoUpdatePolicy": {
                            "mode": "override",
                            "enabled": true,
                            "rules": [{
                                "id": "bad-regex",
                                "name": "Bad regex",
                                "enabled": true,
                                "matcher": { "type": "regex", "pattern": "(" },
                                "action": "immediate",
                                "delay": { "minAgeSeconds": 0, "minVersionLag": 0 }
                            }]
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_regex.status(), 400);
}

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
                "currentDigest": "sha256:old",
                "currentDisplayTag": "1.0.0",
                "candidateTag": "latest",
                "candidateDisplayTag": "1.1.0",
                "candidateDigest": candidate_digest
            }]
        }
    })
}

#[tokio::test]
async fn schedule_auto_policy_enqueues_explicit_target_and_dedupes() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!("/tmp/dockrev-auto-policy-enqueue-{}.yml", ulid::Ulid::new());
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;

    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");

    crate::auto_update::handle_completed_check(&state, "chk_manual", "ui", &now, &summary)
        .await
        .unwrap();
    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );

    crate::auto_update::handle_completed_check(&state, "chk_schedule", "schedule", &now, &summary)
        .await
        .unwrap();
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_duplicate",
        "schedule",
        &now,
        &summary,
    )
    .await
    .unwrap();

    let jobs = state.db.list_jobs().await.unwrap();
    let auto_jobs = jobs
        .iter()
        .filter(|job| job.reason == "auto_policy")
        .collect::<Vec<_>>();
    assert_eq!(auto_jobs.len(), 1, "auto policy jobs: {jobs:?}");
    let job = auto_jobs[0];
    assert_eq!(job.created_by, "auto-policy");
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    assert_eq!(
        job.summary_json["targets"][0]["serviceId"].as_str(),
        Some(service_id.as_str())
    );
    assert_eq!(
        job.summary_json["targets"][0]["targetDigest"].as_str(),
        Some("sha256:new")
    );
}

#[tokio::test]
async fn schedule_auto_policy_restricts_generic_webhook_checks() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-webhook-source-{}.yml",
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;

    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(&state, "chk_generic_webhook", "webhook", &now, &summary)
        .await
        .unwrap();
    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );

    let mut ghcr_summary = summary;
    ghcr_summary["source"] = json!("github_webhook");
    ghcr_summary["matchedServiceIds"] = json!([service_id.clone()]);
    crate::auto_update::handle_completed_check(&state, "chk_ghcr_webhook", "webhook", &now, &ghcr_summary)
        .await
        .unwrap();

    let jobs = state.db.list_jobs().await.unwrap();
    let auto_jobs = jobs
        .iter()
        .filter(|job| job.reason == "auto_policy")
        .collect::<Vec<_>>();
    assert_eq!(auto_jobs.len(), 1, "auto policy jobs: {jobs:?}");
    assert_eq!(auto_jobs[0].service_id.as_deref(), Some(service_id.as_str()));
}

#[tokio::test]
async fn schedule_auto_policy_skips_ignored_services() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!("/tmp/dockrev-auto-policy-ignored-{}.yml", ulid::Ulid::new());
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;
    state
        .db
        .update_service_check_result(
            &service_id,
            crate::snapshot_worker::normalize_digest("sha256:old"),
            None,
            None,
            Some("latest".to_string()),
            None,
            crate::snapshot_worker::normalize_digest("sha256:new"),
            None,
            None,
            Some("ignored".to_string()),
            Some("blocked by rule".to_string()),
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(&state, "chk_schedule", "schedule", &now, &summary)
        .await
        .unwrap();

    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );
}

#[tokio::test]
async fn schedule_auto_policy_skips_dockrev_self_update() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-self-update-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;
    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(&state, "chk_schedule", "schedule", &now, &summary)
        .await
        .unwrap();
    crate::auto_update::process_due_pending(&state, &now, 50)
        .await
        .unwrap();

    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );
}

#[tokio::test]
async fn schedule_auto_policy_marks_arch_mismatch_rejection_skipped() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-arch-mismatch-{}.yml",
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;
    state
        .db
        .update_service_check_result(
            &service_id,
            crate::snapshot_worker::normalize_digest("sha256:old"),
            None,
            None,
            Some("latest".to_string()),
            None,
            crate::snapshot_worker::normalize_digest("sha256:new"),
            Some("mismatch".to_string()),
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(&state, "chk_schedule", "schedule", &now, &summary)
        .await
        .unwrap();
    crate::auto_update::process_due_pending(&state, &now, 50)
        .await
        .unwrap();

    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );
}

#[tokio::test]
async fn schedule_auto_policy_rechecks_updated_pending_gates() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let first_seen = "2026-04-30T00:00:00Z".to_string();
    let twenty_minutes_later = "2026-04-30T00:20:00Z".to_string();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-gate-recheck-{}.yml",
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;
    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &delayed_stack_auto_update_policy(900, 0),
            &first_seen,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_initial",
        "schedule",
        &first_seen,
        &summary,
    )
    .await
    .unwrap();
    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );

    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &delayed_stack_auto_update_policy(3600, 0),
            &twenty_minutes_later,
        )
        .await
        .unwrap();
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_recheck",
        "schedule",
        &twenty_minutes_later,
        &summary,
    )
    .await
    .unwrap();

    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );
}

#[tokio::test]
async fn schedule_auto_policy_re_reserves_after_scope_change() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let first_seen = "2026-04-30T00:00:00Z".to_string();
    let override_at = "2026-04-30T00:05:00Z".to_string();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-scope-change-{}.yml",
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:old"), Some("latest"), Some("sha256:new")).await;

    state
        .db
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &delayed_stack_auto_update_policy(3600, 0),
            &first_seen,
        )
        .await
        .unwrap();

    let summary = auto_update_discovery_summary(&stack_id, &service_id, "sha256:new");
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_initial",
        "schedule",
        &first_seen,
        &summary,
    )
    .await
    .unwrap();
    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );

    state
        .db
        .put_auto_update_policy(
            "service",
            &service_id,
            &immediate_stack_auto_update_policy(),
            &override_at,
        )
        .await
        .unwrap();
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_override",
        "schedule",
        &override_at,
        &summary,
    )
    .await
    .unwrap();

    let jobs = state.db.list_jobs().await.unwrap();
    let auto_jobs = jobs
        .iter()
        .filter(|job| job.reason == "auto_policy")
        .collect::<Vec<_>>();
    assert_eq!(auto_jobs.len(), 1, "auto policy jobs: {jobs:?}");
    assert_eq!(auto_jobs[0].service_id.as_deref(), Some(service_id.as_str()));
}
