#[tokio::test]
async fn stack_detail_preserves_resolved_tag_when_snapshot_is_all_failed() {
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

    // Seed a last-known-good resolved tag on the service itself. This should not be wiped when
    // the latest snapshot is an all_failed/error snapshot.
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

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec![],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 0,
            repo_tags_considered: 0,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 1,
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
        "expected resolvedTag to be preserved for all_failed snapshot: {detail}"
    );
}

#[tokio::test]
async fn check_digest_changes_enqueue_new_version_inference_for_current_and_candidate() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
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

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
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

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
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

    let job = wait_for_job_terminal(&state, &check_id).await;
    assert_eq!(job.status, "success");

    for digest in ["sha256:old", "sha256:new"] {
        let mut enqueued = false;
        for _ in 0..300 {
            let in_flight = state
                .snapshot_worker
                .in_flight_reason("ghcr.io/acme/web", digest, "linux/amd64")
                .await;
            let has_snapshot = state
                .db
                .get_image_digest_tags_snapshot("ghcr.io/acme/web", digest, "linux/amd64")
                .await
                .unwrap()
                .is_some();
            if in_flight.as_deref() == Some("new_version") || has_snapshot {
                enqueued = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            enqueued,
            "digest {digest} should be queued or cached for new-version inference"
        );
    }
}

