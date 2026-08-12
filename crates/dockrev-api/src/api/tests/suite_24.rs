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
