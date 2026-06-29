fn digest_map(entries: &[(&str, &str)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        map.insert((*key).to_string(), serde_json::Value::String((*value).to_string()));
    }
    serde_json::Value::Object(map)
}

#[tokio::test]
async fn get_service_backup_records_returns_related_service_scope_stack_scope_and_all_scope_rows()
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
                    "targets": [{
                        "target": { "kind": "bind-mount", "path": "/srv/data" },
                        "status": "included",
                        "sizeBytes": 1500,
                        "policy": "live_backup",
                        "relatedServices": ["api", "web"]
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
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["backupId"].as_str(), Some("bkp-api"));
    assert_eq!(records[0]["scope"].as_str(), Some("service"));
    assert_eq!(records[0]["sizeBytes"].as_u64(), Some(1500));
    assert_eq!(records[0]["cleanupAfter"].as_str(), Some(cleanup_after.as_str()));
    assert_eq!(records[0]["assets"][0]["policy"].as_str(), Some("live_backup"));
    assert_eq!(records[0]["assets"][0]["status"].as_str(), Some("included"));
    assert_eq!(records[0]["assets"][0]["sizeBytes"].as_u64(), Some(1500));
    assert_eq!(records[1]["backupId"].as_str(), Some("bkp-stack"));
    assert_eq!(records[1]["scope"].as_str(), Some("stack"));
    assert_eq!(records[1]["status"].as_str(), Some("skipped"));
    assert_eq!(records[1]["cleanupAfter"], serde_json::Value::Null);
    assert_eq!(records[1]["assets"][0]["reason"].as_str(), Some("skipped_by_size"));
    assert_eq!(records[2]["backupId"].as_str(), Some("bkp-all"));
    assert_eq!(records[2]["scope"].as_str(), Some("all"));
    assert_eq!(records[2]["status"].as_str(), Some("failed"));
    assert_eq!(records[2]["deletedAt"].as_str(), Some(now.as_str()));
    assert_eq!(records[2]["error"].as_str(), Some("archive failed"));
    assert_eq!(records[2]["assets"][0]["reason"].as_str(), Some("skipped_by_probe_error"));
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
