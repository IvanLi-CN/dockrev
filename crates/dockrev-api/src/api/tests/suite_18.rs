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

    let scan_resp = app
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
                        "preset": "project_deep_clean",
                        "scope": "stack",
                        "stackId": stack_id,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(scan_resp.status(), 200);
    let scan_body = response_json(scan_resp).await;
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

    let scan_resp = app
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
    assert_eq!(scan_resp.status(), 200);
    let scan_body = response_json(scan_resp).await;
    let fingerprint = scan_body["confirmationFingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(runner.stale_generation(), 1);

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
    let plan = crate::cleanup::build_execution_plan(
        state.as_ref(),
        &crate::api::types::CleanupScanRequest {
            reason: crate::api::types::CleanupScanReason::Confirm,
            preset: crate::api::types::CleanupPreset::ProjectDeepClean,
            scope: crate::api::types::CleanupScope::Stack,
            stack_id: Some(stack_id.clone()),
            service_id: None,
        },
        &test_now_rfc3339(),
    )
    .await
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

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "confirm",
                        "preset": "project_deep_clean",
                        "scope": "stack",
                        "stackId": stack_id,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["stackGroups"][0]["services"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(128_000_000)
    );
    assert_eq!(
        body["stackGroups"][0]["services"][0]["hasUnknownSize"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn cleanup_scan_preserves_builder_cache_lower_bound_when_json_includes_shared_rows() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-builder-fallback-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::builder_cache_shared_lower_bound());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "preset": "balanced",
                        "scope": "all"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["estimatedReclaimableBytes"].as_u64(),
        Some(256_000_000)
    );
    assert_eq!(body["hasUnknownSize"].as_bool(), Some(true));
    assert_eq!(
        body["unownedGroup"]["resources"][0]["kind"].as_str(),
        Some("builder_cache")
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimatedReclaimableBytes"].as_u64(),
        Some(256_000_000)
    );
    assert_eq!(
        body["unownedGroup"]["resources"][0]["estimateUnknown"].as_bool(),
        Some(true)
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

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "page",
                        "preset": "balanced",
                        "scope": "all"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
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
