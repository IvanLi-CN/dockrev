#[tokio::test]
async fn schedule_auto_policy_rechecks_preexisting_candidate_from_tag_observation() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!("/tmp/dockrev-auto-policy-reobserved-{}.yml", ulid::Ulid::new());
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
    let current_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000001";
    let candidate_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000002";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some(current_digest.to_string()),
            Some("1.0.0".to_string()),
            Some(r#"["1.0.0"]"#.to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some(candidate_digest.to_string()),
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
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let summary = json!({
        "newVersions": { "count": 0, "services": [] },
        "configuredTagObservations": [{
            "serviceId": service.id,
            "imageRepo": "ghcr.io/acme/web",
            "configuredTag": "latest",
            "digest": candidate_digest,
            "version": "1.1.0",
            "observedAt": now,
        }],
    });
    insert_schedule_check_job(&state, "chk_schedule_reobserved", &now).await;
    crate::auto_update::handle_completed_check(
        &state,
        "chk_schedule_reobserved",
        "schedule",
        &now,
        &summary,
    )
    .await
    .unwrap();

    let auto_jobs = state
        .db
        .list_jobs()
        .await
        .unwrap()
        .into_iter()
        .filter(|job| job.created_by == "auto-policy" && job.reason == "auto_policy")
        .collect::<Vec<_>>();
    assert_eq!(auto_jobs.len(), 1, "auto policy jobs: {auto_jobs:?}");
    assert_eq!(
        auto_jobs[0].summary_json["targets"][0]["targetDigest"].as_str(),
        Some(candidate_digest)
    );
}

#[tokio::test]
async fn schedule_auto_policy_ignores_observations_that_do_not_match_service_candidate() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(UpdateAndRuntimeScanRunner::new()),
    )
    .await;
    let now = test_now_rfc3339();
    let compose_path = format!("/tmp/dockrev-auto-policy-observation-guard-{}.yml", ulid::Ulid::new());
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
    let current_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000001";
    let candidate_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000002";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some(current_digest.to_string()),
            Some("1.0.0".to_string()),
            Some(r#"["1.0.0"]"#.to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some(candidate_digest.to_string()),
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
        .put_auto_update_policy(
            "stack",
            &stack_id,
            &immediate_stack_auto_update_policy(),
            &now,
        )
        .await
        .unwrap();

    let mismatches = [
        json!({
            "serviceId": service.id,
            "imageRepo": "ghcr.io/other/web",
            "configuredTag": "latest",
            "digest": candidate_digest,
            "version": "1.1.0",
        }),
        json!({
            "serviceId": service.id,
            "imageRepo": "ghcr.io/acme/web",
            "configuredTag": "stable",
            "digest": candidate_digest,
            "version": "1.1.0",
        }),
        json!({
            "serviceId": service.id,
            "imageRepo": "ghcr.io/acme/web",
            "configuredTag": "latest",
            "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000003",
            "version": "1.2.0",
        }),
    ];
    for (index, observation) in mismatches.into_iter().enumerate() {
        let job_id = format!("chk_schedule_mismatch_{index}");
        let summary = json!({
            "newVersions": { "count": 0, "services": [] },
            "configuredTagObservations": [observation],
        });
        insert_schedule_check_job(&state, &job_id, &now).await;
        crate::auto_update::handle_completed_check(&state, &job_id, "schedule", &now, &summary)
            .await
            .unwrap();
    }

    assert!(
        state
            .db
            .list_jobs()
            .await
            .unwrap()
            .iter()
            .all(|job| job.reason != "auto_policy")
    );
}
