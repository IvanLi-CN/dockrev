#[tokio::test]
async fn resolved_tag_inference_matches_platform_digest_and_clears_noop_candidate() {
    let runner: Arc<PlatformDigestRunner> = Arc::new(PlatformDigestRunner::default());
    let state = test_state_with(":memory:", Arc::new(DualDigestRegistry), runner.clone()).await;
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
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let mut detail = serde_json::json!({});
    for _ in 0..120 {
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
        detail = response_json(resp).await;
        let status = detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("");
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let svc = &detail["stack"]["services"][0];
    let image = &svc["image"];

    let digest = image["digest"].as_str().unwrap_or("<none>");
    let resolved = image["resolvedTag"].as_str().unwrap_or("<none>");
    assert_eq!(digest, "sha256:plat", "unexpected stack detail: {detail}");
    assert_eq!(resolved, "5.3.0", "unexpected stack detail: {detail}");
    assert!(
        svc["candidate"].is_null(),
        "expected candidate to be cleared when digest matches: {detail}"
    );
}

#[tokio::test]
async fn webhook_trigger_update_creates_job() {
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

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "targets": [],
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
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
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };
    assert_eq!(job["job"]["id"].as_str().unwrap(), job_id);
    assert_eq!(job["job"]["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["type"].as_str().unwrap(), "update");
    assert_eq!(job["job"]["summary"]["mode"].as_str().unwrap(), "apply");
    assert!(job["job"]["finishedAt"].as_str().unwrap().len() > 10);
}

#[tokio::test]
async fn webhook_trigger_update_requires_targets() {
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
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "service",
        "serviceId": svc.id,
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");
}

#[tokio::test]
async fn webhook_update_skips_semver_downgrade_anomaly_candidates() {
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
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "targets": [],
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
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
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };

    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert_eq!(update["changedServices"].as_u64(), Some(0));
    assert_eq!(
        update["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(
        update["skippedVersionAnomaly"][0]["serviceId"].as_str(),
        Some(svc.id.as_str())
    );
}

#[tokio::test]
async fn webhook_update_anomaly_only_skips_backup_when_no_actionable_services() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
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
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "targets": [],
        "allowArchMismatch": false,
        "backupMode": "inherit"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
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
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };

    assert_eq!(job["job"]["status"].as_str(), Some("success"));
    let stack = &job["job"]["summary"]["stacks"][0];
    assert_eq!(stack["backup"]["status"].as_str(), Some("skipped"));
    assert_eq!(
        stack["backup"]["reason"].as_str(),
        Some("no_actionable_services_after_anomaly_skip")
    );
    assert_eq!(stack["update"]["changedServices"].as_u64(), Some(0));
    assert_eq!(
        stack["update"]["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
}

#[tokio::test]
async fn webhook_update_failure_summary_keeps_skipped_anomaly() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  api:
    image: ghcr.io/acme/api:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc_web = services.iter().find(|svc| svc.name == "web").unwrap();
    let svc_api = services.iter().find(|svc| svc.name == "api").unwrap();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc_web.id,
            Some("sha256:cur-web".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand-web".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc_api.id,
            Some("sha256:cur-api".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.3.2".to_string()),
            Some("sha256:cand-api".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "targets": [{
            "serviceId": svc_api.id,
            "targetTag": "latest",
            "targetDigest": "sha256:cand-api",
            "pullTags": []
        }],
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
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
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };

    assert_eq!(job["job"]["status"].as_str(), Some("failed"));
    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert!(
        update["error"]
            .as_str()
            .unwrap_or_default()
            .contains("command failed"),
        "unexpected update summary: {update}"
    );
    assert_eq!(
        update["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(
        update["skippedVersionAnomaly"][0]["serviceId"].as_str(),
        Some(svc_web.id.as_str())
    );
}

#[tokio::test]
async fn webhook_update_failure_summary_includes_retry_details_for_idempotent_steps() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(SemverRetryFailRunner::default()),
    )
    .await;
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
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.3.2".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "targets": [{
            "serviceId": svc.id,
            "targetTag": "latest",
            "targetDigest": "sha256:cand",
            "pullTags": ["0.7.7"]
        }],
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..300 {
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
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        out.expect("job did not finish in time")
    };

    assert_eq!(job["job"]["status"].as_str(), Some("success"));
    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert_eq!(update["changedServices"].as_u64(), Some(1));
    assert_eq!(
        update["oldDigests"][svc.id.as_str()].as_str(),
        Some("sha256:old")
    );
    assert_eq!(
        update["newDigests"][svc.id.as_str()].as_str(),
        Some("sha256:new")
    );
    assert_eq!(
        update["finalDigests"][svc.id.as_str()].as_str(),
        Some("sha256:new")
    );
    assert_eq!(
        update["targetTagsPulled"],
        json!(["ghcr.io/acme/web:latest"])
    );
    assert_eq!(update["pullTagsPulled"], json!([]));
    let warnings = update["pullTagWarnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["serviceId"].as_str(), Some(svc.id.as_str()));
    assert_eq!(
        warnings[0]["tagRef"].as_str(),
        Some("ghcr.io/acme/web:0.7.7")
    );
    assert_eq!(warnings[0]["step"].as_str(), Some("pull_tag"));
    assert_eq!(warnings[0]["retry"]["attempts"].as_u64(), Some(3));
    assert_eq!(warnings[0]["retry"]["maxAttempts"].as_u64(), Some(3));
    assert_eq!(warnings[0]["retry"]["baseMs"].as_u64(), Some(300));
    assert_eq!(warnings[0]["retry"]["maxMs"].as_u64(), Some(3000));
    assert!(
        warnings[0]["lastError"]
            .as_str()
            .unwrap_or_default()
            .contains("status=1"),
        "unexpected update summary: {update}"
    );
}

#[tokio::test]
async fn update_apply_healthcheck_rollback_exposes_attempted_and_final_digests_via_api() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(HealthRollbackUpdateRunner::default()),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-update-health-rollback-{}.yml",
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("5.2".to_string()),
            Some("5.2".to_string()),
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

    let update = serde_json::json!({
        "scope": "service",
        "stackId": stack_id,
        "serviceId": service.id,
        "targetTag": "5.2",
        "targetDigest": "sha256:new",
        "pullTags": [],
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "skip",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.status, "rolled_back");

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
    let payload = response_json(resp).await;

    assert_eq!(payload["job"]["status"].as_str(), Some("rolled_back"));
    assert_eq!(
        payload["job"]["progress"]["message"].as_str(),
        Some("update rolled back after healthcheck failure")
    );
    let update = &payload["job"]["summary"]["stacks"][0]["update"];
    assert_eq!(update["failureStep"].as_str(), Some("healthcheck"));
    assert_eq!(
        update["newDigests"][service.id.as_str()].as_str(),
        Some("sha256:new")
    );
    assert_eq!(
        update["finalDigests"][service.id.as_str()].as_str(),
        Some("sha256:old")
    );
    assert_eq!(update["rollback"]["trigger"].as_str(), Some("healthcheck"));
    assert_eq!(
        update["rollback"]["toDigests"][service.id.as_str()].as_str(),
        Some("sha256:old")
    );
}

#[tokio::test]
async fn service_rollback_target_matches_successful_service_update_history() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let source_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        "2026-04-05T00:01:00Z",
        "2026-04-05T00:02:00Z",
        make_update_history_summary_for_test(&stack_id, &service_id, "sha256:old", "sha256:new"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/rollback-target"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;

    assert_eq!(payload["available"].as_bool(), Some(true));
    assert_eq!(payload["currentDigest"].as_str(), Some("sha256:new"));
    assert_eq!(payload["currentDisplayTag"].as_str(), Some("5.3.0"));
    assert_eq!(payload["targetDigest"].as_str(), Some("sha256:old"));
    assert_eq!(payload["targetDisplayTag"].as_str(), Some("5.2.0"));
    assert_eq!(
        payload["sourceUpdateJobId"].as_str(),
        Some(source_job_id.as_str())
    );
    assert_eq!(
        payload["sourceFinishedAt"].as_str(),
        Some("2026-04-05T00:02:00Z")
    );
    assert!(payload["unavailableReason"].is_null());
}

#[tokio::test]
async fn service_rollback_target_keeps_strict_semver_raw_tag_when_snapshot_has_newer_alias() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-strict-rollback-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    let now = "2026-04-05T00:00:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:new".to_string()),
            None,
            None,
            Some("5.2.0".to_string()),
            None,
            None,
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        now,
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

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{}/rollback-target", service.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;

    assert_eq!(payload["currentDisplayTag"].as_str(), Some("5.2.0"));
}

