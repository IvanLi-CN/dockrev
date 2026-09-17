#[tokio::test]
async fn lists_only_enqueued_auto_policy_jobs_for_recovery() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-recovery".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:recovery".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({}),
                candidate_id: None,
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    assert!(
        db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:01Z")
            .await
            .unwrap()
    );
    db.insert_job(crate::api::types::JobListItem {
        id: "recovery-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "queued".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:02Z".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({
            "mode": "apply",
            "targets": []
        }),
    })
    .await
    .unwrap();
    assert!(
        db.mark_auto_update_pending_enqueued(
            &pending.id,
            "recovery-job",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap()
    );

    let jobs = db.list_enqueued_auto_update_jobs(10).await.unwrap();
    assert_eq!(
        jobs.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(),
        ["recovery-job"]
    );
}
#[tokio::test]
async fn candidate_settlement_is_unique_and_idempotent() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let input = candidate_input(
        "candidate-1",
        "sha256:new",
        "awaiting_inference",
        "2026-04-30T00:00:00Z",
    );
    let first = db
        .upsert_auto_update_candidate(&input, "2026-04-30T00:00:00Z")
        .await
        .unwrap();
    let second = db
        .upsert_auto_update_candidate(&input, "2026-04-30T00:01:00Z")
        .await
        .unwrap();
    assert_eq!(first.id, second.id);

    let settled = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:new".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            reason: Some("digest_bound_version".to_string()),
            attempts: 0,
            retry_at: None,
            settled_at: Some("2026-04-30T00:02:00Z".to_string()),
            now: "2026-04-30T00:02:00Z".to_string(),
        })
        .await
        .unwrap()
        .expect("first settlement changes the row");
    assert_eq!(settled.status, "ready");
    assert_eq!(settled.resolved_version.as_deref(), Some("1.4.0"));

    let repeated = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:new".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            reason: Some("digest_bound_version".to_string()),
            attempts: 0,
            retry_at: None,
            settled_at: Some("2026-04-30T00:02:00Z".to_string()),
            now: "2026-04-30T00:03:00Z".to_string(),
        })
        .await
        .unwrap();
    assert!(repeated.is_none(), "repeated settlement must be a no-op");
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
async fn pending_claim_rejects_a_service_candidate_that_changed_after_preflight() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:new', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-claim",
            "sha256:old",
            "ready",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:old",
        "delayed",
        Some("policy_matched"),
        Some("rule"),
        "2026-04-30T00:00:01Z",
    )
    .await
    .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-claim".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:old".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({}),
                candidate_id: Some("service:sha256:old".to_string()),
            },
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap();
    assert!(!db
        .try_claim_auto_update_pending_if_current(
            &pending.id,
            "service",
            "sha256:old",
            "stack",
            "stack",
            "rule",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap());
}

#[tokio::test]
async fn completed_candidate_remains_visible_after_service_candidate_is_cleared() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:completed', NULL, 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-completed",
            "sha256:completed",
            "ready",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:completed",
        "completed",
        Some("update_job_completed"),
        Some("rule"),
        "2026-04-30T00:01:00Z",
    )
    .await
    .unwrap();

    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-completed",
            "sha256:completed",
            "ready",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:02:00Z",
    )
    .await
    .unwrap();

    let rows = db
        .list_latest_auto_update_candidates(&["service".to_string()])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].policy_status.as_deref(), Some("completed"));
    assert!(db
        .list_auto_update_candidates_for_policy_reconciliation(50)
        .await
        .unwrap()
        .is_empty());
}
