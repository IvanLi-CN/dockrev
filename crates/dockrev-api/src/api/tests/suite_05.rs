#[tokio::test]
async fn get_stack_does_not_reuse_same_digest_tags_across_repo_provenances() {
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
        Some("sha256:shared-digest"),
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
                "1.16.2",
                "sha256:shared-digest",
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
                homepage: None,
                update_guard: None,
            }],
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
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
            Some("sha256:shared-digest".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
            &test_offset_rfc3339(&now, time::Duration::minutes(1)),
        )
        .await
        .unwrap();

    let job_2 = insert_check_job(
        &state,
        "schedule",
        &test_offset_rfc3339(&now, time::Duration::minutes(2)),
    )
    .await;
    state
        .db
        .finish_job(
            &job_2,
            "success",
            &test_offset_rfc3339(&now, time::Duration::minutes(2)),
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/worker",
                "latest",
                "latest",
                "",
                "latest",
                "latest",
                "sha256:shared-digest",
            ),
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
    assert_eq!(
        detail["stack"]["services"][0]["newVersionDiscoveryCount"].as_u64(),
        Some(2)
    );
}

#[tokio::test]
async fn get_stack_prefers_highest_snapshot_semver_for_unsettled_alias_history() {
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
            Some("1.16.2".to_string()),
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
                "1.16.2",
                "sha256:candidate-b",
            ),
        )
        .await
        .unwrap();

    let ready_scan = crate::api::types::ServiceDigestTagsScanSummary {
        repo_tags_total: 3,
        repo_tags_considered: 3,
        manifests_ok: 3,
        manifests_timeout: 0,
        manifests_error: 0,
    };
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate-a",
        "linux/amd64",
        &test_offset_rfc3339(&now, time::Duration::minutes(2)),
        vec![
            "latest".to_string(),
            "1.16".to_string(),
            "1.16.2".to_string(),
        ],
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
async fn check_uses_fixed_parallelism_stagger_and_dual_progress() {
    let registry = Arc::new(StaggeredCheckRegistry::with_peak_gate(
        Duration::from_secs(8),
        crate::config::FIXED_CHECK_PARALLELISM,
    ));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web1:
    image: ghcr.io/acme/web1:5.2
  web2:
    image: ghcr.io/acme/web2:5.2
  web3:
    image: ghcr.io/acme/web3:5.2
  web4:
    image: ghcr.io/acme/web4:5.2
  web5:
    image: ghcr.io/acme/web5:5.2
  web6:
    image: ghcr.io/acme/web6:5.2
  web7:
    image: ghcr.io/acme/web7:5.2
  web8:
    image: ghcr.io/acme/web8:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    let mut saw_split_progress = false;
    for _ in 0..500 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        let progress = &job["job"]["progress"];
        let planned_current = progress["plannedCurrent"].as_u64().unwrap_or(0);
        let completed_current = progress["current"].as_u64().unwrap_or(0);
        if planned_current > completed_current {
            saw_split_progress = true;
        }
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            assert_eq!(progress["plannedCurrent"], progress["current"]);
            assert_eq!(progress["plannedTotal"], progress["total"]);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(finished, "check job did not finish in time");
    assert!(
        saw_split_progress,
        "check progress should expose planned > completed while running"
    );

    let max_in_flight = registry.max_in_flight();
    assert!(
        max_in_flight <= crate::config::FIXED_CHECK_PARALLELISM,
        "max in-flight should be capped at {}, got {max_in_flight}",
        crate::config::FIXED_CHECK_PARALLELISM
    );
    assert!(
        max_in_flight == crate::config::FIXED_CHECK_PARALLELISM,
        "max in-flight should reach fixed parallelism {}, got {max_in_flight}",
        crate::config::FIXED_CHECK_PARALLELISM
    );

    let starts = registry.started_at();
    assert!(
        starts.len() >= 2,
        "expected at least two scheduled manifest requests, got {}",
        starts.len()
    );
    for pair in starts.windows(2) {
        let gap = pair[1].duration_since(pair[0]);
        assert!(
            gap >= Duration::from_millis(800),
            "spawn gap should be ~1s, got {:?}",
            gap
        );
    }
}

#[tokio::test]
async fn check_coalesces_repo_tags_fetch_for_same_image() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(120)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web1:
    image: ghcr.io/acme/web:latest
  web2:
    image: ghcr.io/acme/web:latest
  web3:
    image: ghcr.io/acme/web:latest
  web4:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..800 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
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
    assert!(finished, "check job did not finish in time");
    assert_eq!(
        registry.list_tags_calls(),
        0,
        "check main path should not block on version inference tag scans"
    );
}

