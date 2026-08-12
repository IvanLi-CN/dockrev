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
    let (stack_id, service_id, compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "lifecycle-stack".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let scan = crate::discovery::run_scan(state.as_ref()).await.unwrap();
    assert_eq!(scan.summary.stacks_stopped, 1);
    assert!(scan.actions.iter().any(|action| {
        action.project == "lifecycle-stack"
            && matches!(action.action, crate::api::types::DiscoveryActionKind::MarkedStopped)
    }));

    let discovery = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/discovery/projects?archived=exclude")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), 200);
    let projects = response_json(discovery).await;
    assert_eq!(projects["projects"][0]["status"], "stopped");

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
async fn discovery_docker_enumeration_failure_preserves_existing_state() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FailAllRunner),
    )
    .await;
    let (stack_id, _service_id, compose_path) = seed_manual_rollback_service(&state).await;
    let last_scan_at = "2026-08-12T00:00:00Z".to_string();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "docker-unavailable".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(last_scan_at.clone()),
            last_scan_at: last_scan_at.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path]),
            unarchive_if_active: false,
        })
        .await
        .unwrap();

    assert!(crate::discovery::run_scan(state.as_ref()).await.is_err());

    let project = state
        .db
        .list_discovered_compose_projects(crate::db::ArchivedFilter::Exclude)
        .await
        .unwrap()
        .into_iter()
        .find(|project| project.project == "docker-unavailable")
        .unwrap();
    assert!(matches!(
        project.status,
        crate::api::types::DiscoveredProjectStatus::Active
    ));
    assert_eq!(project.last_scan_at.as_deref(), Some(last_scan_at.as_str()));
    assert!(!state.db.get_stack(&stack_id).await.unwrap().unwrap().archived);
}

#[tokio::test]
async fn stack_lifecycle_claim_blocks_service_lifecycle_and_update() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FakeRunner),
        "docker",
    )
    .await;
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
async fn stack_lifecycle_rejects_archived_and_dockrev_stacks() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(FakeRunner),
        "docker",
    )
    .await;
    let app = api::router(state.clone());

    let mixed_compose_path = format!("/tmp/dockrev-stack-lifecycle-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &mixed_compose_path,
        "services:\n  active:\n    image: ghcr.io/acme/active:latest\n  archived:\n    image: ghcr.io/acme/archived:latest\n",
    )
    .unwrap();
    let mixed_stack_id = seed_stack_from_compose(&state, "mixed", &mixed_compose_path).await;
    std::fs::remove_file(&mixed_compose_path).unwrap();
    let mixed_services = state.db.list_services_for_check(&mixed_stack_id).await.unwrap();
    let archived_service = mixed_services
        .iter()
        .find(|service| service.name == "archived")
        .unwrap();
    state
        .db
        .set_service_archived(
            &archived_service.id,
            true,
            Some("test"),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    let mixed_status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{mixed_stack_id}/lifecycle-status"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mixed_status.status(), 200);
    let mixed_status_json = response_json(mixed_status).await;
    assert_eq!(mixed_status_json["state"].as_str(), Some("stopped"));
    assert_eq!(
        mixed_status_json["unavailableReason"].as_str(),
        Some("stack_contains_archived_service")
    );

    let mixed_trigger = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/stacks/{mixed_stack_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mixed_trigger.status(), 409);
    assert_eq!(
        response_json(mixed_trigger).await["error"]["details"]["reason"].as_str(),
        Some("stack_contains_archived_service")
    );

    let (stack_id, _service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    state
        .db
        .set_stack_archived(&stack_id, true, Some("test"), &now)
        .await
        .unwrap();

    let archived = app
        .clone()
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
    assert_eq!(archived.status(), 409);
    assert_eq!(
        response_json(archived).await["error"]["details"]["reason"].as_str(),
        Some("stack_archived"),
    );

    let compose_path = format!("/tmp/dockrev-stack-lifecycle-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
    )
    .unwrap();
    let dockrev_stack_id = seed_stack_from_compose(&state, "dockrev", &compose_path).await;
    std::fs::remove_file(&compose_path).unwrap();
    let dockrev = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/stacks/{dockrev_stack_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dockrev.status(), 409);
    assert_eq!(
        response_json(dockrev).await["error"]["details"]["reason"].as_str(),
        Some("dockrev_stack_managed_via_supervisor"),
    );

    let dockrev_service = state.db.list_services_for_check(&dockrev_stack_id).await.unwrap()[0].clone();
    state
        .db
        .set_service_archived(
            &dockrev_service.id,
            true,
            Some("test"),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    let archived_dockrev = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/stacks/{dockrev_stack_id}/lifecycle"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"start"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(archived_dockrev.status(), 409);
    assert_eq!(
        response_json(archived_dockrev).await["error"]["details"]["reason"].as_str(),
        Some("stack_contains_archived_service"),
    );
}

#[tokio::test]
async fn service_lifecycle_write_rejects_compose_v1_before_creating_job() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(ComposeV1Runner),
        "docker-compose",
    )
    .await;
    let app = api::router(state.clone());
    let (_stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;

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
    assert_eq!(response.status(), 503);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"].as_str(), Some("compose_v2_required"));
    assert_eq!(body["error"]["details"]["reason"].as_str(), Some("compose_v2_required"));
    assert!(state.db.list_jobs().await.unwrap().is_empty());
}

#[tokio::test]
async fn webhook_update_rejects_compose_v1_before_enqueueing_job() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(ComposeV1Runner),
        "docker-compose",
    )
    .await;
    let app = api::router(state.clone());
    let (stack_id, _service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(
                    serde_json::json!({
                        "action": "update",
                        "scope": "stack",
                        "stackId": stack_id,
                        "targets": [],
                        "allowArchMismatch": false,
                        "backupMode": "inherit",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"].as_str(), Some("compose_v2_required"));
    assert_eq!(
        body["error"]["details"]["reason"].as_str(),
        Some("compose_v2_required")
    );
    assert!(state.db.list_jobs().await.unwrap().is_empty());
}

#[tokio::test]
async fn dry_run_update_remains_available_with_compose_v1() {
    let state = test_state_with_compose_bin(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(ComposeV1Runner),
        "docker-compose",
    )
    .await;
    let app = api::router(state.clone());
    let (stack_id, _service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "stack",
                        "stackId": stack_id,
                        "targets": [],
                        "mode": "dry-run",
                        "allowArchMismatch": false,
                        "backupMode": "inherit",
                        "reason": "ui",
                    })
                    .to_string(),
                ))
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
    assert_eq!(job.status, "success");
    assert_eq!(job.summary_json["mode"].as_str(), Some("dry-run"));
}
