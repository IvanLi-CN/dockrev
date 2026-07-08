fn digest_map(entries: &[(&str, &str)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        map.insert((*key).to_string(), serde_json::Value::String((*value).to_string()));
    }
    serde_json::Value::Object(map)
}

#[tokio::test]
async fn get_service_backup_records_returns_only_related_actual_backup_rows()
{
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-records-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - ./data:/srv/data
  web:
    image: ghcr.io/acme/web:1.0
    volumes:
      - ./data:/srv/data
  other:
    image: ghcr.io/acme/other:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let web_id = service_id_by_name(&state, &stack_id, "web").await;
    let other_id = service_id_by_name(&state, &stack_id, "other").await;
    let now = test_now_rfc3339();
    let older = test_offset_rfc3339(&now, time::Duration::hours(-1));
    let oldest = test_offset_rfc3339(&now, time::Duration::hours(-2));
    let cleanup_after = test_offset_rfc3339(&now, time::Duration::hours(6));

    insert_update_job_with_summary(
        &state,
        "job-svc-api",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&api_id),
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "success",
                    "artifactPath": "/tmp/api.tar.gz",
                    "sizeBytes": 1500,
                    "targets": [
                        {
                            "target": { "kind": "bind-mount", "path": "/srv/data" },
                            "status": "included",
                            "sizeBytes": 1500,
                            "policy": "live_backup",
                            "relatedServices": ["api", "web"]
                        },
                        {
                            "target": { "kind": "docker-volume", "name": "api-cache" },
                            "status": "skipped",
                            "reason": "skipped_by_size",
                            "sizeBytes": 2048
                        }
                    ]
                },
                "update": {
                    "changedServices": 1,
                    "oldDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.0")]),
                    "newDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")]),
                    "finalDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")])
                }
            }]
        }),
        &now,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-api",
        &stack_id,
        "job-svc-api",
        &now,
        "success",
        Some("/tmp/api.tar.gz"),
        Some(1500),
        None,
        Some(&cleanup_after),
        None,
    )
    .await;

    insert_update_job_with_summary(
        &state,
        "job-stack",
        crate::api::types::JobScope::Stack,
        Some(&stack_id),
        None,
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "skipped",
                    "reason": "no_included_targets",
                    "targets": [{
                        "target": { "kind": "bind-mount", "path": "/srv/data" },
                        "status": "skipped",
                        "reason": "skipped_by_size",
                        "sizeBytes": 2048
                    }]
                },
                "update": {
                    "changedServices": 2,
                    "oldDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.0"),
                        (web_id.as_str(), "ghcr.io/acme/web:1.0"),
                    ]),
                    "newDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.1"),
                        (web_id.as_str(), "ghcr.io/acme/web:1.1"),
                    ]),
                    "finalDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.1"),
                        (web_id.as_str(), "ghcr.io/acme/web:1.1"),
                    ])
                }
            }]
        }),
        &older,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-stack",
        &stack_id,
        "job-stack",
        &older,
        "skipped",
        None,
        None,
        None,
        None,
        None,
    )
    .await;

    insert_update_job_with_summary(
        &state,
        "job-all",
        crate::api::types::JobScope::All,
        None,
        None,
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "failed",
                    "error": "archive failed",
                    "targets": [{
                        "target": { "kind": "docker-volume", "name": "api-data" },
                        "status": "skipped",
                        "reason": "skipped_by_probe_error"
                    }]
                },
                "rollback": {
                    "changedServices": 2,
                    "oldDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.1"),
                        (other_id.as_str(), "ghcr.io/acme/other:1.0"),
                    ]),
                    "newDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.0"),
                        (other_id.as_str(), "ghcr.io/acme/other:1.1"),
                    ]),
                    "finalDigests": digest_map(&[
                        (api_id.as_str(), "ghcr.io/acme/api:1.0"),
                        (other_id.as_str(), "ghcr.io/acme/other:1.1"),
                    ])
                }
            }]
        }),
        &oldest,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-all",
        &stack_id,
        "job-all",
        &oldest,
        "failed",
        None,
        None,
        Some("archive failed"),
        None,
        Some(&now),
    )
    .await;

    insert_update_job_with_summary(
        &state,
        "job-unrelated",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&other_id),
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "success",
                    "targets": []
                },
                "update": {
                    "changedServices": 1,
                    "oldDigests": digest_map(&[(other_id.as_str(), "ghcr.io/acme/other:1.0")]),
                    "newDigests": digest_map(&[(other_id.as_str(), "ghcr.io/acme/other:1.1")]),
                    "finalDigests": digest_map(&[(other_id.as_str(), "ghcr.io/acme/other:1.1")])
                }
            }]
        }),
        &oldest,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-unrelated",
        &stack_id,
        "job-unrelated",
        &oldest,
        "success",
        Some("/tmp/unrelated.tar.gz"),
        Some(88),
        None,
        None,
        None,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{api_id}/backup-records"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let records = body["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["backupId"].as_str(), Some("bkp-api"));
    assert_eq!(records[0]["scope"].as_str(), Some("service"));
    assert_eq!(records[0]["sizeBytes"].as_u64(), Some(1500));
    assert_eq!(records[0]["cleanupAfter"].as_str(), Some(cleanup_after.as_str()));
    assert_eq!(records[0]["status"].as_str(), Some("success"));
    assert_eq!(records[0]["assets"].as_array().unwrap().len(), 1);
    assert_eq!(records[0]["assets"][0]["policy"].as_str(), Some("live_backup"));
    assert_eq!(records[0]["assets"][0]["status"].as_str(), Some("included"));
    assert_eq!(records[0]["assets"][0]["sizeBytes"].as_u64(), Some(1500));
}

