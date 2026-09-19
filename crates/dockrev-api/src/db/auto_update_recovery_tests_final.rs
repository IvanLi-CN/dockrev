#[tokio::test]
async fn corrupt_auto_policy_job_is_terminally_skipped() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &candidate_input(
                "candidate-corrupt",
                "sha256:corrupt",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-corrupt".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:corrupt".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({}),
                candidate_id: Some(candidate.id),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "corrupt-auto-policy-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "queued".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:01Z".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    assert!(
        db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:02Z")
            .await
            .unwrap()
    );
    assert!(
        db.mark_auto_update_pending_enqueued(
            &pending.id,
            "corrupt-auto-policy-job",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap()
    );

    assert!(
        db.fail_corrupt_auto_update_job(
            "corrupt-auto-policy-job",
            "2026-04-30T00:01:00Z",
            "invalid_job_summary",
        )
        .await
        .unwrap()
    );
    assert_eq!(
        db.get_job("corrupt-auto-policy-job")
            .await
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    assert_eq!(
        db.get_auto_update_pending_by_id("pending-corrupt")
            .await
            .unwrap()
            .unwrap()
            .status,
        "skipped"
    );
    assert_eq!(
        db.get_auto_update_candidate("service", "sha256:corrupt")
            .await
            .unwrap()
            .unwrap()
            .policy_status
            .as_deref(),
        Some("failed")
    );
}

#[tokio::test]
async fn recovered_auto_policy_job_reopens_its_pending_candidate() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &candidate_input(
                "candidate-restart",
                "sha256:restart",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-restart".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:restart".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({}),
                candidate_id: Some(candidate.id),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "restart-auto-policy-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "running".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:01Z".to_string(),
        started_at: Some("2026-04-30T00:00:02Z".to_string()),
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    assert!(
        db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:03Z")
            .await
            .unwrap()
    );
    assert!(
        db.mark_auto_update_pending_enqueued(
            &pending.id,
            "restart-auto-policy-job",
            "2026-04-30T00:00:04Z",
        )
        .await
        .unwrap()
    );

    let recovered = db
        .recover_incomplete_jobs("2026-04-30T00:01:00Z", "server_restart")
        .await
        .unwrap();
    assert_eq!(recovered, ["restart-auto-policy-job"]);
    assert_eq!(
        db.reopen_auto_update_pending_for_recovered_jobs(
            &recovered,
            "2026-04-30T00:01:00Z",
        )
        .await
        .unwrap(),
        1
    );

    let pending = db
        .get_auto_update_pending_by_id("pending-restart")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.update_job_id, None);
    let candidate = db
        .get_auto_update_candidate("service", "sha256:restart")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.policy_status.as_deref(), Some("delayed"));
    assert_eq!(
        candidate.policy_reason.as_deref(),
        Some("update_job_recovered")
    );
}
