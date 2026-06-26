#[tokio::test]
async fn cleanup_apply_creates_job_when_confirm_snapshot_only_changes_timestamp() {
    let db_path = format!("/tmp/dockrev-cleanup-apply-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state.clone());

    let scan_body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "project_deep_clean",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    let fingerprint = scan_body["confirmationFingerprint"]
        .as_str()
        .unwrap()
        .to_string();

    let apply_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/apply")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "ui",
                        "preset": "project_deep_clean",
                        "scope": "stack",
                        "stackId": stack_id,
                        "confirmationFingerprint": fingerprint,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(apply_resp.status(), 200);
    let apply_body = response_json(apply_resp).await;
    let job_id = apply_body["jobId"].as_str().unwrap().to_string();
    assert!(!job_id.is_empty());

    let queued = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(queued.r#type.as_str(), "cleanup_apply");
    assert_eq!(queued.status, "running");
    assert_eq!(queued.created_by, "ops");
    assert_eq!(queued.reason, "ui");

    let finished = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(finished.status, "success");
}

#[tokio::test]
async fn cleanup_scan_omits_builder_cache_when_no_inventory_hint_exists() {
    let db_path = format!("/tmp/dockrev-cleanup-builder-ephemeral-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::builder_cache_no_inventory_hint());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state);

    let body = serde_json::json!({
        "reason": "confirm",
        "preset": "balanced",
        "scope": "all",
    })
    .to_string();

    let first_body = wait_for_cleanup_scan_ready(&app, serde_json::from_str(&body).unwrap()).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let second_body = wait_for_cleanup_scan_ready(&app, serde_json::from_str(&body).unwrap()).await;

    assert_eq!(
        first_body["confirmationFingerprint"].as_str(),
        second_body["confirmationFingerprint"].as_str()
    );
    assert!(first_body["unownedGroup"].is_null());
    assert_eq!(first_body["estimatedReclaimableBytes"].as_u64(), Some(0));
    assert_eq!(first_body["hasUnknownSize"].as_bool(), Some(false));
}

#[tokio::test]
async fn cleanup_confirm_pending_poll_does_not_reenqueue_refresh() {
    let db_path = format!("/tmp/dockrev-cleanup-confirm-poll-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::stale_on_second_scan());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner.clone()).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state.clone());

    let initial = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "balanced",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    assert_eq!(initial["status"].as_str(), Some("ready"));
    assert_eq!(runner.stale_generation(), 1);

    let inserted = state.cleanup_snapshot_worker.enqueue().await;
    assert!(inserted);
    for _ in 0..200 {
        if state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.cleanup_snapshot_worker.is_running());

    let pending_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "confirm",
                        "refresh": false,
                        "preset": "balanced",
                        "scope": "stack",
                        "stackId": stack_id,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pending_resp.status(), 200);
    let pending_body = response_json(pending_resp).await;
    assert_eq!(pending_body["status"].as_str(), Some("pending"));
    assert_eq!(pending_body["refreshing"].as_bool(), Some(true));

    for _ in 0..200 {
        if !state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!state.cleanup_snapshot_worker.is_running());
    assert_eq!(runner.stale_generation(), 2);
}

#[tokio::test]
async fn cleanup_page_refresh_forces_background_refresh_even_with_fresh_cache() {
    let db_path = format!("/tmp/dockrev-cleanup-page-refresh-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::stale_on_second_scan());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner.clone()).await;
    let app = api::router(state.clone());

    let initial = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "page",
            "preset": "balanced",
            "scope": "all",
        }),
    )
    .await;
    assert_eq!(initial["status"].as_str(), Some("ready"));
    assert_eq!(runner.stale_generation(), 1);

    let refresh_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "refresh": true,
                        "preset": "balanced",
                        "scope": "all",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_resp.status(), 200);
    let refresh_body = response_json(refresh_resp).await;
    assert_eq!(refresh_body["status"].as_str(), Some("ready"));
    assert_eq!(refresh_body["refreshing"].as_bool(), Some(true));

    for _ in 0..200 {
        if runner.stale_generation() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(runner.stale_generation(), 2);
}

#[tokio::test]
async fn cleanup_page_stale_poll_reenqueues_refresh_when_worker_is_idle() {
    let db_path = format!("/tmp/dockrev-cleanup-page-stale-poll-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::stale_on_second_scan());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner.clone()).await;
    let app = api::router(state.clone());

    let initial = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "page",
            "preset": "balanced",
            "scope": "all",
        }),
    )
    .await;
    assert_eq!(initial["status"].as_str(), Some("ready"));
    assert_eq!(runner.stale_generation(), 1);
    assert!(!state.cleanup_snapshot_worker.is_running());

    let checked_at = test_offset_from_now_rfc3339(time::Duration::seconds(
        -(crate::cleanup_snapshot_worker::CLEANUP_CONFIRM_MAX_AGE_SECONDS + 5),
    ));
    let updated_at = test_now_rfc3339();
    state
        .db
        .upsert_cleanup_inventory_snapshot(
            crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
            &serde_json::to_string(&crate::cleanup::build_inventory_snapshot(
                state.db.clone(),
                state.runner.clone(),
            )
            .await
            .unwrap())
            .unwrap(),
            &checked_at,
            &updated_at,
        )
        .await
        .unwrap();

    let poll_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "refresh": false,
                        "preset": "balanced",
                        "scope": "all",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(poll_resp.status(), 200);
    let poll_body = response_json(poll_resp).await;
    assert_eq!(poll_body["status"].as_str(), Some("ready"));
    assert_eq!(poll_body["refreshing"].as_bool(), Some(true));

    for _ in 0..200 {
        if runner.stale_generation() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(runner.stale_generation(), 3);
}

