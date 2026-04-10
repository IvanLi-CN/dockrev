#[tokio::test]
async fn schedule_new_version_notification_does_not_wait_when_display_tags_are_already_resolved() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-resolved-{}.yml",
        ulid::Ulid::new()
    );
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("5.2.0".to_string()),
            Some("[\"5.2.0\"]".to_string()),
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &now,
        vec!["5.2.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["5.3.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "5.2.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "5.3.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "new_version"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:new",
                "linux/amd64",
                "new_version"
            )
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let payload = tokio::time::timeout(Duration::from_millis(120), rx.recv())
        .await
        .expect("resolved display tags should not wait for in-flight inference")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（5.2.0 -> 5.3.0）。")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_uses_cached_snapshot_without_waiting_for_in_flight_task()
{
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-snapshot-ready-{}.yml",
        ulid::Ulid::new()
    );
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &now,
        vec!["5.2.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["5.3.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "latest".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_miss"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue("ghcr.io/acme/web", "sha256:new", "linux/amd64", "force")
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let payload = tokio::time::timeout(Duration::from_millis(120), rx.recv())
        .await
        .expect("ready snapshots should skip settle wait even when worker is in-flight")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（5.2.0 -> 5.3.0）。")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2.0")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.3.0")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_waits_for_stale_snapshot_refresh() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(250)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();
    let stale_snapshot_at = test_offset_from_now_rfc3339(time::Duration::days(-28));

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-stale-snapshot-{}.yml",
        ulid::Ulid::new()
    );
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("5.1.0".to_string()),
            Some("[\"5.1.0\"]".to_string()),
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &stale_snapshot_at,
        vec!["5.1.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &stale_snapshot_at,
        vec!["5.2.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "5.1.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_stale"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:new",
                "linux/amd64",
                "cache_stale"
            )
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let early = tokio::time::timeout(Duration::from_millis(120), rx.recv()).await;
    assert!(
        early.is_err(),
        "stale cached snapshots should keep waiting for the refresh task"
    );

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（5.2.0 -> 5.3.0）。")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2.0")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.3.0")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_falls_back_when_stale_snapshot_times_out() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_secs(15)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let now = test_now_rfc3339();
    let stale_snapshot_at = test_offset_from_now_rfc3339(time::Duration::days(-28));

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-stale-timeout-{}.yml",
        ulid::Ulid::new()
    );
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("5.1.0".to_string()),
            Some("[\"5.1.0\"]".to_string()),
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &stale_snapshot_at,
        vec!["5.1.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &stale_snapshot_at,
        vec!["5.2.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "5.1.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:old",
                "linux/amd64",
                "cache_stale"
            )
            .await
    );
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:new",
                "linux/amd64",
                "cache_stale"
            )
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            &now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let payload = tokio::time::timeout(Duration::from_secs(12), rx.recv())
        .await
        .expect("notification should fall back after settle timeout")
        .expect("notification payload missing");
    let summary = payload["human"]["summary"].as_str().unwrap_or_default();
    assert_eq!(summary, "demo / web 服务有新版本。");
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("latest")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("latest")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_falls_back_to_oci_explicit_version_when_snapshot_tags_stay_latest()
 {
    let state = test_state_with(
        ":memory:",
        Arc::new(ExplicitVersionFallbackRegistry::new("0.30.0")),
        Arc::new(FakeRunner),
    )
    .await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-explicit-version-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:0.29.12
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("0.29.12".to_string()),
            Some("[\"0.29.12\"]".to_string()),
            Some("0.29.12".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 116,
            repo_tags_considered: 40,
            manifests_ok: 14,
            manifests_timeout: 23,
            manifests_error: 3,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "0.29.12".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "0.29.12".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();

    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &job_id,
        "schedule",
        &now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("explicit version fallback should still deliver immediately")
        .expect("notification payload missing");
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("demo / web 服务有新版本（0.29.12 -> 0.30.0）。")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("0.30.0")
    );
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_keeps_generic_copy_when_snapshot_and_explicit_version_stay_raw()
 {
    let state = test_state_with(
        ":memory:",
        Arc::new(LatestOnlyRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let now = test_now_rfc3339();

    let compose_path = format!(
        "/tmp/dockrev-schedule-notify-explicit-miss-{}.yml",
        ulid::Ulid::new()
    );
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 116,
            repo_tags_considered: 40,
            manifests_ok: 14,
            manifests_timeout: 23,
            manifests_error: 3,
        },
    )
    .await;
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "latest".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let job_id = insert_check_job(&state, "schedule", &now).await;
    state
        .db
        .finish_job(&job_id, "success", &now, &serde_json::json!({}))
        .await
        .unwrap();

    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &job_id,
        "schedule",
        &now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("ready raw-only snapshot should still send immediately")
        .expect("notification payload missing");
    let summary = payload["human"]["summary"].as_str().unwrap_or_default();
    assert_eq!(summary, "demo / web 服务有新版本。");
    assert!(!summary.contains("latest -> latest"));
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("latest")
    );
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notification_omits_transition_when_both_sides_stay_raw() {
    let state = test_state_with(
        ":memory:",
        Arc::new(LatestOnlyRegistry),
        Arc::new(FakeRunner),
    )
    .await;

    let compose_path = format!("/tmp/dockrev-schedule-notify-raw-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("latest".to_string()),
            None,
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "latest".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "latest".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let now = "2026-03-09T00:00:00Z";
    let job_id = insert_check_job(&state, "schedule", now).await;
    state
        .db
        .finish_job(&job_id, "success", now, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        state
            .snapshot_worker
            .enqueue(
                "ghcr.io/acme/web",
                "sha256:new",
                "linux/amd64",
                "new_version"
            )
            .await
    );

    let notify_state = state.clone();
    let notify_discovered = discovered.clone();
    let notify_task = tokio::spawn(async move {
        crate::notify::notify_new_versions_discovered(
            notify_state.as_ref(),
            &job_id,
            "schedule",
            now,
            1,
            &notify_discovered,
        )
        .await
        .unwrap();
    });

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    let summary = payload["human"]["summary"].as_str().unwrap_or_default();
    assert_eq!(summary, "demo / web 服务有新版本。");
    assert!(!summary.contains("latest -> latest"));
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("latest")
    );
    notify_task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn webhook_notifications_filter_to_matched_service_ids() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!(
        "/tmp/dockrev-webhook-filter-notify-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  worker:
    image: ghcr.io/acme/worker:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let web = services
        .iter()
        .find(|service| service.name == "web")
        .unwrap()
        .clone();
    let worker = services
        .iter()
        .find(|service| service.name == "worker")
        .unwrap()
        .clone();
    let now = "2026-03-09T00:00:00Z";
    for (service, digest, current_display, candidate_display) in [
        (&web, "sha256:web-new", "1.0.0", "1.1.0"),
        (&worker, "sha256:worker-new", "2.0.0", "2.1.0"),
    ] {
        state
            .db
            .update_service_check_result(
                &service.id,
                Some(format!("sha256:{}-old", service.name)),
                Some(current_display.to_string()),
                Some(format!("[\"{current_display}\"]")),
                Some(service.image_tag.clone()),
                Some(candidate_display.to_string()),
                Some(digest.to_string()),
                Some("match".to_string()),
                Some("[\"linux/amd64\"]".to_string()),
                None,
                None,
                now,
                now,
            )
            .await
            .unwrap();
    }
    let (mut rx, server) = configure_webhook_notifications(&state).await;
    let job_id = insert_check_job(&state, "webhook", now).await;
    let summary = serde_json::json!({
        "source": "github_webhook",
        "matchedServiceIds": [web.id.clone()],
        "servicesChecked": 2,
        "newVersions": {
            "services": [
                {
                    "stackId": stack_id.clone(),
                    "serviceId": web.id.clone(),
                    "imageRef": web.image_ref.clone(),
                    "currentTag": web.image_tag.clone(),
                    "currentDisplayTag": "1.0.0",
                    "candidateTag": "latest",
                    "candidateDisplayTag": "1.1.0",
                    "candidateDigest": "sha256:web-new"
                },
                {
                    "stackId": stack_id.clone(),
                    "serviceId": worker.id.clone(),
                    "imageRef": worker.image_ref.clone(),
                    "currentTag": worker.image_tag.clone(),
                    "currentDisplayTag": "2.0.0",
                    "candidateTag": "latest",
                    "candidateDisplayTag": "2.1.0",
                    "candidateDigest": "sha256:worker-new"
                }
            ]
        }
    });

    super::operations::maybe_notify_check_new_versions(&state, &job_id, "webhook", now, &summary)
        .await
        .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    let service_urls = payload["links"]["serviceUrls"].as_array().unwrap();
    assert_eq!(service_urls.len(), 1);
    assert!(
        service_urls[0]["url"]
            .as_str()
            .unwrap_or_default()
            .contains(&web.id)
    );

    let web_rows = state
        .db
        .list_new_version_notifications_for_service(&web.id)
        .await
        .unwrap();
    let worker_rows = state
        .db
        .list_new_version_notifications_for_service(&worker.id)
        .await
        .unwrap();
    assert_eq!(web_rows.len(), 1);
    assert!(worker_rows.is_empty());
    server.abort();
}

#[tokio::test]
async fn stale_new_version_notifications_are_skipped_when_candidate_was_cleared() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-stale-notify-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_digest: Some("sha256:old".to_string()),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let active_now = "2026-03-09T00:00:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            active_now,
            active_now,
        )
        .await
        .unwrap();

    let cleared_now = "2026-03-09T00:01:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            cleared_now,
            cleared_now,
        )
        .await
        .unwrap();

    let job_id = insert_check_job(&state, "schedule", cleared_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &job_id,
        "schedule",
        cleared_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "stale notification should be skipped after candidate clears"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert!(rows.is_empty());

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(logs.iter().any(|line| {
        line.msg.contains("new-version notification skipped: all 1 services no longer have matching active candidates")
    }));
    server.abort();
}

