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
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-webhook', '2026-04-30T00:00:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.4.0', 'sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210', '1.4.0')",
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
    db.call(|conn| {
        conn.execute(
            "UPDATE jobs SET type = 'CHECK', status = 'SUCCESS', scope = 'SERVICE', created_by = 'WEBHOOK', stack_id = 'STACK', service_id = 'SERVICE' WHERE id = 'check-webhook'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    assert!(
        db.get_auto_update_candidate(
            "service",
            "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
        .get_auto_update_candidate(
            "service",
            "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        )
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
async fn hydration_rejects_successful_source_job_from_another_service() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-foreign', '2026-04-30T00:00:00Z', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '1.0.0', 'latest', '1.4.0', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', '1.4.0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "check-foreign".to_string(),
        r#type: crate::api::types::JobType::Check,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("another-service".to_string()),
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

    db.hydrate_auto_update_candidates("2026-04-30T00:01:00Z")
        .await
        .unwrap();
    let candidate = db
        .get_auto_update_candidate(
            "service",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.source, "unknown");
    assert_eq!(candidate.source_job_id, "");
    assert_eq!(candidate.discovered_at, "");
    assert_eq!(candidate.reason.as_deref(), Some("migration_ambiguous_history"));
}

#[tokio::test]
async fn hydration_keeps_the_earliest_qualified_observation_as_one_consistent_record() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-old', '2026-04-30T00:00:30Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.3.0', 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', '1.3.0')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'missing-check', '2026-04-30T00:00:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.4.0', 'sha256:FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210FEDCBA9876543210', '1.4.0')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-schedule', '2026-04-30T00:01:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.4.0', 'sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210', '1.4.1')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-webhook-latest', '2026-04-30T00:02:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.5.0', 'sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210', '1.5.0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let mut runtime_candidate = candidate_input(
        "runtime-current-candidate",
        "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        "ready",
        "2026-04-30T00:01:00Z",
    );
    runtime_candidate.raw_tag.clear();
    db.upsert_auto_update_candidate(&runtime_candidate, "2026-04-30T00:01:00Z")
        .await
        .unwrap();

    for (id, created_by, reason, summary) in [
        ("check-old", "schedule", "schedule", serde_json::json!({})),
        ("check-schedule", "schedule", "schedule", serde_json::json!({})),
        (
            "check-webhook-latest",
            "webhook",
            "webhook",
            serde_json::json!({"source": "github_webhook"}),
        ),
    ] {
        db.insert_job(crate::api::types::JobListItem {
            id: id.to_string(),
            r#type: crate::api::types::JobType::Check,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "success".to_string(),
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            started_at: None,
            finished_at: Some("2026-04-30T00:02:00Z".to_string()),
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: summary,
        })
        .await
        .unwrap();
    }

    db.call(|conn| {
        conn.execute(
            "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, started_at, summary_json) VALUES ('running-hydration-job', 'update', 'service', 'stack', 'service', 'running', 0, 'inherit', 'auto-policy', 'auto_policy', '2026-04-30T00:02:30Z', '2026-04-30T00:02:31Z', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, summary_json) VALUES ('pending-old-hydration', 'stack', 'stack', 'rule', 'stack', 'service', 'check-old', '1.3.0', '1.3.0', 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', '1.0.0', '2026-04-30T00:00:30Z', '2026-04-30T00:00:30Z', 0, 0, 'enqueued', 'running-hydration-job', '2026-04-30T00:02:30Z', '2026-04-30T00:02:30Z', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO update_job_stop_controls (job_id, updated_at) VALUES ('running-hydration-job', '2026-04-30T00:02:31Z')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    db.hydrate_auto_update_candidates("2026-04-30T00:03:00Z")
        .await
        .unwrap();
    let candidate = db
        .get_auto_update_candidate(
            "service",
            "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.id, "runtime-current-candidate");
    assert_eq!(candidate.status, "ready");
    assert_eq!(candidate.discovered_at, "2026-04-30T00:01:00Z");
    assert_eq!(candidate.source, "schedule");
    assert_eq!(candidate.source_job_id, "check-schedule");
    assert_eq!(candidate.raw_tag, "1.4.0");
    let old_candidate = db
        .get_auto_update_candidate(
            "service",
            "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_candidate.status, "superseded");
    assert_eq!(
        old_candidate.superseded_by_candidate_id.as_deref(),
        Some("runtime-current-candidate")
    );
    let old_pending = db
        .get_auto_update_pending_by_id("pending-old-hydration")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_pending.status, "skipped");
    assert_eq!(
        db.get_update_stop_control("running-hydration-job")
            .await
            .unwrap()
            .unwrap()
            .stop_requested_by
            .as_deref(),
        Some("auto-policy-supersession")
    );
    let candidate_count: i64 = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM auto_update_candidates WHERE service_id = 'service'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(candidate_count, 2);
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
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-runtime', '2026-04-30T00:00:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', '1.4.0', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', '1.4.0')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "check-runtime".to_string(),
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
    let mut input = candidate_input(
        "candidate-runtime",
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "ready",
        "2026-04-30T00:00:00Z",
    );
    input.source_job_id = "legacy-runtime-check".to_string();
    input.source = "unknown".to_string();
    let existing = db
        .upsert_auto_update_candidate(&input, "2026-04-30T00:00:01Z")
        .await
        .unwrap();

    assert!(existing.hydration_origin.is_none());
    let original_discovered_at = existing.discovered_at.clone();

    assert_eq!(
        db.hydrate_auto_update_candidates("2026-04-30T00:00:02Z")
            .await
            .unwrap(),
        1
    );
    let hydrated = db
        .get_auto_update_candidate(
            "service",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.source, "github_webhook");
    assert_eq!(hydrated.source_job_id, "check-runtime");
    assert_eq!(hydrated.hydration_origin.as_deref(), Some("discovery_history"));
    assert_eq!(hydrated.discovered_at, original_discovered_at);
    let hydrated_updated_at = hydrated.updated_at.clone();

    db.call(|conn| {
        conn.execute(
            "UPDATE auto_update_candidates SET image_ref = '   ', discovered_at = '', current_digest = '' WHERE service_id = 'service' AND candidate_digest = 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.hydrate_auto_update_candidates("2026-04-30T00:00:03Z")
        .await
        .unwrap();
    let repaired = db
        .get_auto_update_candidate(
            "service",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repaired.discovered_at, "2026-04-30T00:00:00Z");
    assert_eq!(
        repaired.current_digest.as_deref(),
        Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(repaired.image_ref, "ghcr.io/acme/app");
    let repaired_updated_at = repaired.updated_at.clone();

    db.hydrate_auto_update_candidates("2026-04-30T00:00:03Z")
        .await
        .unwrap();
    let repeated = db
        .get_auto_update_candidate(
            "service",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repeated.updated_at, repaired_updated_at);
    assert_ne!(repaired_updated_at, hydrated_updated_at);
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
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', '', '', '', '', '1.0.0', 'latest', 'latest', 'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'latest')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-old-ambiguous",
            "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "ready",
            "2026-04-29T23:59:00Z",
        ),
        "2026-04-29T23:59:00Z",
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, candidate_id, summary_json) VALUES ('pending-old-ambiguous', 'stack', 'stack', 'rule', 'stack', 'service', 'old-check', 'latest', '1.2.0', 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', '1.0.0', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-04-29T23:59:00Z', '2026-04-29T23:59:00Z', 0, 0, 'pending', NULL, '2026-04-29T23:59:00Z', '2026-04-29T23:59:00Z', 'candidate-old-ambiguous', '{}')",
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
        .get_auto_update_candidate(
            "service",
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "unresolved");
    assert_eq!(candidate.reason.as_deref(), Some("migration_ambiguous_history"));
    assert_eq!(candidate.source, "unknown");
    assert_eq!(candidate.source_job_id, "");
    assert_eq!(candidate.discovered_at, "");
    assert_eq!(
        candidate.hydration_origin.as_deref(),
        Some("discovery_history_ambiguous")
    );
    let old_candidate = db
        .get_auto_update_candidate(
            "service",
            "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_candidate.status, "superseded");
    assert_eq!(
        old_candidate.superseded_by_candidate_id.as_deref(),
        Some(candidate.id.as_str())
    );
    assert_eq!(
        db.get_auto_update_pending_by_id("pending-old-ambiguous")
            .await
            .unwrap()
            .unwrap()
            .status,
        "skipped"
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
    assert_eq!(diagnostics[0].source_job_id, None);
    assert_eq!(diagnostics[0].discovered_at, None);
}

#[tokio::test]
async fn digest_identity_migration_deduplicates_active_pending_rows() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("dockrev-digest-dedup-{suffix}.sqlite3"));
    {
        let db = Db::open(&path).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, attempts, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at, policy_status, resolved_tags, last_error, retry_at, policy_reason, policy_rule_id, policy_evaluated_at, policy_scope_type, policy_scope_id, superseded_at, superseded_by_candidate_id) VALUES ('candidate-legacy', 'stack', 'service', 'ghcr.io/acme/app', 'latest', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'awaiting_inference', 'version_inference_pending', 0, '2026-04-29T23:59:00Z', 'check-legacy', 'schedule', 'latest', '1.0.0', 'sha256:current', '2026-04-29T23:59:00Z', '2026-04-29T23:59:00Z', NULL, '[\"linux/amd64\"]', 'legacy inference error', '2026-05-01T00:00:00Z', 'legacy policy reason', 'legacy-rule', '2026-05-01T00:01:00Z', 'stack', 'stack', '2026-05-01T00:02:00Z', 'candidate-superseded-by')",
                [],
            )?;
            conn.execute(
                "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, attempts, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at, policy_status) VALUES ('candidate-canonical', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'ready', 'digest_bound_version', 0, '2026-04-30T00:00:01Z', 'check', 'schedule', 'latest', '1.0.0', 'sha256:current', '2026-04-30T00:00:01Z', '2026-04-30T00:00:01Z', 'queued')",
                [],
            )?;
            conn.execute(
                "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES ('duplicate-job', 'update', 'service', 'stack', 'service', 'queued', 0, 'inherit', 'auto-policy', 'auto_policy', '2026-04-30T00:00:02Z', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, candidate_id, summary_json) VALUES ('pending-legacy', 'stack', 'stack', 'rule', 'stack', 'service', 'check', 'latest', '1.2.0', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'sha256:current', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', 0, 0, 'pending', NULL, '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', 'candidate-legacy', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, candidate_id, summary_json) VALUES ('pending-canonical', 'stack', 'stack', 'rule', 'stack', 'service', 'check', 'latest', '1.2.0', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'sha256:current', '2026-04-30T00:00:01Z', '2026-04-30T00:00:01Z', 0, 0, 'enqueued', 'duplicate-job', '2026-04-30T00:00:01Z', '2026-04-30T00:00:01Z', 'candidate-canonical', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, candidate_id, summary_json) VALUES ('pending-service-scope', 'service', 'service', 'rule', 'stack', 'service', 'check', 'latest', '1.2.0', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'sha256:current', '2026-04-30T00:00:02Z', '2026-04-30T00:00:02Z', 0, 0, 'pending', NULL, '2026-04-30T00:00:02Z', '2026-04-30T00:00:02Z', 'candidate-canonical', '{}')",
                [],
            )?;
            conn.execute(
                "DELETE FROM schema_migrations WHERE id = '0024_normalize_auto_update_digest_identity'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    let db = Db::open(&path).await.unwrap();
    let rows = db
        .call(|conn| {
            let pending = conn
                .prepare(
                    "SELECT id, candidate_digest, status, json_extract(summary_json, '$.skipReason') FROM auto_update_pending WHERE service_id = 'service' ORDER BY id",
                )?
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let candidate = conn.query_row(
                "SELECT COUNT(*), MIN(id), MIN(candidate_digest), MIN(source_job_id), MIN(discovered_at), MIN(status) FROM auto_update_candidates WHERE service_id = 'service'",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )?;
            let audit = conn.query_row(
                "SELECT resolved_tags, last_error, retry_at, policy_reason, policy_rule_id, policy_evaluated_at, policy_scope_type, policy_scope_id, superseded_at, superseded_by_candidate_id FROM auto_update_candidates WHERE id = 'candidate-canonical'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ))
                },
            )?;
            let job = conn.query_row(
                "SELECT status FROM jobs WHERE id = 'duplicate-job'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((pending, candidate, job, audit))
        })
        .await
        .unwrap();
    let latest = db
        .list_latest_auto_update_candidates(&["service".to_string()])
        .await
        .unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].candidate_digest, "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    assert_eq!(
        rows.0,
        vec![
            (
                "pending-canonical".to_string(),
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
                "enqueued".to_string(),
                None
            ),
            (
                "pending-legacy".to_string(),
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
                "skipped".to_string(),
                Some("migration_duplicate_candidate_digest".to_string())
            ),
            (
                "pending-service-scope".to_string(),
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
                "pending".to_string(),
                None
            )
        ]
    );
    assert_eq!(
        rows.1,
        (
            1,
            "candidate-canonical".to_string(),
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            "check-legacy".to_string(),
            "2026-04-29T23:59:00Z".to_string(),
            "ready".to_string(),
        )
    );
    assert_eq!(rows.2, "queued");
    assert_eq!(
        rows.3,
        (
            Some("[\"linux/amd64\"]".to_string()),
            Some("legacy inference error".to_string()),
            Some("2026-05-01T00:00:00Z".to_string()),
            Some("legacy policy reason".to_string()),
            Some("legacy-rule".to_string()),
            Some("2026-05-01T00:01:00Z".to_string()),
            Some("stack".to_string()),
            Some("stack".to_string()),
            Some("2026-05-01T00:02:00Z".to_string()),
            Some("candidate-superseded-by".to_string()),
        )
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn queued_auto_policy_recovery_requires_exact_candidate_and_target_identity() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_policies (scope_type, scope_id, mode, enabled, rules_json, created_at, updated_at) VALUES ('stack', 'stack', 'override', 1, '[{\"id\":\"rule\",\"enabled\":true}]', '2026-04-30T00:00:00Z', '2026-04-30T00:01:00Z')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let cases = [
        (
            "missing-candidate",
            "missing",
            None,
            "latest",
            "service",
            false,
        ),
        (
            "mismatched-tag",
            "tag",
            Some("candidate"),
            "stable",
            "service",
            false,
        ),
        (
            "wrong-scope",
            "scope",
            Some("candidate"),
            "latest",
            "stack",
            false,
        ),
        (
            "wrong-target-service",
            "target",
            Some("other-service"),
            "latest",
            "service",
            false,
        ),
        (
            "prefixless-target",
            "prefixless",
            Some("candidate"),
            "latest",
            "service",
            true,
        ),
    ];

    for (
        suffix,
        digest_suffix,
        candidate_binding,
        target_tag,
        job_scope,
        expected_claim,
    ) in cases
    {
        let service_id = format!("service-{suffix}");
        let digest = format!("sha256:{digest_suffix}");
        let source_job_id = format!("check-{suffix}");
        let candidate_id = format!("{service_id}:{digest}");
        let mut input = candidate_input(&candidate_id, &digest, "ready", "2026-04-30T00:00:00Z");
        input.service_id = service_id.clone();
        input.source_job_id = source_job_id.clone();
        input.current_digest = Some("sha256:current".to_string());
        db.call({
            let service_id = service_id.clone();
            let digest = digest.clone();
            let source_job_id = source_job_id.clone();
            move |conn| {
                conn.execute(
                    "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES (?1, 'stack', ?1, 'ghcr.io/acme/app', 'latest', 'sha256:current', ?2, 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                    rusqlite::params![service_id, digest],
                )?;
                conn.execute(
                    "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES (?1, 'check', 'service', 'stack', ?2, 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '{}')",
                    rusqlite::params![source_job_id, service_id],
                )?;
                Ok(())
            }
        })
        .await
        .unwrap();
        db.upsert_auto_update_candidate(&input, "2026-04-30T00:00:00Z")
            .await
            .unwrap();
        db.set_auto_update_candidate_policy(
            &service_id,
            &digest,
            "queued",
            Some("policy_matched"),
            Some("rule"),
            "2026-04-30T00:01:00Z",
        )
        .await
        .unwrap();

        let pending_id = format!("pending-{suffix}");
        let pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: pending_id.clone(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule".to_string(),
                    stack_id: "stack".to_string(),
                    service_id: service_id.clone(),
                    source_check_job_id: source_job_id,
                    candidate_tag: "latest".to_string(),
                    candidate_display_tag: "1.4.0".to_string(),
                    candidate_digest: digest.clone(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({
                        "policyUpdatedAt": "2026-04-30T00:01:00Z"
                    }),
                    candidate_id: candidate_binding.map(|binding| {
                        if binding == "candidate" {
                            candidate_id.clone()
                        } else {
                            binding.to_string()
                        }
                    }),
                },
                "2026-04-30T00:01:01Z",
            )
            .await
            .unwrap();
        assert!(db
            .try_claim_auto_update_pending(&pending.id, "2026-04-30T00:01:02Z")
            .await
            .unwrap());

        let job_id = format!("job-{suffix}");
        let target_service_id = if suffix == "wrong-target-service" {
            "other-service"
        } else {
            service_id.as_str()
        };
        let job_scope = if job_scope == "stack" {
            crate::api::types::JobScope::Stack
        } else {
            crate::api::types::JobScope::Service
        };
        let target_digest = if expected_claim {
            digest_suffix.to_string()
        } else {
            digest.clone()
        };
        db.insert_job(crate::api::types::JobListItem {
            id: job_id.clone(),
            r#type: crate::api::types::JobType::Update,
            scope: job_scope.clone(),
            stack_id: Some("stack".to_string()),
            service_id: (job_scope == crate::api::types::JobScope::Service)
                .then(|| service_id.clone()),
            status: "queued".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:01:03Z".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({
                "targets": [{
                    "serviceId": target_service_id,
                    "targetTag": target_tag,
                    "targetDigest": target_digest,
                    "autoPolicyContext": {
                        "expectedCurrentDigest": "sha256:current"
                    }
                }]
            }),
        })
        .await
        .unwrap();
        assert!(db
            .mark_auto_update_pending_enqueued(&pending_id, &job_id, "2026-04-30T00:01:04Z")
            .await
            .unwrap());

        let claimed = db
            .claim_queued_job_by_id_for_recovery(&job_id, "2026-04-30T00:01:05Z")
            .await
            .unwrap();
        assert_eq!(claimed, expected_claim);
        if expected_claim {
            assert_eq!(
                db.get_job(&job_id).await.unwrap().unwrap().status,
                "running"
            );
        } else {
            assert_eq!(
                db.get_job(&job_id).await.unwrap().unwrap().status,
                "cancelled"
            );
            assert_eq!(
                db.get_auto_update_pending_by_id(&pending_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .summary_json["skipReason"],
                "migration_ambiguous_history"
            );
        }
    }
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
        .begin_auto_update_candidate_inference(
            "service",
            "sha256:new",
            "2026-04-30T00:00:30Z",
        )
        .await
        .unwrap()
        .unwrap();
    let failed_generation = failed.evidence_generation;
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
            evidence_generation: failed_generation,
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

    db.reopen_auto_update_candidate_inference(
        "service",
        "sha256:new",
        "force_inference",
        "2026-04-30T00:01:30Z",
    )
    .await
    .unwrap();
    let settled_generation = db
        .begin_auto_update_candidate_inference(
            "service",
            "sha256:new",
            "2026-04-30T00:01:45Z",
        )
        .await
        .unwrap()
        .unwrap()
        .evidence_generation;
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
            evidence_generation: settled_generation,
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
            evidence_generation: settled_generation,
            retry_at: None,
            settled_at: Some("2026-04-30T00:02:00Z".to_string()),
            now: "2026-04-30T00:03:00Z".to_string(),
        })
        .await
        .unwrap();
    assert!(repeated.is_none(), "repeated settlement must be a no-op");
}