#[tokio::test]
async fn cleanup_page_returns_error_after_initial_refresh_failure_until_explicit_retry() {
    let db_path = format!("/tmp/dockrev-cleanup-initial-refresh-failure-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    state
        .cleanup_snapshot_worker
        .set_last_error_for_test(Some("boom".to_string()))
        .await;
    let app = api::router(state.clone());

    let failing_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "refresh": false,
                        "preset": "aggressive",
                        "scope": "all",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failing_resp.status(), 500);
    let failing_body = response_json(failing_resp).await;
    assert_eq!(failing_body["error"]["code"], "internal");
    assert_eq!(
        failing_body["error"]["message"].as_str(),
        Some("cleanup snapshot refresh failed: boom")
    );
    assert!(!state.cleanup_snapshot_worker.is_running());

    let refresh_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "refresh": true,
                        "preset": "aggressive",
                        "scope": "all",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_resp.status(), 200);
    let refresh_body = response_json(refresh_resp).await;
    assert_eq!(refresh_body["status"].as_str(), Some("pending"));
    assert_eq!(refresh_body["refreshing"].as_bool(), Some(true));

    let ready_body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "page",
            "preset": "aggressive",
            "scope": "all",
        }),
    )
    .await;
    assert_eq!(ready_body["status"].as_str(), Some("ready"));
    assert_eq!(ready_body["refreshing"].as_bool(), Some(false));
}

#[tokio::test]
async fn cleanup_page_returns_error_for_stale_cached_snapshot_after_refresh_failure() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-stale-refresh-failure-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let snapshot = crate::cleanup::build_inventory_snapshot(state.db.clone(), state.runner.clone())
        .await
        .unwrap();
    let checked_at = test_offset_from_now_rfc3339(time::Duration::seconds(
        -(crate::cleanup_snapshot_worker::CLEANUP_CONFIRM_MAX_AGE_SECONDS + 5),
    ));
    let updated_at = test_now_rfc3339();
    state
        .db
        .upsert_cleanup_inventory_snapshot(
            crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
            &serde_json::to_string(&snapshot).unwrap(),
            &checked_at,
            &updated_at,
        )
        .await
        .unwrap();
    state
        .cleanup_snapshot_worker
        .set_last_error_for_test(Some("boom".to_string()))
        .await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "refresh": false,
                        "preset": "balanced",
                        "scope": "all",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "internal");
    assert_eq!(
        body["error"]["message"].as_str(),
        Some("cleanup snapshot refresh failed: boom")
    );
    assert!(!state.cleanup_snapshot_worker.is_running());
}

