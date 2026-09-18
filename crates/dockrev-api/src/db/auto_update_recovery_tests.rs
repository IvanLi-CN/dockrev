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
async fn hydrates_missing_candidate_from_successful_webhook_discovery_idempotently() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:new', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-webhook', '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest', '1.4.0', 'sha256:new', '1.4.0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "check-webhook".to_string(),
        r#type: crate::api::types::JobType::Check,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "success".to_string(),
        created_by: "webhook".to_string(),
        reason: "webhook".to_string(),
        created_at: "2026-04-30T00:00:00Z".to_string(),
        started_at: None,
        finished_at: Some("2026-04-30T00:00:00Z".to_string()),
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({"source": "github_webhook"}),
    })
    .await
    .unwrap();

    assert!(
        db.get_auto_update_candidate("service", "sha256:new")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.hydrate_auto_update_candidates("2026-04-30T00:01:00Z")
            .await
            .unwrap(),
        1
    );
    let candidate = db
        .get_auto_update_candidate("service", "sha256:new")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "ready");
    assert_eq!(candidate.source, "github_webhook");
    assert_eq!(candidate.source_job_id, "check-webhook");
    assert_eq!(candidate.discovered_at, "2026-04-30T00:00:00Z");
    assert_eq!(candidate.hydration_origin.as_deref(), Some("discovery_history"));
    assert_eq!(candidate.resolved_version.as_deref(), Some("1.4.0"));

    assert_eq!(
        db.hydrate_auto_update_candidates("2026-04-30T00:02:00Z")
            .await
            .unwrap(),
        1
    );
    let diagnostics = db
        .list_candidate_hydration_diagnostics(&["service".to_string()])
        .await
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].status, "hydrated");
    assert_eq!(diagnostics[0].source.as_deref(), Some("github_webhook"));
}

#[tokio::test]
async fn runtime_candidate_with_discovery_history_is_not_reported_as_missing() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:runtime', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-runtime', '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest', '1.4.0', 'sha256:runtime', '1.4.0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-runtime",
            "sha256:runtime",
            "ready",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:01Z",
    )
    .await
    .unwrap();

    assert!(db
        .list_candidate_hydration_diagnostics(&["service".to_string()])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn awaiting_inference_candidate_is_selected_for_policy_reconciliation() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:floating', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-floating', '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest', 'latest', 'sha256:floating', 'latest')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "check-floating".to_string(),
        r#type: crate::api::types::JobType::Check,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "success".to_string(),
        created_by: "schedule".to_string(),
        reason: "schedule".to_string(),
        created_at: "2026-04-30T00:00:00Z".to_string(),
        started_at: None,
        finished_at: Some("2026-04-30T00:00:00Z".to_string()),
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();

    db.hydrate_auto_update_candidates("2026-04-30T00:01:00Z")
        .await
        .unwrap();
    let candidate = db
        .get_auto_update_candidate("service", "sha256:floating")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "awaiting_inference");
    let rows = db
        .list_auto_update_candidates_for_policy_reconciliation(50)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "awaiting_inference");
}

#[tokio::test]
async fn ambiguous_discovery_history_is_unresolved_and_cannot_authorize_policy() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:ambiguous', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', '', '', '', '', '1.0.0', 'latest', 'latest', 'sha256:ambiguous', 'latest')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    db.hydrate_auto_update_candidates("2026-04-30T00:01:00Z")
        .await
        .unwrap();
    let candidate = db
        .get_auto_update_candidate("service", "sha256:ambiguous")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.reason.as_deref(), Some("migration_ambiguous_history"));
    assert_eq!(candidate.source, "unknown");
    assert_eq!(
        candidate.hydration_origin.as_deref(),
        Some("discovery_history_ambiguous")
    );
    let diagnostics = db
        .list_candidate_hydration_diagnostics(&["service".to_string()])
        .await
        .unwrap();
    assert_eq!(diagnostics[0].status, "ambiguous_history");
    assert_eq!(
        diagnostics[0].reason.as_deref(),
        Some("migration_ambiguous_history")
    );
}

