#[tokio::test]
async fn transient_runtime_unknown_does_not_reopen_same_digest_notification() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestOnlyUpdateRegistry),
        Arc::new(FakeRunner),
    )
    .await;

    let compose_path = format!("/tmp/dockrev-transient-runtime-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    let first_check_now = "2026-03-09T00:00:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-1",
        &service,
        Some(
            crate::service_check::RuntimeServiceObservation::digest_only("sha256:old".to_string()),
        ),
        "linux/amd64",
        first_check_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "5.2".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "5.2".to_string(),
        candidate_tag: "5.2".to_string(),
        candidate_display_tag: "5.2".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let first_job_id = insert_check_job(&state, "schedule", first_check_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &first_job_id,
        "schedule",
        first_check_now,
        1,
        &discovered,
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("first webhook receive timeout")
        .expect("first notification payload missing");

    let uncertain_now = "2026-03-09T00:01:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-2",
        &service,
        None,
        "linux/amd64",
        uncertain_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let restored_now = "2026-03-09T00:02:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-3",
        &service,
        Some(
            crate::service_check::RuntimeServiceObservation::digest_only("sha256:old".to_string()),
        ),
        "linux/amd64",
        restored_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let second_job_id = insert_check_job(&state, "schedule", restored_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &second_job_id,
        "schedule",
        restored_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "same digest should remain deduped after an inconclusive runtime check"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "sent");
    assert_eq!(rows[0].superseded_at, None);

    server.abort();
}

#[tokio::test]
async fn failed_new_version_notification_record_retries_after_all_channels_fail() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-failed-notify-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];

    let failing_app = Router::new().route(
        "/hook",
        post(|| async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let failing_server = tokio::spawn(async move {
        axum::serve(listener, failing_app).await.unwrap();
    });
    let fail_now = "2026-03-09T00:00:00Z";
    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some(format!("http://{addr}/hook"));
    notification.event_new_version_enabled = true;
    state
        .db
        .put_notification_settings(&notification, fail_now)
        .await
        .unwrap();

    let failed_job_id = insert_check_job(&state, "schedule", fail_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &failed_job_id,
        "schedule",
        fail_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "failed");
    assert!(
        rows[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("webhook")
    );

    let (mut rx, success_server) = configure_webhook_notifications(&state).await;
    let retry_now = "2026-03-09T00:01:00Z";
    let retry_job_id = insert_check_job(&state, "schedule", retry_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &retry_job_id,
        "schedule",
        retry_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["check"]["jobId"].as_str(),
        Some(retry_job_id.as_str())
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "failed");
    assert_eq!(rows[1].status, "sent");

    success_server.abort();
    failing_server.abort();
}

#[tokio::test]
async fn ui_reason_check_does_not_send_new_version_notification() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ui-check-notify-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-ui-silent").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "service",
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "reason": "ui"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["checkId"].as_str().unwrap().to_string();
    let _job = wait_for_job_terminal(&state, &job_id).await;

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "ui check should not emit new-version notifications"
    );

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(!logs.iter().any(|line| line.msg.contains("notify:")));
    server.abort();
}

#[tokio::test]
async fn github_packages_repo_selected_upsert_is_case_insensitive_and_preserves_sync_state() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed a selected repo with mixed casing + a sync state.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("Acme"), String::from("Widgets"), true)],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_sync_result("Acme", "Widgets", Some(42), Some(&now), None, &now)
        .await
        .unwrap();

    // Toggle selection using different casing. This should update the existing row, not insert a
    // second case-variant duplicate, and should preserve sync state.
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", false, &now)
        .await
        .unwrap();

    let repos = state.db.list_github_packages_repos().await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].owner, "Acme");
    assert_eq!(repos[0].repo, "Widgets");
    assert!(!repos[0].selected);
    assert_eq!(repos[0].hook_id, Some(42));
}

#[tokio::test]
async fn github_packages_repo_selected_enqueues_register_job_when_enabled() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("Acme", "Widgets", false, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/selected")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "fullName": "acme/widgets",
                        "selected": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(
        !job_id.is_empty(),
        "selected=true should enqueue a register job and return jobId"
    );

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.r#type.as_str(), "github_packages_webhook");
    assert_eq!(job.status, "queued");
    assert!(job.started_at.is_none());

    let repo = state
        .db
        .get_github_packages_repo("acme", "widgets")
        .await
        .unwrap()
        .unwrap();
    assert!(repo.selected);
    assert_eq!(repo.webhook_state, "queued");
    assert_eq!(repo.webhook_job_id.as_deref(), Some(job_id.as_str()));
    assert_eq!(repo.last_op.as_deref(), Some("register"));
}

