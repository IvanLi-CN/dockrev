#[tokio::test]
async fn all_apply_rejects_when_scope_contains_guarded_service() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let (_, service_id, ..) = seed_guarded_service_with_candidate(&state).await;

    let req = serde_json::json!({
        "scope": "all",
        "targets": [],
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(req.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str(), Some("update_guard_blocked"));
    assert_eq!(
        body["error"]["details"]["blockedServiceIds"][0].as_str(),
        Some(service_id.as_str())
    );
}

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

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;

    assert_eq!(
        first_body["confirmationFingerprint"].as_str(),
        second_body["confirmationFingerprint"].as_str()
    );
    assert!(first_body["unownedGroup"].is_null());
    assert_eq!(first_body["estimatedReclaimableBytes"].as_u64(), Some(0));
    assert_eq!(first_body["hasUnknownSize"].as_bool(), Some(false));
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

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/scan")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;

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
