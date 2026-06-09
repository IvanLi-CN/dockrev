#[tokio::test]
async fn github_packages_webhook_deliveries_lists_desc_and_paginates() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    state
        .db
        .insert_github_packages_delivery_if_new(
            "d1",
            "2026-03-01T00:00:00Z",
            Some("acme"),
            Some("alpha"),
        )
        .await
        .unwrap();
    state
        .db
        .insert_github_packages_delivery_if_new(
            "d2",
            "2026-03-01T00:00:00Z",
            Some("acme"),
            Some("beta"),
        )
        .await
        .unwrap();
    state
        .db
        .insert_github_packages_delivery_if_new(
            "d3",
            "2026-03-02T00:00:00Z",
            Some("acme"),
            Some("gamma"),
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?page=1&perPage=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["perPage"], 2);
    assert_eq!(body["total"], 3);
    assert_eq!(body["filteredTotal"], 3);
    assert_eq!(body["summary"]["processed"], 3);
    assert_eq!(body["summary"]["ignored"], 0);
    assert_eq!(body["summary"]["rejected"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(2));
    assert_eq!(body["deliveries"][0]["deliveryId"], "d3");
    assert_eq!(body["deliveries"][0]["fullName"], "acme/gamma");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
    assert_eq!(body["deliveries"][0]["attemptCount"], 1);
    assert_eq!(body["deliveries"][1]["deliveryId"], "d2");
    assert_eq!(body["deliveries"][1]["fullName"], "acme/beta");

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?page=2&perPage=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 3);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(1));
    assert_eq!(body["deliveries"][0]["deliveryId"], "d1");
    assert_eq!(body["deliveries"][0]["fullName"], "acme/alpha");
}

#[tokio::test]
async fn github_packages_webhook_deliveries_returns_empty_when_no_data() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

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
    assert_eq!(body["page"], 1);
    assert_eq!(body["perPage"], 50);
    assert_eq!(body["total"], 0);
    assert_eq!(body["filteredTotal"], 0);
    assert_eq!(body["summary"]["processed"], 0);
    assert_eq!(body["summary"]["ignored"], 0);
    assert_eq!(body["summary"]["rejected"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(0));
}

#[tokio::test]
async fn github_packages_webhook_deliveries_supports_decision_and_query_filters() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-ok".to_string(),
            received_at: "2026-03-02T09:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("alpha".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "processed".to_string(),
            reason: None,
            response_status: Some(200),
            job_id: Some("dsc_ok".to_string()),
            job_ids: vec!["dsc_ok".to_string()],
        })
        .await
        .unwrap();
    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-ignore".to_string(),
            received_at: "2026-03-02T10:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("beta".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "ignored".to_string(),
            reason: Some("repo_not_selected".to_string()),
            response_status: Some(200),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();
    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-reject".to_string(),
            received_at: "2026-03-02T11:00:00Z".to_string(),
            owner: None,
            repo: None,
            event: Some("package".to_string()),
            action: None,
            decision: "rejected".to_string(),
            reason: Some("invalid_signature".to_string()),
            response_status: Some(401),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=processed&q=dsc_ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["summary"]["processed"], 1);
    assert_eq!(body["summary"]["ignored"], 1);
    assert_eq!(body["summary"]["rejected"], 1);
    assert_eq!(body["deliveries"][0]["jobId"], "dsc_ok");
    assert_eq!(
        body["deliveries"][0]["jobIds"],
        serde_json::json!(["dsc_ok"])
    );
    assert_eq!(body["deliveries"][0]["deliveryId"], "d-ok");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["reason"], serde_json::Value::Null);
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
}

