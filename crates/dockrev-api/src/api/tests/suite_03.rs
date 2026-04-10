#[tokio::test]
async fn service_new_version_discovery_timeline_excludes_older_unresolved_current_alias_when_live_current_digest_is_known()
 {
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
        Some("sha256:current-v3"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v3".to_string()),
            None,
            None,
            Some("latest".to_string()),
            Some("0.9.22".to_string()),
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

    for (discovered_at, current_digest, candidate_digest, candidate_display_tag) in [
        (
            test_offset_rfc3339(&now, time::Duration::days(-14)),
            "",
            "sha256:legacy-candidate",
            "latest",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-10)),
            "sha256:current-v3",
            "sha256:live-candidate",
            "0.9.22",
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
                    "latest",
                    "latest",
                    current_digest,
                    "latest",
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
        Some(1)
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
    assert_eq!(items.len(), 2, "timeline body: {}", body);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("0.9.22"));
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("latest"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_keeps_digest_pinned_history_when_live_current_digest_is_known()
 {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web@sha256:current-v3
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current-v3"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v3".to_string()),
            Some("sha256:current-v3".to_string()),
            None,
            Some("latest".to_string()),
            Some("0.9.22".to_string()),
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

    for (
        discovered_at,
        current_digest,
        current_display_tag,
        candidate_digest,
        candidate_display_tag,
    ) in [
        (
            test_offset_rfc3339(&now, time::Duration::days(-2)),
            "",
            "sha256:current-v3",
            "sha256:legacy-candidate",
            "0.9.21",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-10)),
            "sha256:current-v3",
            "sha256:current-v3",
            "sha256:live-candidate",
            "0.9.22",
        ),
    ] {
        let job_id = insert_check_job(&state, "schedule", &discovered_at).await;
        state
            .db
            .finish_job(
                &job_id,
                "success",
                &discovered_at,
                &make_new_version_summary_for_test_with_image_ref(
                    &service_id,
                    "ghcr.io/acme/web@sha256:current-v3",
                    "latest",
                    current_display_tag,
                    current_digest,
                    "latest",
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
        stack_body["stack"]["services"][0]["image"]["resolvedTag"].as_str(),
        Some("sha256:current-v3"),
        "stack body: {}",
        stack_body
    );
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
    assert_eq!(items[0]["version"].as_str(), Some("0.9.22"));
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("0.9.21"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_keeps_digest_pinned_history_when_live_current_digest_resolves_to_display_tag()
 {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web@sha256:current-v3
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current-v3"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v3".to_string()),
            Some("0.9.20".to_string()),
            None,
            Some("latest".to_string()),
            Some("0.9.22".to_string()),
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

    for (
        discovered_at,
        current_digest,
        current_display_tag,
        candidate_digest,
        candidate_display_tag,
    ) in [
        (
            test_offset_rfc3339(&now, time::Duration::days(-2)),
            "",
            "sha256:current-v3",
            "sha256:legacy-candidate",
            "0.9.21",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-10)),
            "sha256:current-v3",
            "0.9.20",
            "sha256:live-candidate",
            "0.9.22",
        ),
    ] {
        let job_id = insert_check_job(&state, "schedule", &discovered_at).await;
        state
            .db
            .finish_job(
                &job_id,
                "success",
                &discovered_at,
                &make_new_version_summary_for_test_with_image_ref(
                    &service_id,
                    "ghcr.io/acme/web@sha256:current-v3",
                    "sha256:current-v3",
                    current_display_tag,
                    current_digest,
                    "latest",
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
        stack_body["stack"]["services"][0]["image"]["resolvedTag"].as_str(),
        Some("0.9.20"),
        "stack body: {}",
        stack_body
    );
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
    assert_eq!(items[0]["version"].as_str(), Some("0.9.22"));
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("0.9.21"));
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("0.9.20"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_dedupes_live_unresolved_alias_candidate() {
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
            Some("1.17.0".to_string()),
            Some("[\"1.17.0\"]".to_string()),
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

    for (discovered_at, candidate_digest) in [
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-90)),
            "sha256:candidate-a",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-30)),
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
                    "latest",
                    "1.17.0",
                    "sha256:current-v1",
                    "latest",
                    "latest",
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
        Some(1)
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
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("latest"));
    assert_eq!(
        items[0]["occurredAt"].as_str(),
        Some(test_offset_rfc3339(&now, time::Duration::minutes(-90)).as_str())
    );
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("1.17.0"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_uses_snapshot_resolved_current_running_version() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:4.39
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current"),
        Some("4.39"),
        Some("sha256:candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    let running_started_at = test_offset_rfc3339(&now, -time::Duration::hours(2));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:current".to_string()),
            Some(running_started_at.clone()),
            Some("4.39.0".to_string()),
            Some("[\"4.39.0\"]".to_string()),
            Some("4.39".to_string()),
            Some("4.39.16".to_string()),
            Some("sha256:candidate".to_string()),
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
        repo_tags_total: 1,
        repo_tags_considered: 1,
        manifests_ok: 1,
        manifests_timeout: 0,
        manifests_error: 0,
    };
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["4.39.15".to_string()],
        ready_scan,
    )
    .await;

    let discovered_at = test_offset_rfc3339(&now, -time::Duration::minutes(30));
    let job_id = insert_check_job(&state, "schedule", &discovered_at).await;
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &discovered_at,
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/web",
                "4.39",
                "4.39.15",
                "sha256:current",
                "4.39",
                "4.39.16",
                "sha256:candidate",
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
        stack_body["stack"]["services"][0]["image"]["resolvedTag"].as_str(),
        Some("4.39.15")
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
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("4.39.16"));
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("4.39.15"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_uses_snapshot_resolved_current_candidate_and_history()
 {
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
            Some("0.35.1".to_string()),
            Some("[\"0.35.1\"]".to_string()),
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
        vec!["latest".to_string(), "0.35.3".to_string()],
        ready_scan.clone(),
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:live-candidate",
        "linux/amd64",
        &test_offset_rfc3339(&now, -time::Duration::minutes(5)),
        vec!["latest".to_string(), "0.35.4".to_string()],
        ready_scan,
    )
    .await;

    for (discovered_at, candidate_digest) in [
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(30)),
            "sha256:history-candidate",
        ),
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(10)),
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
                    "latest",
                    "0.35.1",
                    "sha256:current",
                    "latest",
                    "latest",
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
        stack_body["stack"]["services"][0]["candidate"]["resolvedTag"].as_str(),
        Some("0.35.4")
    );
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
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("0.35.4"));
    assert_eq!(
        items[0]["occurredAt"].as_str(),
        Some(test_offset_rfc3339(&now, -time::Duration::minutes(10)).as_str())
    );
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("0.35.3"));
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("0.35.1"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_and_stack_use_notification_fallback_for_live_candidate()
 {
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
        Some("sha256:candidate-a"),
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
            None,
            Some("sha256:candidate-a".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let job_id = insert_check_job(&state, "schedule", &now).await;
    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        &job_id,
        "ghcr.io/acme/web:latest",
        "latest",
        "1.16.0",
        "latest",
        "1.16.2",
        "sha256:candidate-a",
        &test_offset_rfc3339(&now, -time::Duration::minutes(5)),
    )
    .await;
    let notification_target = (
        service_id.clone(),
        "ghcr.io/acme/web:latest".to_string(),
        "latest".to_string(),
        "sha256:candidate-a".to_string(),
    );
    let notifications = state
        .db
        .list_new_version_notifications_for_service(&service_id)
        .await
        .unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].image_ref, "ghcr.io/acme/web:latest");
    assert_eq!(notifications[0].image_tag, "latest");
    assert_eq!(notifications[0].candidate_digest, "sha256:candidate-a");
    let notification_tags = state
        .db
        .list_stable_candidate_display_tags_for_notification_targets(std::slice::from_ref(
            &notification_target,
        ))
        .await
        .unwrap();
    assert_eq!(
        notification_tags
            .get(&notification_target)
            .map(|tags| tags.iter().cloned().collect::<Vec<_>>()),
        Some(vec!["1.16.2".to_string()])
    );

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
        stack_body["stack"]["services"][0]["candidate"]["resolvedTag"].as_str(),
        Some("1.16.2")
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
    assert_eq!(items.len(), 2, "timeline body: {}", body);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("1.16.2"));
    assert_eq!(items[0]["occurredAt"].as_str(), None);
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("1.16.0"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_keeps_pinned_live_candidate_distinct_from_notification_fallback()
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
        Some("sha256:current-v1"),
        Some("3.2.14-r0-ls73"),
        Some("sha256:candidate-a"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v1".to_string()),
            Some("3.2.10-r0-ls66".to_string()),
            Some("[\"3.2.10-r0-ls66\"]".to_string()),
            Some("3.2.14-r0-ls73".to_string()),
            None,
            Some("sha256:candidate-a".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let job_id = insert_check_job(&state, "schedule", &now).await;
    reserve_new_version_notification_for_test(
        &state,
        &service_id,
        &job_id,
        "ghcr.io/acme/web:3.2.10-r0-ls66",
        "3.2.10-r0-ls66",
        "3.2.10-r0-ls66",
        "3.2.14-r0-ls73",
        "3.2.14",
        "sha256:candidate-a",
        &test_offset_rfc3339(&now, -time::Duration::minutes(5)),
    )
    .await;

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
        stack_body["stack"]["services"][0]["candidate"]["tag"].as_str(),
        Some("3.2.14-r0-ls73")
    );
    assert_eq!(
        stack_body["stack"]["services"][0]["candidate"]["resolvedTag"].as_str(),
        None
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
    assert_eq!(items.len(), 2, "timeline body: {}", body);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("3.2.14-r0-ls73"));
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("3.2.10-r0-ls66"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_collapses_suffix_variants_for_floating_aliases() {
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
        vec!["latest".to_string(), "3.2.14-r0-ls73".to_string()],
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

    for (discovered_at, candidate_digest) in [
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(30)),
            "sha256:history-candidate",
        ),
        (
            test_offset_rfc3339(&now, -time::Duration::minutes(5)),
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
                    "latest",
                    "3.2.10-r0-ls66",
                    "sha256:current",
                    "latest",
                    "latest",
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
        Some(1)
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
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("3.2.14"));
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("3.2.10-r0-ls66"));
}

