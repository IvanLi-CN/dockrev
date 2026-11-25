#[tokio::test]
async fn cleanup_apply_accepts_fresh_snapshot_while_refresh_runs() {
    let db_path = format!(
        "/tmp/dockrev-cleanup-apply-during-refresh-{}.sqlite3",
        ulid::Ulid::new()
    );
    let runner = Arc::new(CleanupRunner::slow_stale_on_second_scan());
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
    let fingerprint = initial["confirmationFingerprint"]
        .as_str()
        .expect("ready confirmation includes fingerprint")
        .to_string();

    assert!(state.cleanup_snapshot_worker.enqueue().await);
    for _ in 0..200 {
        if state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(state.cleanup_snapshot_worker.is_running());

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
    assert_eq!(apply_resp.status(), 200);
    let apply_body = response_json(apply_resp).await;
    let job_id = apply_body["jobId"].as_str().expect("apply returns a job id");
    assert!(!job_id.is_empty());

    for _ in 0..300 {
        if !state.cleanup_snapshot_worker.is_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!state.cleanup_snapshot_worker.is_running());
}