#[tokio::test]
async fn github_packages_webhook_deliveries_requires_auth_when_anonymous_disabled() {
    let state = test_state_auth_required(":memory:").await;
    let app = api::router(state);

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
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_emits_new_event() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );
    let mut body = resp.into_body();

    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "evt-new".to_string(),
            received_at: "2026-03-09T10:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("widgets".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "processed".to_string(),
            reason: None,
            response_status: Some(200),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();
    let event_id = state
        .db
        .insert_github_packages_delivery_event(
            "evt-new",
            "2026-03-09T10:00:00Z",
            &github_delivery_event_payload("evt-new", "2026-03-09T10:00:00Z", "processed", 1)
                .to_string(),
        )
        .await
        .unwrap();

    let evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let event_id_s = event_id.to_string();
    assert_eq!(evt.id.as_deref(), Some(event_id_s.as_str()));
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["deliveryId"].as_str(), Some("evt-new"));
    assert_eq!(payload["attemptCount"].as_u64(), Some(1));
    assert_eq!(payload["decision"].as_str(), Some("processed"));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_honors_after_id_and_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    for (delivery_id, ts, attempt_count) in [
        ("evt-1", "2026-03-09T10:00:00Z", 1_u32),
        ("evt-2", "2026-03-09T10:05:00Z", 2_u32),
    ] {
        state
            .db
            .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
                delivery_id: delivery_id.to_string(),
                received_at: ts.to_string(),
                owner: Some("acme".to_string()),
                repo: Some("widgets".to_string()),
                event: Some("package".to_string()),
                action: Some("published".to_string()),
                decision: "processed".to_string(),
                reason: None,
                response_status: Some(200),
                job_id: None,
                job_ids: Vec::new(),
            })
            .await
            .unwrap();
        state
            .db
            .insert_github_packages_delivery_event(
                delivery_id,
                ts,
                &github_delivery_event_payload(delivery_id, ts, "processed", attempt_count)
                    .to_string(),
            )
            .await
            .unwrap();
    }

    let first_id = 1_i64;

    let resp_query = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/github-packages/webhook/deliveries/events?afterId={first_id}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_query.status(), 200);
    let mut body_query = resp_query.into_body();
    let evt_query = wait_for_sse_event(
        &mut body_query,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let payload_query: serde_json::Value = serde_json::from_str(&evt_query.data).unwrap();
    assert_eq!(payload_query["deliveryId"].as_str(), Some("evt-2"));
    assert_eq!(payload_query["attemptCount"].as_u64(), Some(2));

    let resp_header = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .header("Last-Event-ID", first_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_header.status(), 200);
    let mut body_header = resp_header.into_body();
    let evt_header = wait_for_sse_event(
        &mut body_header,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let payload_header: serde_json::Value = serde_json::from_str(&evt_header.data).unwrap();
    assert_eq!(payload_header["deliveryId"].as_str(), Some("evt-2"));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_emits_processed_and_duplicate_attempt_updates()
 {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
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
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

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
                .header("X-GitHub-Delivery", "evt-dup")
                .header("X-Hub-Signature-256", sig.clone())
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let first_evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let first_payload: serde_json::Value = serde_json::from_str(&first_evt.data).unwrap();
    assert_eq!(first_payload["deliveryId"].as_str(), Some("evt-dup"));
    assert_eq!(first_payload["decision"].as_str(), Some("processed"));
    assert_eq!(first_payload["attemptCount"].as_u64(), Some(1));
    assert_eq!(first_payload["responseStatus"].as_u64(), Some(200));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "evt-dup")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let second_evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let second_payload: serde_json::Value = serde_json::from_str(&second_evt.data).unwrap();
    assert_eq!(second_payload["deliveryId"].as_str(), Some("evt-dup"));
    assert_eq!(second_payload["decision"].as_str(), Some("processed"));
    assert_eq!(second_payload["attemptCount"].as_u64(), Some(2));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_requires_auth_when_anonymous_disabled() {
    let state = test_state_auth_required(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn settings_auth_reports_group_match_details() {
    let state = test_state_with_authz(":memory:", Some("alice"), Some("ops"), false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("X-Forwarded-User", "bob")
                .header("X-Forwarded-User-Picture", "https://example.test/avatar/bob.png")
                .header("Remote-Groups", "dev, ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["forwardHeaderName"], "x-forwarded-user");
    assert_eq!(body["auth"]["groupHeaderName"], "remote-groups");
    assert_eq!(body["auth"]["authorizationMode"], "user_or_group");
    assert_eq!(body["auth"]["matchedBy"], "group");
    assert_eq!(body["auth"]["currentUser"], "bob");
    assert_eq!(
        body["auth"]["avatarUrl"],
        "https://example.test/avatar/bob.png"
    );
    assert_eq!(
        body["auth"]["currentGroups"],
        serde_json::json!(["d**v", "o**s"])
    );
}

#[tokio::test]
async fn settings_auth_reports_group_only_mode() {
    let state = test_state_with_authz(":memory:", None, Some("ops"), false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("Remote-Groups", "dev, ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["authorizationMode"], "group_only");
    assert_eq!(body["auth"]["matchedBy"], "group");
}

#[tokio::test]
async fn settings_auth_serializes_empty_current_groups() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("X-Forwarded-User", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["currentGroups"], serde_json::json!([]));
}

#[tokio::test]
async fn protected_endpoint_returns_authz_details_without_redirect_target() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
    assert_eq!(body["error"]["details"]["authorizationMode"], "user_only");
    assert_eq!(body["error"]["details"]["allowedUserMasked"], "al***ce");
    assert!(
        body["error"]["details"]
            .as_object()
            .is_some_and(|obj| !obj.contains_key("redirectTo"))
    );
}

#[tokio::test]
async fn protected_endpoint_does_not_allow_dev_bypass_when_allowlist_is_configured() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, true).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
    assert_eq!(body["error"]["details"]["authorizationMode"], "user_only");
}

#[tokio::test]
async fn deploy_check_report_requires_authorized_request() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
}

#[tokio::test]
async fn deploy_check_report_rejects_unauthorized_request_before_preflight() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    state
        .db
        .put_instance_public_base_url(
            Some("ftp://dockrev.example.com".to_string()),
            &super::now_rfc3339().unwrap(),
        )
        .await
        .unwrap();
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
}

#[tokio::test]
async fn github_packages_webhook_persists_ignored_delivery_for_unselected_repo() {
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
    // Seed a different repo as selected so the incoming event is not eligible.
    state
        .db
        .put_github_packages_repos(&[(String::from("acme"), String::from("other"), true)], &now)
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
                .header("X-GitHub-Delivery", "unselected-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "repo_not_selected");
    assert!(
        state
            .db
            .github_packages_delivery_exists("unselected-1")
            .await
            .unwrap()
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["deliveries"][0]["deliveryId"], "unselected-1");
    assert_eq!(body["deliveries"][0]["decision"], "ignored");
    assert_eq!(body["deliveries"][0]["reason"], "repo_not_selected");
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
}

#[tokio::test]
async fn runtime_scan_updates_drifted_services() {
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:old".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let image = &detail["stack"]["services"][0]["image"];
    assert_eq!(image["digest"].as_str().unwrap(), "sha256:new");
    assert_eq!(image["resolvedTag"].as_str().unwrap(), "5.3");
}

#[tokio::test]
async fn runtime_scan_keeps_container_image_id_when_shared_moving_tag_was_pulled_elsewhere() {
    let runner: Arc<SharedMovingTagRunner> = Arc::new(SharedMovingTagRunner::new());
    let state = test_state_with(
        ":memory:",
        Arc::new(SharedMovingTagRegistry),
        runner,
    )
    .await;
    let app = api::router(state.clone());

    let trtff_compose_path = format!("/tmp/dockrev-test-trtff-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &trtff_compose_path,
        r#"
services:
  trtff-api:
    image: ghcr.io/sequenxe/trtff:latest
"#,
    )
    .unwrap();
    let ctp_compose_path = format!("/tmp/dockrev-test-ctp-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &ctp_compose_path,
        r#"
services:
  ctp-recorder:
    image: ghcr.io/sequenxe/trtff:latest
"#,
    )
    .unwrap();

    let trtff_stack_id = seed_stack_from_compose(&state, "trtff", &trtff_compose_path).await;
    let ctp_stack_id =
        seed_stack_from_compose(&state, "ctp-recorder", &ctp_compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for (project, stack_id, compose_path) in [
        ("trtff", trtff_stack_id.as_str(), trtff_compose_path.as_str()),
        ("ctp-recorder", ctp_stack_id.as_str(), ctp_compose_path.as_str()),
    ] {
        state
            .db
            .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
                project: project.to_string(),
                stack_id: Some(stack_id.to_string()),
                status: "active".to_string(),
                last_seen_at: Some(now.clone()),
                last_scan_at: now.clone(),
                last_error: None,
                last_config_files: Some(vec![compose_path.to_string()]),
                unarchive_if_active: true,
            })
            .await
            .unwrap();
    }

    let payload = serde_json::json!({
        "scope": "all",
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{trtff_stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let trtff_detail = response_json(resp).await;
    let trtff_service = &trtff_detail["stack"]["services"][0];
    assert_eq!(
        trtff_service["image"]["digest"].as_str().unwrap(),
        "sha256:old-runtime"
    );
    assert_eq!(
        trtff_service["candidate"]["digest"].as_str().unwrap(),
        "sha256:new-runtime"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{ctp_stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ctp_detail = response_json(resp).await;
    let ctp_service = &ctp_detail["stack"]["services"][0];
    assert_eq!(
        ctp_service["image"]["digest"].as_str().unwrap(),
        "sha256:new-runtime"
    );
    assert!(ctp_service["candidate"].is_null());
}

#[tokio::test]
async fn runtime_scan_resolved_tag_inference_matches_check() {
    let compose = r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#;

    let compose_path_a = format!("/tmp/dockrev-test-check-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_path_a, compose).unwrap();
    let compose_path_b = format!("/tmp/dockrev-test-runtime-scan-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_path_b, compose).unwrap();

    // Check path
    let runner_a: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state_a = test_state_with(":memory:", Arc::new(FakeRegistry), runner_a).await;
    let app_a = api::router(state_a.clone());
    let stack_id_a = seed_stack_from_compose(&state_a, "demo", &compose_path_a).await;
    let now_a = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state_a
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id_a.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now_a.clone()),
            last_scan_at: now_a.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path_a.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check_payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id_a,
        "reason": "ui",
    });
    let resp = app_a
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();
    let mut finished = false;
    for _ in 0..120 {
        let resp = app_a
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let snapshot_checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state_a,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &snapshot_checked_at,
        vec!["5.2".to_string(), "5.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let resp = app_a
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail_a = response_json(resp).await;
    let image_a = &detail_a["stack"]["services"][0]["image"];
    let digest_a = image_a["digest"].as_str().unwrap().to_string();
    let resolved_a = image_a["resolvedTag"].as_str().unwrap().to_string();
    let resolved_tags_a = image_a["resolvedTags"].clone();

    // Runtime scan path
    let runner_b: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state_b = test_state_with(":memory:", Arc::new(FakeRegistry), runner_b).await;
    let app_b = api::router(state_b.clone());
    let stack_id_b = seed_stack_from_compose(&state_b, "demo", &compose_path_b).await;
    let now_b = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state_b
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id_b.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now_b.clone()),
            last_scan_at: now_b.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path_b.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let scan_payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id_b,
        "reason": "ui",
    });
    let resp = app_b
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(scan_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();
    let mut finished = false;
    for _ in 0..120 {
        let resp = app_b
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let snapshot_checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state_b,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &snapshot_checked_at,
        vec!["5.2".to_string(), "5.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let resp = app_b
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail_b = response_json(resp).await;
    let image_b = &detail_b["stack"]["services"][0]["image"];
    let digest_b = image_b["digest"].as_str().unwrap().to_string();
    let resolved_b = image_b["resolvedTag"].as_str().unwrap().to_string();
    let resolved_tags_b = image_b["resolvedTags"].clone();

    assert_eq!(digest_a, digest_b);
    assert_eq!(resolved_a, resolved_b);
    assert_eq!(resolved_tags_a, resolved_tags_b);
}