#[tokio::test]
async fn service_rollback_target_skips_incomplete_snapshot_fallback_for_target_display_tag() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-incomplete-rollback-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    let now = "2026-04-05T00:00:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:new".to_string()),
            Some("5.3.0".to_string()),
            Some(serde_json::to_string(&vec!["5.3.0"]).unwrap()),
            Some("5.2".to_string()),
            None,
            None,
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        now,
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
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        now,
        vec!["5.2.0".to_string(), "5.2".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 1,
            manifests_ok: 1,
            manifests_timeout: 1,
            manifests_error: 0,
        },
    )
    .await;
    let source_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service.id),
        "2026-04-05T00:01:00Z",
        "2026-04-05T00:02:00Z",
        make_update_history_summary_for_test(&stack_id, &service.id, "sha256:old", "sha256:new"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{}/rollback-target", service.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;

    assert_eq!(payload["available"].as_bool(), Some(true));
    assert_eq!(payload["currentDisplayTag"].as_str(), Some("5.3.0"));
    assert_eq!(payload["targetDigest"].as_str(), Some("sha256:old"));
    assert!(payload["targetDisplayTag"].is_null());
    assert_eq!(
        payload["sourceUpdateJobId"].as_str(),
        Some(source_job_id.as_str())
    );
}

#[tokio::test]
async fn service_rollback_target_matches_successful_stack_and_all_update_history() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let stack_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::Stack,
        Some(&stack_id),
        None,
        "2026-04-05T00:03:00Z",
        "2026-04-05T00:04:00Z",
        make_update_history_summary_for_test(&stack_id, &service_id, "sha256:old", "sha256:new"),
    )
    .await;
    let stack_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/rollback-target"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stack_resp.status(), 200);
    let stack_payload = response_json(stack_resp).await;
    assert_eq!(stack_payload["available"].as_bool(), Some(true));
    assert_eq!(
        stack_payload["sourceUpdateJobId"].as_str(),
        Some(stack_job_id.as_str())
    );

    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let all_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::All,
        None,
        None,
        "2026-04-05T00:05:00Z",
        "2026-04-05T00:06:00Z",
        make_update_history_summary_for_test(&stack_id, &service_id, "sha256:old", "sha256:new"),
    )
    .await;
    let all_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/rollback-target"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(all_resp.status(), 200);
    let all_payload = response_json(all_resp).await;
    assert_eq!(all_payload["available"].as_bool(), Some(true));
    assert_eq!(
        all_payload["sourceUpdateJobId"].as_str(),
        Some(all_job_id.as_str())
    );
    assert_eq!(all_payload["targetDigest"].as_str(), Some("sha256:old"));
}

