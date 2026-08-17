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
        .metrics
        .insert_samples(&[
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
    assert!(payload.get("resolutionSeconds").is_none());
    assert!(payload.get("peaks").is_none());

    let resp = app.clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/resource-usage/history?window=7d"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["window"].as_str(), Some("7d"));
    assert_eq!(payload["resolutionSeconds"].as_u64(), Some(60));
    assert_eq!(payload["samples"].as_array().unwrap().len(), 2);
    assert_eq!(payload["peaks"].as_array().unwrap().len(), 2);

    let resp = app.clone().oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/resource-usage/history?window=30d"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["window"].as_str(), Some("30d"));
    assert_eq!(payload["resolutionSeconds"].as_u64(), Some(300));
    let samples = payload["samples"].as_array().unwrap();
    let peaks = payload["peaks"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(peaks.len(), 2);
    assert_eq!(samples[0]["cpuPercent"].as_f64(), Some(12.5));
    assert_eq!(samples[1]["cpuPercent"].as_f64(), Some(18.0));
    assert_eq!(samples[1]["netRxBytes"].as_u64(), Some(8_000_000));
    assert_eq!(peaks[0]["cpuPercent"].as_f64(), Some(12.5));
    assert_eq!(peaks[1]["cpuPercent"].as_f64(), Some(18.0));
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
        .metrics
        .insert_samples(&[
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
            crate::db::ServiceResourceSampleInput {
                service_id: "orphan-service".to_string(),
                sampled_at: worker_sampled_at.clone(),
                cpu_percent: 99.0,
                mem_used_bytes: Some(64 * 1024 * 1024),
                mem_limit_bytes: Some(512 * 1024 * 1024),
                net_rx_bytes: Some(9_000),
                net_tx_bytes: Some(10_000),
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
    assert_eq!(worker["sampleCount"].as_u64(), Some(0));
    assert_eq!(worker["sampledAt"].as_str(), Some(worker_sampled_at.as_str()));
    assert_eq!(worker["cpuPercent"].as_f64(), Some(3.25));
    assert_eq!(worker["memUsedBytes"].as_u64(), Some(64 * 1024 * 1024));
    assert!(worker["netRxRateBps"].is_null());
    assert!(worker["netTxRateBps"].is_null());
    assert_eq!(worker["stale"].as_bool(), Some(true));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/resource-usage/overview?window=3m")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    let rows = payload["services"].as_array().unwrap();
    let web = rows
        .iter()
        .find(|row| row["serviceId"].as_str() == Some(web_id.as_str()))
        .unwrap();
    assert_eq!(web["sampleCount"].as_u64(), Some(0));
    let worker = rows
        .iter()
        .find(|row| row["serviceId"].as_str() == Some(worker_id.as_str()))
        .unwrap();
    assert_eq!(worker["sampleCount"].as_u64(), Some(0));

    for window in ["7d", "30d"] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/services/resource-usage/overview?window={window}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let payload = response_json(resp).await;
        assert_eq!(payload["window"].as_str(), Some(window));
        let rows = payload["services"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|row| row["serviceId"].as_str() != Some("orphan-service")));
        let web = rows
            .iter()
            .find(|row| row["serviceId"].as_str() == Some(web_id.as_str()))
            .unwrap();
        assert_eq!(web["sampleCount"].as_u64(), Some(2));
        let worker = rows
            .iter()
            .find(|row| row["serviceId"].as_str() == Some(worker_id.as_str()))
            .unwrap();
        assert_eq!(worker["sampleCount"].as_u64(), Some(1));
    }
}

#[tokio::test]
async fn resource_usage_overview_backfills_latest_samples_during_upgrade() {
    let db_path = format!(
        "/tmp/dockrev-resource-latest-backfill-{}.sqlite",
        ulid::Ulid::new()
    );
    let compose_path = format!("/tmp/dockrev-resource-upgrade-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
    labels:
      - homepage.group=Brain
      - homepage.name=Web
      - homepage.href=https://web.example.com
"#,
    )
    .unwrap();
    let web_id = {
        let state = test_state(&db_path).await;
        let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
        let services = state.db.list_services_for_check(&stack_id).await.unwrap();
        let web_id = services[0].id.clone();

        state
            .db
            .insert_legacy_metric_fixture(&[
                crate::db::ServiceResourceSampleInput {
                    service_id: web_id.clone(),
                    sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-30)),
                    cpu_percent: 8.0,
                    mem_used_bytes: Some(80),
                    mem_limit_bytes: Some(200),
                    net_rx_bytes: Some(1_000),
                    net_tx_bytes: Some(2_000),
                    block_read_bytes: None,
                    block_write_bytes: None,
                    pids: Some(2),
                    container_count: 1,
                },
                crate::db::ServiceResourceSampleInput {
                    service_id: web_id.clone(),
                    sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-10)),
                    cpu_percent: 12.0,
                    mem_used_bytes: Some(120),
                    mem_limit_bytes: Some(200),
                    net_rx_bytes: Some(3_000),
                    net_tx_bytes: Some(5_000),
                    block_read_bytes: None,
                    block_write_bytes: None,
                    pids: Some(3),
                    container_count: 1,
                },
            ])
            .await
            .unwrap();

        web_id
    };

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("DELETE FROM service_resource_latest_samples", [])
            .unwrap();
    }

    let state = test_state(&db_path).await;
    let app = api::router(state.clone());

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
    let rows = payload["services"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let web = &rows[0];
    assert_eq!(web["serviceId"].as_str(), Some(web_id.as_str()));
    assert_eq!(web["sampleCount"].as_u64(), Some(2));
    assert_eq!(web["cpuPercent"].as_f64(), Some(12.0));
    assert_eq!(web["memUsedBytes"].as_u64(), Some(120));
    let net_rx = web["netRxRateBps"].as_f64().unwrap();
    assert!((net_rx - 100.0).abs() < 0.01, "unexpected net rx rate: {net_rx}");
    let net_tx = web["netTxRateBps"].as_f64().unwrap();
    assert!((net_tx - 150.0).abs() < 0.01, "unexpected net tx rate: {net_tx}");

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/homepage/nav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["serviceId"].as_str(), Some(web_id.as_str()));
    assert_eq!(items[0]["resource"]["sampleCount"].as_u64(), Some(2));
    assert_eq!(items[0]["resource"]["cpuPercent"].as_f64(), Some(12.0));
}

