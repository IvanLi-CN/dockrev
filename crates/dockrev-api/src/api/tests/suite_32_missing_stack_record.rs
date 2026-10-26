#[tokio::test]
async fn missing_stack_record_api_filters_previews_deletes_and_is_idempotent() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let compose_path = format!(
        "/tmp/dockrev-missing-stack-api-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "missing-api", &compose_path).await;
    let now = test_now_rfc3339();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "missing-api".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "missing".to_string(),
            last_seen_at: None,
            last_scan_at: now,
            last_error: Some("compose_files_missing".to_string()),
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: false,
        })
        .await
        .unwrap();
    std::fs::remove_file(&compose_path).unwrap();

    let present = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(present.status(), 200);
    let present_body = response_json(present).await;
    assert!(present_body["stacks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|stack| stack["id"] != stack_id));

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/stacks?state=missing&archived=include")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), 200);
    let missing_body = response_json(missing).await;
    let missing_item = missing_body["stacks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stack| stack["id"] == stack_id)
        .expect("missing stack should be listed by the missing state filter");
    assert_eq!(missing_item["missing"], true);

    let preview = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/stacks/{stack_id}/record-deletion-preview"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    let preview_body = response_json(preview).await;
    assert_eq!(preview_body["missing"], true);
    assert_eq!(preview_body["canDelete"], true);
    assert!(preview_body["dockerResources"].is_object());

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert_eq!(response_json(deleted).await["outcome"], "deleted");
    assert!(state.db.get_stack(&stack_id).await.unwrap().is_none());

    let repeated = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated.status(), 200);
    assert_eq!(response_json(repeated).await["outcome"], "already_absent");
}

#[tokio::test]
async fn stack_record_deletion_api_requires_auth_and_rejects_present_stacks() {
    let state = test_state_with_authz(":memory:", Some("ops"), None, false).await;
    let app = api::router(state.clone());
    let compose_path = format!(
        "/tmp/dockrev-present-stack-api-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "present-api", &compose_path).await;

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);

    let preview = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/stacks/{stack_id}/record-deletion-preview"))
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    let preview_body = response_json(preview).await;
    assert_eq!(preview_body["missing"], false);
    assert_eq!(preview_body["canDelete"], false);
    assert_eq!(preview_body["blockingReason"], "stack_is_not_missing");

    let rejected = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/stacks/{stack_id}"))
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), 409);
    assert_eq!(response_json(rejected).await["error"]["details"]["reason"], "stack_is_not_missing");
    assert!(state.db.get_stack(&stack_id).await.unwrap().is_some());

    std::fs::remove_file(compose_path).unwrap();
}
