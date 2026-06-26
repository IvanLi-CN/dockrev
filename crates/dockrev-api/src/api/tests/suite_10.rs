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
    assert!(settings["auth"].is_object());
    assert!(settings["instance"].is_object());
    assert!(settings["instance"]["publicBaseUrl"].is_null());
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
async fn resource_usage_history_returns_samples_for_window() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-history-{}.yml", ulid::Ulid::new());
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

    let now = time::OffsetDateTime::now_utc();
    let sampled_at_1 = (now - time::Duration::minutes(20))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let sampled_at_2 = (now - time::Duration::minutes(5))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .insert_service_resource_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: service_id.clone(),
                sampled_at: sampled_at_1,
                cpu_percent: 12.5,
                mem_used_bytes: Some(128 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(5_000_000),
                net_tx_bytes: Some(2_500_000),
                block_read_bytes: Some(1_300_000),
                block_write_bytes: Some(900_000),
                pids: Some(8),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: service_id.clone(),
                sampled_at: sampled_at_2,
                cpu_percent: 18.0,
                mem_used_bytes: Some(156 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(8_000_000),
                net_tx_bytes: Some(4_800_000),
                block_read_bytes: Some(2_300_000),
                block_write_bytes: Some(1_700_000),
                pids: Some(11),
                container_count: 1,
            },
        ])
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/resource-usage/history?window=1h"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(payload["window"].as_str(), Some("1h"));
    let samples = payload["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0]["containerCount"].as_u64(), Some(1));
    assert_eq!(samples[1]["cpuPercent"].as_f64(), Some(18.0));
}

#[tokio::test]
async fn resource_usage_overview_returns_latest_samples_and_rates() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-overview-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
  worker:
    image: busybox:1.36
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let web_id = services
        .iter()
        .find(|svc| svc.name == "web")
        .unwrap()
        .id
        .clone();
    let worker_id = services
        .iter()
        .find(|svc| svc.name == "worker")
        .unwrap()
        .id
        .clone();

    let now = time::OffsetDateTime::now_utc();
    let sampled_at_1 = (now - time::Duration::minutes(10))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let sampled_at_2 = (now - time::Duration::minutes(5))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let worker_sampled_at = (now - time::Duration::hours(2))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .insert_service_resource_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: sampled_at_1,
                cpu_percent: 10.0,
                mem_used_bytes: Some(128 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(1_000_000),
                net_tx_bytes: Some(2_000_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(5),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: sampled_at_2,
                cpu_percent: 15.5,
                mem_used_bytes: Some(160 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(1_300_000),
                net_tx_bytes: Some(2_600_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(6),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: worker_id.clone(),
                sampled_at: worker_sampled_at.clone(),
                cpu_percent: 3.25,
                mem_used_bytes: Some(64 * 1024 * 1024),
                mem_limit_bytes: Some(512 * 1024 * 1024),
                net_rx_bytes: Some(5_000),
                net_tx_bytes: Some(7_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(2),
                container_count: 1,
            },
        ])
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/resource-usage/overview?window=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["enabled"].as_bool(), Some(true));
    assert_eq!(payload["window"].as_str(), Some("1h"));
    assert_eq!(payload["staleAfterSeconds"].as_u64(), Some(60));
    let rows = payload["services"].as_array().unwrap();
    assert_eq!(rows.len(), 2);

    let web = rows
        .iter()
        .find(|row| row["serviceId"].as_str() == Some(web_id.as_str()))
        .unwrap();
    assert_eq!(web["sampleCount"].as_u64(), Some(2));
    assert_eq!(web["cpuPercent"].as_f64(), Some(15.5));
    assert_eq!(web["memUsedBytes"].as_u64(), Some(160 * 1024 * 1024));
    assert_eq!(web["stale"].as_bool(), Some(true));
    assert_eq!(web["netRxRateBps"].as_f64(), Some(1000.0));
    assert_eq!(web["netTxRateBps"].as_f64(), Some(2000.0));

    let worker = rows
        .iter()
        .find(|row| row["serviceId"].as_str() == Some(worker_id.as_str()))
        .unwrap();
    assert_eq!(worker["sampleCount"].as_u64(), Some(1));
    assert_eq!(worker["sampledAt"].as_str(), Some(worker_sampled_at.as_str()));
    assert_eq!(worker["cpuPercent"].as_f64(), Some(3.25));
    assert_eq!(worker["memUsedBytes"].as_u64(), Some(64 * 1024 * 1024));
    assert!(worker["netRxRateBps"].is_null());
    assert!(worker["netTxRateBps"].is_null());
    assert_eq!(worker["stale"].as_bool(), Some(true));
}

#[tokio::test]
async fn resource_usage_overview_degrades_when_monitor_disabled() {
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
    let settings = response_json(resp).await;
    let put = serde_json::json!({
        "backup": settings["backup"],
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
                .uri("/api/services/resource-usage/overview?window=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["enabled"].as_bool(), Some(false));
    assert_eq!(payload["window"].as_str(), Some("1h"));
    assert_eq!(payload["staleAfterSeconds"].as_u64(), Some(120));
    assert_eq!(payload["services"].as_array().unwrap().len(), 0);
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