#[tokio::test]
async fn get_service_backup_records_keeps_service_scope_noop_updates() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-records-noop-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "noop", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let now = test_now_rfc3339();

    insert_update_job_with_summary(
        &state,
        "job-noop",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&api_id),
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "success",
                    "targets": [{
                        "target": { "kind": "bind-mount", "path": "/srv/data" },
                        "status": "included",
                        "policy": "live_backup",
                        "sizeBytes": 128
                    }]
                },
                "update": {
                    "changedServices": 0,
                    "oldDigests": {},
                    "newDigests": {},
                    "finalDigests": {}
                }
            }]
        }),
        &now,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-noop",
        &stack_id,
        "job-noop",
        &now,
        "success",
        Some("/tmp/noop.tar.gz"),
        Some(128),
        None,
        None,
        None,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{api_id}/backup-records"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let records = body["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["backupId"].as_str(), Some("bkp-noop"));
    assert_eq!(records[0]["scope"].as_str(), Some("service"));
    assert_eq!(records[0]["assets"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_service_backup_records_hides_skipped_no_included_targets_rows() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-records-hidden-noise-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - ./data:/srv/data
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "noise", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let now = test_now_rfc3339();

    insert_update_job_with_summary(
        &state,
        "job-hidden-noise",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&api_id),
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "skipped",
                    "reason": "no_included_targets",
                    "targets": [{
                        "target": { "kind": "bind-mount", "path": "/srv/data" },
                        "status": "skipped",
                        "reason": "skipped_by_user"
                    }]
                },
                "update": {
                    "changedServices": 1,
                    "oldDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.0")]),
                    "newDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")]),
                    "finalDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")])
                }
            }]
        }),
        &now,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-hidden-noise",
        &stack_id,
        "job-hidden-noise",
        &now,
        "skipped",
        None,
        None,
        None,
        None,
        None,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{api_id}/backup-records"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let records = body["records"].as_array().unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn get_service_backup_records_hides_failed_rows_without_actual_backup_artifacts() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_dir = format!("/tmp/dockrev-backup-records-hidden-failed-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir.clone()).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
    volumes:
      - ./data:/srv/data
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "failed", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let now = test_now_rfc3339();

    insert_update_job_with_summary(
        &state,
        "job-hidden-failed",
        crate::api::types::JobScope::Stack,
        Some(&stack_id),
        None,
        json!({
            "stacks": [{
                "stackId": stack_id,
                "backup": {
                    "status": "failed",
                    "error": "archive failed",
                    "targets": [{
                        "target": { "kind": "bind-mount", "path": "/srv/data" },
                        "status": "included",
                        "policy": "live_backup",
                        "sizeBytes": 256
                    }]
                },
                "update": {
                    "changedServices": 1,
                    "oldDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.0")]),
                    "newDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")]),
                    "finalDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")])
                }
            }]
        }),
        &now,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-hidden-failed",
        &stack_id,
        "job-hidden-failed",
        &now,
        "failed",
        None,
        None,
        Some("archive failed"),
        None,
        None,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{api_id}/backup-records"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let records = body["records"].as_array().unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn get_service_backup_records_excludes_other_stack_backups_from_shared_all_scope_jobs()
{
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_dir_a = format!("/tmp/dockrev-backup-records-a-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir_a.clone()).unwrap();
    let compose_path_a = format!("{compose_dir_a}/compose.yml");
    std::fs::write(
        &compose_path_a,
        r#"
services:
  api:
    image: ghcr.io/acme/api:1.0
"#,
    )
    .unwrap();
    let stack_a_id = seed_stack_from_compose(&state, "alpha", &compose_path_a).await;
    let api_id = service_id_by_name(&state, &stack_a_id, "api").await;

    let compose_dir_b = format!("/tmp/dockrev-backup-records-b-{}", ulid::Ulid::new());
    std::fs::create_dir_all(compose_dir_b.clone()).unwrap();
    let compose_path_b = format!("{compose_dir_b}/compose.yml");
    std::fs::write(
        &compose_path_b,
        r#"
services:
  worker:
    image: ghcr.io/acme/worker:1.0
"#,
    )
    .unwrap();
    let stack_b_id = seed_stack_from_compose(&state, "beta", &compose_path_b).await;
    let worker_id = service_id_by_name(&state, &stack_b_id, "worker").await;

    let now = test_now_rfc3339();
    insert_update_job_with_summary(
        &state,
        "job-all-cross-stack",
        crate::api::types::JobScope::All,
        None,
        None,
        json!({
            "stacks": [
                {
                    "stackId": stack_a_id,
                    "backup": {
                        "status": "success",
                        "targets": [{
                            "target": { "kind": "bind-mount", "path": "/srv/api-data" },
                            "status": "included",
                            "policy": "live_backup",
                            "sizeBytes": 512
                        }]
                    },
                    "update": {
                        "changedServices": 1,
                        "oldDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.0")]),
                        "newDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")]),
                        "finalDigests": digest_map(&[(api_id.as_str(), "ghcr.io/acme/api:1.1")])
                    }
                },
                {
                    "stackId": stack_b_id,
                    "backup": {
                        "status": "success",
                        "targets": [{
                            "target": { "kind": "bind-mount", "path": "/srv/worker-data" },
                            "status": "included",
                            "policy": "live_backup",
                            "sizeBytes": 2048
                        }]
                    },
                    "update": {
                        "changedServices": 1,
                        "oldDigests": digest_map(&[(worker_id.as_str(), "ghcr.io/acme/worker:1.0")]),
                        "newDigests": digest_map(&[(worker_id.as_str(), "ghcr.io/acme/worker:1.1")]),
                        "finalDigests": digest_map(&[(worker_id.as_str(), "ghcr.io/acme/worker:1.1")])
                    }
                }
            ]
        }),
        &now,
    )
    .await;

    insert_backup_record(
        &state,
        "bkp-alpha",
        &stack_a_id,
        "job-all-cross-stack",
        &now,
        "success",
        Some("/tmp/alpha.tar.gz"),
        Some(512),
        None,
        None,
        None,
    )
    .await;
    insert_backup_record(
        &state,
        "bkp-beta",
        &stack_b_id,
        "job-all-cross-stack",
        &now,
        "success",
        Some("/tmp/beta.tar.gz"),
        Some(2048),
        None,
        None,
        None,
    )
    .await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{api_id}/backup-records"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let records = body["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["backupId"].as_str(), Some("bkp-alpha"));
    assert_eq!(records[0]["assets"].as_array().unwrap().len(), 1);
    assert_eq!(
        records[0]["assets"][0]["target"]["path"].as_str(),
        Some("/srv/api-data")
    );
}

