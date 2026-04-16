#[tokio::test]
async fn runtime_scan_preserves_candidate_resolved_tag_when_candidate_digest_unchanged() {
    let compose_path = format!(
        "/tmp/dockrev-test-runtime-scan-preserve-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap();

    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:older".to_string()),
            None,
            None,
            Some("5.2".to_string()),
            Some("5.2".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let scan_payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(scan_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let candidate = &detail["stack"]["services"][0]["candidate"];
    assert_eq!(candidate["digest"].as_str(), Some("sha256:new"));
    assert_eq!(candidate["resolvedTag"].as_str(), Some("5.2"));
}

#[tokio::test]
async fn runtime_scan_no_drift_does_not_hit_registry() {
    let registry = Arc::new(CountingRegistry::default());
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:match"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:match".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    assert_eq!(
        registry.total_calls(),
        0,
        "runtime scan should not hit registry when there is no drift"
    );
}

#[tokio::test]
async fn runtime_scan_candidate_change_for_strict_semver_does_not_enqueue_inference() {
    let registry = Arc::new(StrictSemverDriftRegistry::new(Duration::from_millis(400)));
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:older".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job = response_json(resp).await;
    assert_eq!(
        job["job"]["summary"]["servicesDrifted"]
            .as_u64()
            .unwrap_or_default(),
        1,
        "runtime scan summary: {job}"
    );
    assert_eq!(
        job["job"]["summary"]["servicesUpdated"]
            .as_u64()
            .unwrap_or_default(),
        1
    );

    let in_flight = state
        .snapshot_worker
        .in_flight_reason("ghcr.io/acme/web", "sha256:new", "linux/amd64")
        .await;
    assert!(
        in_flight.is_none(),
        "strict semver runtime-scan candidate changes should not enqueue version inference"
    );
}

#[tokio::test]
async fn service_settings_repo_url_roundtrip_and_empty_string_clear() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    assert!(body["repoUrl"].is_null(), "initial settings: {body}");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": false,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} },
                        "repoUrl": "https://github.com/acme/web"
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
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
    );
    assert_eq!(body["autoRollback"].as_bool(), Some(false));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["stack"]["services"][0]["settings"]["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
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
                        "repoUrl": "   "
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
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
    assert!(body["repoUrl"].is_null(), "cleared settings: {body}");

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored.repo_url_auto_disabled,
        "explicit repoUrl clear should disable auto backfill: {stored:?}"
    );
}

#[tokio::test]
async fn put_service_settings_rejects_invalid_repo_url() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": true,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} },
                        "repoUrl": "github.com/acme/web"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str(), Some("invalid_argument"));
}

#[tokio::test]
async fn put_service_settings_rejects_repo_url_with_credentials() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": true,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} },
                        "repoUrl": "https://token@github.com/acme/web"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str(), Some("invalid_argument"));
}

#[tokio::test]
async fn put_service_settings_preserves_repo_url_when_field_is_omitted() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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

    state
        .db
        .put_service_settings(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: false,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: Some("https://github.com/acme/web".to_string()),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

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
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        settings.repo_url.as_deref(),
        Some("https://github.com/acme/web"),
        "unexpected settings after omitted repoUrl field: {settings:?}"
    );
    assert!(settings.auto_rollback);

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !stored.repo_url_auto_disabled,
        "repo auto backfill disable flag should remain false: {stored:?}"
    );
}

#[tokio::test]
async fn new_service_settings_default_repo_auto_disabled_is_false() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.settings.repo_url.is_none());
    assert!(
        !stored.repo_url_auto_disabled,
        "new services should allow repo auto backfill by default: {stored:?}"
    );
}

#[tokio::test]
async fn put_service_settings_omitted_repo_url_preserves_disable_flag() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();

    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: false,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": true,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored.repo_url_auto_disabled,
        "omitting repoUrl must preserve auto-backfill disable flag: {stored:?}"
    );
    assert!(stored.settings.auto_rollback);
}

#[tokio::test]
async fn put_service_settings_non_empty_repo_url_reenables_auto_backfill() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();

    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/services/{service_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoRollback": true,
                        "backupTargets": { "bindPaths": {}, "volumeNames": {} },
                        "repoUrl": "https://github.com/acme/web"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.settings.repo_url.as_deref(),
        Some("https://github.com/acme/web")
    );
    assert!(
        !stored.repo_url_auto_disabled,
        "saving a repoUrl should re-enable auto backfill: {stored:?}"
    );
}

