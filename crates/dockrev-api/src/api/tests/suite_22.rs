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

#[tokio::test]
async fn service_lifecycle_status_and_start_task_are_service_scoped() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FakeRunner),
        "docker",
    )
    .await;
    let app = api::router(state.clone());
    let (_stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/lifecycle-status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    assert_eq!(response_json(status).await["state"].as_str(), Some("stopped"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let job_id = response_json(response).await["jobId"]
        .as_str()
        .unwrap()
        .to_string();
    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "service_lifecycle");
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    assert_eq!(job.summary_json["action"].as_str(), Some("start"));
    assert_eq!(job.status, "success");
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert_eq!(
        logs.first().map(|line| line.msg.as_str()),
        Some("service lifecycle start started")
    );
}

#[tokio::test]
async fn stack_lifecycle_status_and_start_task_are_stack_scoped() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FakeRunner),
        "docker",
    )
    .await;
    let app = api::router(state.clone());
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}/lifecycle-status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    assert_eq!(response_json(status).await["state"].as_str(), Some("stopped"));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/stacks/{stack_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let job_id = response_json(response).await["jobId"]
        .as_str()
        .unwrap()
        .to_string();
    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "stack_lifecycle");
    assert_eq!(job.scope.as_str(), "stack");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id, None);
    assert_eq!(job.summary_json["action"].as_str(), Some("start"));
    assert_eq!(
        job.summary_json["serviceIds"].as_array().unwrap(),
        &[serde_json::Value::String(service_id)]
    );
    assert_eq!(job.status, "success");
}

#[tokio::test]
async fn stack_lifecycle_claim_blocks_service_lifecycle_and_update() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let stack_job_id = ids::new_job_id();
    let stack_job = crate::api::types::JobRecord::new_running(
        stack_job_id.clone(),
        crate::api::types::JobType::StackLifecycle,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &now,
    );
    let target = crate::db::ServiceOperationTarget {
        service_id: service_id.clone(),
        stack_id: stack_id.clone(),
    };
    assert!(state
        .db
        .insert_service_operation_job_if_unblocked(
            stack_job.to_db(),
            vec![target.clone()],
            None,
        )
        .await
        .unwrap()
        .is_none());

    for job_type in [
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobType::Update,
    ] {
        let job = crate::api::types::JobRecord::new_running(
            ids::new_job_id(),
            job_type,
            crate::api::types::JobScope::Service,
            Some(stack_id.clone()),
            Some(service_id.clone()),
            &now,
        );
        let conflict = state
            .db
            .insert_service_operation_job_if_unblocked(job.to_db(), vec![target.clone()], None)
            .await
            .unwrap()
            .expect("stack lifecycle must reserve every service target");
        assert_eq!(conflict.id, stack_job_id);
    }

    let read_conflict = crate::api::find_pending_service_operation_conflict(
        &state,
        &stack_id,
        &service_id,
    )
    .await
    .unwrap()
    .expect("service status must expose the stack lifecycle lock");
    assert_eq!(read_conflict.job.id, stack_job_id);
    assert_eq!(read_conflict.reason, "stack_lifecycle_in_progress");
}

#[tokio::test]
async fn service_lifecycle_claim_blocks_stack_lifecycle() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let service_job_id = ids::new_job_id();
    let service_job = crate::api::types::JobRecord::new_running(
        service_job_id.clone(),
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::Service,
        Some(stack_id.clone()),
        Some(service_id.clone()),
        &now,
    );
    let target = crate::db::ServiceOperationTarget {
        service_id,
        stack_id: stack_id.clone(),
    };
    assert!(state
        .db
        .insert_service_operation_job_if_unblocked(
            service_job.to_db(),
            vec![target.clone()],
            None,
        )
        .await
        .unwrap()
        .is_none());

    let stack_job = crate::api::types::JobRecord::new_running(
        ids::new_job_id(),
        crate::api::types::JobType::StackLifecycle,
        crate::api::types::JobScope::Stack,
        Some(stack_id),
        None,
        &now,
    );
    let conflict = state
        .db
        .insert_service_operation_job_if_unblocked(stack_job.to_db(), vec![target], None)
        .await
        .unwrap()
        .expect("service lifecycle must block the containing stack lifecycle");
    assert_eq!(conflict.id, service_job_id);
}

