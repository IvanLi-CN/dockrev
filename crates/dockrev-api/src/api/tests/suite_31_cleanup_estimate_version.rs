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
    state
        .db
        .upsert_cleanup_inventory_snapshot(
            crate::cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
            &future_json.to_string(),
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
