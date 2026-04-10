#[tokio::test]
async fn github_packages_webhook_does_not_reuse_covering_stack_check_job() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-stack-running-{}.yml",
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
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-stack-running").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_check_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &now,
    )
    .to_db();
    existing.created_by = "ivan".to_string();
    existing.reason = "ui".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-stack-running-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert_ne!(job_id, existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));

    let existing = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(existing.scope.as_str(), "stack");
    assert_eq!(existing.status, "running");
}

#[tokio::test]
async fn github_packages_webhook_does_not_reuse_covering_all_check_job() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-all-running-{}.yml",
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
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-all-running").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_check_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    existing.created_by = "ivan".to_string();
    existing.reason = "ui".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-all-running-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert_ne!(job_id, existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));

    let existing = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(existing.scope.as_str(), "all");
    assert_eq!(existing.status, "running");
}

#[tokio::test]
async fn github_packages_webhook_dedupes_concurrent_service_checks_across_deliveries() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-concurrent-{}.yml",
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
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-concurrent").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let req1 = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/webhooks/github-packages")
            .header("X-GitHub-Event", "package")
            .header("X-GitHub-Delivery", "svc-concurrent-1")
            .header("X-Hub-Signature-256", sig.clone())
            .body(Body::from(payload_bytes.clone()))
            .unwrap(),
    );
    let req2 = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/webhooks/github-packages")
            .header("X-GitHub-Event", "package")
            .header("X-GitHub-Delivery", "svc-concurrent-2")
            .header("X-Hub-Signature-256", sig)
            .body(Body::from(payload_bytes))
            .unwrap(),
    );
    let (resp1, resp2) = tokio::join!(req1, req2);
    let resp1 = resp1.unwrap();
    let resp2 = resp2.unwrap();
    assert_eq!(resp1.status(), 200);
    assert_eq!(resp2.status(), 200);

    let body1 = response_json(resp1).await;
    let body2 = response_json(resp2).await;
    let job_id_1 = body1["jobId"].as_str().unwrap().to_string();
    let job_id_2 = body2["jobId"].as_str().unwrap().to_string();
    assert_eq!(job_id_1, job_id_2);
    assert_eq!(
        body1["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(
        body2["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);

    let reused_count = [body1["reusedJobIds"].clone(), body2["reusedJobIds"].clone()]
        .into_iter()
        .filter(|value| value == &serde_json::json!([job_id_1.clone()]))
        .count();
    let inserted_count = [body1["reusedJobIds"].clone(), body2["reusedJobIds"].clone()]
        .into_iter()
        .filter(|value| value == &serde_json::json!([]))
        .count();
    assert_eq!(reused_count, 1);
    assert_eq!(inserted_count, 1);

    let job = wait_for_job_terminal(&state, &job_id_1).await;
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
}

#[tokio::test]
async fn merge_job_summary_fields_unions_webhook_arrays() {
    let state = test_state(":memory:").await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.summary_json = serde_json::json!({
        "source": "github_webhook",
        "matchedServiceIds": ["svc-a"],
        "reusedJobIds": ["chk-old"],
        "deliveryId": "delivery-1",
        "deliveryIds": ["delivery-1"],
        "repo": "ghcr.io/acme/web",
        "repos": ["ghcr.io/acme/web"]
    });
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .merge_job_summary_fields(
            &job_id,
            &serde_json::json!({
                "matchedServiceIds": ["svc-b", "svc-a"],
                "reusedJobIds": ["chk-old", "chk-new"],
                "deliveryId": "delivery-2",
                "deliveryIds": ["delivery-2"],
                "repo": "ghcr.io/acme/api",
                "repos": ["ghcr.io/acme/api"]
            }),
        )
        .await
        .unwrap();

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(
        job.summary_json["matchedServiceIds"],
        serde_json::json!(["svc-a", "svc-b"])
    );
    assert_eq!(
        job.summary_json["reusedJobIds"],
        serde_json::json!(["chk-old", "chk-new"])
    );
    assert_eq!(
        job.summary_json["deliveryId"],
        serde_json::json!("delivery-2")
    );
}

#[tokio::test]
async fn webhook_reused_ui_check_still_sends_new_version_notification() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(150)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-reuse-notify-{}.yml",
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
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-reuse-notify").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let ui_resp = app
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
    assert_eq!(ui_resp.status(), 200);
    let ui_body = response_json(ui_resp).await;
    let job_id = ui_body["checkId"].as_str().unwrap().to_string();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let webhook_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "notify-reuse-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(webhook_resp.status(), 200);
    let webhook_body = response_json(webhook_resp).await;
    assert_eq!(webhook_body["jobId"], job_id);
    assert_eq!(webhook_body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(
        webhook_body["reusedJobIds"],
        serde_json::json!([job_id.clone()])
    );

    let delivered = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        delivered["schema"].as_str(),
        Some("dockrev.notification.new_version_discovered.v2")
    );
    assert_eq!(delivered["kind"].as_str(), Some("new_version_discovered"));
    assert_eq!(delivered["channel"].as_str(), Some("webhook"));
    assert_eq!(delivered["check"]["jobId"].as_str(), Some(job_id.as_str()));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.reason, "ui");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(
        job.summary_json["deliveryId"].as_str(),
        Some("notify-reuse-1")
    );
    wait_for_job_log_contains(&state, &job_id, "notify: webhook=ok").await;
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("github webhook reused check job"))
    );
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("notify: webhook=ok"))
    );
    server.abort();
}