#[tokio::test]
async fn service_lifecycle_rejects_archived_services_and_stacks() {
    for archive_stack in [false, true] {
        let state = test_state(":memory:").await;
        let app = api::router(state.clone());
        let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
        let now = test_now_rfc3339();
        if archive_stack {
            state
                .db
                .set_stack_archived(&stack_id, true, Some("test"), &now)
                .await
                .unwrap();
        } else {
            state
                .db
                .set_service_archived(&service_id, true, Some("test"), &now)
                .await
                .unwrap();
        }

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/services/{service_id}/lifecycle"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"action":"start"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 409);
        assert_eq!(
            response_json(response).await["error"]["details"]["reason"].as_str(),
            Some(if archive_stack { "stack_archived" } else { "service_archived" }),
        );
    }
}

#[tokio::test]
async fn service_lifecycle_conflict_exposes_existing_service_operation_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let job_id = ids::new_job_id();
    let job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Rollback,
        crate::api::types::JobScope::Service,
        Some(stack_id),
        Some(service_id.clone()),
        &now,
    );
    state.db.insert_job(job.to_db()).await.unwrap();

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/lifecycle-status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let payload = response_json(status).await;
    assert_eq!(payload["activeJob"]["id"].as_str(), Some(job_id.as_str()));
    assert_eq!(payload["activeJob"]["type"].as_str(), Some("rollback"));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(response_json(response).await["error"]["details"]["existingJobId"].as_str(), Some(job_id.as_str()));
}

#[tokio::test]
async fn service_operation_claim_allows_only_one_concurrent_lifecycle_job() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let first_id = ids::new_job_id();
    let second_id = ids::new_job_id();
    let first = crate::api::types::JobRecord::new_running(
        first_id.clone(),
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::Service,
        Some(stack_id.clone()),
        Some(service_id.clone()),
        &now,
    );
    let second = crate::api::types::JobRecord::new_running(
        second_id.clone(),
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::Service,
        Some(stack_id.clone()),
        Some(service_id.clone()),
        &now,
    );
    let target = crate::db::ServiceOperationTarget {
        service_id,
        stack_id,
    };

    let (first_result, second_result) = tokio::join!(
        state.db.insert_service_operation_job_if_unblocked(
            first.to_db(),
            vec![target.clone()],
            None,
        ),
        state
            .db
            .insert_service_operation_job_if_unblocked(second.to_db(), vec![target], None),
    );
    let first_result = first_result.unwrap();
    let second_result = second_result.unwrap();
    assert_ne!(first_result.is_none(), second_result.is_none());

    let (accepted_id, conflict) = match (first_result, second_result) {
        (None, Some(conflict)) => (first_id, conflict),
        (Some(conflict), None) => (second_id, conflict),
        _ => unreachable!("exactly one lifecycle job must acquire the service claim"),
    };
    assert_eq!(conflict.id, accepted_id);
}

#[tokio::test]
async fn service_operation_claim_persists_explicit_update_targets() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let update_id = ids::new_job_id();
    let lifecycle_id = ids::new_job_id();
    let mut update = crate::api::types::JobRecord::new_running(
        update_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    );
    update.summary_json = serde_json::json!({ "mode": "apply", "targets": [] });
    let target = crate::db::ServiceOperationTarget {
        service_id: service_id.clone(),
        stack_id: stack_id.clone(),
    };
    assert!(state
        .db
        .insert_service_operation_job_if_unblocked(update.to_db(), vec![target.clone()], None)
        .await
        .unwrap()
        .is_none());

    let lifecycle = crate::api::types::JobRecord::new_running(
        lifecycle_id,
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::Service,
        Some(stack_id),
        Some(service_id),
        &now,
    );
    let conflict = state
        .db
        .insert_service_operation_job_if_unblocked(lifecycle.to_db(), vec![target], None)
        .await
        .unwrap()
        .expect("the update reservation should block the lifecycle operation");
    assert_eq!(conflict.id, update_id);
}

