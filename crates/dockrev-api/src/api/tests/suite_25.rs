#[tokio::test]
async fn update_stop_endpoint_accepts_once_then_conflicts_and_audits_requester() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state.clone());
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        Some("stack-1".to_string()),
        Some("service-1".to_string()),
        &now,
    )
    .to_db();
    job.summary_json = json!({ "mode": "apply" });
    state.db.insert_job(job).await.unwrap();
    state
        .db
        .create_update_stop_control(&job_id, &now)
        .await
        .unwrap();

    let request = || {
        Request::builder()
            .method("POST")
            .uri(format!("/api/jobs/{job_id}/stop"))
            .header("X-Forwarded-User", "alice")
            .body(Body::empty())
            .unwrap()
    };
    let first = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(first.status(), 202);
    let first_body = first.into_body().collect().await.unwrap().to_bytes();
    let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    assert_eq!(first_json["jobId"], job_id);
    assert_eq!(first_json["state"], "requested");

    assert_eq!(app.oneshot(request()).await.unwrap().status(), 409);
    let control = state.db.get_update_stop_control(&job_id).await.unwrap().unwrap();
    assert_eq!(control.stop_requested_by.as_deref(), Some("alice"));
}

#[tokio::test]
async fn update_stop_endpoint_rejects_unauthorized_requests() {
    let state = test_state_auth_required(":memory:").await;
    let app = api::router(state.clone());
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        Some("stack-1".to_string()),
        Some("service-1".to_string()),
        &now,
    )
    .to_db();
    job.summary_json = json!({ "mode": "apply" });
    state.db.insert_job(job).await.unwrap();
    state
        .db
        .create_update_stop_control(&job_id, &now)
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/jobs/{job_id}/stop"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
