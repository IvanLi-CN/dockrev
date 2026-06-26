#[tokio::test]
async fn health_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn version_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["version"], "0.1.0");
}

#[tokio::test]
async fn unknown_api_path_is_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn supervisor_paths_are_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/self-upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["code"], "supervisor_misrouted");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn service_digest_tags_lists_all_matches_without_truncation() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
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
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Use a bare hash to assert normalization (sha256: prefix added server-side).
    set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:match",
        "linux/amd64",
        &checked_at,
        (0..50).rev().map(|idx| format!("1.0.{idx}")).collect(),
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 50,
            repo_tags_considered: 50,
            manifests_ok: 50,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/digest-tags?digest=match"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 50);
    assert_eq!(tags[0].as_str().unwrap(), "1.0.49");
    assert_eq!(tags[49].as_str().unwrap(), "1.0.0");
    assert_eq!(body["checkedAt"].as_str(), Some(checked_at.as_str()));
}

#[tokio::test]
async fn service_digest_tags_accept_digest_only_image_refs() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-digest-only-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web@sha256:match
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
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
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/digest-tags?digest=match"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str(), Some("pending"));
    assert_eq!(body["digest"].as_str(), Some("sha256:match"));
    assert!(body["retryAfterMs"].as_u64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn service_digest_tags_snapshot_returns_pending_when_missing() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap(), "pending");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert!(body["retryAfterMs"].as_u64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn service_digest_tags_snapshot_returns_pending_while_target_digest_is_in_flight() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate",
        "linux/amd64",
        &checked_at,
        vec!["v0.1.9".to_string(), "0.1.9".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/web",
            "sha256:candidate",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=sha256:candidate",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
}

#[tokio::test]
async fn service_digest_tags_snapshot_returns_cached_snapshot_while_non_force_task_is_in_flight() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate",
        "linux/amd64",
        &checked_at,
        vec!["v0.1.9".to_string(), "0.1.9".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/web",
            "sha256:candidate",
            "linux/amd64",
            "cache_stale",
        )
        .await;
    assert!(enqueued);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=sha256:candidate",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
    assert_eq!(body["tags"][0].as_str().unwrap_or("<none>"), "v0.1.9");
}

#[tokio::test]
async fn force_refresh_promotes_non_force_in_flight_snapshot_to_pending() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate",
        "linux/amd64",
        &checked_at,
        vec!["v0.1.9".to_string(), "0.1.9".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/web",
            "sha256:candidate",
            "linux/amd64",
            "cache_stale",
        )
        .await;
    assert!(enqueued);
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:candidate", "linux/amd64")
            .await
            .as_deref(),
        Some("cache_stale")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:candidate"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "running");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:candidate", "linux/amd64")
            .await
            .as_deref(),
        Some("force")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=sha256:candidate",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
}

#[tokio::test]
async fn service_digest_tags_snapshot_unknown_digest_is_not_enqueued() {
    let registry = Arc::new(CountingRegistry::default());
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let calls_before = registry.total_calls();
    let unknown_digest = "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest={unknown_digest}",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        registry.total_calls(),
        calls_before,
        "unknown digest should not trigger snapshot worker scans"
    );

    let image_repo = crate::snapshot_worker::image_repo_from_image_ref(&svc.image_ref).unwrap();
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, unknown_digest, "linux/amd64")
        .await
        .unwrap();
    assert!(snapshot.is_none(), "unknown digest should not be persisted");
}

#[tokio::test]
async fn service_digest_tags_snapshot_uses_anchor_tag_outside_depth() {
    let state = test_state_with(
        ":memory:",
        Arc::new(AnchoredSnapshotRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:legacy-1
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..40 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready");
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_str().unwrap(), "legacy-1");
    assert_eq!(body["scan"]["repoTagsTotal"].as_u64().unwrap(), 131);
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 40);
}

