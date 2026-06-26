#[tokio::test]
async fn deploy_check_report_fails_when_github_packages_callback_scheme_is_not_http() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "ftp://dockrev.example.com/api/webhooks/github-packages".to_string();
    settings.pat = Some("ghp_example".to_string());
    settings.webhook_secret = Some("secret123".to_string());
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", true, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["report"]["overall"]["result"], "fail");
    let blocking = body["report"]["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.github_packages"));

    let github_packages = body["report"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "feature.github_packages")
        .unwrap();
    assert_eq!(github_packages["status"], "fail");
    assert!(
        github_packages["evidence"]
            .as_str()
            .unwrap()
            .contains("callbackUrl(invalid_scheme)")
    );
}

#[tokio::test]
async fn deploy_check_report_fails_when_github_packages_has_no_selected_repos() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    settings.pat = Some("ghp_example".to_string());
    settings.webhook_secret = Some("secret123".to_string());
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["report"]["overall"]["result"], "fail");
    let blocking = body["report"]["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.github_packages"));

    let github_packages = body["report"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "feature.github_packages")
        .unwrap();
    assert_eq!(github_packages["status"], "fail");
    assert!(
        github_packages["evidence"]
            .as_str()
            .unwrap()
            .contains("repos(selected=0)")
    );
}

#[tokio::test]
async fn github_packages_settings_masks_pat() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let put = serde_json::json!({
      "enabled": true,
      "callbackUrl": "https://dockrev.example.com/api/webhooks/github-packages",
      "targets": [],
      "repos": [],
      "pat": "ghp_example"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/github-packages/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/github-packages/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(
        body["callbackUrl"],
        "https://dockrev.example.com/api/webhooks/github-packages"
    );
    assert_eq!(body["patMasked"], "******");
}

#[tokio::test]
async fn github_packages_resolve_owner_requires_pat_saved() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/resolve")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"acme"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "ghcr_pat_missing"
    );
}

#[tokio::test]
async fn github_packages_resolve_repo_returns_visibility_and_activity_fields() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/resolve")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"acme/widgets"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["kind"], "repo");
    assert_eq!(body["owner"], "acme");
    assert_eq!(body["repos"][0]["fullName"], "acme/widgets");
    assert_eq!(body["repos"][0]["selected"], true);
    assert_eq!(body["repos"][0]["visibility"], "unknown");
    assert!(body["repos"][0]["lastActivityAt"].is_null());
    assert!(body["repos"][0]["ghcrLinked"].is_null());
    assert_eq!(body["repos"][0]["deployed"], false);
}

#[test]
fn github_http_status_from_error_parses_status_code() {
    let err = anyhow::anyhow!("github http 403 Forbidden: bad credentials");
    assert_eq!(super::github_http_status_from_error(&err), Some(403));
}

#[tokio::test]
async fn github_owner_resolve_error_map_timeout_reason() {
    let err = anyhow::anyhow!("upstream request timed out");
    let api_err = super::map_github_owner_resolve_error("acme", err);
    let resp = api_err.into_response();
    assert_eq!(resp.status(), 500);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "github_upstream_timeout"
    );
}

#[tokio::test]
async fn github_owner_resolve_error_map_auth_reason() {
    let err = anyhow::anyhow!("github http 401 Unauthorized: bad credentials");
    let api_err = super::map_github_owner_resolve_error("acme", err);
    let resp = api_err.into_response();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "ghcr_pat_invalid_or_scope_insufficient"
    );
}

#[test]
fn urls_match_is_tolerant_of_trailing_slash_and_default_ports() {
    assert!(super::urls_match(
        "https://dockrev.example.com/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages/",
    ));
    assert!(super::urls_match(
        "https://dockrev.example.com:443/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages",
    ));
    assert!(super::urls_match(
        "http://dockrev.example.com:80/api/webhooks/github-packages",
        "http://dockrev.example.com/api/webhooks/github-packages/",
    ));
    assert!(!super::urls_match(
        "https://dockrev.example.com/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages?x=1",
    ));
}

