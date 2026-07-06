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

async fn service_id_by_name(
    state: &Arc<AppState>,
    stack_id: &str,
    service_name: &str,
) -> String {
    state
        .db
        .list_services_for_check(stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|svc| svc.name == service_name)
        .unwrap()
        .id
}

async fn insert_update_job_with_summary(
    state: &Arc<AppState>,
    job_id: &str,
    scope: crate::api::types::JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    summary: serde_json::Value,
    created_at: &str,
) {
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: job_id.to_string(),
            r#type: crate::api::types::JobType::Update,
            scope,
            stack_id: stack_id.map(ToString::to_string),
            service_id: service_id.map(ToString::to_string),
            status: "success".to_string(),
            created_by: "test".to_string(),
            reason: "ui".to_string(),
            created_at: created_at.to_string(),
            started_at: Some(created_at.to_string()),
            finished_at: Some(created_at.to_string()),
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: summary,
        })
        .await
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn insert_backup_record(
    state: &Arc<AppState>,
    backup_id: &str,
    stack_id: &str,
    job_id: &str,
    created_at: &str,
    status: &str,
    artifact_path: Option<&str>,
    size_bytes: Option<u64>,
    error: Option<&str>,
    cleanup_after: Option<&str>,
    deleted_at: Option<&str>,
) {
    state
        .db
        .insert_backup(backup_id, stack_id, job_id, created_at)
        .await
        .unwrap();
    state
        .db
        .finish_backup(
            backup_id,
            status,
            created_at,
            artifact_path,
            size_bytes,
            error,
        )
        .await
        .unwrap();
    if let Some(cleanup_after) = cleanup_after {
        state
            .db
            .schedule_backup_cleanup(backup_id, cleanup_after)
            .await
            .unwrap();
    }
    if let Some(deleted_at) = deleted_at {
        state
            .db
            .mark_backup_deleted(backup_id, deleted_at)
            .await
            .unwrap();
    }
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
async fn schedule_auto_policy_enqueues_due_delayed_pending() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let first_seen = "2026-04-30T00:00:00Z".to_string();
    let due_at = "2026-04-30T01:00:00Z".to_string();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-delayed-due-{}.yml",
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

    crate::auto_update::process_due_pending(&state, &due_at, 50)
        .await
        .unwrap();

    let jobs = state.db.list_jobs().await.unwrap();
    let auto_jobs = jobs
        .iter()
        .filter(|job| job.reason == "auto_policy")
        .collect::<Vec<_>>();
    assert_eq!(auto_jobs.len(), 1, "auto policy jobs: {jobs:?}");
    assert_eq!(auto_jobs[0].stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(auto_jobs[0].service_id.as_deref(), Some(service_id.as_str()));
}

#[tokio::test]
async fn schedule_auto_policy_rechecks_pending_after_delay_shortens() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let first_seen = "2026-04-30T00:00:00Z".to_string();
    let shortened_at = "2026-04-30T00:05:00Z".to_string();
    let compose_path = format!(
        "/tmp/dockrev-auto-policy-delay-shortens-{}.yml",
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
            &delayed_stack_auto_update_policy(604800, 0),
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
            &delayed_stack_auto_update_policy(0, 0),
            &shortened_at,
        )
        .await
        .unwrap();
    crate::auto_update::process_due_pending(&state, &shortened_at, 50)
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

#[tokio::test]
async fn get_service_backup_targets_resolves_compose_candidates_and_storage_info() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let base_dir = format!("/tmp/dockrev-backups-{}", ulid::Ulid::new());
    state
        .db
        .put_settings(
            &crate::api::types::BackupSettings {
                enabled: true,
                require_success: true,
                base_dir: base_dir.clone(),
                skip_targets_over_bytes: 1_000_000,
            },
            &crate::api::types::ResourceMonitorSettings {
                enabled: false,
                sample_interval_seconds: 60,
                retention_days: 7,
            },
            &crate::api::types::SchedulesSettings {
                update_check: crate::api::types::ScheduleItemSettings {
                    enabled: false,
                    cron: "0 * * * *".to_string(),
                },
                ghcr_webhook_audit: crate::api::types::ScheduleItemSettings {
                    enabled: false,
                    cron: "0 * * * *".to_string(),
                },
            },
            &crate::api::types::ReleaseNotesSettings {
                octo_rill: crate::api::types::OctoRillReleaseNotesSettings {
                    enabled: false,
                    api_base_url: None,
                    api_key: None,
                    default_view: crate::api::types::ReleaseNotesView::Smart,
                },
            },
            None,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let compose_dir = format!("/tmp/dockrev-backup-targets-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
      - ./data:/srv/data
      - /srv/shared/uploads:/srv/uploads
  worker:
    image: ghcr.io/acme/worker:1.0
    volumes:
      - ./data:/srv/data
volumes:
  api-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = service_id_by_name(&state, &stack_id, "api").await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/backup-targets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["storage"]["baseDir"].as_str(), Some(base_dir.as_str()));
    assert_eq!(body["storage"]["compression"].as_str(), Some("gzip"));
    assert_eq!(body["storage"]["keepLast"].as_u64(), Some(1));
    assert_eq!(body["storage"]["deleteAfterStableSeconds"].as_u64(), Some(3600));
    assert_eq!(
        body["storage"]["artifactPattern"].as_str(),
        Some(format!("{base_dir}/<stackId>/<timestamp>.tar.gz").as_str())
    );
    assert_eq!(body["volumeNames"][0]["key"].as_str(), Some("api-data"));
    assert_eq!(
        body["bindPaths"][0]["key"].as_str(),
        Some(format!("{compose_dir}/./data").as_str())
    );
    assert_eq!(
        body["bindPaths"][1]["key"].as_str(),
        Some("/srv/shared/uploads")
    );
    assert_eq!(
        body["bindPaths"][0]["relatedServiceCount"].as_u64(),
        Some(2)
    );
    assert_eq!(
        body["bindPaths"][0]["policy"].as_str(),
        Some("disabled")
    );
}

#[tokio::test]
async fn get_service_backup_targets_reads_latest_compose_mounts_without_stack_resync() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-live-compose-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
  worker:
    image: ghcr.io/acme/worker:1.0
volumes:
  api-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = service_id_by_name(&state, &stack_id, "api").await;

    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
      - ./cache:/srv/cache
  worker:
    image: ghcr.io/acme/worker:1.0
    volumes:
      - ./cache:/srv/cache
volumes:
  api-data:
"#,
    )
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/backup-targets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let shared_path = format!("{compose_dir}/./cache");
    assert_eq!(body["volumeNames"][0]["key"].as_str(), Some("api-data"));
    assert_eq!(body["bindPaths"][0]["key"].as_str(), Some(shared_path.as_str()));
    assert_eq!(
        body["bindPaths"][0]["relatedServiceCount"].as_u64(),
        Some(2)
    );
}