#[tokio::test]
async fn service_lifecycle_does_not_block_on_read_only_update_preview() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FakeRunner),
        "docker",
    )
    .await;
    let app = api::router(state.clone());
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let job_id = ids::new_job_id();
    let mut preview = crate::api::types::JobRecord::new_running(
        job_id,
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        Some(stack_id),
        Some(service_id.clone()),
        &now,
    );
    preview.summary_json = serde_json::json!({ "mode": "dry-run" });
    state.db.insert_job(preview.to_db()).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn service_lifecycle_does_not_block_on_unselected_stack_update_target() {
    let state = test_state(":memory:").await;
    let compose_dir = format!("/tmp/dockrev-targeted-update-{}", ulid::Ulid::new());
    std::fs::create_dir_all(&compose_dir).unwrap();
    let compose_path = format!("{compose_dir}/compose.yml");
    std::fs::write(
        &compose_path,
        "services:\n  api:\n    image: ghcr.io/acme/api:1.0\n  web:\n    image: ghcr.io/acme/web:1.0\n",
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "targeted-update", &compose_path).await;
    let api_id = service_id_by_name(&state, &stack_id, "api").await;
    let web_id = service_id_by_name(&state, &stack_id, "web").await;
    let now = test_now_rfc3339();
    let mut update = crate::api::types::JobRecord::new_running(
        ids::new_job_id(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &now,
    );
    update.summary_json = serde_json::json!({
        "mode": "apply",
        "targets": [{ "serviceId": web_id }],
    });
    state.db.insert_job(update.to_db()).await.unwrap();

    let target = crate::db::ServiceOperationTarget {
        service_id: api_id,
        stack_id,
    };
    let lifecycle = crate::api::types::JobRecord::new_running(
        ids::new_job_id(),
        crate::api::types::JobType::ServiceLifecycle,
        crate::api::types::JobScope::Service,
        Some(target.stack_id.clone()),
        Some(target.service_id.clone()),
        &now,
    );

    let conflict = state
        .db
        .insert_service_operation_job_if_unblocked(lifecycle.to_db(), vec![target], None)
        .await
        .unwrap();
    assert!(conflict.is_none());
}

#[tokio::test]
async fn service_lifecycle_does_not_block_on_empty_target_update() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let mut update = crate::api::types::JobRecord::new_running(
        ids::new_job_id(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    );
    update.summary_json = serde_json::json!({ "mode": "apply", "targets": [] });
    state.db.insert_job(update.to_db()).await.unwrap();

    let conflict = state
        .db
        .find_latest_pending_update_blocking_service(&stack_id, &service_id)
        .await
        .unwrap();
    assert!(conflict.is_none());
}

#[tokio::test]
async fn lifecycle_conflict_lookup_is_not_limited_by_unrelated_update_queue_depth() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let blocking_id = ids::new_job_id();
    let blocking = crate::api::types::JobRecord::new_running(
        blocking_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::All,
        None,
        None,
        "2026-01-01T00:00:00Z",
    );
    state.db.insert_job(blocking.to_db()).await.unwrap();

    for index in 0..201 {
        let unrelated = crate::api::types::JobRecord::new_running(
            ids::new_job_id(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            Some("stack-unrelated".to_string()),
            Some(format!("service-unrelated-{index}")),
            &format!("2026-02-01T00:{:02}:{:02}Z", index / 60, index % 60),
        );
        state.db.insert_job(unrelated.to_db()).await.unwrap();
    }

    let conflict = crate::api::find_pending_service_operation_conflict(
        &state,
        &stack_id,
        &service_id,
    )
    .await
    .unwrap()
    .expect("global update must still block the service");
    assert_eq!(conflict.job.id, blocking_id);
}