#[tokio::test]
async fn candidate_hydration_migration_copies_history_and_is_idempotent() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("dockrev-hydration-migration-{suffix}.sqlite3"));
    {
        let db = Db::open(&path).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:migrated', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-schedule', '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest', '1.5.0', 'sha256:migrated', '1.5.0')",
                [],
            )?;
            conn.execute(
                "DELETE FROM schema_migrations WHERE id = '0020_hydrate_auto_update_candidates_from_discoveries'",
                [],
            )?;
            conn.execute(
                "ALTER TABLE auto_update_candidates DROP COLUMN hydration_origin",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        db.insert_job(crate::api::types::JobListItem {
            id: "check-schedule".to_string(),
            r#type: crate::api::types::JobType::Check,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "success".to_string(),
            created_by: "schedule".to_string(),
            reason: "schedule".to_string(),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            started_at: None,
            finished_at: Some("2026-04-30T00:00:00Z".to_string()),
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();
    }

    let db = Db::open(&path).await.unwrap();
    let candidate = db
        .get_auto_update_candidate("service", "sha256:migrated")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.source, "schedule");
    assert_eq!(candidate.hydration_origin.as_deref(), Some("discovery_history"));
    assert_eq!(candidate.discovered_at, "2026-04-30T00:00:00Z");
    drop(db);

    let db = Db::open(&path).await.unwrap();
    let count = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM auto_update_candidates WHERE service_id = 'service' AND candidate_digest = 'sha256:migrated'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    drop(db);
    std::fs::remove_file(path).unwrap();
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

    let failed = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:new".to_string(),
            status: "unresolved".to_string(),
            resolved_version: None,
            resolved_tags: None,
            reason: Some("inference_failed".to_string()),
            last_error: Some("temporary registry failure".to_string()),
            attempts: 1,
            retry_at: Some("2026-04-30T00:01:00Z".to_string()),
            settled_at: Some("2026-04-30T00:01:00Z".to_string()),
            now: "2026-04-30T00:01:00Z".to_string(),
        })
        .await
        .unwrap()
        .expect("failed settlement changes the row");
    assert_eq!(
        failed.last_error.as_deref(),
        Some("temporary registry failure")
    );

    let settled = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:new".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            resolved_tags: Some(vec!["1.4.0".to_string(), "stable".to_string()]),
            reason: Some("digest_bound_version".to_string()),
            last_error: None,
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
    assert_eq!(settled.last_error, None);
    assert_eq!(
        settled.resolved_tags.as_deref(),
        Some(["1.4.0".to_string(), "stable".to_string()].as_slice())
    );

    let repeated = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:new".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            resolved_tags: Some(vec!["1.4.0".to_string(), "stable".to_string()]),
            reason: Some("digest_bound_version".to_string()),
            last_error: None,
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
async fn skipping_stale_pending_preserves_newer_policy_projection() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &AutoUpdateCandidateInput {
                id: "candidate-pending-race".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/app".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:pending-race".to_string(),
                resolved_version: Some("1.4.0".to_string()),
                status: "ready".to_string(),
                reason: Some("test".to_string()),
                attempts: 0,
                retry_at: None,
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:old".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    let make_pending = |id: &str, rule_id: &str| AutoUpdatePendingInput {
        id: id.to_string(),
        policy_scope_type: "stack".to_string(),
        policy_scope_id: "stack".to_string(),
        rule_id: rule_id.to_string(),
        stack_id: "stack".to_string(),
        service_id: "service".to_string(),
        source_check_job_id: "check".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.4.0".to_string(),
        candidate_digest: "sha256:pending-race".to_string(),
        current_display_tag: "1.0.0".to_string(),
        first_seen_at: "2026-04-30T00:00:00Z".to_string(),
        due_at: "2026-04-30T00:00:00Z".to_string(),
        min_age_seconds: 0,
        min_version_lag: 0,
        summary_json: serde_json::json!({}),
        candidate_id: Some(candidate.id.clone()),
    };
    let stale = db
        .reserve_auto_update_pending(&make_pending("pending-old", "rule-old"), "2026-04-30T00:01:00Z")
        .await
        .unwrap();
    db.reserve_auto_update_pending(&make_pending("pending-new", "rule-new"), "2026-04-30T00:02:00Z")
        .await
        .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:pending-race",
        "delayed",
        Some("rule_matches"),
        Some("rule-new"),
        "2026-04-30T00:02:00Z",
    )
    .await
    .unwrap();

    db.mark_auto_update_pending_skipped(&stale.id, "policy_changed", "2026-04-30T00:03:00Z")
        .await
        .unwrap();

    let candidate = db
        .get_auto_update_candidate("service", "sha256:pending-race")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.policy_status.as_deref(), Some("delayed"));
    assert_eq!(candidate.policy_rule_id.as_deref(), Some("rule-new"));
    assert_eq!(
        db.get_auto_update_pending_by_id("pending-new")
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
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