#[tokio::test]
async fn service_rollback_target_reports_pending_conflict() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let conflict_id = ids::new_job_id();
    let mut conflict = crate::api::types::JobRecord::new_running(
        conflict_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        "2026-04-05T00:07:00Z",
    )
    .to_db();
    conflict.created_by = "ui".to_string();
    conflict.reason = "ui".to_string();
    state.db.insert_job(conflict).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/rollback-target"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;

    assert_eq!(payload["available"].as_bool(), Some(false));
    assert_eq!(
        payload["unavailableReason"].as_str(),
        Some("stack_update_in_progress")
    );
    assert_eq!(payload["activeJobId"].as_str(), Some(conflict_id.as_str()));
    assert_eq!(payload["activeJobStatus"].as_str(), Some("running"));
}

#[tokio::test]
async fn trigger_service_rollback_returns_conflict_without_matching_history() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (_stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/rollback"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let payload = response_json(resp).await;
    assert_eq!(payload["error"]["code"].as_str(), Some("conflict"));
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("no_matching_update_history")
    );
    assert!(payload["error"]["details"]["existingJobId"].is_null());
}

#[tokio::test]
async fn trigger_service_rollback_creates_rolled_back_job() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let source_job_id = insert_successful_update_history_job(
        &state,
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        "2026-04-05T00:08:00Z",
        "2026-04-05T00:09:00Z",
        make_update_history_summary_for_test(&stack_id, &service_id, "sha256:old", "sha256:new"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/rollback"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "rollback");
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.status, "rolled_back");
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.summary_json["mode"].as_str(), Some("rollback"));
    assert_eq!(
        job.summary_json["progress"]["message"].as_str(),
        Some("rollback finished")
    );
    assert_eq!(
        job.summary_json["sourceUpdateJobId"].as_str(),
        Some(source_job_id.as_str())
    );
    assert_eq!(
        job.summary_json["targetDigest"].as_str(),
        Some("sha256:old")
    );
    let rollback = &job.summary_json["stacks"][0]["rollback"];
    assert!(rollback["changedServices"].is_number());
    assert!(rollback["oldDigests"].is_object());
    assert!(rollback["newDigests"].is_object());
    assert!(rollback["finalDigests"].is_object());
}