#[tokio::test]
async fn stale_awaiting_inference_settlement_preserves_newer_retry_state() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-awaiting-monotonic",
            "sha256:awaiting-monotonic",
            "awaiting_inference",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();

    let _ = db
        .begin_auto_update_candidate_inference(
            "service",
            "sha256:awaiting-monotonic",
            "2026-04-30T00:00:10Z",
        )
        .await
        .unwrap();
    let _ = db
        .begin_auto_update_candidate_inference(
            "service",
            "sha256:awaiting-monotonic",
            "2026-04-30T00:00:11Z",
        )
        .await
        .unwrap();
    let newer_generation = db
        .begin_auto_update_candidate_inference(
            "service",
            "sha256:awaiting-monotonic",
            "2026-04-30T00:00:12Z",
        )
        .await
        .unwrap()
        .unwrap()
        .evidence_generation;
    db.settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
        service_id: "service".to_string(),
        candidate_digest: "sha256:awaiting-monotonic".to_string(),
        status: "awaiting_inference".to_string(),
        resolved_version: None,
        resolved_tags: None,
        reason: Some("version_inference_pending".to_string()),
        last_error: Some("newer registry failure".to_string()),
        attempts: 2,
        evidence_generation: newer_generation,
        retry_at: Some("2026-04-30T00:05:00Z".to_string()),
        settled_at: None,
        now: "2026-04-30T00:02:00Z".to_string(),
    })
    .await
    .unwrap()
    .expect("newer inference settlement changes the row");

    let stale = db
        .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:awaiting-monotonic".to_string(),
            status: "awaiting_inference".to_string(),
            resolved_version: None,
            resolved_tags: None,
            reason: Some("version_inference_pending".to_string()),
            last_error: Some("older registry failure".to_string()),
            attempts: 1,
            evidence_generation: newer_generation - 1,
            retry_at: Some("2026-04-30T00:04:00Z".to_string()),
            settled_at: None,
            now: "2026-04-30T00:03:00Z".to_string(),
        })
        .await
        .unwrap();
    assert!(stale.is_none());
    let current = db
        .get_auto_update_candidate("service", "sha256:awaiting-monotonic")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.attempts, 2);
    assert_eq!(current.retry_at.as_deref(), Some("2026-04-30T00:05:00Z"));
    assert_eq!(current.last_error.as_deref(), Some("newer registry failure"));
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

include!("auto_update_recovery_tests_tail.rs");
