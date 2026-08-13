#[tokio::test]
async fn deploy_check_report_keeps_visible_invalid_discovery_from_blocking_compose_access() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let compose_file = format!("/tmp/dockrev-preflight-invalid-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_file, "services: [invalid\n").unwrap();

    let stack_id = ids::new_stack_id();
    let now = test_now_rfc3339();
    state
        .db
        .insert_stack(
            &crate::api::types::StackRecord {
                id: stack_id.clone(),
                name: "invalid-discovery".to_string(),
                archived: false,
                compose: crate::api::types::ComposeConfig {
                    kind: "path".to_string(),
                    compose_files: vec![compose_file.clone()],
                    env_file: None,
                },
                backup: crate::api::types::StackBackupConfig::default(),
                services: Vec::new(),
            },
            &[],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "invalid-discovery".to_string(),
            stack_id: Some(stack_id),
            status: "invalid".to_string(),
            last_seen_at: None,
            last_scan_at: now,
            last_error: Some("compose_file_invalid".to_string()),
            last_config_files: Some(vec![compose_file.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let app = api::router(state.clone());
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    let compose_access = body["report"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "core.compose_access")
        .unwrap();
    assert_eq!(compose_access["status"], "pass");

    let projects = state
        .db
        .list_discovered_compose_projects(crate::db::ArchivedFilter::Exclude)
        .await
        .unwrap();
    assert!(projects.iter().any(|project| {
        project.project == "invalid-discovery"
            && matches!(project.status, crate::api::types::DiscoveredProjectStatus::Invalid)
    }));

    std::fs::remove_file(compose_file).unwrap();
}

#[tokio::test]
async fn management_events_requires_authorized_request() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    assert_eq!(response_json(response).await["error"]["code"], "auth_required");
}

#[tokio::test]
async fn management_events_replay_memory_buffer_and_expose_metrics_without_sql_event_writes() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let hub = state.management_events.clone();
    let generation = hub.generation().to_string();

    hub.publish_immediate(
        "stacks",
        vec![crate::management_events::ManagementEventEntity {
            entity_type: "stack".to_string(),
            id: "stack-1".to_string(),
        }],
        serde_json::json!({ "operation": "updated" }),
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .header("last-event-id", format!("{generation}:0"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
    assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    let mut body = response.into_body();
    let event = wait_for_sse_event(&mut body, "management", Duration::from_secs(3)).await;
    assert_eq!(event.id.as_deref(), Some(format!("{generation}:1").as_str()));
    let payload: serde_json::Value = serde_json::from_str(&event.data).unwrap();
    assert_eq!(payload["domain"], "stacks");
    assert_eq!(payload["entities"][0]["id"], "stack-1");

    let status = app
        .oneshot(
            Request::builder()
                .uri("/api/events/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let metrics = response_json(status).await;
    assert_eq!(metrics["generation"], generation);
    assert_eq!(metrics["publishedEvents"], 1);
    assert_eq!(metrics["bufferedEvents"], 1);
    assert_eq!(metrics["publishFailures"], 0);
}

#[tokio::test]
async fn management_events_generation_mismatch_requires_resync() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/events?afterId=previous-generation:7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let mut body = response.into_body();
    let event = wait_for_sse_event(&mut body, "resync_required", Duration::from_secs(3)).await;
    let payload: serde_json::Value = serde_json::from_str(&event.data).unwrap();
    assert_eq!(payload["reason"], "generation_changed");
    assert_eq!(payload["generation"], state.management_events.generation());
}

async fn wait_for_management_event(
    state: &Arc<AppState>,
    cursor: &str,
    matches: impl Fn(&crate::management_events::ManagementEventRecord) -> bool,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        if let crate::management_events::ManagementEventReplay::Events { events, .. } = state
            .management_events
            .replay_after(Some(cursor))
            .await
            && events.iter().any(&matches)
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for management event"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn job_progress_publishes_management_event() {
    let state = test_state(":memory:").await;
    let job_id = ids::new_check_id();
    let now = test_now_rfc3339();
    state
        .db
        .insert_job(
            crate::api::types::JobRecord::new_running(
                job_id.clone(),
                crate::api::types::JobType::Check,
                crate::api::types::JobScope::All,
                None,
                None,
                &now,
            )
            .to_db(),
        )
        .await
        .unwrap();
    let cursor = format!("{}:0", state.management_events.generation());

    state
        .db
        .set_job_progress(&job_id, &json!({ "phase": "scan", "percent": 50 }))
        .await
        .unwrap();

    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "jobs"
            && event.event.summary["jobId"] == job_id
            && event.event.summary["operation"] == "progress_updated"
    })
    .await;
}

#[tokio::test]
async fn discovery_archive_and_restore_publish_management_events() {
    let state = test_state(":memory:").await;
    let project = "management-events-project";
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: project.to_string(),
            stack_id: None,
            status: "active".to_string(),
            last_seen_at: None,
            last_scan_at: test_now_rfc3339(),
            last_error: None,
            last_config_files: None,
            unarchive_if_active: true,
        })
        .await
        .unwrap();
    let app = api::router(state.clone());
    let cursor = format!("{}:0", state.management_events.generation());

    let archive = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/discovery/projects/{project}/archive"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(archive.status(), 204);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "discovery"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "project" && entity.id == project
            })
            && event.event.summary["operation"] == "archived"
    })
    .await;

    let restore = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/discovery/projects/{project}/restore"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore.status(), 204);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "discovery"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "project" && entity.id == project
            })
            && event.event.summary["operation"] == "restored"
    })
    .await;
}