#[test]
fn streamed_update_percent_uses_floor_to_match_stack_progress() {
    // Regression guard: streamed percent must not exceed the subsequent
    // stack-complete percent (which uses integer division / floor).
    let streamed = super::update_progress_percent(9, 13, 1.0);
    let stack_complete = super::progress_percent(10, 13);
    assert_eq!(streamed, 76);
    assert_eq!(stack_complete, 76);
    assert!(streamed <= stack_complete);
}

#[tokio::test]
async fn github_packages_webhook_validates_signature_and_dedupes_delivery() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    // Seed settings + selected repo.
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: true,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
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

    let app = api::router(state.clone());

    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d1")
                .header("X-Hub-Signature-256", "sha256=deadbeef")
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=rejected&q=d1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(0));

    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert!(
        body["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );

    // Same delivery id should be ignored even if repo selection changed after first processing.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), false)],
            &now,
        )
        .await
        .unwrap();

    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "duplicate_delivery");
    assert_eq!(body["attemptCount"], 2);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?q=d2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["deliveries"][0]["deliveryId"], "d2");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["reason"], serde_json::Value::Null);
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
    assert_eq!(body["deliveries"][0]["attemptCount"], 2);
    assert!(
        body["deliveries"][0]["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );
}

#[tokio::test]
async fn github_packages_webhook_respects_disabled_setting() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: false,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
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

    let app = api::router(state.clone());
    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "disabled-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "disabled");
    assert!(
        !state
            .db
            .github_packages_delivery_exists("disabled-1")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn github_packages_webhook_ignores_non_package_event_without_persisting() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: true,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state.clone());
    let payload = serde_json::json!({ "zen": "keep it simple" });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "ping")
                .header("X-GitHub-Delivery", "ping-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "not_package_event");
    assert!(
        !state
            .db
            .github_packages_delivery_exists("ping-1")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn github_packages_webhook_matches_selected_repos_case_insensitively() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: true,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    // Store with mixed casing.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("Acme"), String::from("Widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state);

    // Payload uses different casing than stored.
    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "case-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert!(
        body["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );
}

#[tokio::test]
async fn github_packages_webhook_matches_managed_service_and_enqueues_check() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-single-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-single").await;
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
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
                .header("X-GitHub-Delivery", "svc-match-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("chk_"));
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "check");
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(job.summary_json["repo"].as_str(), Some("ghcr.io/acme/web"));
    assert_eq!(job.summary_json["deliveryId"].as_str(), Some("svc-match-1"));
    assert_eq!(job.summary_json["fallbackUsed"], false);
}

#[tokio::test]
async fn github_packages_webhook_digest_only_service_ref_still_discovers_candidate() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(LatestTagUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-digest-only-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web@sha256:old
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-digest-only").await;
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
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
                .header("X-GitHub-Delivery", "svc-digest-only-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(
        logs.iter()
            .all(|line| !line.msg.contains("invalid image ref")),
        "digest-only refs should no longer be rejected: {logs:?}"
    );

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let service = stack
        .services
        .iter()
        .find(|svc| svc.id == service_id)
        .unwrap();
    assert_eq!(service.image.tag, "latest");
    let candidate = service
        .candidate
        .as_ref()
        .expect("candidate should be discovered for digest-only ref");
    assert_eq!(candidate.tag, "latest");
    assert_eq!(candidate.digest, "sha256:new");
}

#[tokio::test]
async fn github_packages_webhook_matches_multiple_services_without_discovery_noise() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path_a = format!(
        "/tmp/dockrev-ghcr-webhook-multi-a-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path_a,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  other:
    image: ghcr.io/acme/other:1.0
"#,
    )
    .unwrap();
    let stack_a = seed_stack_from_compose(&state, "demo-a", &compose_path_a).await;
    seed_discovered_project(&state, &stack_a, "demo-a").await;

    let compose_path_b = format!(
        "/tmp/dockrev-ghcr-webhook-multi-b-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path_b,
        r#"
services:
  api:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_b = seed_stack_from_compose(&state, "demo-b", &compose_path_b).await;
    seed_discovered_project(&state, &stack_b, "demo-b").await;

    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let service_ids = vec![
        state
            .db
            .list_services_for_check(&stack_a)
            .await
            .unwrap()
            .into_iter()
            .find(|service| service.name == "web")
            .unwrap()
            .id,
        state.db.list_services_for_check(&stack_b).await.unwrap()[0]
            .id
            .clone(),
    ];

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
                .header("X-GitHub-Delivery", "svc-match-2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let matched = body["matchedServiceIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert_eq!(matched.len(), 2);
    for service_id in &service_ids {
        assert!(matched.contains(&service_id.as_str()));
    }
    let job_ids = body["jobIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(job_ids.len(), 2);
    assert!(job_ids.iter().all(|job_id| job_id.starts_with("chk_")));

    for job_id in &job_ids {
        let job = wait_for_job_terminal(&state, job_id).await;
        assert_eq!(job.r#type.as_str(), "check");
        assert_eq!(job.scope.as_str(), "service");
    }

    let jobs = state.db.list_jobs().await.unwrap();
    assert_eq!(
        jobs.iter()
            .filter(|job| job.r#type.as_str() == "check")
            .count(),
        2
    );
    assert_eq!(
        jobs.iter()
            .filter(|job| job.r#type.as_str() == "discovery")
            .count(),
        0
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let delivery_job_ids = body["deliveries"][0]["jobIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(delivery_job_ids.len(), 2);
    for job_id in &job_ids {
        assert!(delivery_job_ids.contains(&job_id.as_str()));
    }
}

#[tokio::test]
async fn github_packages_webhook_zero_match_falls_back_to_discovery() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(body["fallbackReason"], "no_managed_service_match");
    assert_eq!(body["matchedServiceIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("dsc_"));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "discovery");
    assert_eq!(job.reason, "github_webhook");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(job.summary_json["fallbackUsed"], true);
    assert_eq!(
        job.summary_json["fallbackReason"].as_str(),
        Some("no_managed_service_match")
    );
}

#[tokio::test]
async fn github_packages_webhook_reuses_pending_discovery_fallback_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_discovery_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    existing.created_by = "schedule".to_string();
    existing.reason = "schedule".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["jobId"], existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([existing_id.clone()]));
    assert_eq!(
        body["reusedJobIds"],
        serde_json::json!([existing_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn github_packages_webhook_replaces_stale_discovery_fallback_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let stale_at = (time::OffsetDateTime::now_utc() - time::Duration::hours(3))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_discovery_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &stale_at,
    )
    .to_db();
    existing.created_by = "schedule".to_string();
    existing.reason = "schedule".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-stale-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(body["fallbackReason"], "no_managed_service_match");
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("dsc_"));
    assert_ne!(job_id, existing_id);

    let stale = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(stale.status, "failed");
    assert_eq!(
        stale.summary_json["terminated"]["reason"].as_str(),
        Some("stale_check")
    );
    let stale_logs = state.db.list_job_logs(&existing_id).await.unwrap();
    assert!(
        stale_logs
            .iter()
            .any(|line| line.msg.contains("job terminated: reason=stale_check"))
    );

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.reason, "github_webhook");
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);
}

#[tokio::test]
async fn github_packages_webhook_reuses_pending_service_check_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-reuse-{}.yml", ulid::Ulid::new());
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
        crate::api::types::JobScope::Service,
        Some(stack_id.clone()),
        Some(service_id.clone()),
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
                .header("X-GitHub-Delivery", "svc-reuse-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["jobId"], existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([existing_id.clone()]));
    assert_eq!(
        body["reusedJobIds"],
        serde_json::json!([existing_id.clone()])
    );
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);
}