#[tokio::test]
async fn get_stack_version_inference_cache_miss_returns_pending() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

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
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "pending"
    );
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["reason"]
            .as_str()
            .unwrap_or("<none>"),
        "cache_miss"
    );
}

#[tokio::test]
async fn get_stack_shows_pending_when_new_version_task_is_inflight_even_with_cache() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(400)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["0.13.0".to_string(), "latest".to_string()],
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
            "sha256:new",
            "linux/amd64",
            "new_version",
        )
        .await;
    assert!(enqueued);

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
    let status = detail["stack"]["services"][0]["versionInference"]["status"]
        .as_str()
        .unwrap_or("<none>");
    assert!(
        status == "pending" || status == "ready",
        "unexpected stack detail: {detail}"
    );
    if status == "pending" {
        assert_eq!(
            detail["stack"]["services"][0]["versionInference"]["reason"]
                .as_str()
                .unwrap_or("<none>"),
            "new_version"
        );
    }
}

#[tokio::test]
async fn get_stack_all_failed_recent_snapshot_is_ready_without_reenqueue() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        Vec::new(),
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 2,
        },
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
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "ready"
    );
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["reason"]
            .as_str()
            .unwrap_or("<none>"),
        "all_failed"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        registry.list_tags_calls(),
        0,
        "recent all_failed cache should not immediately re-enqueue inference"
    );
}

#[tokio::test]
async fn force_refresh_endpoint_requires_known_digest_and_dedupes_per_digest() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
        Some("sha256:candidate"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or("<none>"),
        "invalid_argument"
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
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

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
                .body(Body::from(r#"{"digest":"sha256:missing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

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
                .body(Body::from(r#"{"digest":"sha256:current"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "force");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:current"
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:current", "linux/amd64")
            .await
            .as_deref(),
        Some("force")
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:candidate", "linux/amd64")
            .await,
        None
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
                .body(Body::from(r#"{"digest":"sha256:current"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "running");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:current"
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
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "force");
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
}

#[tokio::test]
async fn stack_detail_does_not_go_pending_when_only_force_task_is_in_flight() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
        Some("sha256:candidate"),
    )
    .await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["v1.0.0".to_string(), "latest".to_string()],
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
        "sha256:candidate",
        "linux/amd64",
        &now,
        vec!["v1.1.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    // Trigger a digest-scoped force refresh (manual), which should stay local to the popover UX
    // and must not flip stack-level `versionInference.status` to `pending`.
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
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "ready"
    );
}

#[tokio::test]
async fn stack_detail_clears_resolved_tag_when_snapshot_has_no_semver_tags() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    let _service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["v0.8.7".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
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
    let image = &detail["stack"]["services"][0]["image"];
    assert_eq!(image["resolvedTag"].as_str().unwrap_or("<none>"), "v0.8.7");

    // Snapshot refreshed, but it no longer contains any semver tags.
    let now2 = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now2,
        vec!["latest".to_string(), "stable".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
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
    let image = &detail["stack"]["services"][0]["image"];
    assert!(
        image.get("resolvedTag").is_none(),
        "expected resolvedTag to be cleared when snapshot has no semver tags: {detail}"
    );
    assert!(
        image.get("resolvedTags").is_none(),
        "expected resolvedTags to be cleared when snapshot has no semver tags: {detail}"
    );
}

#[tokio::test]
async fn stack_detail_preserves_resolved_tag_when_snapshot_has_no_semver_tags_but_scan_is_incomplete()
 {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service = services.first().expect("service must exist");

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed a last-known-good resolved tag on the service itself.
    state
        .db
        .update_service_check_result(
            &service.id,
            crate::snapshot_worker::normalize_digest("sha256:current"),
            Some("v0.8.7".to_string()),
            Some(serde_json::to_string(&vec!["v0.8.7"]).unwrap()),
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

    // Snapshot refreshed, but it no longer contains any semver tags. The scan is incomplete
    // (`repo_tags_considered` < `repo_tags_total`), so it must not wipe the last-known-good
    // resolved tag values.
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["latest".to_string(), "stable".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 100,
            repo_tags_considered: 40,
            manifests_ok: 40,
            manifests_timeout: 0,
            manifests_error: 0,
        },
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
    let image = &detail["stack"]["services"][0]["image"];
    assert_eq!(
        image["resolvedTag"].as_str().unwrap_or("<none>"),
        "v0.8.7",
        "expected resolvedTag to be preserved for incomplete scan: {detail}"
    );
    assert_eq!(
        image["resolvedTags"][0].as_str().unwrap_or("<none>"),
        "v0.8.7",
        "expected resolvedTags to be preserved for incomplete scan: {detail}"
    );
}