#[tokio::test]
async fn github_packages_writes_publish_management_events() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let cursor = format!("{}:0", state.management_events.generation());

    let settings = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/github-packages/settings")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "enabled": false,
                        "callbackUrl": "https://dockrev.example.com/api/webhooks/github-packages",
                        "targets": [],
                        "repos": []
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings.status(), 200);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "github_packages"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "settings" && entity.id == "default"
            })
            && event.event.summary["operation"] == "settings_updated"
    })
    .await;

    let selection = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/selected")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "fullName": "acme/widgets", "selected": false }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(selection.status(), 200);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "github_packages"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "repo" && entity.id == "acme/widgets"
            })
            && event.event.summary["operation"] == "repo_selection_updated"
    })
    .await;
}

#[tokio::test]
async fn settings_writes_publish_management_events() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let cursor = format!("{}:0", state.management_events.generation());

    let notifications = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": { "enabled": false },
                        "webhook": { "enabled": false },
                        "telegram": { "enabled": false },
                        "webPush": { "enabled": false }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(notifications.status(), 200);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "settings"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "notifications" && entity.id == "default"
            })
            && event.event.summary["operation"] == "notifications_updated"
    })
    .await;

    let deploy_welcome = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/deploy-welcome")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "neverAutoOpen": true }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deploy_welcome.status(), 200);
    wait_for_management_event(&state, &cursor, |event| {
        event.event.domain == "settings"
            && event.event.entities.iter().any(|entity| {
                entity.entity_type == "deploy_welcome" && entity.id == "default"
            })
            && event.event.summary["operation"] == "deploy_welcome_updated"
    })
    .await;
}

#[tokio::test]
async fn stack_settings_publish_management_event() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_path = format!("/tmp/dockrev-stack-settings-events-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "stack-events", &compose_path).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/stacks/{stack_id}/settings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "autoUpdatePolicy": {
                            "mode": "override",
                            "enabled": false,
                            "rules": []
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let cursor = format!("{}:0", state.management_events.generation());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        if let crate::management_events::ManagementEventReplay::Events { events, .. } = state
            .management_events
            .replay_after(Some(&cursor))
            .await
            && events.iter().any(|event| {
                event.event.domain == "stacks"
                    && event.event.entities.iter().any(|entity| {
                        entity.entity_type == "stack" && entity.id == stack_id
                    })
                    && event.event.summary["operation"] == "settings_updated"
            })
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for stack settings management event"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    std::fs::remove_file(compose_path).unwrap();
}

#[tokio::test]
async fn stale_webhook_job_replacement_publishes_terminal_management_event() {
    let state = test_state(":memory:").await;
    let compose_path = format!("/tmp/dockrev-stale-event-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "stale-event", &compose_path).await;
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    let stale_at = (time::OffsetDateTime::now_utc() - time::Duration::hours(3))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let now = test_now_rfc3339();
    let stale_job_id = ids::new_check_id();
    state
        .db
        .insert_job(
            crate::api::types::JobRecord::new_running(
                stale_job_id.clone(),
                crate::api::types::JobType::Check,
                crate::api::types::JobScope::Service,
                Some(stack_id.clone()),
                Some(service_id.clone()),
                &stale_at,
            )
            .to_db(),
        )
        .await
        .unwrap();
    let replacement = crate::api::types::JobRecord::new_running(
        ids::new_check_id(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Service,
        Some(stack_id),
        Some(service_id),
        &now,
    )
    .to_db();
    let result = state
        .db
        .insert_or_reuse_webhook_check_job_for_service(
            replacement,
            &now,
            time::Duration::minutes(30),
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::db::PendingJobUpsert::Inserted));

    let cursor = format!("{}:0", state.management_events.generation());
    let crate::management_events::ManagementEventReplay::Events { events, .. } = state
        .management_events
        .replay_after(Some(&cursor))
        .await
    else {
        panic!("expected management event replay");
    };
    assert!(events.iter().any(|event| {
        event.event.domain == "jobs"
            && event.event.summary["jobId"] == stale_job_id
            && event.event.summary["status"] == "failed"
            && event.event.summary["reason"] == "stale_check"
            && event.event.summary["terminal"] == true
    }));

    std::fs::remove_file(compose_path).unwrap();
}

#[tokio::test]
async fn stale_webhook_discovery_replacement_publishes_terminal_management_event() {
    let state = test_state(":memory:").await;
    let stale_at = (time::OffsetDateTime::now_utc() - time::Duration::hours(3))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let now = test_now_rfc3339();
    let stale_job_id = ids::new_discovery_id();
    state
        .db
        .insert_job(
            crate::api::types::JobRecord::new_running(
                stale_job_id.clone(),
                crate::api::types::JobType::Discovery,
                crate::api::types::JobScope::All,
                None,
                None,
                &stale_at,
            )
            .to_db(),
        )
        .await
        .unwrap();
    let replacement = crate::api::types::JobRecord::new_running(
        ids::new_discovery_id(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    let result = state
        .db
        .insert_or_reuse_webhook_discovery_job(
            replacement,
            &now,
            time::Duration::minutes(30),
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::db::PendingJobUpsert::Inserted));

    let cursor = format!("{}:0", state.management_events.generation());
    let crate::management_events::ManagementEventReplay::Events { events, .. } = state
        .management_events
        .replay_after(Some(&cursor))
        .await
    else {
        panic!("expected management event replay");
    };
    assert!(events.iter().any(|event| {
        event.event.domain == "jobs"
            && event.event.summary["jobId"] == stale_job_id
            && event.event.summary["status"] == "failed"
            && event.event.summary["reason"] == "stale_check"
            && event.event.summary["terminal"] == true
    }));
}
