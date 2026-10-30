async fn seed_preflight_project(
    state: &Arc<AppState>,
    project: &str,
    stack_id: Option<&str>,
    status: &str,
    files: Vec<String>,
) {
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: project.to_string(),
            stack_id: stack_id.map(str::to_string),
            status: status.to_string(),
            last_seen_at: None,
            last_scan_at: test_now_rfc3339(),
            last_error: None,
            last_config_files: Some(files),
            unarchive_if_active: false,
        })
        .await
        .unwrap();
}

async fn seed_preflight_stack(state: &Arc<AppState>, name: &str, files: Vec<String>) -> String {
    let id = ids::new_stack_id();
    state
        .db
        .insert_stack(
            &crate::api::types::StackRecord {
                id: id.clone(),
                name: name.to_string(),
                archived: false,
                compose: crate::api::types::ComposeConfig {
                    kind: "path".to_string(),
                    compose_files: files,
                    env_file: None,
                },
                backup: crate::api::types::StackBackupConfig::default(),
                services: Vec::new(),
            },
            &[],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    id
}

fn preflight_compose_check(body: &serde_json::Value) -> &serde_json::Value {
    body["report"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "core.compose_access")
        .unwrap()
}

#[tokio::test]
async fn deploy_check_missing_projects_do_not_fail_or_remove_records() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let path = format!("/tmp/dockrev-missing-preflight-{}.yml", ulid::Ulid::new());
    let archived_path = format!("{path}-archived");
    let orphan_path = format!("{path}-orphan");
    let id = seed_preflight_stack(&state, "missing", vec![path.clone()]).await;
    let archived_id =
        seed_preflight_stack(&state, "missing-archived", vec![archived_path.clone()]).await;
    state
        .db
        .set_stack_archived(
            &archived_id,
            true,
            Some("user_archive"),
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    seed_preflight_project(&state, "missing", Some(&id), "missing", vec![path.clone()]).await;
    seed_preflight_project(
        &state,
        "missing-archived",
        Some(&archived_id),
        "missing",
        vec![archived_path],
    )
    .await;
    seed_preflight_project(&state, "missing-orphan", None, "missing", vec![orphan_path]).await;
    let app = api::router(state.clone());
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    assert_eq!(preflight_compose_check(&body)["status"], "pass", "{body}");
    assert!(state.db.is_stack_missing(&id).await.unwrap());
    assert!(
        state
            .db
            .get_stack(&archived_id)
            .await
            .unwrap()
            .unwrap()
            .archived
    );
    assert_eq!(
        state
            .db
            .list_discovered_compose_projects(crate::db::ArchivedFilter::Include)
            .await
            .unwrap()
            .len(),
        3
    );

    std::fs::write(&path, "services:\n  app:\n    image: alpine:3.20\n").unwrap();
    crate::discovery::run_scan(state.as_ref()).await.unwrap();
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    assert_eq!(preflight_compose_check(&body)["status"], "pass", "{body}");
    assert!(!state.db.is_stack_missing(&id).await.unwrap());
    std::fs::remove_file(&path).unwrap();
    let body = wait_for_deploy_check_report_ready(&app, None).await;
    assert_eq!(preflight_compose_check(&body)["status"], "fail", "{body}");
    assert!(
        preflight_compose_check(&body)["evidence"]
            .as_str()
            .unwrap()
            .contains(&path)
    );
}

#[tokio::test]
async fn deploy_check_reports_partial_and_unlinked_invalid_paths() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let path = format!("/tmp/dockrev-partial-preflight-{}.yml", ulid::Ulid::new());
    let missing_path = format!("{path}-missing");
    let orphan_path = format!("{path}-orphan");
    std::fs::write(&path, "services:\n  app:\n    image: alpine:3.20\n").unwrap();
    let id =
        seed_preflight_stack(&state, "partial", vec![path.clone(), missing_path.clone()]).await;
    seed_preflight_project(
        &state,
        "partial",
        Some(&id),
        "invalid",
        vec![path.clone(), missing_path.clone()],
    )
    .await;
    seed_preflight_project(
        &state,
        "orphan-invalid",
        None,
        "invalid",
        vec![orphan_path.clone()],
    )
    .await;
    let body = wait_for_deploy_check_report_ready(&api::router(state.clone()), None).await;
    let check = preflight_compose_check(&body);
    assert_eq!(check["status"], "fail", "{body}");
    let evidence = check["evidence"].as_str().unwrap();
    assert!(evidence.contains(&missing_path));
    assert!(evidence.contains(&orphan_path));
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn deploy_check_keeps_non_missing_and_mixed_project_failures_scoped() {
    for status in ["active", "stopped", "invalid"] {
        let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
        let path = format!("/tmp/dockrev-{status}-preflight-{}.yml", ulid::Ulid::new());
        let id = seed_preflight_stack(&state, status, vec![path.clone()]).await;
        seed_preflight_project(&state, status, Some(&id), status, vec![path.clone()]).await;
        seed_preflight_project(
            &state,
            "other-missing",
            Some(&id),
            "missing",
            vec![format!("{path}-missing")],
        )
        .await;
        let unrelated_path = format!("/tmp/dockrev-unrelated-missing-{}.yml", ulid::Ulid::new());
        let unrelated_id =
            seed_preflight_stack(&state, "unrelated", vec![unrelated_path.clone()]).await;
        seed_preflight_project(
            &state,
            "unrelated",
            Some(&unrelated_id),
            "missing",
            vec![unrelated_path.clone()],
        )
        .await;
        let body = wait_for_deploy_check_report_ready(&api::router(state.clone()), None).await;
        assert_eq!(preflight_compose_check(&body)["status"], "fail", "{body}");
        assert!(
            preflight_compose_check(&body)["evidence"]
                .as_str()
                .unwrap()
                .contains(&path)
        );
        let impacts = body["report"]["operationImpacts"].as_array().unwrap();
        let impact = impacts
            .iter()
            .find(|item| item["checkId"] == "core.compose_access")
            .unwrap();
        assert!(impact["objectIds"].as_array().unwrap().contains(&json!(id)));
        assert!(
            !impact["objectIds"]
                .as_array()
                .unwrap()
                .contains(&json!(unrelated_id))
        );
        assert!(
            !preflight_compose_check(&body)["evidence"]
                .as_str()
                .unwrap()
                .contains(&unrelated_path)
        );
        assert!(!state.db.is_stack_missing(&id).await.unwrap());
    }
}