#[tokio::test]
async fn service_scope_check_only_updates_target_service() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-check-service-scope-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  worker:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-service-scope").await;

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let target_service_id = services
        .iter()
        .find(|service| service.name == "web")
        .unwrap()
        .id
        .clone();
    let other_service_id = services
        .iter()
        .find(|service| service.name == "worker")
        .unwrap()
        .id
        .clone();

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
                        "serviceId": target_service_id,
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
    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.summary_json["servicesChecked"].as_u64(), Some(1));

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let target = stack
        .services
        .iter()
        .find(|service| service.id == target_service_id)
        .unwrap();
    let other = stack
        .services
        .iter()
        .find(|service| service.id == other_service_id)
        .unwrap();
    assert_eq!(
        target
            .candidate
            .as_ref()
            .map(|candidate| candidate.tag.as_str()),
        Some("5.2")
    );
    assert!(other.candidate.is_none());
}

#[tokio::test]
async fn webhook_reason_check_sends_new_version_notification() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-notify-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-notify").await;
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "notify-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["schema"].as_str(),
        Some("dockrev.notification.new_version_discovered.v2")
    );
    assert_eq!(payload["kind"].as_str(), Some("new_version_discovered"));
    assert_eq!(payload["channel"].as_str(), Some("webhook"));
    assert_eq!(payload["check"]["jobId"].as_str(), Some(job_id.as_str()));
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.2")
    );

    let job = wait_for_job_terminal(&state, &job_id).await;
    wait_for_job_log_contains(&state, &job_id, "notify: webhook=ok").await;
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(job.finished_at.is_some());
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("notify: webhook=ok"))
    );
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notifications_are_deduped_by_active_record() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let first_now = test_now_rfc3339();
    let second_now = test_offset_rfc3339(&first_now, time::Duration::minutes(1));

    let compose_path = format!("/tmp/dockrev-schedule-notify-{}.yml", ulid::Ulid::new());
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
            &first_now,
            &first_now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &first_now,
        vec!["1.0.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &first_now,
        vec!["1.1.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
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
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let first_job_id = insert_check_job(&state, "schedule", &first_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &first_job_id,
        "schedule",
        &first_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let first_payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        first_payload["check"]["jobId"].as_str(),
        Some(first_job_id.as_str())
    );
    let first_service = &first_payload["links"]["serviceUrls"][0];
    assert_eq!(first_service["currentDisplayTag"].as_str(), Some("1.0.0"));
    assert_eq!(first_service["candidateDisplayTag"].as_str(), Some("1.1.0"));
    let summary = first_payload["human"]["summary"]
        .as_str()
        .unwrap_or_default();
    assert!(summary.contains("1.0.0 -> 1.1.0"));
    assert!(!summary.contains("latest -> latest"));

    let second_job_id = insert_check_job(&state, "schedule", &second_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &second_job_id,
        "schedule",
        &second_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "duplicate schedule notification should be skipped"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "sent");
    assert_eq!(rows[0].reason, "schedule");

    let logs = state.db.list_job_logs(&second_job_id).await.unwrap();
    assert!(logs.iter().any(|line| {
        line.msg.contains(
            "new-version notification skipped: all 1 services already have active records",
        )
    }));
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_waits_for_version_inference_settle() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-settle-{}.yml",
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
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
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "latest".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_miss"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue("ghcr.io/acme/web", "sha256:new", "linux/amd64", "force")
            .await
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:old", "linux/amd64")
            .await
            .as_deref(),
        Some("cache_miss")
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:new", "linux/amd64")
            .await
            .as_deref(),
        Some("force")
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_now = now.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &notify_now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let early = tokio::time::timeout(Duration::from_millis(120), rx.recv()).await;
    assert!(
        early.is_err(),
        "notification should wait for version inference settle"
    );

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（5.2.0 -> 5.3.0）。")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2.0")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.3.0")
    );
    let sent_at = payload["sentAt"].as_str().expect("sentAt missing");
    assert!(
        sent_at > now.as_str(),
        "payload sentAt should reflect delayed dispatch"
    );
    notify_task.await.unwrap();
    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].created_at, sent_at);
    assert_eq!(rows[0].sent_at.as_deref(), Some(sent_at));
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_uses_frozen_current_digest_after_live_drift() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-frozen-current-{}.yml",
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
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
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "latest".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:new".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
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
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_miss"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue("ghcr.io/acme/web", "sha256:new", "linux/amd64", "force")
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    let summary = payload["human"]["summary"].as_str().unwrap_or_default();
    assert_eq!(summary, "demo / web 服务有新版本（5.2.0 -> 5.3.0）。");
    assert!(!summary.contains("5.3.0 -> 5.3.0"));
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2.0")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.3.0")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_waits_for_non_strict_semver_aliases_like_main() {
    let registry = Arc::new(BranchAliasRegistry::new("main", Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-main-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:main
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
            None,
            None,
            Some("main".to_string()),
            None,
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
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "main".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "main".to_string(),
        candidate_tag: "main".to_string(),
        candidate_display_tag: "main".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_miss"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue("ghcr.io/acme/web", "sha256:new", "linux/amd64", "force")
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let early = tokio::time::timeout(Duration::from_millis(120), rx.recv()).await;
    assert!(
        early.is_err(),
        "non-strict aliases should wait for version inference settle"
    );

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（5.2.0 -> 5.3.0）。")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2.0")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.3.0")
    );
    notify_task.await.unwrap();
    server.abort();
}

