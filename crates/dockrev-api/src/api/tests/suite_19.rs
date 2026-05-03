#[tokio::test]
async fn service_scope_update_skips_dockrev_image() {
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
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:0.5.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "dockrev", &compose_path).await;
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
        Some(crate::service_check::RuntimeServiceObservation::digest_only(
            "sha256:old".to_string(),
        )),
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

    let payload = serde_json::json!({
        "scope": "service",
        "stackId": stack_id,
        "serviceId": service_id,
        "targetTag": svc.image_tag,
        "targetDigest": expected_digest,
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
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();
    let job = wait_for_job_terminal(&state, &job_id).await;

    assert_eq!(job.status, "success");
    let summary = job.summary_json;
    assert_eq!(summary["stacks"][0]["backup"]["reason"], "no_actionable_services");
    assert_eq!(summary["stacks"][0]["update"]["changedServices"], 0);
}