#[tokio::test]
async fn sync_stack_from_compose_clears_repo_url_when_service_image_changes() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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

    state
        .db
        .put_service_settings(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: Some("https://github.com/acme/web".to_string()),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/worker".to_string(),
                image_tag: "latest".to_string(),
                homepage: None,
            }],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        settings.repo_url.is_none(),
        "unexpected settings: {settings:?}"
    );
}

#[tokio::test]
async fn sync_stack_from_compose_preserves_repo_auto_disabled_when_service_image_changes() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();

    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/worker".to_string(),
                image_tag: "latest".to_string(),
                homepage: None,
            }],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let stored = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.settings.repo_url.is_none());
    assert!(
        stored.repo_url_auto_disabled,
        "image changes should preserve explicit repo auto-backfill disable flag: {stored:?}"
    );
}

#[tokio::test]
async fn sync_stack_from_compose_updates_and_clears_homepage_metadata_when_image_unchanged() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
    labels:
      - homepage.group=Developer
      - homepage.name=Acme API
      - homepage.icon=si-github
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let homepage_updated = crate::api::types::ServiceHomepage {
        group: Some("Platform".to_string()),
        name: Some("Acme Worker".to_string()),
        icon: Some("mdi-rocket-launch".to_string()),
        href: Some("https://worker.example.com".to_string()),
        description: Some("Updated homepage metadata".to_string()),
    };

    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web:latest".to_string(),
                image_tag: "latest".to_string(),
                homepage: Some(homepage_updated.clone()),
            }],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let updated_stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let updated_service = updated_stack
        .services
        .iter()
        .find(|service| service.name == "web")
        .expect("web service after metadata update");
    assert_eq!(updated_service.homepage, Some(homepage_updated));

    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web:latest".to_string(),
                image_tag: "latest".to_string(),
                homepage: None,
            }],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let cleared_stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let cleared_service = cleared_stack
        .services
        .iter()
        .find(|service| service.name == "web")
        .expect("web service after metadata clear");
    assert!(cleared_service.homepage.is_none());
}

#[tokio::test]
async fn enqueue_startup_repo_link_backfill_only_when_eligible_and_reuses_pending_job() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();

    let first = crate::repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref())
        .await
        .unwrap();
    let second = crate::repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref())
        .await
        .unwrap();

    assert!(first.is_some());
    assert_eq!(first, second, "startup backfill job should be reused");
    let jobs = state
        .db
        .list_jobs_by_type_and_statuses(
            crate::api::types::JobType::RepoLinkBackfill,
            &["queued", "running"],
            10,
        )
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].scope, crate::api::types::JobScope::All);

    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    state
        .db
        .finish_job(
            first.as_deref().unwrap(),
            "success",
            &test_now_rfc3339(),
            &json!({}),
        )
        .await
        .unwrap();

    let none = crate::repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref())
        .await
        .unwrap();
    assert!(
        none.is_none(),
        "explicitly disabled null repoUrl rows should not enqueue startup backfill"
    );
}

#[tokio::test]
async fn enqueue_stack_repo_link_backfill_reuses_stack_scope_and_all_scope_jobs() {
    let state = test_state(":memory:").await;

    let compose_path_a = format!("/tmp/dockrev-test-a-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path_a,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_a = seed_stack_from_compose(&state, "demo-a", &compose_path_a).await;

    let first = crate::repo_link_backfill::enqueue_stack_backfill_if_needed(
        state.as_ref(),
        &stack_a,
        "discovery_sync",
    )
    .await
    .unwrap();
    let second = crate::repo_link_backfill::enqueue_stack_backfill_if_needed(
        state.as_ref(),
        &stack_a,
        "discovery_sync",
    )
    .await
    .unwrap();
    assert_eq!(
        first, second,
        "same stack should reuse pending stack backfill"
    );

    let compose_path_b = format!("/tmp/dockrev-test-b-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path_b,
        r#"
services:
  worker:
    image: ghcr.io/acme/worker:latest
"#,
    )
    .unwrap();
    let stack_b = seed_stack_from_compose(&state, "demo-b", &compose_path_b).await;
    let global = crate::repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref())
        .await
        .unwrap();
    let reused = crate::repo_link_backfill::enqueue_stack_backfill_if_needed(
        state.as_ref(),
        &stack_b,
        "discovery_sync",
    )
    .await
    .unwrap();
    assert_eq!(
        reused, global,
        "stack-scoped enqueue should yield to pending all-scope repo backfill"
    );
}