#[tokio::test]
async fn cleanup_scan_keeps_stable_fingerprint_when_builder_cache_falls_back_to_text_summary() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-builder-text-ephemeral-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::builder_cache_text_fallback());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state);

    let body = serde_json::json!({
        "reason": "confirm",
        "preset": "balanced",
        "scope": "all",
    })
    .to_string();

    let first_body = wait_for_cleanup_scan_ready(&app, serde_json::from_str(&body).unwrap()).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let second_body = wait_for_cleanup_scan_ready(&app, serde_json::from_str(&body).unwrap()).await;

    assert_eq!(
        first_body["confirmationFingerprint"].as_str(),
        second_body["confirmationFingerprint"].as_str()
    );
    assert_eq!(
        first_body["unownedGroup"]["resources"][0]["kind"].as_str(),
        Some("builder_cache")
    );
}

#[tokio::test]
async fn cleanup_apply_returns_stale_snapshot_with_latest_confirm_payload() {
    let db_path = format!("/tmp/dockrev-cleanup-stale-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::stale_on_second_scan());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner.clone()).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state.clone());

    let scan_body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "balanced",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    let fingerprint = scan_body["confirmationFingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(runner.stale_generation(), 1);

    let inserted = state.cleanup_snapshot_worker.enqueue().await;
    assert!(inserted);
    for _ in 0..200 {
        if runner.stale_generation() >= 2 && !state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(runner.stale_generation(), 2);
    assert!(!state.cleanup_snapshot_worker.is_running());

    let apply_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/apply")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "ui",
                        "preset": "balanced",
                        "scope": "stack",
                        "stackId": stack_id,
                        "confirmationFingerprint": fingerprint,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(apply_resp.status(), 409);
    let apply_body = response_json(apply_resp).await;
    assert_eq!(apply_body["error"]["code"], "cleanup_snapshot_stale");
    assert_ne!(
        apply_body["error"]["details"]["latest"]["confirmationFingerprint"]
            .as_str()
            .unwrap(),
        scan_body["confirmationFingerprint"].as_str().unwrap()
    );
    assert_eq!(
        apply_body["error"]["details"]["latest"]["scope"].as_str(),
        Some("stack")
    );
    assert_eq!(
        apply_body["error"]["details"]["latest"]["stackGroups"][0]["stackId"].as_str(),
        Some(stack_id.as_str())
    );
    assert_eq!(runner.stale_generation(), 2);
}

#[tokio::test]
async fn cleanup_apply_without_snapshot_returns_stale_pending_payload_and_enqueues_refresh() {
    let db_path = format!("/tmp/dockrev-cleanup-apply-missing-snapshot-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state.clone());

    let apply_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/apply")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "ui",
                        "preset": "balanced",
                        "scope": "stack",
                        "stackId": stack_id,
                        "confirmationFingerprint": "missing",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(apply_resp.status(), 409);
    let apply_body = response_json(apply_resp).await;
    assert_eq!(apply_body["error"]["code"], "cleanup_snapshot_stale");
    assert_eq!(apply_body["error"]["details"]["latest"]["status"].as_str(), Some("pending"));
    assert_eq!(apply_body["error"]["details"]["latest"]["refreshing"].as_bool(), Some(true));
    assert_eq!(apply_body["error"]["details"]["latest"]["scope"].as_str(), Some("stack"));

    for _ in 0..200 {
        if state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.cleanup_snapshot_worker.is_running());
}

#[tokio::test]
async fn cleanup_job_summary_records_skipped_in_use_volume() {
    let db_path = format!("/tmp/dockrev-cleanup-volume-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let request = crate::api::types::CleanupScanRequest {
        reason: crate::api::types::CleanupScanReason::Confirm,
        preset: crate::api::types::CleanupPreset::ProjectDeepClean,
        refresh: true,
        scope: crate::api::types::CleanupScope::Stack,
        stack_id: Some(stack_id.clone()),
        service_id: None,
    };
    let snapshot =
        crate::cleanup::build_inventory_snapshot(state.db.clone(), state.runner.clone())
            .await
            .unwrap();
    let plan = crate::cleanup::build_execution_plan_from_snapshot(
        &snapshot,
        &request,
        &test_now_rfc3339(),
    )
    .unwrap();
    let response = plan.to_response(crate::api::types::CleanupScanReason::Confirm);
    assert_eq!(response.stack_groups.len(), 1);
    assert_eq!(response.stack_groups[0].stack_orphans.len(), 1);

    let job_id = ids::new_job_id();
    let mut job = crate::models::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::CleanupApply,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &test_now_rfc3339(),
    );
    job.summary_json = plan.initial_job_summary();
    let mut job_db = job.to_db();
    job_db.created_by = "ops".to_string();
    job_db.reason = "ui".to_string();
    state.db.insert_job(job_db).await.unwrap();

    crate::cleanup::run_cleanup_job(state.clone(), &job_id, plan)
        .await
        .unwrap();

    let stored = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(stored.status, "success");
    assert_eq!(
        stored.summary_json["deletedCountsByKind"],
        serde_json::json!({})
    );
    assert_eq!(
        stored.summary_json["skippedInUse"][0]["kind"].as_str(),
        Some("volume")
    );
    assert_eq!(
        stored.summary_json["skippedInUse"][0]["label"].as_str(),
        Some("demo_named")
    );
    assert_eq!(
        stored.summary_json["skippedInUse"][0]["reason"].as_str(),
        Some("still_attached")
    );
}

#[tokio::test]
async fn cleanup_scan_uses_system_df_volume_size_when_usage_data_is_missing() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-volume-fallback-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::volume_estimate_fallback());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state);

    let body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "project_deep_clean",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    assert_eq!(
        body["stackGroups"][0]["services"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(128_000_000)
    );
    assert_eq!(
        body["stackGroups"][0]["services"][0]["hasUnknownSize"].as_bool(),
        Some(false)
    );
    assert_eq!(
        body["serverDiskUsage"],
        serde_json::json!({
            "usedBytes": 40_587_440_947_u64,
            "totalBytes": 85_899_345_920_u64
        })
    );
}

#[tokio::test]
async fn cleanup_scan_uses_mountpoint_du_when_volume_metadata_has_no_size() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-volume-mountpoint-fallback-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::volume_mountpoint_fallback());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state);

    let body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "project_deep_clean",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    assert_eq!(
        body["stackGroups"][0]["services"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(1_572_864)
    );
    assert_eq!(
        body["stackGroups"][0]["services"][0]["hasUnknownSize"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn cleanup_scan_omits_volumes_without_stable_identity() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-volume-missing-identity-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::volume_missing_identity());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let (stack_id, _service_id, _compose_path) = seed_cleanup_stack(
        &state,
        "demo",
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .await;
    let app = api::router(state);

    let body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "project_deep_clean",
            "scope": "stack",
            "stackId": stack_id,
        }),
    )
    .await;
    assert_eq!(body["stackGroups"], serde_json::json!([]));
    assert!(body["unownedGroup"].is_null());
    assert_eq!(body["estimatedReclaimableBytes"].as_u64(), Some(0));
    assert_eq!(body["hasUnknownSize"].as_bool(), Some(false));
}

