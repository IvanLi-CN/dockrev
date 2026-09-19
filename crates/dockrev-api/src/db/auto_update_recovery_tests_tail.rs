#[tokio::test]
async fn awaiting_inference_candidate_is_selected_for_policy_reconciliation() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-floating', '2026-04-30T00:00:00Z', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0', 'latest', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'latest')",
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
        .get_auto_update_candidate(
            "service",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
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
async fn each_incomplete_discovery_field_is_unresolved_and_not_executable() {
    let cases = [
        ("image", "", "check-image", "2026-04-30T00:00:00Z", "sha256:current", "latest", "1.4.0"),
        ("source-job", "ghcr.io/acme/app:latest", "", "2026-04-30T00:00:00Z", "sha256:current", "latest", "1.4.0"),
        ("discovered-at", "ghcr.io/acme/app:latest", "check-discovered-at", "", "sha256:current", "latest", "1.4.0"),
        ("current-digest", "ghcr.io/acme/app:latest", "check-current-digest", "2026-04-30T00:00:00Z", "", "latest", "1.4.0"),
        ("candidate-tags", "ghcr.io/acme/app:latest", "check-candidate-tags", "2026-04-30T00:00:00Z", "sha256:current", "", ""),
    ];

    for (suffix, image_ref, source_job_id, discovered_at, current_digest, candidate_tag, candidate_display_tag) in cases {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let candidate_digest = match suffix {
            "image" => "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "source-job" => "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "discovered-at" => "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "current-digest" => "sha256:4444444444444444444444444444444444444444444444444444444444444444",
            "candidate-tags" => "sha256:5555555555555555555555555555555555555555555555555555555555555555",
            _ => unreachable!("unexpected incomplete discovery fixture: {suffix}"),
        }
        .to_string();
        db.call({
            let image_ref = image_ref.to_string();
            let source_job_id = source_job_id.to_string();
            let discovered_at = discovered_at.to_string();
            let current_digest = current_digest.to_string();
            let candidate_tag = candidate_tag.to_string();
            let candidate_display_tag = candidate_display_tag.to_string();
            let candidate_digest = candidate_digest.clone();
            move |conn| {
                conn.execute(
                    "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', ?1, 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                    rusqlite::params![candidate_digest],
                )?;
                conn.execute(
                    "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', ?1, ?2, ?3, ?4, '1.0.0', 'latest', ?5, ?6, ?7)",
                    rusqlite::params![image_ref, source_job_id, discovered_at, current_digest, candidate_tag, candidate_digest, candidate_display_tag],
                )?;
                Ok(())
            }
        })
        .await
        .unwrap();

        if !source_job_id.is_empty() {
            db.insert_job(crate::api::types::JobListItem {
                id: source_job_id.to_string(),
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

        db.hydrate_auto_update_candidates("2026-04-30T00:01:00Z")
            .await
            .unwrap();
        let candidate = db
            .get_auto_update_candidate("service", &candidate_digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(candidate.status, "unresolved", "case={suffix}");
        assert_eq!(
            candidate.reason.as_deref(),
            Some("migration_ambiguous_history"),
            "case={suffix}"
        );
        assert!(
            db.list_auto_update_pending_candidates("2026-04-30T00:01:00Z", 10)
                .await
                .unwrap()
                .is_empty(),
            "case={suffix}"
        );
        assert_eq!(
            db.list_jobs()
                .await
                .unwrap()
                .iter()
                .filter(|job| job.reason == "auto_policy")
                .count(),
            0,
            "case={suffix}"
        );
    }
}

#[tokio::test]
async fn bare_candidate_digest_is_unresolved_and_cannot_authorize_policy() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'latest', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('service', 'ghcr.io/acme/app:latest', 'check-bare-digest', '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest', 'latest', 'latest', 'latest')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.insert_job(crate::api::types::JobListItem {
        id: "check-bare-digest".to_string(),
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
    assert!(db
        .get_auto_update_candidate("service", "latest")
        .await
        .unwrap()
        .is_none());
    let diagnostics = db
        .list_candidate_hydration_diagnostics(&["service".to_string()])
        .await
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].status, "ambiguous_history");
    assert_eq!(
        diagnostics[0].reason.as_deref(),
        Some("migration_ambiguous_history")
    );
    assert_eq!(diagnostics[0].candidate_digest.as_deref(), Some("latest"));
    assert_eq!(diagnostics[0].source_job_id, None);
    assert_eq!(diagnostics[0].discovered_at, None);
    assert!(
        db.list_auto_update_pending_candidates("2026-04-30T00:01:00Z", 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.list_jobs()
            .await
            .unwrap()
            .iter()
            .filter(|job| job.reason == "auto_policy")
            .count(),
        0
    );
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

#[tokio::test]
async fn candidate_hydration_runs_from_a_real_pre_0020_database_fixture() {
    let path = std::env::temp_dir().join(format!(
        "dockrev-pre-0020-hydration-{}.sqlite3",
        ulid::Ulid::new()
    ));
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE jobs (
  id TEXT PRIMARY KEY NOT NULL,
  type TEXT NOT NULL,
  scope TEXT NOT NULL,
  stack_id TEXT,
  service_id TEXT,
  status TEXT NOT NULL,
  allow_arch_mismatch INTEGER NOT NULL,
  backup_mode TEXT NOT NULL,
  created_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  summary_json TEXT NOT NULL
);
CREATE TABLE services (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL,
  name TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  image_tag TEXT NOT NULL,
  current_digest TEXT,
  candidate_digest TEXT,
  auto_rollback INTEGER NOT NULL,
  backup_targets_bind_paths_json TEXT NOT NULL,
  backup_targets_volume_names_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE update_job_stop_controls (
  job_id TEXT PRIMARY KEY NOT NULL,
  apply_committed_at TEXT,
  stop_requested_at TEXT,
  stop_requested_by TEXT,
  recovery_snapshot_json TEXT,
  recovery_attempted_at TEXT,
  recovery_error TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE service_new_version_discoveries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL,
  image_ref TEXT NOT NULL DEFAULT '',
  source_job_id TEXT NOT NULL,
  discovered_at TEXT NOT NULL,
  current_digest TEXT NOT NULL DEFAULT '',
  current_display_tag TEXT NOT NULL DEFAULT '',
  current_tag TEXT NOT NULL DEFAULT '',
  candidate_tag TEXT NOT NULL DEFAULT '',
  candidate_digest TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL DEFAULT ''
);
INSERT INTO schema_migrations (id, applied_at) VALUES
  ('0007_remove_manual_stacks', '2026-01-01T00:00:00Z'),
  ('0008_drop_version_inference_snapshots', '2026-01-01T00:00:00Z'),
  ('0009_add_new_version_notifications', '2026-01-01T00:00:00Z'),
  ('0010_add_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0011_track_candidate_display_tags_in_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0012_track_image_ref_in_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0013_add_update_job_stop_controls', '2026-01-01T00:00:00Z');
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest,
  auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json,
  created_at, updated_at
) VALUES (
  'legacy-service', 'legacy-stack', 'app', 'ghcr.io/acme/app', 'latest',
  'sha256:current', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 0, '{}', '{}',
  '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z'
);
INSERT INTO jobs (
  id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode,
  created_by, reason, created_at, finished_at, summary_json
) VALUES (
  'legacy-schedule-check', 'check', 'service', 'legacy-stack', 'legacy-service',
  'success', 0, 'inherit', 'schedule', 'schedule',
  '2026-04-30T00:00:00Z', '2026-04-30T00:00:01Z', '{}'
);
INSERT INTO service_new_version_discoveries (
  service_id, image_ref, source_job_id, discovered_at, current_digest,
  current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag
) VALUES (
  'legacy-service', 'ghcr.io/acme/app:latest', 'legacy-schedule-check',
  '2026-04-30T00:00:00Z', 'sha256:current', '1.0.0', 'latest',
  'latest', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'latest'
);
"#,
        )
        .unwrap();
    }

    let db = Db::open(&path).await.unwrap();
    let candidate = db
        .get_auto_update_candidate(
            "legacy-service",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.source, "unknown");
    assert_eq!(candidate.source_job_id, "");
    assert_eq!(candidate.discovered_at, "");
    assert_eq!(candidate.reason.as_deref(), Some("migration_ambiguous_history"));
    assert_eq!(
        candidate.hydration_origin.as_deref(),
        Some("discovery_history_ambiguous")
    );
    assert_eq!(candidate.status, "unresolved");
    drop(db);

    let db = Db::open(&path).await.unwrap();
    let count = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM auto_update_candidates WHERE service_id = 'legacy-service' AND candidate_digest = 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    drop(db);
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
}

#[tokio::test]
async fn auto_policy_enqueue_guard_rejects_changed_candidate_without_creating_a_job() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
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
    db.insert_job(crate::api::types::JobListItem {
        id: "source-check".to_string(),
        r#type: crate::api::types::JobType::Check,
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
    let candidate = db
        .upsert_auto_update_candidate(
            &candidate_input(
                "candidate-old",
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "delayed",
        Some("policy_matched"),
        Some("rule"),
        "2026-04-30T00:00:01Z",
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute(
            "UPDATE auto_update_candidates SET source_job_id = 'source-check' WHERE id = 'candidate-old'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-race".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "source-check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({
                    "currentDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "policyUpdatedAt": "2026-04-30T00:00:00Z"
                }),
                candidate_id: Some(candidate.id.clone()),
            },
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap();
    assert!(db
        .try_claim_auto_update_pending_if_current(
            &pending.id,
            "service",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "stack",
            "stack",
            "rule",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap());

    db.call(|conn| {
        conn.execute(
            "UPDATE services SET candidate_digest = 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE id = 'service'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let mut job = crate::api::types::JobRecord::new_running(
        "race-job".to_string(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        Some("stack".to_string()),
        Some("service".to_string()),
        "2026-04-30T00:01:02Z",
    );
    job.status = "queued".to_string();
    job.started_at = None;
    let outcome = db
        .insert_service_operation_job_if_unblocked_with_auto_policy_guard(
            job.to_db(),
            vec![crate::db::ServiceOperationTarget {
                service_id: "service".to_string(),
                stack_id: "stack".to_string(),
            }],
            None,
            crate::db::AutoPolicyEnqueueGuard {
                pending_id: pending.id.clone(),
                service_id: "service".to_string(),
                candidate_id: candidate.id,
                candidate_digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                expected_current_digest: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        crate::db::ServiceOperationAcquireOutcome::StaleAutoPolicy
    ));
    assert!(db.get_job("race-job").await.unwrap().is_none());
    assert_eq!(
        db.get_auto_update_pending_by_id(&pending.id)
            .await
            .unwrap()
        .unwrap()
            .status,
        "skipped"
    );
}

#[tokio::test]
async fn auto_policy_enqueue_guard_rejects_malformed_candidate_digest() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:0000000000000000000000000000000000000000000000000000000000000000', 'latest', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
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
    db.insert_job(crate::api::types::JobListItem {
        id: "source-check-malformed".to_string(),
        r#type: crate::api::types::JobType::Check,
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
    let candidate = db
        .upsert_auto_update_candidate(
            &candidate_input(
                "candidate-malformed",
                "latest",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.set_auto_update_candidate_policy(
        "service",
        "latest",
        "delayed",
        Some("policy_matched"),
        Some("rule"),
        "2026-04-30T00:00:01Z",
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute(
            "UPDATE auto_update_candidates SET source_job_id = 'source-check-malformed' WHERE id = 'candidate-malformed'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-malformed".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "source-check-malformed".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                due_at: "2026-04-30T00:00:00Z".to_string(),
                min_age_seconds: 0,
                min_version_lag: 0,
                summary_json: serde_json::json!({
                    "currentDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                    "policyUpdatedAt": "2026-04-30T00:00:00Z"
                }),
                candidate_id: Some(candidate.id.clone()),
            },
            "2026-04-30T00:00:02Z",
        )
        .await
        .unwrap();
    assert!(!db
        .try_claim_auto_update_pending_if_current(
            &pending.id,
            "service",
            "latest",
            "stack",
            "stack",
            "rule",
            "2026-04-30T00:00:03Z",
        )
        .await
        .unwrap());

    let mut job = crate::api::types::JobRecord::new_running(
        "malformed-auto-policy-job".to_string(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        Some("stack".to_string()),
        Some("service".to_string()),
        "2026-04-30T00:01:02Z",
    );
    job.status = "queued".to_string();
    job.started_at = None;
    let outcome = db
        .insert_service_operation_job_if_unblocked_with_auto_policy_guard(
            job.to_db(),
            vec![crate::db::ServiceOperationTarget {
                service_id: "service".to_string(),
                stack_id: "stack".to_string(),
            }],
            None,
            crate::db::AutoPolicyEnqueueGuard {
                pending_id: pending.id.clone(),
                service_id: "service".to_string(),
                candidate_id: candidate.id,
                candidate_digest: "latest".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule".to_string(),
                expected_current_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        crate::db::ServiceOperationAcquireOutcome::StaleAutoPolicy
    ));
    assert!(db
        .get_job("malformed-auto-policy-job")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.get_auto_update_pending_by_id(&pending.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
}

#[tokio::test]
async fn skipping_pending_auto_update_stops_attached_jobs_idempotently() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let pending_input = |id: &str| AutoUpdatePendingInput {
        id: id.to_string(),
        policy_scope_type: "stack".to_string(),
        policy_scope_id: "stack".to_string(),
        rule_id: format!("rule-{id}"),
        stack_id: "stack".to_string(),
        service_id: "service".to_string(),
        source_check_job_id: "check".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.4.0".to_string(),
        candidate_digest: format!("sha256:{id}"),
        current_display_tag: "1.0.0".to_string(),
        first_seen_at: "2026-04-30T00:00:00Z".to_string(),
        due_at: "2026-04-30T00:00:00Z".to_string(),
        min_age_seconds: 0,
        min_version_lag: 0,
        summary_json: serde_json::json!({}),
        candidate_id: None,
    };
    let queued = db
        .reserve_auto_update_pending(&pending_input("queued"), "2026-04-30T00:00:00Z")
        .await
        .unwrap();
    assert!(db.try_claim_auto_update_pending(&queued.id, "2026-04-30T00:00:01Z").await.unwrap());
    db.insert_job(crate::api::types::JobListItem {
        id: "queued-auto-policy-job".to_string(),
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
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    assert!(db.mark_auto_update_pending_enqueued(&queued.id, "queued-auto-policy-job", "2026-04-30T00:00:03Z").await.unwrap());
    db.mark_auto_update_pending_skipped(&queued.id, "policy_changed", "2026-04-30T00:00:04Z")
        .await
        .unwrap();
    db.mark_auto_update_pending_skipped(&queued.id, "should_not_replace", "2026-04-30T00:00:05Z")
        .await
        .unwrap();
    assert_eq!(db.get_job("queued-auto-policy-job").await.unwrap().unwrap().status, "cancelled");
    assert_eq!(db.get_auto_update_pending_by_id(&queued.id).await.unwrap().unwrap().summary_json["skipReason"], "policy_changed");

    let running = db
        .reserve_auto_update_pending(&pending_input("running"), "2026-04-30T00:00:00Z")
        .await
        .unwrap();
    assert!(db.try_claim_auto_update_pending(&running.id, "2026-04-30T00:00:01Z").await.unwrap());
    db.insert_job(crate::api::types::JobListItem {
        id: "running-auto-policy-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "running".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:02Z".to_string(),
        started_at: Some("2026-04-30T00:00:02Z".to_string()),
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();
    assert!(db.mark_auto_update_pending_enqueued(&running.id, "running-auto-policy-job", "2026-04-30T00:00:03Z").await.unwrap());
    db.mark_auto_update_pending_skipped(&running.id, "superseded", "2026-04-30T00:00:04Z")
        .await
        .unwrap();
    assert_eq!(
        db.get_update_stop_control("running-auto-policy-job")
            .await
            .unwrap()
            .unwrap()
            .stop_requested_by
            .as_deref(),
        Some("auto-policy-skip")
    );
}

#[tokio::test]
async fn enqueue_race_requests_stop_for_a_running_auto_policy_job() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let pending = db
        .reserve_auto_update_pending(
            &AutoUpdatePendingInput {
                id: "pending-enqueue-race".to_string(),
                policy_scope_type: "stack".to_string(),
                policy_scope_id: "stack".to_string(),
                rule_id: "rule-enqueue-race".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                source_check_job_id: "check".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: "1.4.0".to_string(),
                candidate_digest: "sha256:enqueue-race".to_string(),
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
        id: "enqueue-race-job".to_string(),
        r#type: crate::api::types::JobType::Update,
        scope: crate::api::types::JobScope::Service,
        stack_id: Some("stack".to_string()),
        service_id: Some("service".to_string()),
        status: "running".to_string(),
        created_by: "auto-policy".to_string(),
        reason: "auto_policy".to_string(),
        created_at: "2026-04-30T00:00:02Z".to_string(),
        started_at: Some("2026-04-30T00:00:02Z".to_string()),
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    })
    .await
    .unwrap();

    db.mark_auto_update_pending_skipped(
        &pending.id,
        "candidate_superseded",
        "2026-04-30T00:00:03Z",
    )
    .await
    .unwrap();
    assert!(!db
        .mark_auto_update_pending_enqueued(
            &pending.id,
            "enqueue-race-job",
            "2026-04-30T00:00:04Z",
        )
        .await
        .unwrap());

    assert_eq!(
        db.get_update_stop_control("enqueue-race-job")
            .await
            .unwrap()
            .unwrap()
            .stop_requested_by
            .as_deref(),
        Some("auto-policy-enqueue-race")
    );
    assert_eq!(
        db.get_job("enqueue-race-job")
            .await
            .unwrap()
            .unwrap()
            .status,
        "running"
    );
}

#[tokio::test]
async fn candidate_upsert_keeps_earliest_qualified_provenance_tuple() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let mut candidate = candidate_input(
        "candidate-provenance",
        "sha256:provenance",
        "ready",
        "2026-04-30T00:02:00Z",
    );
    candidate.source_job_id = "schedule-check".to_string();
    db.upsert_auto_update_candidate(&candidate, "2026-04-30T00:02:00Z")
        .await
        .unwrap();
    candidate.source_job_id = "webhook-check".to_string();
    candidate.source = "github_webhook".to_string();
    candidate.discovered_at = "2026-04-30T00:01:00Z".to_string();
    db.upsert_auto_update_candidate(&candidate, "2026-04-30T00:03:00Z")
        .await
        .unwrap();
    let row = db
        .get_auto_update_candidate("service", "sha256:provenance")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.source_job_id, "webhook-check");
    assert_eq!(row.source, "github_webhook");
    assert_eq!(row.discovered_at, "2026-04-30T00:01:00Z");
}

#[tokio::test]
async fn unresolved_candidate_can_be_reopened_for_force_inference() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    db.upsert_auto_update_candidate(
        &candidate_input(
            "candidate-unresolved",
            "sha256:unresolved",
            "unresolved",
            "2026-04-30T00:00:00Z",
        ),
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    assert!(db.reopen_auto_update_candidate_inference("service", "sha256:unresolved", "force", "2026-04-30T01:00:00Z").await.unwrap());
    let candidate = db
        .get_auto_update_candidate("service", "sha256:unresolved")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.status, "awaiting_inference");
    assert_eq!(candidate.policy_status.as_deref(), Some("waiting_inference"));
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