#[tokio::test]
async fn check_non_strict_semver_alias_enqueues_new_version_inference() {
    let registry = Arc::new(AliasDriftRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-alias-test-{}.yml", ulid::Ulid::new());
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

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

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

    let job = wait_for_job_terminal(&state, &check_id).await;
    assert_eq!(job.status, "success");

    for digest in ["sha256:old", "sha256:new"] {
        let mut enqueued = false;
        for _ in 0..300 {
            let in_flight = state
                .snapshot_worker
                .in_flight_reason("ghcr.io/acme/web", digest, "linux/amd64")
                .await;
            let has_snapshot = state
                .db
                .get_image_digest_tags_snapshot("ghcr.io/acme/web", digest, "linux/amd64")
                .await
                .unwrap()
                .is_some();
            if in_flight.as_deref() == Some("new_version") || has_snapshot {
                enqueued = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            enqueued,
            "digest {digest} should still be inferred for non-strict semver aliases"
        );
    }
}

#[tokio::test]
async fn check_non_strict_semver_alias_skips_new_version_inference_when_snapshot_is_fresh() {
    let registry = Arc::new(AliasDriftRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-alias-fresh-snapshot-{}.yml",
        ulid::Ulid::new()
    );
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

    let now = test_now_rfc3339();
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
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &now,
        vec!["5.2.0".to_string(), "5.2".to_string()],
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
        vec!["5.3.0".to_string(), "5.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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

    let job = wait_for_job_terminal(&state, &check_id).await;
    assert_eq!(job.status, "success");

    let services = job.summary_json["newVersions"]["services"]
        .as_array()
        .expect("new version services missing");
    assert_eq!(services[0]["currentDisplayTag"].as_str(), Some("5.2.0"));
    assert_eq!(services[0]["candidateDisplayTag"].as_str(), Some("5.3.0"));

    let events = state.snapshot_worker.events_since(0, 200).await;
    let has_new_version_enqueue = events.events.iter().any(|event| {
        event.data["type"].as_str() == Some("task_enqueued")
            && event.data["reason"].as_str() == Some("new_version")
            && matches!(
                event.data["digest"].as_str(),
                Some("sha256:old") | Some("sha256:new")
            )
    });
    assert!(
        !has_new_version_enqueue,
        "fresh snapshots should suppress redundant new-version inference enqueues"
    );
}

#[tokio::test]
async fn check_non_strict_semver_alias_ignores_stale_current_resolved_tag_in_summary() {
    let registry = Arc::new(AliasDriftRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-alias-stale-summary-{}.yml", ulid::Ulid::new());
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

    let now = test_now_rfc3339();
    let stale_snapshot_at = test_offset_from_now_rfc3339(time::Duration::days(-28));
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("5.1.0".to_string()),
            Some("[\"5.1.0\"]".to_string()),
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
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &stale_snapshot_at,
        vec!["5.1.0".to_string(), "5.2".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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

    let job = wait_for_job_terminal(&state, &check_id).await;
    assert_eq!(job.status, "success");
    let services = job.summary_json["newVersions"]["services"]
        .as_array()
        .expect("new version services missing");
    assert_eq!(services.len(), 1);
    assert_eq!(services[0]["currentDisplayTag"].as_str(), Some("5.2"));
    assert_eq!(services[0]["candidateDisplayTag"].as_str(), Some("5.2"));

    let events = state.snapshot_worker.events_since(0, 200).await;
    let queued_refresh = events.events.iter().any(|event| {
        event.data["type"].as_str() == Some("task_enqueued")
            && event.data["reason"].as_str() == Some("new_version")
            && matches!(
                event.data["digest"].as_str(),
                Some("sha256:old") | Some("sha256:new")
            )
    });
    assert!(
        queued_refresh,
        "stale current aliases should refresh snapshot inference before trusting resolved tags"
    );
}

#[tokio::test]
async fn check_candidate_digest_change_for_strict_semver_does_not_enqueue_inference() {
    let registry = Arc::new(StrictSemverDriftRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
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
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

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
    for _ in 0..120 {
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
        detail["stack"]["services"][0]["candidate"]["digest"]
            .as_str()
            .unwrap_or("<none>"),
        "sha256:new"
    );

    let in_flight = state
        .snapshot_worker
        .in_flight_reason("ghcr.io/acme/web", "sha256:new", "linux/amd64")
        .await;
    assert!(
        in_flight.is_none(),
        "strict semver check candidate changes should not enqueue version inference"
    );
}

#[tokio::test]
async fn checks_conflict_when_check_is_already_running() {
    let registry = Arc::new(SlowRegistry {
        delay: Duration::from_millis(250),
    });
    let runner = Arc::new(FakeRunner);
    let state = test_state_with(":memory:", registry, runner).await;
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

    let resp2 = app
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
    assert_eq!(resp2.status(), 409);
    let body = response_json(resp2).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "conflict");
    assert_eq!(
        body["error"]["details"]["existingJobId"].as_str().unwrap(),
        check_id.as_str()
    );
}

#[tokio::test]
async fn checks_terminate_stale_running_job_then_start_new_one() {
    let state = test_state(":memory:").await;
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

    let old_dt = time::OffsetDateTime::now_utc() - time::Duration::hours(3);
    let old_now = old_dt
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let stale_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        stale_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &old_now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

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
    let new_id = triggered["checkId"].as_str().unwrap().to_string();
    assert_ne!(new_id, stale_id);

    let stale = state.db.get_job(&stale_id).await.unwrap().unwrap();
    assert_eq!(stale.status, "failed");
    assert!(stale.finished_at.is_some());
    assert_eq!(
        stale.summary_json["terminated"]["reason"].as_str().unwrap(),
        "stale_check"
    );
}

#[tokio::test]
async fn check_job_exposes_progress_in_detail_and_list() {
    let state = test_state(":memory:").await;
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

    let mut done = None;
    for _ in 0..80 {
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
            done = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let done = done.expect("check job did not finish in time");
    assert_eq!(done["job"]["progress"]["phase"].as_str().unwrap(), "done");
    assert_eq!(done["job"]["progress"]["percent"].as_u64().unwrap(), 100);
    assert_eq!(
        done["job"]["summary"]["progress"]["phase"]
            .as_str()
            .unwrap(),
        "done"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == check_id)
        .cloned()
        .expect("check job not in list");
    assert_eq!(item["progress"]["phase"].as_str().unwrap(), "done");
}

#[tokio::test]
async fn finish_job_preserves_existing_progress_when_summary_omits_progress() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_discovery_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &created_at,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let progress = serde_json::json!({
        "phase": "scan",
        "message": "scanned projects (3/5)",
        "current": 3,
        "total": 5,
        "percent": 60,
        "currentTarget": "demo",
        "updatedAt": created_at,
    });
    state.db.set_job_progress(&job_id, &progress).await.unwrap();

    let finished_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &finished_at,
            &serde_json::json!({ "scan": { "projectsSeen": 5 } }),
        )
        .await
        .unwrap();

    let detail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), 200);
    let detail = response_json(detail_resp).await;
    assert_eq!(detail["job"]["progress"]["phase"].as_str().unwrap(), "scan");
    assert_eq!(detail["job"]["progress"]["percent"].as_u64().unwrap(), 60);
    assert!(
        detail["job"]["progress"]
            .get("plannedCurrent")
            .is_none(),
        "legacy progress should keep plannedCurrent absent"
    );
    assert!(
        detail["job"]["progress"]
            .get("plannedTotal")
            .is_none(),
        "legacy progress should keep plannedTotal absent"
    );
    assert!(
        detail["job"]["progress"]
            .get("plannedPercent")
            .is_none(),
        "legacy progress should keep plannedPercent absent"
    );
    assert_eq!(
        detail["job"]["summary"]["progress"]["phase"]
            .as_str()
            .unwrap(),
        "scan"
    );

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list = response_json(list_resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .cloned()
        .expect("job not in list");
    assert_eq!(item["progress"]["phase"].as_str().unwrap(), "scan");
    assert_eq!(item["progress"]["percent"].as_u64().unwrap(), 60);
    assert!(
        item["progress"].get("plannedCurrent").is_none(),
        "legacy progress should keep plannedCurrent absent"
    );
    assert!(
        item["progress"].get("plannedTotal").is_none(),
        "legacy progress should keep plannedTotal absent"
    );
    assert!(
        item["progress"].get("plannedPercent").is_none(),
        "legacy progress should keep plannedPercent absent"
    );
}

#[tokio::test]
async fn jobs_endpoints_include_planned_progress_fields_and_invariants() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &created_at,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let progress = serde_json::json!({
        "phase": "check",
        "message": "scheduled",
        "current": 2,
        "total": 5,
        "percent": 40,
        "plannedCurrent": 4,
        "plannedTotal": 6,
        "plannedPercent": 67,
        "currentTarget": "svc-web",
        "updatedAt": created_at,
    });
    state.db.set_job_progress(&job_id, &progress).await.unwrap();

    let detail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), 200);
    let detail = response_json(detail_resp).await;
    let detail_progress = &detail["job"]["progress"];
    assert_eq!(detail_progress["plannedCurrent"].as_u64().unwrap(), 4);
    assert_eq!(detail_progress["plannedTotal"].as_u64().unwrap(), 6);
    assert_eq!(detail_progress["plannedPercent"].as_u64().unwrap(), 67);
    assert!(
        detail_progress["plannedCurrent"].as_u64().unwrap()
            >= detail_progress["current"].as_u64().unwrap()
    );
    assert!(
        detail_progress["plannedTotal"].as_u64().unwrap()
            >= detail_progress["total"].as_u64().unwrap()
    );

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list = response_json(list_resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .cloned()
        .expect("job not in list");
    assert_eq!(item["progress"]["plannedCurrent"].as_u64().unwrap(), 4);
    assert_eq!(item["progress"]["plannedTotal"].as_u64().unwrap(), 6);
    assert_eq!(item["progress"]["plannedPercent"].as_u64().unwrap(), 67);
    assert!(
        item["progress"]["plannedCurrent"].as_u64().unwrap()
            >= item["progress"]["current"].as_u64().unwrap()
    );
    assert!(
        item["progress"]["plannedTotal"].as_u64().unwrap()
            >= item["progress"]["total"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn jobs_endpoints_preserve_explicit_null_planned_percent() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Stack,
        Some("stack-prod".to_string()),
        None,
        &created_at,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let progress = serde_json::json!({
        "phase": "apply",
        "message": "pulling image for web",
        "current": 0,
        "total": 1,
        "percent": 15,
        "plannedCurrent": 0,
        "plannedTotal": 1,
        "plannedPercent": null,
        "currentTarget": "stack-prod",
        "updatedAt": created_at,
    });
    state.db.set_job_progress(&job_id, &progress).await.unwrap();

    let detail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), 200);
    let detail = response_json(detail_resp).await;
    let detail_progress = &detail["job"]["progress"];
    assert_eq!(detail_progress["plannedCurrent"].as_u64().unwrap(), 0);
    assert_eq!(detail_progress["plannedTotal"].as_u64().unwrap(), 1);
    assert!(
        detail_progress
            .get("plannedPercent")
            .is_some_and(|v| v.is_null()),
        "explicit null plannedPercent should survive detail serialization"
    );

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list = response_json(list_resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .cloned()
        .expect("job not in list");
    assert_eq!(item["progress"]["plannedCurrent"].as_u64().unwrap(), 0);
    assert_eq!(item["progress"]["plannedTotal"].as_u64().unwrap(), 1);
    assert!(
        item["progress"]
            .get("plannedPercent")
            .is_some_and(|v| v.is_null()),
        "explicit null plannedPercent should survive list serialization"
    );
}

#[tokio::test]
async fn jobs_events_stream_emits_job_event_for_new_event_log() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );

    let mut body = resp.into_body();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({
                    "type": "job_progress",
                    "jobId": job_id.clone(),
                    "phase": "scan",
                    "message": "in progress",
                    "current": 1,
                    "total": 2,
                    "percent": 50,
                })
                .to_string(),
            },
        )
        .await
        .unwrap();

    let evt = wait_for_sse_event(&mut body, "job_event", Duration::from_secs(3)).await;
    assert!(evt.id.is_some(), "SSE event should include id");
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["jobId"].as_str().unwrap(), job_id);
    assert_eq!(payload["type"].as_str().unwrap(), "job_progress");
}
