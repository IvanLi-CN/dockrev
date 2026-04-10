#[tokio::test]
async fn archived_services_stack_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  worker:
    image: ghcr.io/acme/worker:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "targets": [],
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
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

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
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn archived_services_all_update_skips_notify() {
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
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "all",
        "targets": [],
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
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

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
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn empty_new_digests_does_not_skip_notify() {
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

    // Use apply mode to produce updater summary with `newDigests: {}` (FakeRunner returns empty container id).
    // Skip backups to keep the test isolated.
    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "targets": [],
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
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                !logs
                    .iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped")),
                "notify should not be skipped just because newDigests is empty"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn update_apply_settles_service_snapshot_before_job_terminal() {
    let runner = Arc::new(UpdateAndRuntimeScanRunner::new());
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-update-settle-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-update-settle").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()[0]
        .clone();
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
    assert_eq!(job.status, "success");

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let service = stack.services.iter().find(|svc| svc.name == "web").unwrap();
    assert_eq!(service.image.digest.as_deref(), Some("sha256:new"));
    assert!(
        service.candidate.is_none(),
        "candidate should be cleared after apply settle"
    );

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("update_state_settled"))
    );
}

#[tokio::test]
async fn update_apply_settle_keeps_existing_runtime_started_at_when_inspect_time_is_missing() {
    let runner = Arc::new(UpdateAndRuntimeScanRunner::new());
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-update-settle-runtime-{}.yml",
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
    seed_discovered_project(&state, &stack_id, "demo-update-settle-runtime").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let prior_started_at = test_offset_rfc3339(&now, -time::Duration::hours(2));
    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()[0]
        .clone();
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service.id,
            Some("sha256:new".to_string()),
            Some(prior_started_at.clone()),
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
    assert_eq!(job.status, "success");

    let context = state
        .db
        .get_service_new_version_timeline_context(&service.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        context.current_runtime_started_at.as_deref(),
        Some(prior_started_at.as_str())
    );
}

#[tokio::test]
async fn update_apply_settle_clears_runtime_started_at_when_scaled_replicas_disagree() {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let runner = Arc::new(UpdateAndRuntimeScanRunner::new_with_started_ats(vec![
        test_offset_rfc3339(&now, -time::Duration::minutes(8)),
        test_offset_rfc3339(&now, time::Duration::minutes(2)),
    ]));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-update-settle-runtime-ambiguous-{}.yml",
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
    seed_discovered_project(&state, &stack_id, "demo-update-settle-runtime-ambiguous").await;

    let prior_started_at = test_offset_rfc3339(&now, -time::Duration::hours(2));
    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()[0]
        .clone();
    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &service.id,
            Some("sha256:old".to_string()),
            Some(prior_started_at),
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
    assert_eq!(job.status, "success");

    let context = state
        .db
        .get_service_new_version_timeline_context(&service.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(context.current_runtime_started_at, None);
}

#[tokio::test]
async fn webhook_trigger_check_creates_job_and_updates_stack() {
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

    let trigger = serde_json::json!({
        "action": "check",
        "scope": "stack",
        "stackId": stack_id,
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

    let mut finished = false;
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
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "webhook check job did not finish in time");

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
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);

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
    let jobs = response_json(resp).await;
    let job = jobs["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .unwrap();
    assert_eq!(job["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["type"].as_str().unwrap(), "check");
}

#[tokio::test]
async fn check_persists_registry_digest_when_runtime_digest_missing() {
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

    let mut finished = false;
    for _ in 0..50 {
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
    let digest = detail["stack"]["services"][0]["image"]["digest"]
        .as_str()
        .unwrap();
    assert_eq!(digest, "sha256:old");
}

#[tokio::test]
async fn resolved_tag_inference_does_not_skip_candidate_tag_when_candidate_digest_none() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
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
    let compose_project = state.db.get_stack_compose_project(&stack_id).await.unwrap();
    assert_eq!(compose_project.as_deref(), Some("demo"));

    let img = crate::registry::ImageRef::parse("ghcr.io/acme/web:latest").unwrap();
    let runtime = super::docker_compose_service_runtime_digest(
        &state,
        "demo",
        "web",
        &super::repo_candidates(&img),
    )
    .await
    .unwrap();
    assert_eq!(
        runtime
            .as_ref()
            .map(|observation| observation.digest.as_str()),
        Some("sha256:match")
    );

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
    let job_detail = response_json(resp).await;

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

    let image = &detail["stack"]["services"][0]["image"];
    let digest = image["digest"].as_str().unwrap_or("<none>");
    let resolved = image["resolvedTag"].as_str().unwrap_or("<none>");
    let runner_calls = runner.calls.lock().unwrap().clone();
    assert_eq!(
        digest, "sha256:match",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
    assert_eq!(
        resolved, "5.3.0",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
}

#[tokio::test]
async fn resolved_tag_inference_runs_for_major_minor_tags() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
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

    let resolved = detail["stack"]["services"][0]["image"]["resolvedTag"]
        .as_str()
        .unwrap_or("<none>");
    assert_ne!(
        resolved, "<none>",
        "expected resolvedTag for 5.2 tag: {detail}"
    );
}

#[tokio::test]
async fn candidate_resolved_tag_inference_prefers_semver_for_floating_candidate() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(CandidateResolvedTagRegistry),
        runner.clone(),
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
    let candidate = &detail["stack"]["services"][0]["candidate"];
    assert_eq!(candidate["tag"].as_str().unwrap_or("<none>"), "latest");
    assert_eq!(
        candidate["resolvedTag"].as_str().unwrap_or("<none>"),
        "v0.2.15"
    );
    assert_eq!(
        candidate["digest"].as_str().unwrap_or("<none>"),
        "sha256:new"
    );
}