#[tokio::test]
async fn resource_usage_overview_ignores_out_of_order_older_latest_write() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-out-of-order-{}.yml", ulid::Ulid::new());
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
    let web_id = services[0].id.clone();

    state
        .metrics
        .insert_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-20)),
                cpu_percent: 10.0,
                mem_used_bytes: Some(100),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(1_000),
                net_tx_bytes: Some(2_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(2),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-10)),
                cpu_percent: 12.5,
                mem_used_bytes: Some(120),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(2_000),
                net_tx_bytes: Some(4_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(3),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-30)),
                cpu_percent: 6.0,
                mem_used_bytes: Some(60),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(500),
                net_tx_bytes: Some(800),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(1),
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
    let web = &payload["services"].as_array().unwrap()[0];
    assert_eq!(web["serviceId"].as_str(), Some(web_id.as_str()));
    assert_eq!(web["cpuPercent"].as_f64(), Some(12.5));
    assert_eq!(web["sampleCount"].as_u64(), Some(3));
    let net_rx = web["netRxRateBps"].as_f64().unwrap();
    assert!((net_rx - 100.0).abs() < 0.01, "unexpected net rx rate: {net_rx}");
}

#[tokio::test]
async fn resource_usage_overview_ignores_previous_sample_outside_requested_window() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-resource-window-boundary-{}.yml",
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
    let web_id = services[0].id.clone();

    state
        .metrics
        .insert_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::minutes(-25)),
                cpu_percent: 8.0,
                mem_used_bytes: Some(80),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(500),
                net_tx_bytes: Some(1_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(2),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: web_id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::minutes(-2)),
                cpu_percent: 12.0,
                mem_used_bytes: Some(120),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(2_000),
                net_tx_bytes: Some(4_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(3),
                container_count: 1,
            },
        ])
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/resource-usage/overview?window=3m")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    let web = &payload["services"].as_array().unwrap()[0];
    assert_eq!(web["serviceId"].as_str(), Some(web_id.as_str()));
    assert_eq!(web["sampleCount"].as_u64(), Some(1));
    assert!(web["netRxRateBps"].is_null());
    assert!(web["netTxRateBps"].is_null());
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
        "backup": {
            "enabled": settings["backup"]["enabled"],
            "requireSuccess": settings["backup"]["requireSuccess"],
            "skipTargetsOverBytes": settings["backup"]["skipTargetsOverBytes"],
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
async fn api_jobs_compact_omits_raw_summary_and_keeps_derived_fields() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let now = test_now_rfc3339();
    let job_id = ids::new_job_id();
    let job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    state.db.insert_job(job).await.unwrap();
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &now,
            &serde_json::json!({
            "targetDisplayTag": "1.2.3",
            "action": "stop",
                "secretDiagnostic": "must not leave this endpoint",
                "progress": {
                    "phase": "done",
                    "message": "update finished",
                    "current": 1,
                    "total": 1,
                    "percent": 100,
                    "updatedAt": now,
                }
            }),
        )
        .await
        .unwrap();

    let compact = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs?view=compact&limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(compact.status(), 200);
    let compact = response_json(compact).await;
    let item = &compact["jobs"][0];
    assert!(item.get("summary").is_none());
    assert!(item.get("secretDiagnostic").is_none());
    assert_eq!(item["targetVersion"].as_str(), Some("1.2.3"));
    assert_eq!(item["displayLabel"].as_str(), Some("停止任务"));
    assert_eq!(item["progress"]["percent"].as_u64(), Some(100));

    let default_response = app
        .oneshot(
            Request::builder()
                .uri("/api/jobs?limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let default_response = response_json(default_response).await;
    assert_eq!(
        default_response["jobs"][0]["summary"]["secretDiagnostic"].as_str(),
        Some("must not leave this endpoint"),
    );
}

#[tokio::test]
async fn api_jobs_compact_projects_failed_stack_transition_reason() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let now = test_now_rfc3339();
    let job_id = ids::new_job_id();
    state
        .db
        .insert_job(
            crate::api::types::JobRecord::new_running(
                job_id.clone(),
                crate::api::types::JobType::Update,
                crate::api::types::JobScope::All,
                None,
                None,
                &now,
            )
            .to_db(),
        )
        .await
        .unwrap();
    state
        .db
        .finish_job(
            &job_id,
            "rolled_back",
            &now,
            &serde_json::json!({
                "stacks": [{
                    "update": {
                        "failureStep": "healthcheck",
                        "lastError": "container never became healthy"
                    }
                }],
                "secretDiagnostic": "must not leave this endpoint"
            }),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/jobs?view=compact&limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response = response_json(response).await;
    let item = &response["jobs"][0];
    assert!(item.get("summary").is_none());
    assert_eq!(
        item["resultReason"]["summary"].as_str(),
        Some("健康检查失败，已回滚")
    );
    assert_eq!(
        item["resultReason"]["raw"].as_str(),
        Some("container never became healthy")
    );
}

#[tokio::test]
async fn homepage_nav_returns_single_read_model_with_resources_and_status() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-homepage-nav-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:5.2.1
    labels:
      - homepage.group=Brain
      - homepage.name=Acme API
      - homepage.icon=si-github
      - homepage.href=https://api.example.com
      - homepage.description=Primary API
  worker:
    image: ghcr.io/acme/worker:5.2.0
    labels:
      - homepage.group=Ops
      - homepage.name=Worker
      - homepage.description=No href should hide
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let api_service = services.iter().find(|service| service.name == "api").unwrap();

    state
        .db
        .update_service_check_result(
            &api_service.id,
            Some("sha256:cur".to_string()),
            Some("5.2.1".to_string()),
            Some(serde_json::to_string(&vec!["5.2.1"]).unwrap()),
            Some("5.2.3".to_string()),
            Some("5.2.3".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some(serde_json::to_string(&vec!["linux/amd64"]).unwrap()),
            None,
            None,
            &test_now_rfc3339(),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    state
        .metrics
        .insert_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: api_service.id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-20)),
                cpu_percent: 10.0,
                mem_used_bytes: Some(100),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(1_000),
                net_tx_bytes: Some(2_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(2),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: api_service.id.clone(),
                sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-10)),
                cpu_percent: 12.5,
                mem_used_bytes: Some(120),
                mem_limit_bytes: Some(200),
                net_rx_bytes: Some(2_000),
                net_tx_bytes: Some(4_000),
                block_read_bytes: None,
                block_write_bytes: None,
                pids: Some(3),
                container_count: 1,
            },
        ])
        .await
        .unwrap();
    state
        .metrics
        .insert_samples(&[crate::db::ServiceResourceSampleInput {
            service_id: "orphaned-service".to_string(),
            sampled_at: test_offset_from_now_rfc3339(time::Duration::seconds(-10)),
            cpu_percent: 99.0,
            mem_used_bytes: Some(100),
            mem_limit_bytes: Some(200),
            net_rx_bytes: Some(1_000),
            net_tx_bytes: Some(2_000),
            block_read_bytes: None,
            block_write_bytes: None,
            pids: Some(2),
            container_count: 1,
        }])
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/homepage/nav")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert!(payload["generatedAt"].as_str().is_some());
    assert!(payload["lastCheckAt"].as_str().is_some());
    assert_eq!(payload["resourceSummary"]["enabled"].as_bool(), Some(true));
    let summary_services = payload["resourceSummary"]["services"]
        .as_array()
        .unwrap();
    let summary_api = summary_services
        .iter()
        .find(|row| row["serviceId"].as_str() == Some(api_service.id.as_str()))
        .unwrap();
    assert!(summary_services
        .iter()
        .all(|row| row["serviceId"].as_str() != Some("orphaned-service")));
    assert_eq!(summary_api["sampleCount"].as_u64(), Some(2));
    assert_eq!(payload["items"].as_array().unwrap().len(), 1);
    let item = &payload["items"].as_array().unwrap()[0];
    assert_eq!(item["serviceName"].as_str(), Some("api"));
    assert_eq!(item["homepage"]["name"].as_str(), Some("Acme API"));
    assert_eq!(item["candidate"]["tag"].as_str(), Some("5.2.3"));
    assert_eq!(item["resource"]["cpuPercent"].as_f64(), Some(12.5));
    let net_rx = item["resource"]["netRxRateBps"].as_f64().unwrap();
    assert!((net_rx - 100.0).abs() < 0.01, "unexpected net rx rate: {net_rx}");
}

