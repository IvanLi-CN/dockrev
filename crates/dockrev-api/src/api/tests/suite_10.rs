#[tokio::test]
async fn settings_and_notifications_roundtrip() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert!(settings["backup"].is_object());
    assert!(settings["resourceMonitor"].is_object());
    assert!(settings["schedules"].is_object());
    assert!(settings["releaseNotes"].is_object());
    assert!(settings["auth"].is_object());
    assert!(settings["instance"].is_object());
    assert!(settings["instance"]["publicBaseUrl"].is_null());
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["enabled"].as_bool(),
        Some(false)
    );
    assert!(settings["releaseNotes"]["octoRill"]["apiBaseUrl"].is_null());
    assert!(settings["releaseNotes"]["octoRill"]["apiKeyMasked"].is_null());
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["defaultView"].as_str(),
        Some("smart")
    );
    assert_eq!(settings["resourceMonitor"]["enabled"].as_bool(), Some(true));
    assert_eq!(
        settings["resourceMonitor"]["sampleIntervalSeconds"].as_u64(),
        Some(10)
    );
    assert_eq!(
        settings["resourceMonitor"]["retentionDays"].as_u64(),
        Some(30)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("*/30 * * * *")
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["cron"].as_str(),
        Some("0 3 * * *")
    );

    let put = serde_json::json!({
        "backup": {
            "enabled": true,
            "requireSuccess": true,
            "baseDir": "/tmp/dockrev-backups",
            "skipTargetsOverBytes": 123
        },
        "resourceMonitor": {
            "enabled": false,
            "sampleIntervalSeconds": 60
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["backup"]["skipTargetsOverBytes"].as_u64().unwrap(),
        123
    );
    assert!(settings["instance"].is_object());
    assert!(settings["instance"]["publicBaseUrl"].is_null());
    assert_eq!(
        settings["resourceMonitor"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["resourceMonitor"]["sampleIntervalSeconds"].as_u64(),
        Some(60)
    );
    assert_eq!(
        settings["resourceMonitor"]["retentionDays"].as_u64(),
        Some(30)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("*/30 * * * *")
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["cron"].as_str(),
        Some("0 3 * * *")
    );

    let invalid = serde_json::json!({
        "backup": settings["backup"],
        "resourceMonitor": {
            "enabled": true,
            "sampleIntervalSeconds": 7
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let set_base_url = serde_json::json!({
        "backup": settings["backup"],
        "instance": {
            "publicBaseUrl": "https://dockrev.example.com"
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(set_base_url.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["instance"]["publicBaseUrl"].as_str(),
        Some("https://dockrev.example.com/")
    );

    let set_octo_rill = serde_json::json!({
        "backup": settings["backup"],
        "releaseNotes": {
            "octoRill": {
                "enabled": true,
                "apiBaseUrl": "https://octo.example.com/octo-rill/",
                "apiKey": " orill_ak_test_secret ",
                "defaultView": "translated"
            }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(set_octo_rill.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["apiBaseUrl"].as_str(),
        Some("https://octo.example.com/octo-rill")
    );
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["apiKeyMasked"].as_str(),
        Some("******")
    );
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["defaultView"].as_str(),
        Some("translated")
    );

    let preserve_octo_rill_key = serde_json::json!({
        "backup": settings["backup"],
        "releaseNotes": {
            "octoRill": {
                "enabled": true,
                "defaultView": "original"
            }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(preserve_octo_rill_key.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["apiKeyMasked"].as_str(),
        Some("******")
    );
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["defaultView"].as_str(),
        Some("original")
    );

    let preserve_octo_rill_mask = serde_json::json!({
        "backup": settings["backup"],
        "releaseNotes": {
            "octoRill": {
                "apiKey": "******"
            }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(preserve_octo_rill_mask.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["releaseNotes"]["octoRill"]["apiKeyMasked"].as_str(),
        Some("******")
    );

    let clear_octo_rill_key = serde_json::json!({
        "backup": settings["backup"],
        "releaseNotes": {
            "octoRill": {
                "apiKey": null
            }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(clear_octo_rill_key.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert!(settings["releaseNotes"]["octoRill"]["apiKeyMasked"].is_null());

    let invalid_base_url = serde_json::json!({
        "backup": settings["backup"],
        "instance": {
            "publicBaseUrl": "ftp://dockrev.example.com/"
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid_base_url.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("instance_public_base_url_invalid")
    );

    let invalid_octo_rill_base_url = serde_json::json!({
        "backup": settings["backup"],
        "releaseNotes": {
            "octoRill": {
                "apiBaseUrl": "https://user:pass@octo.example.com/"
            }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid_octo_rill_base_url.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("octo_rill_api_base_url_invalid")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/svc-test/resource-usage/history?window=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("resource_monitor_disabled")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/svc-test/resource-usage/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("resource_monitor_disabled")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"].is_object());

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "https://example.com/hook" },
        "telegram": {
            "enabled": true,
            "botToken": "123456:telegram-bot-token",
            "chatId": "-1001234567890"
        },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"]["enabled"].as_bool().unwrap());
    assert_eq!(conf["webhook"]["url"].as_str().unwrap(), "******");
    assert_eq!(conf["telegram"]["botToken"].as_str(), None);
    assert_eq!(conf["telegram"]["botTokenConfigured"].as_bool(), Some(true));
    assert_eq!(conf["telegram"]["chatId"].as_str(), Some("-1001234567890"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "  -10055667788  " },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(
        db_conf.telegram_bot_token.as_deref(),
        Some("123456:telegram-bot-token")
    );
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "******" },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "   " },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id, None);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert_eq!(conf["telegram"]["botToken"].as_str(), None);
    assert_eq!(conf["telegram"]["botTokenConfigured"].as_bool(), Some(true));
    assert!(conf["telegram"]["chatId"].is_null());
}

#[tokio::test]
async fn settings_schedule_cron_validation() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    let backup = settings["backup"].clone();

    let invalid = serde_json::json!({
        "backup": backup,
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "not a cron" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("cron_invalid")
    );
    assert_eq!(
        payload["error"]["details"]["field"].as_str(),
        Some("schedules.updateCheck.cron")
    );

    let put_5 = serde_json::json!({
        "backup": settings["backup"],
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "* * * * *" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put_5.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("* * * * *")
    );

    let put_6 = serde_json::json!({
        "backup": settings["backup"],
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "0 * * * * *" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put_6.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("0 * * * * *")
    );
}

#[tokio::test]
async fn notifications_test_endpoint_supports_channel_override() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                        "channel": "webhook",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["ok"].as_bool(), Some(true));
    let results = payload["results"].as_object().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results["webhook"]["ok"].as_bool(), Some(false));
    assert!(
        results["webhook"]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("webhook.url missing")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["ok"].as_bool(), Some(true));
    let results = payload["results"].as_object().unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn notifications_test_endpoint_emits_v2_payload_to_webhook() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(1);
    let hook_app = Router::new().route(
        "/hook",
        post({
            let tx = tx.clone();
            move |Json(payload): Json<serde_json::Value>| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(payload).await;
                    axum::http::StatusCode::OK
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, hook_app).await.unwrap();
    });

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some(format!("http://{addr}/hook"));
    state
        .db
        .put_notification_settings(&notification, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                        "channel": "webhook",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("webhook payload missing");
    assert_eq!(
        payload["schema"].as_str(),
        Some("dockrev.notification.test.v2")
    );
    assert_eq!(payload["kind"].as_str(), Some("notification_test"));
    assert_eq!(payload["channel"].as_str(), Some("webhook"));
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("dockrev: test notification")
    );
    assert_eq!(
        payload["debug"]["requestedChannel"].as_str(),
        Some("webhook")
    );
    assert!(payload.get("type").is_none());
    assert!(payload.get("ts").is_none());
    assert!(payload.get("message").is_none());
    assert!(payload.get("title").is_none());
    assert!(payload.get("body").is_none());

    server.abort();
}

#[tokio::test]
async fn resource_usage_events_emits_error_when_runtime_stats_unavailable() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-events-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
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
    let evt = wait_for_sse_event(&mut body, "resource_usage_error", Duration::from_secs(2)).await;
    let data: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(data["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(data["error"].as_str(), Some("runtime_stats_unavailable"));
}

#[tokio::test]
async fn resource_usage_events_emits_error_when_initial_snapshot_fails() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-resource-events-initial-fail-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let evt = wait_for_sse_event(&mut body, "resource_usage_error", Duration::from_secs(2)).await;
    let data: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(data["serviceId"].as_str(), Some(service_id.as_str()));
    assert!(!data["error"].as_str().unwrap_or_default().is_empty());
}

#[tokio::test]
async fn resource_usage_events_keep_streaming_past_sampler_idle_window() {
    let runner = Arc::new(ResourceUsageStreamRunner::default());
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner.clone()).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-resource-events-stream-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-resource-stream").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let snapshot =
        wait_for_sse_event(&mut body, "resource_usage_snapshot", Duration::from_secs(2)).await;
    let snapshot_data: serde_json::Value = serde_json::from_str(&snapshot.data).unwrap();
    assert_eq!(
        snapshot_data["serviceId"].as_str(),
        Some(service_id.as_str())
    );

    let tick_ids = tokio::time::timeout(Duration::from_secs(20), async {
        let mut ids = Vec::new();
        while ids.len() < 12 {
            let evt =
                wait_for_sse_event(&mut body, "resource_usage_tick", Duration::from_secs(15)).await;
            ids.push(
                evt.id
                    .as_deref()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or_default(),
            );
        }
        ids
    })
    .await
    .expect("resource usage SSE should stay alive past the sampler idle window");

    assert!(tick_ids.last().copied().unwrap_or_default() >= 13);
    assert!(runner.stats_calls.load(Ordering::SeqCst) >= 12);
}

#[tokio::test]
async fn deploy_welcome_roundtrip() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-welcome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["neverAutoOpen"], false);
    assert!(body["updatedAt"].is_null());

    let put = serde_json::json!({ "neverAutoOpen": true });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/deploy-welcome")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["neverAutoOpen"], true);
    assert!(body["updatedAt"].as_str().unwrap().len() > 10);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-welcome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["neverAutoOpen"], true);
}

#[tokio::test]
async fn deploy_check_report_is_read_only() {
    let state = test_state_with_authz(":memory:", Some("ops"), None, false).await;

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

    let app = api::router(state.clone());
    let before_jobs = state.db.list_jobs().await.unwrap().len();

    let body = wait_for_deploy_check_report_ready(&app, Some("ops")).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["report"]["overall"]["result"], "pass");
    assert_eq!(body["report"]["overall"]["blockingCheckIds"], serde_json::json!([]));
    let checks = body["report"]["checks"].as_array().unwrap();
    assert!(checks.iter().any(|c| c["id"] == "core.docker_engine"));
    assert!(checks.iter().any(|c| c["id"] == "core.compose_access"));
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "core.service_image_ref_valid")
    );
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "core.update_executor_ready")
    );
    let registry_auth = checks
        .iter()
        .find(|c| c["id"] == "feature.registry_auth")
        .unwrap();
    assert_eq!(registry_auth["status"], "na");
    assert_eq!(registry_auth["naReason"], "missing_prerequisite");
    let webhook = checks
        .iter()
        .find(|c| c["id"] == "feature.notifications.webhook")
        .unwrap();
    assert_eq!(webhook["status"], "na");
    assert_eq!(webhook["required"], false);

    let after_jobs = state.db.list_jobs().await.unwrap().len();
    assert_eq!(before_jobs, after_jobs);
}

#[tokio::test]
async fn deploy_check_report_marks_stale_cached_report_refreshing_and_reenqueues_worker() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let checked_at = test_offset_from_now_rfc3339(time::Duration::seconds(
        -(crate::deploy_check_refresh_worker::DEPLOY_CHECK_REPORT_STALE_AFTER_SECONDS + 5),
    ));
    let updated_at = test_now_rfc3339();
    let stale_report = crate::api::types::DeployCheckReportResponse {
        overall: crate::api::types::DeployCheckOverall {
            result: crate::api::types::DeployCheckResult::Pass,
            blocking_check_ids: Vec::new(),
            summary: "stale cached report".to_string(),
        },
        generated_at: checked_at.clone(),
        checks: Vec::new(),
    };
    state
        .db
        .upsert_deploy_check_report_snapshot(
            crate::deploy_check_refresh_worker::DEPLOY_CHECK_SNAPSHOT_KEY,
            &serde_json::to_string(&stale_report).unwrap(),
            &checked_at,
            &updated_at,
        )
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["refreshing"], true);
    assert_eq!(
        body["retryAfterMs"].as_u64(),
        Some(crate::deploy_check_refresh_worker::DEPLOY_CHECK_PENDING_RETRY_AFTER_MS)
    );
    assert!(state.deploy_check_refresh_worker.is_running());

    let ready_body = wait_for_deploy_check_report_ready(&app, Some("ops")).await;
    assert_eq!(ready_body["status"], "ready");
    assert_eq!(ready_body["refreshing"], false);
    let refreshed_generated_at = ready_body["report"]["generatedAt"].as_str().unwrap();
    assert_ne!(refreshed_generated_at, checked_at.as_str());
}

#[tokio::test]
async fn deploy_check_report_returns_error_after_initial_refresh_failure_until_explicit_retry() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    state
        .deploy_check_refresh_worker
        .set_last_error_for_test(Some("boom".to_string()))
        .await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "internal");
    assert_eq!(
        body["error"]["message"].as_str(),
        Some("deploy-check refresh failed: boom")
    );
    assert!(!state.deploy_check_refresh_worker.is_running());

    let refresh_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/deploy-check/report/refresh")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_resp.status(), 200);
    let refresh_body = response_json(refresh_resp).await;
    assert_eq!(refresh_body["status"], "pending");
    assert_eq!(refresh_body["refreshing"], true);

    let ready_body = wait_for_deploy_check_report_ready(&app, Some("ops")).await;
    assert_eq!(ready_body["status"], "ready");
    assert_eq!(ready_body["refreshing"], false);
}

#[tokio::test]
async fn deploy_check_report_serves_stale_cached_report_and_allows_explicit_refresh_after_failure() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let checked_at = test_offset_from_now_rfc3339(time::Duration::seconds(
        -(crate::deploy_check_refresh_worker::DEPLOY_CHECK_REPORT_STALE_AFTER_SECONDS + 5),
    ));
    let updated_at = test_now_rfc3339();
    let stale_report = crate::api::types::DeployCheckReportResponse {
        overall: crate::api::types::DeployCheckOverall {
            result: crate::api::types::DeployCheckResult::Pass,
            blocking_check_ids: Vec::new(),
            summary: "stale cached report".to_string(),
        },
        generated_at: checked_at.clone(),
        checks: Vec::new(),
    };
    state
        .db
        .upsert_deploy_check_report_snapshot(
            crate::deploy_check_refresh_worker::DEPLOY_CHECK_SNAPSHOT_KEY,
            &serde_json::to_string(&stale_report).unwrap(),
            &checked_at,
            &updated_at,
        )
        .await
        .unwrap();
    state
        .deploy_check_refresh_worker
        .set_last_error_for_test(Some("boom".to_string()))
        .await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["refreshing"], false);
    assert!(body["report"].is_object());
    assert!(body["report"]["generatedAt"].as_str().is_some());
    assert!(!state.deploy_check_refresh_worker.is_running());

    let refresh_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/deploy-check/report/refresh")
                .header("X-Forwarded-User", "ops")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_resp.status(), 200);
    let refresh_body = response_json(refresh_resp).await;
    assert_eq!(refresh_body["status"], "pending");
    assert_eq!(refresh_body["refreshing"], true);

    let ready_body = wait_for_deploy_check_report_ready(&app, Some("ops")).await;
    assert_eq!(ready_body["status"], "ready");
    assert_eq!(ready_body["refreshing"], false);
}

#[tokio::test]
async fn deploy_check_report_fails_when_enabled_feature_is_misconfigured() {
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

    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = None;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_notification_settings(&notification, &now)
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
    assert!(blocking.contains(&"feature.notifications.webhook"));
}

#[tokio::test]
async fn deploy_check_report_fails_when_webhook_scheme_is_not_http() {
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

    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some("ftp://dockrev.example.com/hook".to_string());
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_notification_settings(&notification, &now)
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
    assert!(blocking.contains(&"feature.notifications.webhook"));
}
