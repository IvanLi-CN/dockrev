#[tokio::test]
async fn service_new_version_discovery_timeline_keeps_pinned_suffix_history_distinct_when_snapshot_has_plain_semver()
 {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:3.2.10-r0-ls66
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current".to_string()),
            Some("3.2.10-r0-ls66".to_string()),
            Some("[\"3.2.10-r0-ls66\"]".to_string()),
            Some("latest".to_string()),
            None,
            Some("sha256:live-candidate".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let ready_scan = crate::api::types::ServiceDigestTagsScanSummary {
        repo_tags_total: 2,
        repo_tags_considered: 2,
        manifests_ok: 2,
        manifests_timeout: 0,
        manifests_error: 0,
    };
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:history-candidate",
        "linux/amd64",
        &test_offset_rfc3339(&now, -time::Duration::minutes(20)),
        vec!["3.2.14-r0-ls73".to_string(), "3.2.14".to_string()],
        ready_scan.clone(),
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:live-candidate",
        "linux/amd64",
        &test_offset_rfc3339(&now, -time::Duration::minutes(10)),
        vec!["latest".to_string(), "3.2.14".to_string()],
        ready_scan,
    )
    .await;

    let history_job_id = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, -time::Duration::minutes(30)),
    )
    .await;
    state
        .db
        .finish_job(
            &history_job_id,
            "success",
            &test_offset_rfc3339(&now, -time::Duration::minutes(30)),
            &make_new_version_summary_for_test(
                &service_id,
                "3.2.10-r0-ls66",
                "3.2.10-r0-ls66",
                "sha256:current",
                "3.2.14-r0-ls73",
                "",
                "sha256:history-candidate",
            ),
        )
        .await
        .unwrap();
    let live_job_id = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, -time::Duration::minutes(5)),
    )
    .await;
    state
        .db
        .finish_job(
            &live_job_id,
            "success",
            &test_offset_rfc3339(&now, -time::Duration::minutes(5)),
            &make_new_version_summary_for_test(
                &service_id,
                "3.2.10-r0-ls66",
                "3.2.10-r0-ls66",
                "sha256:current",
                "latest",
                "latest",
                "sha256:live-candidate",
            ),
        )
        .await
        .unwrap();

    let stack_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stack_resp.status(), 200);
    let stack_body = response_json(stack_resp).await;
    assert_eq!(
        stack_body["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(2)
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/new-version-discovery-timeline"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let items = body["items"].as_array().expect("timeline items");
    assert_eq!(items.len(), 3, "timeline body: {}", body);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("3.2.14"));
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("3.2.14-r0-ls73"));
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("3.2.10-r0-ls66"));
}

#[tokio::test]
async fn get_stack_keeps_pinned_suffix_candidates_distinct() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:3.2.10-r0-ls66
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current"),
        Some("3.2.14"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current".to_string()),
            Some("3.2.10-r0-ls66".to_string()),
            Some("[\"3.2.10-r0-ls66\"]".to_string()),
            Some("3.2.14".to_string()),
            Some("3.2.14".to_string()),
            Some("sha256:live-candidate".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    for (discovered_at, candidate_tag, candidate_display_tag, candidate_digest) in [
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(30)),
            "3.2.14-r0-ls73",
            "3.2.14-r0-ls73",
            "sha256:history-candidate",
        ),
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(5)),
            "3.2.14",
            "3.2.14",
            "sha256:live-candidate",
        ),
    ] {
        let job_id = insert_check_job(&state, "schedule", &discovered_at).await;
        state
            .db
            .finish_job(
                &job_id,
                "success",
                &discovered_at,
                &make_new_version_summary_for_test(
                    &service_id,
                    "3.2.10-r0-ls66",
                    "3.2.10-r0-ls66",
                    "sha256:current",
                    candidate_tag,
                    candidate_display_tag,
                    candidate_digest,
                ),
            )
            .await
            .unwrap();
    }

    let stack_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stack_resp.status(), 200);
    let stack_body = response_json(stack_resp).await;
    assert_eq!(
        stack_body["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(2)
    );
}