#[tokio::test]
async fn cleanup_scan_uses_builder_cache_summary_when_json_includes_shared_rows() {

    let db_path = format!(
        "/tmp/dockrev-cleanup-builder-fallback-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::builder_cache_shared_lower_bound());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state);

    let body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "page",
            "preset": "balanced",
            "scope": "all"
        }),
    )
    .await;
    assert_eq!(
        body["estimatedReclaimableBytes"].as_u64(),
        Some(384_000_000)
    );
    assert_eq!(body["hasUnknownSize"].as_bool(), Some(false));
    assert_eq!(
        body["unownedGroup"]["resources"][0]["kind"].as_str(),
        Some("builder_cache")
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(384_000_000)
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimateUnknown"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn cleanup_scan_keeps_builder_cache_estimate_when_json_falls_back_to_text_summary() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-builder-text-fallback-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::builder_cache_text_fallback());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state);

    let body = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "page",
            "preset": "balanced",
            "scope": "all"
        }),
    )
    .await;
    assert_eq!(
        body["estimatedReclaimableBytes"].as_u64(),
        Some(384_000_000)
    );
    assert_eq!(body["hasUnknownSize"].as_bool(), Some(false));
    assert_eq!(
        body["unownedGroup"]["resources"][0]["kind"].as_str(),
        Some("builder_cache")
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(384_000_000)
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimateUnknown"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn service_rollback_target_diagnostics_capture_available_resolution() {
    let state = test_state(":memory:").await;

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let source_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        "2026-04-05T00:01:00Z",
        "2026-04-05T00:02:00Z",
        make_update_history_summary_for_test(&stack_id, &service_id, "sha256:old", "sha256:new"),
    )
    .await;

    let resolved = crate::api::resolve_service_rollback_target(&state, &service_id)
        .await
        .unwrap();
    let diagnostics = crate::api::build_rollback_resolution_diagnostics(
        crate::api::RollbackResolutionRequestKind::GetTarget,
        &resolved,
    );

    assert_eq!(diagnostics.request_kind.as_str(), "get_target");
    assert_eq!(diagnostics.service_id, service_id);
    assert_eq!(diagnostics.stack_id, stack_id);
    assert_eq!(diagnostics.current_digest, "sha256:new");
    assert!(diagnostics.available);
    assert_eq!(diagnostics.unavailable_reason, None);
    assert_eq!(diagnostics.target_digest.as_deref(), Some("sha256:old"));
    assert_eq!(
        diagnostics.source_update_job_id.as_deref(),
        Some(source_job_id.as_str())
    );
    assert_eq!(diagnostics.active_job_id, None);
    assert_eq!(diagnostics.active_job_status, None);
    assert_eq!(diagnostics.scanned_successful_updates, 1);
}

