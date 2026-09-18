#[tokio::test]
async fn pending_claim_rejects_a_service_candidate_that_changed_after_preflight() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:old-current', 'sha256:old', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_policies (scope_type, scope_id, mode, enabled, rules_json, created_at, updated_at) VALUES ('stack', 'stack', 'override', 1, '[{\"id\":\"rule\",\"enabled\":true}]', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z')",
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
                summary_json: serde_json::json!({
                    "currentDigest": "sha256:old-current",
                    "policyUpdatedAt": "2026-04-30T00:00:00Z"
                }),
                candidate_id: Some("service:sha256:old".to_string()),
            },
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap();
    db.call(|conn| {
        conn.execute(
            "UPDATE services SET current_digest = 'sha256:new-current' WHERE id = 'service'",
            [],
        )?;
        Ok(())
    })
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
async fn pending_claim_rejects_an_unqualified_source_job() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:unqualified', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_policies (scope_type, scope_id, mode, enabled, rules_json, created_at, updated_at) VALUES ('stack', 'stack', 'override', 1, '[{\"id\":\"rule\",\"enabled\":true}]', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-unqualified-source",
            "sha256:unqualified",
            "ready",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:unqualified",
        "delayed",
        Some("policy_matched"),
        Some("rule"),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "unqualified-source".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "success".to_string(),
        created_by: "schedule".to_string(),
        reason: "schedule".to_string(),
        created_at: "2026-04-30T00:00:00Z".to_string(),
        started_at: None,
        finished_at: Some("2026-04-30T00:00:01Z".to_string()),
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-unqualified-source".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "unqualified-source".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:unqualified".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({
                    "currentDigest": "sha256:current",
                    "policyUpdatedAt": "2026-04-30T00:00:00Z"
                }),
                candidate_id: Some("candidate-unqualified-source".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    assert!(!db
        .try_claim_auto_update_pending_if_current(
            &pending.id,
            "service",
            "sha256:unqualified",
            "stack",
            "stack",
            "rule",
            "2026-04-30T00:00:01Z",
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

#[tokio::test]
async fn cancelled_stale_queued_job_updates_candidate_projection() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:queued', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_policies (scope_type, scope_id, mode, enabled, rules_json, created_at, updated_at) VALUES ('stack', 'stack', 'override', 1, '[{\"id\":\"rule\",\"enabled\":true}]', '2026-04-30T00:00:01Z', '2026-04-30T00:00:01Z')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &candidate_input(
                "candidate-queued",
                "sha256:queued",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:01Z",
        )
        .await
        .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:queued",
        "delayed",
        Some("policy_matched"),
        Some("rule"),
        "2026-04-30T00:00:02Z",
    )
    .await
    .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-queued".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:queued".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({
                    "policyUpdatedAt": "2026-04-30T00:00:01Z"
                }),
                candidate_id: Some(candidate.id),
            },
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap();
    assert!(db
        .try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:03Z")
        .await
        .unwrap());
    db.insert_job(crate::api::types::JobListItem {
        id: "stale-queued-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "queued".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:03Z".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    assert!(db
        .mark_auto_update_pending_enqueued(
            &pending.id,
            "stale-queued-job",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap());
    db.call(|conn| {
        conn.execute(
            "UPDATE auto_update_policies SET enabled = 0, updated_at = '2026-04-30T00:00:04Z' WHERE scope_type = 'stack' AND scope_id = 'stack'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    assert!(!db
        .claim_queued_job_by_id("stale-queued-job", "2026-04-30T00:00:05Z")
        .await
        .unwrap());
    let candidate = db
        .get_auto_update_candidate("service", "sha256:queued")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.policy_status.as_deref(), Some("skipped"));
    assert_eq!(candidate.policy_scope_type.as_deref(), Some("stack"));
    assert_eq!(candidate.policy_scope_id.as_deref(), Some("stack"));
    assert_eq!(candidate.update_job_id.as_deref(), Some("stale-queued-job"));
}