#[tokio::test]
async fn service_new_version_discovery_timeline_falls_back_to_current_tag_without_snapshot() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;
    let now = test_now_rfc3339();
    let running_started_at = test_offset_rfc3339(&now, -time::Duration::minutes(20));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:current".to_string()),
            Some(running_started_at.clone()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/new-version-discovery-timeline"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let items = body["items"].as_array().expect("timeline items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[0]["version"].as_str(), Some("latest"));
    assert_eq!(
        items[0]["occurredAt"].as_str(),
        Some(running_started_at.as_str())
    );
}

#[tokio::test]
async fn runtime_scan_updates_runtime_started_at_after_same_digest_restart() {
    let now = test_now_rfc3339();
    let restarted_at = test_offset_rfc3339(&now, time::Duration::minutes(5));
    let runner: Arc<CheckAndRuntimeScanRunner> = Arc::new(
        CheckAndRuntimeScanRunner::new_with_started_at("sha256:match", Some(restarted_at.clone())),
    );
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    let old_started_at = test_offset_rfc3339(&now, -time::Duration::hours(1));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:match".to_string()),
            Some(old_started_at),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let context = state
        .db
        .get_service_new_version_timeline_context(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        context.current_runtime_started_at.as_deref(),
        Some(restarted_at.as_str())
    );
}

#[tokio::test]
async fn runtime_scan_clears_runtime_started_at_when_scaled_replicas_disagree() {
    let now = test_now_rfc3339();
    let replica_started_ats = vec![
        test_offset_rfc3339(&now, -time::Duration::minutes(10)),
        test_offset_rfc3339(&now, time::Duration::minutes(5)),
    ];
    let runner: Arc<CheckAndRuntimeScanRunner> = Arc::new(
        CheckAndRuntimeScanRunner::new_with_started_ats("sha256:match", replica_started_ats),
    );
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    let prior_started_at = test_offset_rfc3339(&now, -time::Duration::hours(1));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:match".to_string()),
            Some(prior_started_at),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let context = state
        .db
        .get_service_new_version_timeline_context(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(context.current_runtime_started_at, None);
}

#[tokio::test]
async fn sync_stack_compose_reset_clears_runtime_started_at_from_timeline_context() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current-v1"), None, None)
            .await;
    let now = test_now_rfc3339();
    let runtime_started_at = test_offset_rfc3339(&now, -time::Duration::hours(2));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:current-v1".to_string()),
            Some(runtime_started_at),
            Some("1.16.0".to_string()),
            Some("[\"1.16.0\"]".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    let before = state
        .db
        .get_service_new_version_timeline_context(&service_id)
        .await
        .unwrap()
        .expect("timeline context before sync");
    assert!(before.current_runtime_started_at.is_some());

    let synced_at = test_offset_rfc3339(&now, time::Duration::minutes(5));
    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web:5.3".to_string(),
                image_tag: "5.3".to_string(),
            }],
            &synced_at,
        )
        .await
        .unwrap();

    let after = state
        .db
        .get_service_new_version_timeline_context(&service_id)
        .await
        .unwrap()
        .expect("timeline context after sync");
    assert_eq!(after.current_runtime_started_at, None);
}

#[tokio::test]
async fn get_stack_normalizes_unsettled_discovery_history_from_notifications() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current-v1"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v1".to_string()),
            Some("1.16.0".to_string()),
            Some("[\"1.16.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.17.0".to_string()),
            Some("sha256:live-candidate".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let job_1 = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(
            &job_1,
            "success",
            &now,
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "latest",
                "sha256:candidate-a",
            ),
        )
        .await
        .unwrap();
    let job_2 = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, time::Duration::minutes(1)),
    )
    .await;
    state
        .db
        .finish_job(
            &job_2,
            "success",
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "latest",
                "sha256:candidate-b",
            ),
        )
        .await
        .unwrap();
    let job_3 = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, time::Duration::minutes(2)),
    )
    .await;
    state
        .db
        .finish_job(
            &job_3,
            "success",
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "latest",
                "sha256:candidate-c",
            ),
        )
        .await
        .unwrap();

    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        &job_1,
        "ghcr.io/acme/web",
        "latest",
        "1.16.0",
        "latest",
        "1.16.2",
        "sha256:candidate-a",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
    )
    .await;
    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        &job_2,
        "ghcr.io/acme/web",
        "latest",
        "1.16.0",
        "latest",
        "1.16.2",
        "sha256:candidate-b",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
    )
    .await;
    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        &job_3,
        "ghcr.io/acme/web",
        "latest",
        "1.16.0",
        "latest",
        "1.17.0",
        "sha256:candidate-c",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let detail = response_json(resp).await;
    assert_eq!(
        detail["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(2)
    );
}

