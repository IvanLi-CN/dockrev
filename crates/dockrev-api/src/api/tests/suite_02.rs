#[tokio::test]
async fn service_candidates_endpoint_is_removed() {
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
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/candidates"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn service_update_conflicts_when_target_digest_mismatches_latest_scan() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestOnlyUpdateRegistry),
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
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let expected_digest = detail["stack"]["services"][0]["candidate"]["digest"]
        .as_str()
        .unwrap()
        .to_string();

    let missing_tag = serde_json::json!({
        "scope": "service",
        "serviceId": service_id.clone(),
        "targetDigest": expected_digest,
        "pullTags": [],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(missing_tag.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");

    let wrong_tag = serde_json::json!({
        "scope": "service",
        "serviceId": service_id.clone(),
        "targetTag": "cross-tag-not-allowed",
        "targetDigest": expected_digest.clone(),
        "pullTags": [],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(wrong_tag.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");

    let bad = serde_json::json!({
        "scope": "service",
        "serviceId": service_id,
        "targetTag": svc.image_tag,
        "targetDigest": "sha256:wrong",
        "pullTags": [],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(bad.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "conflict");

    let ok = serde_json::json!({
        "scope": "service",
        "serviceId": svc.id,
        "targetTag": svc.image_tag,
        "targetDigest": expected_digest,
        "pullTags": [],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(ok.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    assert!(triggered["jobId"].as_str().unwrap().starts_with("job_"));

    let legacy_targets = serde_json::json!({
        "scope": "service",
        "serviceId": svc.id,
        "targets": [{
            "serviceId": svc.id,
            "targetTag": svc.image_tag,
            "targetDigest": expected_digest,
            "pullTags": []
        }],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(legacy_targets.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    assert!(triggered["jobId"].as_str().unwrap().starts_with("job_"));
}

#[tokio::test]
async fn get_stack_roundtrips_homepage_metadata_from_compose_labels() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
    labels:
      - homepage.group=Developer
      - homepage.name=Acme API
      - homepage.icon=si-github
      - homepage.href=https://api.example.com
      - homepage.description=Primary API
      - homepage.widget.type=custom
  grafana:
    image: grafana/grafana:11
    labels:
      homepage.group: Monitoring
      homepage.name: Grafana
      homepage.icon: mdi-chart-timeline-variant
      homepage.href: https://grafana.example.com
      homepage.description: Dashboards
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
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let services = body["stack"]["services"]
        .as_array()
        .expect("stack services array");
    let web = services
        .iter()
        .find(|item| item["name"].as_str() == Some("web"))
        .expect("web service");
    let grafana = services
        .iter()
        .find(|item| item["name"].as_str() == Some("grafana"))
        .expect("grafana service");

    assert_eq!(web["homepage"]["group"].as_str(), Some("Developer"));
    assert_eq!(web["homepage"]["name"].as_str(), Some("Acme API"));
    assert_eq!(web["homepage"]["icon"].as_str(), Some("si-github"));
    assert_eq!(web["homepage"]["href"].as_str(), Some("https://api.example.com"));
    assert_eq!(
        web["homepage"]["description"].as_str(),
        Some("Primary API")
    );
    assert!(
        web["homepage"]["widget"].is_null(),
        "widget metadata should stay out of API payloads: {}",
        web
    );

    assert_eq!(grafana["homepage"]["group"].as_str(), Some("Monitoring"));
    assert_eq!(grafana["homepage"]["name"].as_str(), Some("Grafana"));
    assert_eq!(
        grafana["homepage"]["icon"].as_str(),
        Some("mdi-chart-timeline-variant")
    );
    assert_eq!(
        grafana["homepage"]["href"].as_str(),
        Some("https://grafana.example.com")
    );
    assert_eq!(
        grafana["homepage"]["description"].as_str(),
        Some("Dashboards")
    );
}

#[tokio::test]
async fn get_stack_hides_legacy_update_guard_metadata_even_when_compose_has_traefik_labels() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  edge:
    image: ghcr.io/acme/edge:5.2
    labels:
      - traefik.http.routers.edge.rule=Host(`edge.example.com`)
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
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let service = &body["stack"]["services"][0];
    assert_eq!(service["name"].as_str(), Some("edge"));
    assert!(service.get("updateGuard").is_none());
}

async fn seed_guarded_service_with_candidate(
    state: &Arc<AppState>,
) -> (String, String, String, String) {
    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
    labels:
      - traefik.http.routers.web.rule=Host(`api.example.com`)
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("5.2".to_string()),
            Some(r#"["5.2"]"#.to_string()),
            Some("5.2".to_string()),
            Some("5.2.3".to_string()),
            Some("sha256:candidate".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    (
        stack_id,
        svc.id,
        svc.image_tag,
        "sha256:candidate".to_string(),
    )
}

#[tokio::test]
async fn service_apply_accepts_traefik_labeled_service_and_dry_run_still_works() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let (stack_id, service_id, target_tag, target_digest) =
        seed_guarded_service_with_candidate(&state).await;

    let apply_req = serde_json::json!({
        "scope": "service",
        "stackId": stack_id,
        "serviceId": service_id,
        "targetTag": target_tag,
        "targetDigest": target_digest,
        "pullTags": [],
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(apply_req.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert!(body["jobId"].as_str().unwrap().starts_with("job_"));

    let dry_run_req = serde_json::json!({
        "scope": "service",
        "stackId": stack_id,
        "serviceId": service_id,
        "targetTag": target_tag,
        "targetDigest": target_digest,
        "pullTags": [],
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(dry_run_req.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert!(body["jobId"].as_str().unwrap().starts_with("job_"));
}

#[tokio::test]
async fn register_stack_then_check_updates() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
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

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["id"].as_str().unwrap(), stack_id.as_str());

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id.clone(),
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
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn check_progress_event_includes_planned_fields() {
    let registry = Arc::new(StaggeredCheckRegistry::new(Duration::from_millis(900)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
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

    let sse_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{check_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sse_resp.status(), 200);

    let mut body = sse_resp.into_body();
    let evt = wait_for_sse_event(&mut body, "job_progress", Duration::from_secs(5)).await;
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert!(payload["plannedCurrent"].is_number());
    assert!(payload["plannedTotal"].is_number());
    assert!(payload["plannedPercent"].is_number());
}

#[tokio::test]
async fn get_stack_reports_new_version_discovery_count_by_visible_version() {
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
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/web:latest",
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "1.16.1",
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
                "1.16.1",
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
                "1.16.2",
                "sha256:candidate-c",
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
async fn service_new_version_discovery_timeline_returns_candidate_history_and_runtime_started_at() {
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
    let running_started_at = test_offset_rfc3339(&now, -time::Duration::hours(4));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:current-v1".to_string()),
            Some(running_started_at.clone()),
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

    let historical_discovered_at = test_offset_rfc3339(&now, time::Duration::minutes(-90));
    let current_candidate_discovered_at = test_offset_rfc3339(&now, time::Duration::minutes(-30));

    let historical_job_id = insert_check_job(&state, "schedule", &historical_discovered_at).await;
    state
        .db
        .finish_job(
            &historical_job_id,
            "success",
            &historical_discovered_at,
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "1.16.1",
                "sha256:candidate-a",
            ),
        )
        .await
        .unwrap();
    let current_job_id =
        insert_check_job(&state, "schedule", &current_candidate_discovered_at).await;
    state
        .db
        .finish_job(
            &current_job_id,
            "success",
            &current_candidate_discovered_at,
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "1.16.2",
                "sha256:live-candidate",
            ),
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
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("1.16.2"));
    assert_eq!(
        items[0]["occurredAt"].as_str(),
        Some(current_candidate_discovered_at.as_str())
    );
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("1.16.1"));
    assert_eq!(
        items[1]["occurredAt"].as_str(),
        Some(historical_discovered_at.as_str())
    );
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("1.16.0"));
    assert_eq!(
        items[2]["occurredAt"].as_str(),
        Some(running_started_at.as_str())
    );
}

#[tokio::test]
async fn service_new_version_discovery_timeline_keeps_history_when_live_candidate_has_no_row() {
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
    let running_started_at = test_offset_rfc3339(&now, -time::Duration::hours(4));
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service_id,
            Some("sha256:current-v1".to_string()),
            Some(running_started_at.clone()),
            Some("1.16.0".to_string()),
            Some("[\"1.16.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.16.3".to_string()),
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

    let older_discovered_at = test_offset_rfc3339(&now, time::Duration::minutes(-120));
    let newer_discovered_at = test_offset_rfc3339(&now, time::Duration::minutes(-45));

    let older_job_id = insert_check_job(&state, "schedule", &older_discovered_at).await;
    state
        .db
        .finish_job(
            &older_job_id,
            "success",
            &older_discovered_at,
            &make_new_version_summary_for_test(
                &service_id,
                "latest",
                "1.16.0",
                "sha256:current-v1",
                "latest",
                "1.16.1",
                "sha256:candidate-a",
            ),
        )
        .await
        .unwrap();
    let newer_job_id = insert_check_job(&state, "schedule", &newer_discovered_at).await;
    state
        .db
        .finish_job(
            &newer_job_id,
            "success",
            &newer_discovered_at,
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
    assert_eq!(items.len(), 4);
    assert_eq!(items[0]["kind"].as_str(), Some("currentCandidate"));
    assert_eq!(items[0]["version"].as_str(), Some("1.16.3"));
    assert_eq!(items[0]["occurredAt"].as_str(), None);
    assert_eq!(items[1]["version"].as_str(), Some("1.16.2"));
    assert_eq!(items[2]["version"].as_str(), Some("1.16.1"));
    assert_eq!(items[3]["kind"].as_str(), Some("currentRunning"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_normalizes_live_candidate_from_notifications() {
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

    let discovered_at = test_offset_rfc3339(&now, time::Duration::minutes(-30));
    let job_id = insert_check_job(&state, "schedule", &discovered_at).await;
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &discovered_at,
            &make_new_version_summary_for_test_with_image_ref(
                &service_id,
                "ghcr.io/acme/web:latest",
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
        &test_offset_rfc3339(&now, time::Duration::minutes(-10)),
    )
    .await;

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
    assert_eq!(
        items[0]["occurredAt"].as_str(),
        Some(discovered_at.as_str())
    );
    assert_eq!(items[1]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[1]["version"].as_str(), Some("1.16.0"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_collapses_repeated_alias_history() {
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
            None,
            None,
            None,
            Some("latest".to_string()),
            Some("1.18.1".to_string()),
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

    for (discovered_at, candidate_digest, candidate_display_tag) in [
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-90)),
            "sha256:candidate-a",
            "latest",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-30)),
            "sha256:candidate-b",
            "latest",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-10)),
            "sha256:live-candidate",
            "1.18.1",
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
                    "",
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
    assert_eq!(items[0]["version"].as_str(), Some("1.18.1"));
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("latest"));
    assert_eq!(
        items[1]["occurredAt"].as_str(),
        Some(test_offset_rfc3339(&now, time::Duration::minutes(-90)).as_str())
    );
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("latest"));
}

#[tokio::test]
async fn service_new_version_discovery_timeline_excludes_older_unresolved_current_alias_from_stable_baseline()
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
        Some("sha256:current-v1206"),
        Some("latest"),
        Some("sha256:live-candidate"),
    )
    .await;
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:current-v1206".to_string()),
            Some("v1.20.6".to_string()),
            Some("[\"v1.20.6\"]".to_string()),
            Some("latest".to_string()),
            Some("v1.21.1".to_string()),
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
        current_display_tag,
        current_digest,
        candidate_digest,
        candidate_display_tag,
    ) in [
        (
            test_offset_rfc3339(&now, time::Duration::days(-14)),
            "latest",
            "",
            "sha256:legacy-candidate",
            "latest",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-90)),
            "v1.20.6",
            "sha256:current-v1206",
            "sha256:candidate-v1210",
            "1.21.0",
        ),
        (
            test_offset_rfc3339(&now, time::Duration::minutes(-10)),
            "v1.20.6",
            "sha256:current-v1206",
            "sha256:live-candidate",
            "1.21.1",
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
    assert_eq!(items[0]["version"].as_str(), Some("1.21.1"));
    assert_eq!(items[1]["kind"].as_str(), Some("historicalCandidate"));
    assert_eq!(items[1]["version"].as_str(), Some("1.21.0"));
    assert_eq!(
        items[1]["occurredAt"].as_str(),
        Some(test_offset_rfc3339(&now, time::Duration::minutes(-90)).as_str())
    );
    assert_eq!(items[2]["kind"].as_str(), Some("currentRunning"));
    assert_eq!(items[2]["version"].as_str(), Some("1.20.6"));
}