#[tokio::test]
async fn github_packages_repo_delete_enqueues_unregister_job_and_keeps_row_until_worker_finishes() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("Acme"), String::from("Widgets"), true)],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_sync_result(
            "Acme",
            "Widgets",
            Some(12345),
            Some(&now),
            None,
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/delete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "fullName": "acme/widgets"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!job_id.is_empty());

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.r#type.as_str(), "github_packages_webhook");
    assert_eq!(job.status, "queued");

    let repo = state
        .db
        .get_github_packages_repo("acme", "widgets")
        .await
        .unwrap();
    let repo = repo.expect("row should remain until unregister worker succeeds");
    assert_eq!(repo.webhook_state, "queued");
    assert_eq!(repo.webhook_job_id.as_deref(), Some(job_id.as_str()));
    assert_eq!(repo.last_op.as_deref(), Some("unregister"));
}

#[tokio::test]
async fn github_packages_webhook_sync_all_enqueues_and_reuses_pending_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;
    assert_eq!(first_body["ok"], true);
    assert_eq!(first_body["reused"], false);
    assert_eq!(first_body["status"], "queued");
    let first_job_id = first_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!first_job_id.is_empty());

    let first_job = state.db.get_job(&first_job_id).await.unwrap().unwrap();
    assert_eq!(
        first_job.r#type.as_str(),
        "github_packages_webhook_sync_all"
    );
    assert_eq!(first_job.status, "queued");
    assert_eq!(first_job.summary_json["op"], "sync_all");
    assert_eq!(
        first_job.summary_json["repos"].as_array().map(|v| v.len()),
        Some(2)
    );

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;
    assert_eq!(second_body["ok"], true);
    assert_eq!(second_body["reused"], true);
    assert_eq!(second_body["jobId"], first_job_id);
    assert_eq!(second_body["status"], "queued");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_enqueues_and_dedupes_by_repo() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;
    assert_eq!(first_body["ok"], true);
    assert_eq!(first_body["reused"], false);
    let first_job_id = first_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!first_job_id.is_empty());

    let first_job = state.db.get_job(&first_job_id).await.unwrap().unwrap();
    assert_eq!(
        first_job.r#type.as_str(),
        "github_packages_webhook_sync_repo"
    );
    assert_eq!(first_job.status, "queued");
    assert_eq!(first_job.service_id.as_deref(), Some("acme/widgets"));

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;
    assert_eq!(second_body["ok"], true);
    assert_eq!(second_body["reused"], true);
    assert_eq!(second_body["jobId"], first_job_id);

    let third = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/worker" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(third.status(), 200);
    let third_body = response_json(third).await;
    assert_eq!(third_body["ok"], true);
    assert_eq!(third_body["reused"], false);
    let third_job_id = third_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!third_job_id.is_empty());
    assert_ne!(third_job_id, first_job_id);
}

#[tokio::test]
async fn github_packages_webhook_sync_all_returns_400_when_no_selected_repos() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "invalid_argument");
    assert_eq!(body["error"]["message"], "no tracked repos selected");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_404_for_untracked_repo() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/worker" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "repo is not tracked");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_400_for_invalid_full_name() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "invalid-full-name" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "invalid_argument");
    assert_eq!(body["error"]["message"], "invalid fullName");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_409_when_unregister_in_progress() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/delete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(body["error"]["message"], "repo unregister in progress");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_reuses_pending_legacy_register_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", false, &now)
        .await
        .unwrap();

    let selected = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/selected")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets", "selected": true }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(selected.status(), 200);
    let selected_body = response_json(selected).await;
    let legacy_job_id = selected_body["jobId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!legacy_job_id.is_empty());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["reused"], true);
    assert_eq!(body["jobId"], legacy_job_id);
    assert_eq!(body["status"], "queued");
}

#[tokio::test]
async fn github_packages_webhook_sync_all_ignores_repos_with_unregister_pending() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_job_state(
            "acme",
            "widgets",
            "queued",
            Some("job_unregister_demo"),
            Some("unregister"),
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["reused"], false);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!job_id.is_empty());

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    let repos = job.summary_json["repos"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].as_str(), Some("acme/worker"));
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_can_enqueue_while_sync_all_pending() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let full = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(full.status(), 200);
    let full_body = response_json(full).await;
    let full_job_id = full_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!full_job_id.is_empty());

    let repo = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repo.status(), 200);
    let repo_body = response_json(repo).await;
    assert_eq!(repo_body["reused"], false);
    let repo_job_id = repo_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!repo_job_id.is_empty());
    assert_ne!(repo_job_id, full_job_id);
}