#[tokio::test]
async fn service_rollback_target_diagnostics_capture_unavailable_resolution() {
    let state = test_state(":memory:").await;

    let (_stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let resolved = crate::api::resolve_service_rollback_target(&state, &service_id)
        .await
        .unwrap();
    let diagnostics = crate::api::build_rollback_resolution_diagnostics(
        crate::api::RollbackResolutionRequestKind::TriggerRollback,
        &resolved,
    );

    assert_eq!(diagnostics.request_kind.as_str(), "trigger_rollback");
    assert_eq!(diagnostics.service_id, service_id);
    assert_eq!(diagnostics.current_digest, "sha256:new");
    assert!(!diagnostics.available);
    assert_eq!(
        diagnostics.unavailable_reason.as_deref(),
        Some("no_matching_update_history")
    );
    assert_eq!(diagnostics.target_digest, None);
    assert_eq!(diagnostics.source_update_job_id, None);
    assert_eq!(diagnostics.active_job_id, None);
    assert_eq!(diagnostics.active_job_status, None);
    assert_eq!(diagnostics.scanned_successful_updates, 0);
}

#[tokio::test]
async fn service_rollback_target_reports_current_digest_missing_consistently() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = "2026-04-05T00:10:00Z";
    state
        .db
        .update_service_check_result(
            &service_id,
            None,
            None,
            None,
            Some("5.2".to_string()),
            None,
            None,
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();

    let resolved = crate::api::resolve_service_rollback_target(&state, &service_id)
        .await
        .unwrap();
    let diagnostics = crate::api::build_rollback_resolution_diagnostics(
        crate::api::RollbackResolutionRequestKind::GetTarget,
        &resolved,
    );
    assert_eq!(diagnostics.stack_id, stack_id);
    assert_eq!(diagnostics.current_digest, "");
    assert!(!diagnostics.available);
    assert_eq!(
        diagnostics.unavailable_reason.as_deref(),
        Some("current_digest_missing")
    );
    assert_eq!(diagnostics.scanned_successful_updates, 0);

    let get_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/rollback-target"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    let get_payload = response_json(get_resp).await;
    assert_eq!(
        get_payload["unavailableReason"].as_str(),
        Some("current_digest_missing")
    );

    let post_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/rollback"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post_resp.status(), 409);
    let post_payload = response_json(post_resp).await;
    assert_eq!(
        post_payload["error"]["details"]["reason"].as_str(),
        Some("current_digest_missing")
    );
}