#[tokio::test]
async fn get_service_backup_targets_returns_empty_candidates_when_service_missing_from_compose() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-missing-compose-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
volumes:
  api-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = service_id_by_name(&state, &stack_id, "api").await;

    std::fs::write(
        &compose_path,
        r#"
services:
  worker:
    image: ghcr.io/acme/worker:1.0
"#,
    )
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/backup-targets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["bindPaths"].as_array().map(|items| items.len()), Some(0));
    assert_eq!(body["volumeNames"].as_array().map(|items| items.len()), Some(0));
}

#[tokio::test]
async fn put_service_backup_targets_removes_unique_targets_and_skips_shared_targets() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-update-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
      - ./shared:/srv/shared
  web:
    image: ghcr.io/acme/web:1.0
    volumes:
      - ./shared:/srv/shared
volumes:
  api-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let web_id = service_id_by_name(&state, &stack_id, "web").await;
    let shared_path = format!("{compose_dir}/./shared");

    state
        .db
        .put_service_backup_targets(
            &api_id,
            &crate::db::ServiceBackupTargetsUpdate {
                stack_targets: vec![
                    crate::api::types::BackupTarget::DockerVolume {
                        name: "api-data".to_string(),
                    },
                    crate::api::types::BackupTarget::BindMount {
                        path: shared_path.clone(),
                    },
                ],
                bind_paths: vec![crate::db::ServiceBackupTargetPolicyRow {
                    key: shared_path.clone(),
                    policy: crate::api::types::BackupTargetPolicy::LiveBackup,
                }],
                volume_names: vec![crate::db::ServiceBackupTargetPolicyRow {
                    key: "api-data".to_string(),
                    policy: crate::api::types::BackupTargetPolicy::LiveBackup,
                }],
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    state
        .db
        .put_service_backup_targets(
            &web_id,
            &crate::db::ServiceBackupTargetsUpdate {
                stack_targets: vec![crate::api::types::BackupTarget::BindMount {
                    path: shared_path.clone(),
                }],
                bind_paths: vec![crate::db::ServiceBackupTargetPolicyRow {
                    key: shared_path.clone(),
                    policy: crate::api::types::BackupTargetPolicy::LiveBackup,
                }],
                volume_names: Vec::new(),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{api_id}/backup-targets"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "bindPaths": [{ "key": shared_path, "policy": "disabled" }],
                        "volumeNames": [{ "key": "api-data", "policy": "disabled" }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let _body = response_json(resp).await;

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let stack_target_keys = stack
        .backup
        .targets
        .iter()
        .map(|target| target.key().to_string())
        .collect::<Vec<_>>();
    assert_eq!(stack_target_keys, vec![shared_path.clone()]);

    let api_settings = state.db.get_service_settings(&api_id).await.unwrap().unwrap();
    assert!(matches!(
        api_settings.backup_targets.bind_paths.get(&shared_path),
        Some(crate::api::types::TernaryChoice::Skip)
    ));
    assert!(matches!(
        api_settings.backup_targets.volume_names.get("api-data"),
        Some(crate::api::types::TernaryChoice::Skip)
    ));
}

#[tokio::test]
async fn put_service_backup_targets_removes_disabled_unique_targets_even_with_legacy_stack_entries() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-unrelated-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - api-data:/var/lib/api
volumes:
  api-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let legacy_path = "/srv/manual/legacy".to_string();

    state
        .db
        .put_service_backup_targets(
            &api_id,
            &crate::db::ServiceBackupTargetsUpdate {
                stack_targets: vec![
                    crate::api::types::BackupTarget::DockerVolume {
                        name: "api-data".to_string(),
                    },
                    crate::api::types::BackupTarget::BindMount {
                        path: legacy_path.clone(),
                    },
                ],
                bind_paths: Vec::new(),
                volume_names: vec![crate::db::ServiceBackupTargetPolicyRow {
                    key: "api-data".to_string(),
                    policy: crate::api::types::BackupTargetPolicy::LiveBackup,
                }],
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{api_id}/backup-targets"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "bindPaths": [],
                        "volumeNames": [{ "key": "api-data", "policy": "disabled" }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let stack_target_keys = stack
        .backup
        .targets
        .iter()
        .map(|target| target.key().to_string())
        .collect::<Vec<_>>();
    assert!(stack_target_keys.is_empty());
}

#[tokio::test]
async fn put_service_backup_targets_adds_enabled_targets_to_stack() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-add-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - cache-data:/var/lib/cache
volumes:
  cache-data:
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{api_id}/backup-targets"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "bindPaths": [],
                        "volumeNames": [{ "key": "cache-data", "policy": "stop_related_services" }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let _body = response_json(resp).await;

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    assert_eq!(stack.backup.targets.len(), 1);
    assert_eq!(stack.backup.targets[0].key(), "cache-data");
}