#[tokio::test]
async fn get_stack_does_not_use_current_repo_snapshot_to_inflate_old_unsettled_alias_history() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        None,
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();

    let job_1 = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(
            &job_1,
            "success",
            &now,
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "latest",
                "",
                "latest",
                "latest",
                "sha256:candidate-a",
            ),
        )
        .await
        .unwrap();
    let job_2 = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, time::Duration::minutes(1)),
    )
    .await;
    state
        .db
        .finish_job(
            &job_2,
            "success",
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "latest",
                "",
                "latest",
                "latest",
                "sha256:candidate-b",
            ),
        )
        .await
        .unwrap();

    let replacement_compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &replacement_compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/worker:latest
"#,
    )
    .unwrap();
    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&replacement_compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/worker".to_string(),
                image_tag: "latest".to_string(),
            }],
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
        )
        .await
        .unwrap();
    state
        .db
        .update_service_check_result(
            &service_id,
            None,
            None,
            None,
            Some("latest".to_string()),
            None,
            Some("sha256:live-candidate".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
        )
        .await
        .unwrap();

    let ready_scan = crate::api::types::ServiceDigestTagsScanSummary {
        repo_tags_total: 2,
        repo_tags_considered: 2,
        manifests_ok: 2,
        manifests_timeout: 0,
        manifests_error: 0,
    };
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/worker",
        "sha256:candidate-a",
        "linux/amd64",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
        vec!["latest".to_string(), "v2.0.0".to_string()],
        ready_scan.clone(),
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/worker",
        "sha256:candidate-b",
        "linux/amd64",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
        vec!["latest".to_string(), "v2.0.0".to_string()],
        ready_scan,
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let detail = response_json(resp).await;
    assert_eq!(
        detail["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(1)
    );
}

#[tokio::test]
async fn get_stack_does_not_use_new_repo_notifications_to_inflate_old_unsettled_alias_history() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        None,
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();

    let job_1 = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(
            &job_1,
            "success",
            &now,
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/web",
                "latest",
                "latest",
                "",
                "latest",
                "latest",
                "sha256:candidate-a",
            ),
        )
        .await
        .unwrap();
    let job_2 = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, time::Duration::minutes(1)),
    )
    .await;
    state
        .db
        .finish_job(
            &job_2,
            "success",
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/web",
                "latest",
                "latest",
                "",
                "latest",
                "latest",
                "sha256:candidate-b",
            ),
        )
        .await
        .unwrap();

    let replacement_compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &replacement_compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/worker:latest
"#,
    )
    .unwrap();
    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&replacement_compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/worker".to_string(),
                image_tag: "latest".to_string(),
            }],
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
        )
        .await
        .unwrap();
    state
        .db
        .update_service_check_result(
            &service_id,
            None,
            None,
            None,
            Some("latest".to_string()),
            None,
            Some("sha256:live-candidate".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
        )
        .await
        .unwrap();

    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        "job_worker_1",
        "ghcr.io/acme/worker",
        "latest",
        "2.0.0",
        "latest",
        "2.0.0",
        "sha256:candidate-a",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
    )
    .await;
    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        "job_worker_2",
        "ghcr.io/acme/worker",
        "latest",
        "2.0.0",
        "latest",
        "2.0.0",
        "sha256:candidate-b",
        &test_offset_rfc3339(&now, time::Duration::minutes(3)),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let detail = response_json(resp).await;
    assert_eq!(
        detail["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(1)
    );
}