#[tokio::test]
#[ignore = "performance baseline; run explicitly in a quiet development or CI environment"]
async fn homepage_metrics_isolation() {
    let db_path = format!("/tmp/dockrev-homepage-perf-{}.sqlite3", ulid::Ulid::new());
    let state = test_state_with_authz(&db_path, Some("performance-user"), None, false).await;
    let app = api::router(state.clone());
    let compose_path = format!("/tmp/dockrev-homepage-perf-{}.yml", ulid::Ulid::new());
    let mut compose = "services:\n".to_string();
    for index in 0..51 {
        compose.push_str(&format!(
            "  service-{index}:\n    image: nginx:1.27\n    labels:\n      - homepage.group=Performance\n      - homepage.name=Service {index}\n      - homepage.href=https://service-{index}.example.test\n"
        ));
    }
    std::fs::write(&compose_path, compose).unwrap();
    let stack_id = seed_stack_from_compose(&state, "performance", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    assert_eq!(services.len(), 51);
    let primary_probe = rusqlite::Connection::open(&db_path).unwrap();
    let primary_data_version_before: i64 = primary_probe
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();

    let now = time::OffsetDateTime::now_utc();
    let mut samples = Vec::with_capacity(services.len() * 2);
    for offset_seconds in [-10, -5] {
        let sampled_at = (now + time::Duration::seconds(offset_seconds))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        for (index, service) in services.iter().enumerate() {
            samples.push(crate::db::ServiceResourceSampleInput {
                service_id: service.id.clone(),
                sampled_at: sampled_at.clone(),
                cpu_percent: 10.0 + index as f64,
                mem_used_bytes: Some(128 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some((index as u64 + 1) * 1_000),
                net_tx_bytes: Some((index as u64 + 1) * 2_000),
                block_read_bytes: Some((index as u64 + 1) * 500),
                block_write_bytes: Some((index as u64 + 1) * 250),
                pids: Some(3),
                container_count: 1,
            });
        }
    }
    state.metrics.insert_samples(&samples).await.unwrap();

    let sampling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let sampling_batches = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sampler_metrics = state.metrics.clone();
    let sampler_services = services
        .iter()
        .enumerate()
        .map(|(index, service)| (index, service.id.clone()))
        .collect::<Vec<_>>();
    let sampler_active = sampling.clone();
    let sampler_batches = sampling_batches.clone();
    let (sampling_started_tx, sampling_started_rx) = tokio::sync::oneshot::channel();
    let sampling_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let sampler = tokio::spawn(async move {
        let mut tick = 0_i64;
        let mut sampling_started_tx = Some(sampling_started_tx);
        while sampler_active.load(std::sync::atomic::Ordering::Relaxed)
            && std::time::Instant::now() < sampling_deadline
        {
            let sampled_at = (time::OffsetDateTime::now_utc()
                + time::Duration::milliseconds(tick))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
            let batch = sampler_services
                .iter()
                .map(|(index, service_id)| crate::db::ServiceResourceSampleInput {
                    service_id: service_id.clone(),
                    sampled_at: sampled_at.clone(),
                    cpu_percent: 10.0 + *index as f64,
                    mem_used_bytes: Some(128 * 1024 * 1024),
                    mem_limit_bytes: Some(1024 * 1024 * 1024),
                    net_rx_bytes: Some((tick as u64 + 1) * (*index as u64 + 1) * 1_000),
                    net_tx_bytes: Some((tick as u64 + 1) * (*index as u64 + 1) * 2_000),
                    block_read_bytes: Some((tick as u64 + 1) * (*index as u64 + 1) * 500),
                    block_write_bytes: Some((tick as u64 + 1) * (*index as u64 + 1) * 250),
                    pids: Some(3),
                    container_count: 1,
                })
                .collect::<Vec<_>>();
            sampler_metrics.insert_samples(&batch).await.unwrap();
            sampler_batches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(sender) = sampling_started_tx.take() {
                let _ = sender.send(());
            }
            tick += 1;
            tokio::task::yield_now().await;
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), sampling_started_rx)
        .await
        .expect("continuous metrics writer did not start")
        .expect("continuous metrics writer stopped before its first write");

    let mut requests = Vec::with_capacity(100);
    for _ in 0..100 {
        let app = app.clone();
        requests.push(tokio::spawn(async move {
            let started_at = std::time::Instant::now();
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/homepage/nav")
                        .header("X-Forwarded-User", "performance-user")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            started_at.elapsed()
        }));
    }
    let mut latencies = Vec::with_capacity(requests.len());
    for request in requests {
        latencies.push(request.await.unwrap());
    }
    sampler.await.unwrap();
    assert!(
        sampling_batches.load(std::sync::atomic::Ordering::Relaxed) >= 2,
        "continuous metrics writer completed fewer than two batches"
    );
    let primary_data_version_after: i64 = primary_probe
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        primary_data_version_before, primary_data_version_after,
        "homepage navigation must not write the primary database"
    );
    latencies.sort_unstable();
    let p95 = latencies[(latencies.len() * 95).div_ceil(100) - 1];
    assert!(
        p95 <= std::time::Duration::from_millis(300),
        "homepage navigation p95 was {p95:?} under 51-service metrics load"
    );
}