use crate::{
    api::{ServiceLogEventEnvelope, ServiceLogLine},
    service_logs::ServiceLogRealtimeMessage,
};

#[tokio::test]
async fn service_logs_snapshot_returns_single_service_stream() {
    let db_path = format!("/tmp/dockrev-service-logs-snapshot-{}.db", ulid::Ulid::new());
    let compose_path = format!("/tmp/dockrev-service-logs-snapshot-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), Arc::new(ServiceLogsRunner)).await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs?tail=5"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    assert_eq!(body["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(body["lines"].as_array().map(Vec::len), Some(5));
    assert!(body["lines"][0].get("container").is_none());
    assert_eq!(body["lines"][0]["raw"].as_str(), Some("\u{1b}[32mapi boot ok\u{1b}[0m"));
    assert_eq!(
        body["lines"][3]["plain"].as_str(),
        Some("\u{1b}[31merror burst\u{1b}[0m")
    );
    assert_eq!(body["lines"][1]["meta"]["format"].as_str(), Some("json"));
    assert_eq!(body["lines"][1]["meta"]["level"].as_str(), Some("info"));
    assert_eq!(
        body["lines"][1]["meta"]["message"].as_str(),
        Some("runtime perf")
    );
    assert_eq!(
        body["lines"][1]["meta"]["attributes"]["component"].as_str(),
        Some("admin_read")
    );
    assert_eq!(
        body["lines"][1]["meta"]["attributes"]["elapsed_ms"].as_i64(),
        Some(24)
    );
    assert_eq!(body["lines"][2]["meta"]["format"].as_str(), Some("text"));
    assert_eq!(body["lines"][2]["meta"]["level"].as_str(), Some("info"));
    assert_eq!(
        body["lines"][2]["meta"]["timestamp"].as_str(),
        Some("2026-07-07T05:54:01.126674Z")
    );
    assert_eq!(
        body["lines"][2]["meta"]["message"].as_str(),
        Some("openai proxy request started")
    );
    assert_eq!(
        body["lines"][2]["meta"]["attributes"]["proxy_request_id"].as_u64(),
        Some(2722)
    );
    assert_eq!(
        body["lines"][2]["meta"]["attributes"]["method"].as_str(),
        Some("POST")
    );
    assert_eq!(
        body["lines"][2]["meta"]["attributes"]["uri"].as_str(),
        Some("/v1/responses")
    );
    assert_eq!(
        body["lines"][2]["meta"]["attributes"]["proxy_request_started"].as_bool(),
        Some(true)
    );
    assert_eq!(
        body["lines"][2]["meta"]["attributes"]["content_length"].as_str(),
        Some("Some(569164)")
    );
}

#[tokio::test]
async fn service_logs_snapshot_keeps_standalone_indented_line_separate() {
    let db_path = format!(
        "/tmp/dockrev-service-logs-indented-{}.db",
        ulid::Ulid::new()
    );
    let compose_path = format!(
        "/tmp/dockrev-service-logs-indented-{}.yml",
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
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), Arc::new(ServiceLogsRunner)).await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs?tail=2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let lines = body["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0]["raw"].as_str(),
        Some("\u{1b}[31merror burst\u{1b}[0m")
    );
    assert_eq!(
        lines[1]["raw"].as_str(),
        Some("    standalone indented output")
    );
}

#[tokio::test]
async fn service_logs_snapshot_groups_multiline_application_error() {
    let db_path = format!(
        "/tmp/dockrev-service-logs-multiline-{}.db",
        ulid::Ulid::new()
    );
    let compose_path = format!(
        "/tmp/dockrev-service-logs-multiline-{}.yml",
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
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), Arc::new(ServiceLogsRunner)).await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs?tail=9"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let lines = body["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 6);

    let multiline = lines
        .iter()
        .find(|line| {
            line["raw"]
                .as_str()
                .is_some_and(|raw| raw.contains("failed to broadcast pool attempt"))
        })
        .expect("multiline log should be present");
    let raw = multiline["raw"].as_str().unwrap();
    assert_eq!(
        multiline["ts"].as_str(),
        Some("2026-07-01T08:12:51.833063000Z")
    );
    assert!(raw.contains("\n\nCaused by:\n    (code: 5) database is locked"));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["raw"]
                .as_str()
                .is_some_and(|raw| raw.contains("database is locked")))
            .count(),
        1
    );
}

