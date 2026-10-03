#[tokio::test]
async fn cleanup_apply_rejects_future_estimate_version_and_enqueues_refresh() {
    let db_path = format!("/tmp/dockrev-cleanup-future-snapshot-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::volume_in_use());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner).await;
    let app = api::router(state.clone());
    let scan = wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "preset": "aggressive",
            "scope": "all",
        }),
    )
    .await;
    let row = state
        .db
        .get_cleanup_inventory_snapshot(crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY)
        .await
        .unwrap()
        .unwrap();
    let mut future_json = serde_json::from_str::<serde_json::Value>(&row.snapshot_json).unwrap();
    future_json["estimateVersion"] = serde_json::json!(
        crate::api::types::CLEANUP_ESTIMATE_VERSION + 1
    );
    let observed_future_snapshot = future_json.to_string();
    state
        .db
        .upsert_cleanup_inventory_snapshot(
            crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
            &observed_future_snapshot,
            &test_now_rfc3339(),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cleanups/apply")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "reason": "ui",
                        "preset": "aggressive",
                        "scope": "all",
                        "confirmationFingerprint": scan["confirmationFingerprint"],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "cleanup_snapshot_stale");
    assert_eq!(body["error"]["details"]["latest"]["status"], "pending");
    for _ in 0..200 {
        if state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.cleanup_snapshot_worker.is_running());
}

#[tokio::test]
async fn cleanup_confirm_future_estimate_version_does_not_requeue_running_refresh() {
    let db_path = format!("/tmp/dockrev-cleanup-future-poll-{}.sqlite3", ulid::Ulid::new());
    let runner = Arc::new(CleanupRunner::slow_stale_on_second_scan());
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), runner.clone()).await;
    let app = api::router(state.clone());
    let confirm = serde_json::json!({
        "reason": "confirm",
        "refresh": false,
        "preset": "aggressive",
        "scope": "all",
    });
    wait_for_cleanup_scan_ready(
        &app,
        serde_json::json!({
            "reason": "confirm",
            "refresh": true,
            "preset": "aggressive",
            "scope": "all",
        }),
    )
    .await;
    assert_eq!(runner.stale_generation(), 1);

    let row = state
        .db
        .get_cleanup_inventory_snapshot(crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY)
        .await
        .unwrap()
        .unwrap();
    let mut future_json = serde_json::from_str::<serde_json::Value>(&row.snapshot_json).unwrap();
    future_json["estimateVersion"] = serde_json::json!(
        crate::api::types::CLEANUP_ESTIMATE_VERSION + 1
    );
    let observed_future_snapshot = future_json.to_string();
    state
        .db
        .upsert_cleanup_inventory_snapshot(
            crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
            &observed_future_snapshot,
            &test_now_rfc3339(),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    assert!(state.cleanup_snapshot_worker.enqueue().await);
    for _ in 0..200 {
        if state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.cleanup_snapshot_worker.is_running());

    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/cleanups/scan")
                    .header("X-Forwarded-User", "ops")
                    .header("content-type", "application/json")
                    .body(Body::from(confirm.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response_json(response).await["status"], "pending");
    }

    for _ in 0..200 {
        if !state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!state.cleanup_snapshot_worker.is_running());
    assert_eq!(runner.stale_generation(), 2);
    assert!(
        !state
            .cleanup_snapshot_worker
            .enqueue_if_snapshot_unchanged(&observed_future_snapshot)
            .await
            .unwrap()
    );
    assert_eq!(runner.stale_generation(), 2);
}