#[tokio::test]
async fn github_packages_webhook_overview_reports_repo_and_job_summary() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc();
    let now_s = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let audit_older = (now - time::Duration::hours(2))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let audit_newer = (now - time::Duration::hours(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("ok-repo"), true),
                (String::from("acme"), String::from("missing-repo"), true),
                (String::from("acme"), String::from("error-repo"), true),
                (String::from("acme"), String::from("unselected-repo"), false),
            ],
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "ok-repo",
            "ok",
            Some(111),
            Some(&now_s),
            Some(&audit_older),
            None,
            None,
            Some("register"),
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "missing-repo",
            "missing",
            None,
            None,
            Some(&audit_newer),
            Some("webhook missing"),
            None,
            Some("audit_all"),
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "error-repo",
            "error",
            None,
            None,
            None,
            Some("permission denied"),
            None,
            Some("register"),
            &now_s,
        )
        .await
        .unwrap();

    let queued_job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: queued_job_id,
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now_s.clone(),
            created_by: "ivan".to_string(),
            reason: "ui".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({"op":"register","repos":["acme/ok-repo"]}),
        })
        .await
        .unwrap();

    let running_job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: running_job_id.clone(),
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "running".to_string(),
            created_at: (now + time::Duration::seconds(1))
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
            created_by: "schedule".to_string(),
            reason: "schedule".to_string(),
            started_at: Some(now_s.clone()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({"op":"audit_all","repos":[]}),
        })
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/overview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;

    assert_eq!(body["summary"]["tracked"], 3);
    assert_eq!(body["summary"]["ok"], 1);
    assert_eq!(body["summary"]["missing"], 1);
    assert_eq!(body["summary"]["error"], 1);
    assert_eq!(body["summary"]["conflict"], 0);
    assert_eq!(body["jobsQueued"], 1);
    assert_eq!(body["jobsRunning"], 1);
    assert_eq!(body["runningJobId"].as_str(), Some(running_job_id.as_str()));
    assert_eq!(body["lastAuditAt"].as_str(), Some(audit_newer.as_str()));
}

#[tokio::test]
async fn github_packages_repos_support_state_filter_search_and_pagination() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let now = test_now_rfc3339();

    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("alpha"), true),
                (String::from("acme"), String::from("beta"), true),
                (String::from("acme"), String::from("gamma"), true),
                (String::from("acme"), String::from("unselected"), false),
            ],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "alpha",
            "ok",
            Some(4242),
            Some(&now),
            None,
            None,
            None,
            Some("register"),
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "beta",
            "missing",
            None,
            None,
            Some(&now),
            Some("webhook absent"),
            None,
            Some("audit_all"),
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "gamma",
            "error",
            None,
            None,
            None,
            Some("timeout-key"),
            None,
            Some("register"),
            &now,
        )
        .await
        .unwrap();

    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?selectedFilter=selected&webhookState=missing&page=1&perPage=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(filtered.status(), 200);
    let body = response_json(filtered).await;
    assert_eq!(body["total"], 4);
    assert_eq!(body["selectedTotal"], 3);
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["repos"].as_array().map(|rows| rows.len()), Some(1));
    assert_eq!(body["repos"][0]["fullName"], "acme/beta");

    for (query, full_name) in [
        ("alpha", "acme/alpha"),
        ("missing", "acme/beta"),
        ("4242", "acme/alpha"),
        ("timeout-key", "acme/gamma"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/github-packages/repos?selectedFilter=selected&q={query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = response_json(response).await;
        assert_eq!(body["filteredTotal"], 1, "unexpected query result for {query}");
        assert_eq!(body["repos"][0]["fullName"], full_name, "unexpected query match for {query}");
    }

    let paged = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?selectedFilter=selected&page=2&perPage=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(paged.status(), 200);
    let body = response_json(paged).await;
    assert_eq!(body["page"], 2);
    assert_eq!(body["perPage"], 1);
    assert_eq!(body["filteredTotal"], 3);
    assert_eq!(body["repos"][0]["fullName"], "acme/beta");

    let unfiltered = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?selectedFilter=selected&webhookState=all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unfiltered.status(), 200);
    assert_eq!(response_json(unfiltered).await["filteredTotal"], 3);

    let omitted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?selectedFilter=selected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(omitted.status(), 200);
    assert_eq!(response_json(omitted).await["filteredTotal"], 3);

    let capped = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?selectedFilter=selected&perPage=999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capped.status(), 200);
    assert_eq!(response_json(capped).await["perPage"], 200);

    let invalid = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/repos?webhookState=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    assert_eq!(response_json(invalid).await["error"]["code"], "invalid_argument");
}
