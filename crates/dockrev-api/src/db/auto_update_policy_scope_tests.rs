#[tokio::test]
async fn policy_scope_replacement_stops_attached_auto_policy_jobs() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let pending_input = |id: &str, scope_type: &str, scope_id: &str| AutoUpdatePendingInput {
        id: id.to_string(),
        policy_scope_type: scope_type.to_string(),
        policy_scope_id: scope_id.to_string(),
        rule_id: "rule".to_string(),
        stack_id: "stack".to_string(),
        service_id: "service".to_string(),
        source_check_job_id: "check".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.4.0".to_string(),
        candidate_digest:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        current_display_tag: "1.0.0".to_string(),
        first_seen_at: "2026-04-30T00:00:00Z".to_string(),
        due_at: "2026-04-30T00:00:00Z".to_string(),
        min_age_seconds: 0,
        min_version_lag: 0,
        summary_json: serde_json::json!({}),
        candidate_id: None,
    };
    let insert_job = |id: &str, status: &str| {
        let db = db.clone();
        let id = id.to_string();
        let status = status.to_string();
        async move {
            db.insert_job(crate::api::types::JobListItem {
                id,
                r#type: crate::api::types::JobType::Update,
                scope: crate::api::types::JobScope::Service,
                stack_id: Some("stack".to_string()),
                service_id: Some("service".to_string()),
                status,
                created_by: "auto-policy".to_string(),
                reason: "auto_policy".to_string(),
                created_at: "2026-04-30T00:00:00Z".to_string(),
                started_at: None,
                finished_at: None,
                allow_arch_mismatch: false,
                backup_mode: "inherit".to_string(),
                summary_json: serde_json::json!({}),
            })
            .await
            .unwrap();
        }
    };

    let queued = db
        .reserve_auto_update_pending(
            &pending_input("pending-queued-old", "stack", "stack"),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    assert!(db
        .try_claim_auto_update_pending(&queued.id, "2026-04-30T00:00:01Z")
        .await
        .unwrap());
    insert_job("queued-policy-job", "queued").await;
    assert!(db
        .mark_auto_update_pending_enqueued(
            &queued.id,
            "queued-policy-job",
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap());

    let running = db
        .reserve_auto_update_pending(
            &pending_input("pending-running-old", "service", "service"),
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap();
    assert_eq!(
        db.get_auto_update_pending_by_id(&queued.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "skipped"
    );
    assert_eq!(
        db.get_job("queued-policy-job")
            .await
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
    assert!(db
        .try_claim_auto_update_pending(&running.id, "2026-04-30T00:00:04Z")
        .await
        .unwrap());
    insert_job("running-policy-job", "running").await;
    assert!(db
        .mark_auto_update_pending_enqueued(
            &running.id,
            "running-policy-job",
            "2026-04-30T00:00:05Z",
        )
        .await
        .unwrap());

    let current = db
        .reserve_auto_update_pending(
            &pending_input("pending-current", "stack", "stack-new"),
            "2026-04-30T00:00:06Z",
        )
        .await
        .unwrap();
    assert_eq!(current.status, "pending");
    assert_eq!(
        db.get_auto_update_pending_by_id(&running.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "skipped"
    );
    assert_eq!(
        db.get_job("running-policy-job")
            .await
            .unwrap()
            .unwrap()
            .status,
        "running"
    );
    assert_eq!(
        db.get_update_stop_control("running-policy-job")
            .await
            .unwrap()
            .unwrap()
            .stop_requested_by
            .as_deref(),
        Some("auto-policy-policy-change")
    );
}

#[tokio::test]
async fn policy_scope_reservation_reuses_case_insensitive_trimmed_scope() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let pending_input = |id: &str, scope_type: &str, scope_id: &str| AutoUpdatePendingInput {
        id: id.to_string(),
        policy_scope_type: scope_type.to_string(),
        policy_scope_id: scope_id.to_string(),
        rule_id: "rule".to_string(),
        stack_id: "stack".to_string(),
        service_id: "service".to_string(),
        source_check_job_id: "check".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.4.0".to_string(),
        candidate_digest:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        current_display_tag: "1.0.0".to_string(),
        first_seen_at: "2026-04-30T00:00:00Z".to_string(),
        due_at: "2026-04-30T00:00:00Z".to_string(),
        min_age_seconds: 0,
        min_version_lag: 0,
        summary_json: serde_json::json!({}),
        candidate_id: None,
    };

    let original = db
        .reserve_auto_update_pending(
            &pending_input("pending-original", "stack", "stack-id"),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let reused = db
        .reserve_auto_update_pending(
            &pending_input("pending-duplicate", " STACK ", " Stack-Id "),
            "2026-04-30T00:00:01Z",
        )
        .await
        .unwrap();

    assert_eq!(reused.id, original.id);
    assert_eq!(reused.policy_scope_type, "stack");
    assert_eq!(reused.policy_scope_id, "stack-id");
    assert_eq!(
        db.list_auto_update_pending_candidates("2026-04-30T00:00:02Z", 10)
            .await
            .unwrap()
            .len(),
        1
    );
}