#[tokio::test]
async fn service_digest_tags_snapshot_failure_eventually_returns_ready() {
    let state = test_state_with(
        ":memory:",
        Arc::new(ListTagsFailRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:match".to_string()),
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
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..40 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready even after worker failure");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert_eq!(body["tags"].as_array().unwrap().len(), 0);
    assert!(body["scan"]["manifestsError"].as_u64().unwrap_or_default() >= 1);

    // Once the fallback snapshot is persisted, the endpoint should stop returning pending.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn snapshot_worker_limits_concurrent_runs() {
    let registry = Arc::new(SnapshotConcurrencyProbeRegistry::default());
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

    let image_repo = "ghcr.io/acme/web";
    let host_platform = "linux/amd64";
    let mut digests: Vec<String> = Vec::new();
    for i in 0..16 {
        let digest = format!("sha256:{:064x}", i + 1);
        digests.push(digest.clone());
        state
            .snapshot_worker
            .enqueue(image_repo, &digest, host_platform, "concurrency_probe")
            .await;
    }

    let mut all_ready = false;
    for _ in 0..800 {
        let mut ready = 0usize;
        for digest in &digests {
            if state
                .db
                .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
                .await
                .unwrap()
                .is_some()
            {
                ready += 1;
            }
        }
        if ready == digests.len() {
            all_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(all_ready, "all queued snapshot tasks should complete");
    assert!(
        registry.max_in_flight() <= crate::snapshot_worker::SNAPSHOT_WORKER_MAX_CONCURRENCY,
        "observed list_tags concurrency {} > configured cap {}",
        registry.max_in_flight(),
        crate::snapshot_worker::SNAPSHOT_WORKER_MAX_CONCURRENCY
    );
}

#[tokio::test]
async fn check_enqueues_digest_tags_snapshot_and_endpoint_eventually_returns_ready() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    // Use the same scan-time code path as real jobs.
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    // Use a bare hash to assert normalization (sha256: prefix added server-side).
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let pending = response_json(resp).await;
    assert_eq!(pending["status"].as_str().unwrap(), "pending");

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert!(body["checkedAt"].as_str().is_some_and(|s| !s.is_empty()));

    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 40);
    assert_eq!(tags[0].as_str().unwrap(), "1.0.49");
    assert_eq!(tags[39].as_str().unwrap(), "1.0.10");

    assert_eq!(body["scan"]["repoTagsTotal"].as_u64().unwrap(), 50);
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 40);
    assert_eq!(body["scan"]["manifestsOk"].as_u64().unwrap(), 40);
    assert_eq!(body["scan"]["manifestsTimeout"].as_u64().unwrap(), 0);
    assert_eq!(body["scan"]["manifestsError"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn digest_tags_snapshot_endpoint_ignores_legacy_service_snapshot_table() {
    let state = test_state_with(":memory:", Arc::new(PruneRegistry), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed extra historical digests to ensure the prune step is exercised.
    let seed_snapshot = |digest: &str| {
        serde_json::json!({
          "digest": digest,
          "tags": ["seed"],
          "checkedAt": now.as_str(),
          "scan": {
            "repoTagsTotal": 3,
            "repoTagsConsidered": 3,
            "manifestsOk": 3,
            "manifestsTimeout": 0,
            "manifestsError": 0,
          }
        })
        .to_string()
    };
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old1",
            &seed_snapshot("sha256:old1"),
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old2",
            &seed_snapshot("sha256:old2"),
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old3",
            &seed_snapshot("sha256:old3"),
            &now,
            &now,
        )
        .await
        .unwrap();

    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        // Ensure current digest is known even if the registry is inconsistent.
        Some(
            crate::service_check::RuntimeServiceObservation::digest_only("sha256:cur".to_string()),
        ),
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    // Legacy service-scoped snapshot rows should no longer be served by the endpoint.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=old2",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // current digest should be generated asynchronously and eventually become ready.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=cur",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut ready = false;
    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=cur",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            ready = true;
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(ready, "current digest snapshot should become ready");
}

#[tokio::test]
async fn same_tag_digest_candidate_does_not_pick_higher_semver_tag() {
    let state = test_state_with(
        ":memory:",
        Arc::new(CrossTagSemverRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  valkey:
    image: valkey/valkey:8-alpine
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        // Simulate runtime being behind registry (digest-only update).
        Some(
            crate::service_check::RuntimeServiceObservation::digest_only("sha256:old".to_string()),
        ),
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

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
    let svc = &detail["stack"]["services"][0];
    assert_eq!(svc["image"]["tag"].as_str().unwrap(), "8-alpine");
    assert_eq!(svc["candidate"]["tag"].as_str().unwrap(), "8-alpine");
    assert_eq!(svc["candidate"]["digest"].as_str().unwrap(), "sha256:new");
}