#[tokio::test]
async fn service_logs_snapshot_falls_back_to_project_scan_when_service_filter_is_empty() {
    let db_path = format!(
        "/tmp/dockrev-service-logs-project-fallback-{}.db",
        ulid::Ulid::new()
    );
    let compose_path = format!(
        "/tmp/dockrev-service-logs-project-fallback-{}.yml",
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
    let state = test_state_with(
        &db_path,
        Arc::new(FakeRegistry),
        Arc::new(ServiceLogsProjectWideRunner),
    )
    .await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs?tail=3"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    assert_eq!(body["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(body["lines"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["lines"][0]["plain"].as_str(),
        Some("resolved from project scan")
    );
}

#[tokio::test]
async fn service_logs_snapshot_includes_stderr_stream() {
    let db_path = format!(
        "/tmp/dockrev-service-logs-stderr-{}.db",
        ulid::Ulid::new()
    );
    let compose_path = format!(
        "/tmp/dockrev-service-logs-stderr-{}.yml",
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
    let state = test_state_with(
        &db_path,
        Arc::new(FakeRegistry),
        Arc::new(ServiceLogsStderrRunner),
    )
    .await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs?tail=3"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    assert_eq!(body["lines"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["lines"][0]["plain"].as_str(), Some("stderr-only line"));
}

#[tokio::test]
async fn service_logs_events_include_stderr_follow_stream() {
    let db_path = format!(
        "/tmp/dockrev-service-logs-stderr-events-{}.db",
        ulid::Ulid::new()
    );
    let compose_path = format!(
        "/tmp/dockrev-service-logs-stderr-events-{}.yml",
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
    let state = test_state_with(
        &db_path,
        Arc::new(FakeRegistry),
        Arc::new(ServiceLogsStderrRunner),
    )
    .await;

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let mut subscription = state.service_log_hub.subscribe(&service_id).await;
    let message = tokio::time::timeout(Duration::from_secs(2), subscription.recv())
        .await
        .expect("stderr follow event should arrive")
        .expect("stderr follow event should be readable");

    let ServiceLogRealtimeMessage::Event(ServiceLogEventEnvelope::Line { line, .. }) = message else {
        panic!("expected service log line event");
    };
    assert_eq!(line.plain, "stderr-follow line");
}

#[tokio::test]
async fn service_logs_events_replay_and_reset_gap() {
    let db_path = format!("/tmp/dockrev-service-logs-events-{}.db", ulid::Ulid::new());
    let compose_path = format!("/tmp/dockrev-service-logs-events-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let state = test_state_with(&db_path, Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-logs").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    state
        .service_log_hub
        .seed_test_buffer(
            &service_id,
            vec![
                ServiceLogEventEnvelope::Line {
                    id: 10,
                    service_id: service_id.clone(),
                    line: ServiceLogLine {
                        ts: "2026-06-29T08:00:00.000000000Z".to_string(),
                        raw: "raw 10".to_string(),
                        plain: "raw 10".to_string(),
                        meta: None,
                    },
                },
                ServiceLogEventEnvelope::Line {
                    id: 11,
                    service_id: service_id.clone(),
                    line: ServiceLogLine {
                        ts: "2026-06-29T08:00:01.000000000Z".to_string(),
                        raw: "raw 11".to_string(),
                        plain: "raw 11".to_string(),
                        meta: None,
                    },
                },
            ],
        )
        .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs/events?afterId=10"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();
    let replay = wait_for_sse_event(&mut body, "service_log_line", Duration::from_secs(2)).await;
    let replay_data: serde_json::Value = serde_json::from_str(&replay.data).unwrap();
    assert_eq!(replay.id.as_deref(), Some("11"));
    assert_eq!(replay_data["line"]["raw"].as_str(), Some("raw 11"));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/logs/events?afterId=5"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();
    let reset = wait_for_sse_event(&mut body, "service_log_reset", Duration::from_secs(2)).await;
    let reset_data: serde_json::Value = serde_json::from_str(&reset.data).unwrap();
    assert_eq!(reset_data["reason"].as_str(), Some("buffer_gap_reset"));
}
