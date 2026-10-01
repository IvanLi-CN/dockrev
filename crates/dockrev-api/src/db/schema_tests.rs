use super::*;
use std::path::PathBuf;

fn temporary_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("dockrev-schema-test-{}.sqlite3", ulid::Ulid::new()))
}

#[tokio::test]
async fn startup_preserves_discovery_projects_until_a_successful_scan() {
    let db_path = temporary_db_path();
    let initial_db = Db::open(&db_path).await.unwrap();
    drop(initial_db);
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    for stack_id in [
        "legacy_missing",
        "unarchived_missing",
        "active_project",
        "user_archived",
    ] {
        conn.execute(
            r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES (?1, ?1, 'path', '[]', '[]', 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
"#,
            [stack_id],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE stacks SET archived = 1, archived_reason = 'user_archive' WHERE id = 'user_archived'",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
UPDATE stacks
SET compose_files_json = '["/srv/legacy/docker-compose.yml"]',
    env_file = '/srv/legacy/.env',
    backup_targets_json = '[{"kind":"bind-mount","path":"/srv/legacy/data"}]',
    backup_retention_keep_last = 7,
    backup_retention_delete_after_stable_seconds = 3600
WHERE id = 'legacy_missing'
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag, auto_rollback,
  backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at
) VALUES (
  'legacy-missing-service', 'legacy_missing', 'app', 'alpine:3.20', '3.20', 0,
  '["/srv/legacy/data"]', '["legacy_data"]',
  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
)
"#,
        [],
    )
    .unwrap();

    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (
  project, stack_id, status, archived, archived_reason
) VALUES ('legacy-missing', 'legacy_missing', 'missing', 1, 'user_archive')
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (project, stack_id, status)
VALUES ('unarchived-missing', 'unarchived_missing', 'missing')
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (project, stack_id, status)
VALUES ('active-project', 'active_project', 'active')
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (project, stack_id, status)
VALUES ('missing-user-archive', 'user_archived', 'missing')
"#,
        [],
    )
    .unwrap();
    drop(conn);

    let reopened_db = Db::open(&db_path).await.unwrap();
    drop(reopened_db);
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let states = conn
        .prepare("SELECT id, archived, archived_reason FROM stacks ORDER BY id")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        states,
        vec![
            ("active_project".to_string(), 0, None),
            ("legacy_missing".to_string(), 0, None,),
            ("unarchived_missing".to_string(), 0, None,),
            (
                "user_archived".to_string(),
                1,
                Some("user_archive".to_string()),
            ),
        ]
    );

    let legacy_stack_metadata = conn
        .query_row(
            r#"
SELECT compose_files_json, env_file, backup_targets_json,
       backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
       archived_at
FROM stacks
WHERE id = 'legacy_missing'
"#,
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .unwrap();
    let (
        compose_files_json,
        env_file,
        backup_targets_json,
        backup_keep_last,
        backup_delete_after,
        archived_at,
    ) = legacy_stack_metadata;
    assert_eq!(
        (
            compose_files_json,
            env_file,
            backup_targets_json,
            backup_keep_last,
            backup_delete_after,
        ),
        (
            "[\"/srv/legacy/docker-compose.yml\"]".to_string(),
            Some("/srv/legacy/.env".to_string()),
            "[{\"kind\":\"bind-mount\",\"path\":\"/srv/legacy/data\"}]".to_string(),
            7,
            3600,
        )
    );
    assert!(archived_at.is_none());

    let legacy_service_metadata = conn
        .query_row(
            r#"
SELECT stack_id, name, image_ref, image_tag, auto_rollback,
       backup_targets_bind_paths_json, backup_targets_volume_names_json
FROM services
WHERE id = 'legacy-missing-service'
"#,
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        legacy_service_metadata,
        (
            "legacy_missing".to_string(),
            "app".to_string(),
            "alpine:3.20".to_string(),
            "3.20".to_string(),
            0,
            "[\"/srv/legacy/data\"]".to_string(),
            "[\"legacy_data\"]".to_string(),
        )
    );

    let unarchived_discovery_state = conn
        .query_row(
            "SELECT archived, archived_reason FROM discovered_compose_projects WHERE project = 'unarchived-missing'",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .unwrap();
    assert_eq!(unarchived_discovery_state, (0, None));

    let discovery_state = conn
        .query_row(
            "SELECT archived, archived_reason FROM discovered_compose_projects WHERE project = 'legacy-missing'",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .unwrap();
    assert_eq!(discovery_state, (1, Some("user_archive".to_string())));

    drop(conn);
    std::fs::remove_file(&db_path).unwrap();
    let wal_path = db_path.with_extension("sqlite3-wal");
    let shm_path = db_path.with_extension("sqlite3-shm");
    let _ = std::fs::remove_file(wal_path);
    let _ = std::fs::remove_file(shm_path);
}

#[tokio::test]
async fn candidate_migration_links_auditable_pending_rows_and_closes_ambiguous_history() {
    let db_path = temporary_db_path();
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL
);
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
  current_digest TEXT,
  candidate_digest TEXT
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
CREATE TABLE auto_update_pending (
  id TEXT PRIMARY KEY NOT NULL,
  policy_scope_type TEXT NOT NULL,
  policy_scope_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  source_check_job_id TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  due_at TEXT NOT NULL,
  min_age_seconds INTEGER NOT NULL,
  min_version_lag INTEGER NOT NULL,
  status TEXT NOT NULL,
  update_job_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  summary_json TEXT NOT NULL DEFAULT '{}'
);
INSERT INTO schema_migrations (id, applied_at) VALUES
  ('0007_remove_manual_stacks', '2026-01-01T00:00:00Z'),
  ('0008_drop_version_inference_snapshots', '2026-01-01T00:00:00Z'),
  ('0009_add_new_version_notifications', '2026-01-01T00:00:00Z'),
  ('0010_add_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0011_track_candidate_display_tags_in_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0012_track_image_ref_in_new_version_discoveries', '2026-01-01T00:00:00Z'),
  ('0013_add_update_job_stop_controls', '2026-01-01T00:00:00Z');
INSERT INTO services (id, stack_id, current_digest, candidate_digest)
VALUES
  ('service-1', 'stack-1', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'),
  ('service-3', 'stack-1', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'sha256:2222222222222222222222222222222222222222222222222222222222222222'),
  ('service-4', 'stack-1', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'sha256:noncheck');
INSERT INTO jobs (
  id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by,
  reason, created_at, summary_json
) VALUES
  ('schedule-check', 'check', 'stack', 'stack-1', NULL, 'success', 0, 'inherit', 'schedule', 'schedule',
   '2026-04-30T00:00:00Z', '{}'),
  ('unknown-check', 'check', 'stack', 'stack-1', NULL, 'success', 0, 'inherit', 'test', 'ui',
   '2026-04-30T00:00:00Z', '{malformed'),
  ('running-auto-job', 'update', 'service', 'stack-1', 'service-2', 'running', 0, 'inherit', 'auto-policy', 'auto_policy',
   '2026-04-30T00:00:02Z', '{}'),
  ('non-check-schedule', 'update', 'service', 'stack-1', 'service-4', 'success', 0, 'inherit', 'schedule', 'schedule',
   '2026-04-30T00:00:00Z', '{}'),
  ('queued-noncheck-job', 'update', 'service', 'stack-1', 'service-4', 'queued', 0, 'inherit', 'auto-policy', 'auto_policy',
   '2026-04-30T00:00:02Z', '{}'),
  ('queued-duplicate-job', 'update', 'service', 'stack-1', 'service-1', 'queued', 0, 'inherit', 'auto-policy', 'auto_policy',
   '2026-04-30T00:00:03Z', '{}');
INSERT INTO auto_update_pending (
  id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id,
  source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest,
  current_display_tag, first_seen_at, due_at, min_age_seconds, min_version_lag,
  status, update_job_id, created_at, updated_at, summary_json
) VALUES
  ('pending-auditable', 'stack', 'stack-1', 'rule-1', 'stack-1', 'service-1',
   'schedule-check', 'latest', 'latest', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0',
   '2026-04-30T00:00:00Z', '2026-04-30T00:15:00Z', 900, 0,
   'pending', NULL, '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z',
   '{"imageRef":"ghcr.io/acme/app:latest","currentDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'),
  ('pending-duplicate', 'stack', 'stack-1', 'rule-1', 'stack-1', 'service-1',
   'schedule-check', 'latest', 'latest', 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', '1.0.0',
   '2026-04-30T00:00:01Z', '2026-04-30T00:15:00Z', 900, 0,
   'enqueued', 'queued-duplicate-job', '2026-04-30T00:00:01Z', '2026-04-30T00:00:01Z',
   '{"imageRef":"ghcr.io/acme/app:latest","currentDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'),
  ('pending-ambiguous', 'stack', 'stack-1', 'rule-1', 'stack-1', 'service-2',
   'unknown-check', 'latest', 'latest', 'sha256:other', '1.0.0',
   '2026-04-30T00:00:00Z', '2026-04-30T00:15:00Z', 900, 0,
   'pending', 'running-auto-job', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', '{}'),
  ('pending-strict', 'stack', 'stack-1', 'rule-1', 'stack-1', 'service-3',
   'schedule-check', '1.2.3', '1.2.3', 'sha256:2222222222222222222222222222222222222222222222222222222222222222', '1.0.0',
   '2026-04-30T00:00:00Z', '2026-04-30T00:15:00Z', 900, 0,
   'pending', NULL, '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z',
   '{"imageRef":"ghcr.io/acme/app:1.2.3","currentDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'),
  ('pending-noncheck', 'stack', 'stack-1', 'rule-noncheck', 'stack-1', 'service-4',
   'non-check-schedule', 'latest', 'latest', 'sha256:noncheck', '1.0.0',
   '2026-04-30T00:00:00Z', '2026-04-30T00:15:00Z', 900, 0,
   'enqueued', 'queued-noncheck-job', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z',
   '{"imageRef":"ghcr.io/acme/app:latest","currentDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}');
"#,
        )
        .unwrap();
    }

    let db = Db::open(&db_path).await.unwrap();
    let migrated = db
        .call(|conn| {
            let candidate = conn.query_row(
                "SELECT id, status, reason, resolved_version FROM auto_update_candidates WHERE service_id = 'service-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )?;
            let pending = conn.query_row(
                "SELECT candidate_id, status FROM auto_update_pending WHERE id = 'pending-auditable'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )?;
            let ambiguous = conn.query_row(
                "SELECT candidate_id, status, summary_json FROM auto_update_pending WHERE id = 'pending-ambiguous'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?;
            let stop = conn.query_row(
                "SELECT stop_requested_at, stop_requested_by FROM update_job_stop_controls WHERE job_id = 'running-auto-job'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )?;
            let strict = conn.query_row(
                "SELECT status, resolved_version, image_ref, policy_scope_type, policy_scope_id, update_job_id FROM auto_update_candidates WHERE service_id = 'service-3'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )?;
    Ok((candidate, pending, ambiguous, stop, strict))
        })
        .await
        .unwrap();
    assert_eq!(
        migrated.0,
        (
            "service-1:sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            "awaiting_inference".to_string(),
            "migration_pending_history".to_string(),
            None
        )
    );
    assert_eq!(
        migrated.1,
        (
            Some(
                "service-1:sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string()
            ),
            "skipped".to_string()
        )
    );
    assert_eq!(migrated.2.0, None);
    assert_eq!(migrated.2.1, "skipped");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&migrated.2.2).unwrap()["skipReason"],
        "migration_ambiguous_history"
    );
    assert!(
        migrated
            .3
            .0
            .as_deref()
            .is_some_and(|value| { chrono::DateTime::parse_from_rfc3339(value).is_ok() })
    );
    assert_eq!(
        migrated.3.1,
        Some("migration-ambiguous-history".to_string())
    );
    let duplicate = db
        .call(|conn| {
            let pending = conn.query_row(
                "SELECT candidate_id, candidate_digest, status, json_extract(summary_json, '$.skipReason') FROM auto_update_pending WHERE id = 'pending-duplicate'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )?;
            let job = conn.query_row(
                "SELECT status FROM jobs WHERE id = 'queued-duplicate-job'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((pending, job))
        })
        .await
        .unwrap();
    assert_eq!(
        duplicate.0,
        (
            Some(
                "service-1:sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string()
            ),
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            "enqueued".to_string(),
            None
        )
    );
    assert_eq!(duplicate.1, "queued");
    assert_eq!(
        migrated.4,
        (
            "ready".to_string(),
            Some("1.2.3".to_string()),
            "ghcr.io/acme/app".to_string(),
            Some("stack".to_string()),
            Some("stack-1".to_string()),
            None
        )
    );

    let noncheck = db
        .call(|conn| {
            let pending = conn.query_row(
                "SELECT candidate_id, status FROM auto_update_pending WHERE id = 'pending-noncheck'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )?;
            let job = conn.query_row(
                "SELECT status FROM jobs WHERE id = 'queued-noncheck-job'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((pending, job))
        })
        .await
        .unwrap();
    assert_eq!(noncheck.0, (None, "skipped".to_string()));
    assert_eq!(noncheck.1, "cancelled");
    let noncheck_candidate_count = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM auto_update_candidates WHERE service_id = 'service-4'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(noncheck_candidate_count, 0);

    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let wal_path = db_path.with_extension("sqlite3-wal");
    let shm_path = db_path.with_extension("sqlite3-shm");
    let _ = std::fs::remove_file(wal_path);
    let _ = std::fs::remove_file(shm_path);
}

#[tokio::test]
async fn source_provenance_migration_invalidates_pending_candidate_job_mismatch() {
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:candidate', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        for id in ["candidate-check", "pending-check"] {
            conn.execute(
                "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES (?1, 'check', 'service', 'stack', 'service', 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '{}')",
                [id],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &AutoUpdateCandidateInput {
                id: "service:sha256:candidate".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/app".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: "sha256:candidate".to_string(),
                resolved_version: Some("1.4.0".to_string()),
                status: "ready".to_string(),
                reason: Some("digest_bound_version".to_string()),
                attempts: 1,
                retry_at: None,
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "candidate-check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some("sha256:current".to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.reserve_auto_update_pending(
        &AutoUpdatePendingInput {
            id: "pending-mismatched-source".to_string(),
            policy_scope_type: "service".to_string(),
            policy_scope_id: "service".to_string(),
            rule_id: "rule".to_string(),
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            source_check_job_id: "pending-check".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.4.0".to_string(),
            candidate_digest: "sha256:candidate".to_string(),
            current_display_tag: "1.0.0".to_string(),
            first_seen_at: "2026-04-30T00:00:00Z".to_string(),
            due_at: "2026-04-30T00:00:00Z".to_string(),
            min_age_seconds: 0,
            min_version_lag: 0,
            summary_json: serde_json::json!({
                "currentDigest": "sha256:current"
            }),
            candidate_id: Some(candidate.id),
        },
        "2026-04-30T00:00:00Z",
    )
    .await
    .unwrap();
    db.call(move |conn| {
        conn.execute(
            "DELETE FROM schema_migrations WHERE id = '0022_harden_auto_update_source_provenance'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    drop(db);

    let db = Db::open(&db_path).await.unwrap();
    let pending = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT candidate_id, status, summary_json FROM auto_update_pending WHERE id = 'pending-mismatched-source'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(pending.0, None);
    assert_eq!(pending.1, "skipped");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&pending.2).unwrap()["skipReason"],
        "migration_ambiguous_history"
    );

    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}

#[tokio::test]
async fn source_provenance_migration_rejects_pending_baseline_mismatch() {
    let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
    let current_digest = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let candidate_digest =
        "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let stale_digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    db.call(move |conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', ?1, ?2, 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            rusqlite::params![current_digest, candidate_digest],
        )?;
        conn.execute(
            "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES ('check', 'check', 'service', 'stack', 'service', 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '{}')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let candidate = db
        .upsert_auto_update_candidate(
            &AutoUpdateCandidateInput {
                id: "candidate-baseline-mismatch".to_string(),
                stack_id: "stack".to_string(),
                service_id: "service".to_string(),
                image_ref: "ghcr.io/acme/app".to_string(),
                raw_tag: "latest".to_string(),
                candidate_digest: candidate_digest.to_string(),
                resolved_version: Some("1.4.0".to_string()),
                status: "ready".to_string(),
                reason: Some("digest_bound_version".to_string()),
                attempts: 0,
                retry_at: None,
                discovered_at: "2026-04-30T00:00:00Z".to_string(),
                source_job_id: "check".to_string(),
                source: "schedule".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.0.0".to_string(),
                current_digest: Some(stale_digest.to_string()),
            },
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
    db.reserve_auto_update_pending(
        &AutoUpdatePendingInput {
            id: "pending-baseline-mismatch".to_string(),
            policy_scope_type: "service".to_string(),
            policy_scope_id: "service".to_string(),
            rule_id: "rule".to_string(),
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            source_check_job_id: "check".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.4.0".to_string(),
            current_display_tag: "1.0.0".to_string(),
            candidate_digest: candidate_digest.to_string(),
            first_seen_at: "2026-04-30T00:00:00Z".to_string(),
            due_at: "2026-04-30T00:00:00Z".to_string(),
            min_age_seconds: 0,
            min_version_lag: 0,
            summary_json: serde_json::json!({"currentDigest": stale_digest}),
            candidate_id: Some(candidate.id),
        },
        "2026-04-30T00:01:00Z",
    )
    .await
    .unwrap();

    db.call(|conn| {
        conn.execute(
            "DELETE FROM schema_migrations WHERE id = '0022_harden_auto_update_source_provenance'",
            [],
        )?;
        super::apply_migration_0022_harden_auto_update_source_provenance(conn)?;
        Ok(())
    })
    .await
    .unwrap();

    let pending = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT candidate_id, status, json_extract(summary_json, '$.skipReason') FROM auto_update_pending WHERE id = 'pending-baseline-mismatch'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(
        pending,
        (
            None,
            "skipped".to_string(),
            Some("migration_ambiguous_history".to_string())
        )
    );
}

#[tokio::test]
async fn notification_digest_migration_deduplicates_equivalent_active_rows() {
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    db.call(|conn| {
        conn.execute(
            r#"
INSERT INTO new_version_notifications (
  id, service_id, job_id, reason, image_ref, image_tag, current_tag,
  current_display_tag, candidate_tag, candidate_display_tag, candidate_digest,
  status, sent_channels_json, created_at, sent_at, last_error
) VALUES
  ('notification-pending', 'service', 'job-1', 'new_version', 'ghcr.io/acme/app', 'latest', 'latest',
   '1.0.0', 'latest', 'latest', 'ABC', 'pending', '["webhook"]', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', 'delivery retry'),
  ('notification-sent', 'service', 'job-2', 'new_version', 'ghcr.io/acme/app', 'latest', 'latest',
   '1.0.0', 'latest', '1.2.3', 'sha256:abc', 'sent', '["slack"]', '2026-04-30T00:00:01Z', NULL, NULL)
"#,
            [],
        )?;
        conn.execute(
            "DELETE FROM schema_migrations WHERE id = '0025_normalize_new_version_notification_digest_identity'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    drop(db);

    let db = Db::open(&db_path).await.unwrap();
    let rows = db
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, candidate_digest, status, last_error, sent_channels_json, sent_at FROM new_version_notifications ORDER BY id",
            )?;
            Ok(stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(rows[0].1, "sha256:abc");
    assert_eq!(rows[0].2, "superseded");
    assert_eq!(rows[0].3.as_deref(), Some("delivery retry"));
    assert_eq!(rows[0].4, "[\"webhook\"]");
    assert_eq!(rows[0].5.as_deref(), Some("2026-04-30T00:00:00Z"));
    assert_eq!(rows[1].1, "sha256:abc");
    assert_eq!(rows[1].2, "sent");
    assert_eq!(rows[1].3.as_deref(), Some("delivery retry"));
    assert_eq!(rows[1].4, "[\"slack\",\"webhook\"]");
    assert_eq!(rows[1].5.as_deref(), Some("2026-04-30T00:00:00Z"));

    drop(db);
    let db = Db::open(&db_path).await.unwrap();
    let active_count = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM new_version_notifications WHERE service_id = 'service' AND candidate_digest = 'sha256:abc' AND status IN ('pending', 'sent')",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(active_count, 1);
    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}

#[test]
fn anomaly_occurrence_migration_backfills_pending_legacy_states() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE notification_anomaly_states (
  owner TEXT NOT NULL,
  repo TEXT NOT NULL,
  state TEXT NOT NULL,
  active INTEGER NOT NULL,
  occurrence_count INTEGER NOT NULL,
  last_error TEXT,
  last_seen_at TEXT NOT NULL,
  notification_pending INTEGER NOT NULL DEFAULT 0,
  notification_batch_id TEXT,
  notification_sent_channels_json TEXT NOT NULL DEFAULT '[]',
  PRIMARY KEY (owner, repo)
);
INSERT INTO notification_anomaly_states (
  owner, repo, state, active, occurrence_count, last_error, last_seen_at,
  notification_pending, notification_batch_id, notification_sent_channels_json
) VALUES
  ('acme', 'api', 'timeout', 1, 3, 'upstream timeout', '2026-04-30T00:00:00Z', 1, 'legacy-batch', '["webhook"]'),
  ('acme', 'web', 'rate_limit', 1, 2, 'rate limited', '2026-04-30T00:01:00Z', 1, NULL, '[]'),
  ('acme', 'worker', 'timeout', 0, 1, NULL, '2026-04-30T00:02:00Z', 0, NULL, '[]');
"#,
    )
    .unwrap();

    super::schema_notification_inbox::apply_migration_0033_add_notification_anomaly_occurrences(
        &mut conn,
    )
    .unwrap();
    super::schema_notification_inbox::apply_migration_0040_backfill_notification_anomaly_occurrences(
        &mut conn,
    )
    .unwrap();

    let rows = conn
        .prepare(
            "SELECT owner, repo, state, last_error, occurrence_count, batch_id, notification_pending, sent_channels_json, created_at FROM notification_anomaly_occurrences ORDER BY repo",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        rows,
        vec![
            (
                "acme".to_string(),
                "api".to_string(),
                "timeout".to_string(),
                Some("upstream timeout".to_string()),
                3,
                "legacy-batch".to_string(),
                1,
                "[\"webhook\"]".to_string(),
                "2026-04-30T00:00:00Z".to_string(),
            ),
            (
                "acme".to_string(),
                "web".to_string(),
                "rate_limit".to_string(),
                Some("rate limited".to_string()),
                2,
                "migration-0040:acme/web".to_string(),
                1,
                "[]".to_string(),
                "2026-04-30T00:01:00Z".to_string(),
            ),
        ]
    );
}

#[tokio::test]
async fn hydration_migration_survives_equivalent_legacy_candidate_rows() {
    let current_digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let candidate_digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('legacy-stack', 'legacy-stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('legacy-service', 'legacy-stack', 'service', 'ghcr.io/acme/app', 'latest', ?1, ?2, 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            rusqlite::params![current_digest, candidate_digest],
        )?;
        conn.execute(
            "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, finished_at, summary_json) VALUES ('legacy-check', 'check', 'service', 'legacy-stack', 'legacy-service', 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '2026-04-30T00:00:01Z', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('legacy-service', 'ghcr.io/acme/app:latest', 'legacy-check', '2026-04-30T00:00:00Z', ?1, '1.0.0', 'latest', 'latest', ?2, '1.2.3')",
            rusqlite::params![current_digest, candidate_digest],
        )?;
        for (id, digest) in [
            ("legacy-uppercase", candidate_digest),
            ("legacy-prefixed", "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
        ] {
            conn.execute(
                "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, attempts, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at) VALUES (?1, 'legacy-stack', 'legacy-service', 'ghcr.io/acme/app', 'latest', ?2, 'awaiting_inference', 'version_inference_pending', 0, '2026-04-30T00:00:00Z', 'legacy-check', 'schedule', 'latest', '1.0.0', ?3, '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z')",
                rusqlite::params![id, digest, current_digest],
            )?;
        }
        conn.execute(
            "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, attempts, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at, superseded_by_candidate_id) VALUES ('legacy-observer', 'legacy-stack', 'legacy-service', 'ghcr.io/acme/app', 'latest', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'superseded', 'newer_candidate', 0, '2026-04-29T00:00:00Z', 'legacy-check', 'schedule', 'latest', '1.0.0', ?1, '2026-04-29T00:00:00Z', '2026-04-29T00:00:00Z', 'legacy-uppercase')",
            rusqlite::params![current_digest],
        )?;
        conn.execute(
            "DELETE FROM schema_migrations WHERE id IN ('0020_hydrate_auto_update_candidates_from_discoveries', '0024_normalize_auto_update_digest_identity')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    drop(db);

    let db = Db::open(&db_path).await.unwrap();
    let candidates = db
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT candidate_digest FROM auto_update_candidates WHERE service_id = 'legacy-service' AND candidate_digest = ?1 ORDER BY id",
            )?;
            Ok(stmt
                .query_map([format!("sha256:{candidate_digest}")], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(
        candidates,
        vec!["sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"]
    );
    let superseded_by = db
        .call(|conn| {
            Ok(conn.query_row(
                "SELECT superseded_by_candidate_id FROM auto_update_candidates WHERE id = 'legacy-observer'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(superseded_by.as_deref(), Some("legacy-prefixed"));
    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}

#[tokio::test]
async fn hydration_migration_normalizes_a_single_prefixless_candidate_before_upsert() {
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('single-legacy-stack', 'single-legacy-stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('single-legacy-service', 'single-legacy-stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, finished_at, summary_json) VALUES ('single-legacy-check', 'check', 'service', 'single-legacy-stack', 'single-legacy-service', 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '2026-04-30T00:00:01Z', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO service_new_version_discoveries (service_id, image_ref, source_job_id, discovered_at, current_digest, current_display_tag, current_tag, candidate_tag, candidate_digest, candidate_display_tag) VALUES ('single-legacy-service', 'ghcr.io/acme/app:latest', 'single-legacy-check', '2026-04-30T00:00:00Z', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '1.0.0', 'latest', 'latest', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'latest')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, attempts, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at) VALUES ('single-legacy-candidate', 'single-legacy-stack', 'single-legacy-service', 'ghcr.io/acme/app', 'latest', '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', 'awaiting_inference', 'version_inference_pending', 0, '2026-04-30T00:00:00Z', 'single-legacy-check', 'schedule', 'latest', '1.0.0', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z')",
            [],
        )?;
        conn.execute(
            "DELETE FROM schema_migrations WHERE id IN ('0020_hydrate_auto_update_candidates_from_discoveries', '0024_normalize_auto_update_digest_identity')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    drop(db);

    let reopened = Db::open(&db_path).await;
    assert!(
        reopened.is_ok(),
        "startup migration failed: {:?}",
        reopened.err()
    );
    let db = reopened.unwrap();
    let candidate = db
        .get_auto_update_candidate(
            "single-legacy-service",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(candidate.id, "single-legacy-candidate");
    assert_eq!(
        candidate.candidate_digest,
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    assert_eq!(candidate.source, "schedule");
    assert_eq!(
        candidate.hydration_origin.as_deref(),
        Some("discovery_history")
    );

    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}

#[tokio::test]
async fn invalid_digest_migration_rejects_blank_candidate_identities() {
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    db.call(|conn| {
        conn.execute(
            "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('blank-stack', 'blank-stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('blank-service', 'blank-stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '   ', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, created_at, updated_at, summary_json) VALUES ('blank-pending', 'stack', 'blank-stack', 'rule', 'blank-stack', 'blank-service', 'missing-check', 'latest', 'latest', '   ', '1.0.0', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', 0, 0, 'pending', NULL, '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', '{}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, reason, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, created_at, updated_at) VALUES ('malformed-candidate', 'blank-stack', 'blank-service', 'ghcr.io/acme/app', 'latest', 'not-a-digest', 'ready', 'digest_bound_version', '2026-04-30T00:00:00Z', 'missing-check', 'schedule', 'latest', '1.0.0', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES ('malformed-candidate-job', 'update', 'service', 'blank-stack', 'blank-service', 'queued', 0, 'inherit', 'auto-policy', 'auto_policy', '2026-04-30T00:00:00Z', '{\"mode\":\"apply\"}')",
            [],
        )?;
        conn.execute(
            "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, current_digest, first_seen_at, due_at, min_age_seconds, min_version_lag, status, update_job_id, candidate_id, created_at, updated_at, summary_json) VALUES ('malformed-link-pending', 'stack', 'blank-stack', 'malformed-rule', 'blank-stack', 'blank-service', 'missing-check', 'latest', '1.1.0', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', '1.0.0', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', 0, 0, 'enqueued', 'malformed-candidate-job', 'malformed-candidate', '2026-04-30T00:00:00Z', '2026-04-30T00:00:00Z', '{}')",
            [],
        )?;
        conn.execute(
            "DELETE FROM schema_migrations WHERE id = '0026_reject_invalid_auto_update_digest_identity'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    drop(db);

    let db = Db::open(&db_path).await.unwrap();
    let result = db
        .call(|conn| {
            let service_digest = conn.query_row(
                "SELECT candidate_digest FROM services WHERE id = 'blank-service'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )?;
            let pending_status = conn.query_row(
                "SELECT status, json_extract(summary_json, '$.skipReason') FROM auto_update_pending WHERE id = 'blank-pending'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )?;
            let linked_pending = conn.query_row(
                "SELECT candidate_id, status FROM auto_update_pending WHERE id = 'malformed-link-pending'",
                [],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
            )?;
            let linked_job_status = conn.query_row(
                "SELECT status FROM jobs WHERE id = 'malformed-candidate-job'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((service_digest, pending_status, linked_pending, linked_job_status))
        })
        .await
        .unwrap();
    assert_eq!(result.0, None);
    assert_eq!(
        result.1,
        (
            "skipped".to_string(),
            Some("migration_ambiguous_history".to_string())
        )
    );
    assert_eq!(result.2, (None, "skipped".to_string()));
    assert_eq!(result.3, "cancelled");

    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}

#[tokio::test]
async fn notification_summary_backfill_updates_only_reconstructable_unread_items() {
    let db_path = temporary_db_path();
    let db = Db::open(&db_path).await.unwrap();
    let result = db
        .call(|conn| {
            conn.execute(
                r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES ('stack-notification', '生产服务', 'path', '[]', '[]', 0, 0,
          '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')
"#,
                [],
            )?;
            for (id, name) in [
                ("service-api", "支付 API"),
                ("service-worker", "后台 Worker"),
                ("service-web", "前端"),
            ] {
                conn.execute(
                    r#"
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag, auto_rollback,
  backup_targets_bind_paths_json, backup_targets_volume_names_json,
  created_at, updated_at
) VALUES (?1, 'stack-notification', ?2, 'ghcr.io/acme/app', 'latest', 0,
          '[]', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')
"#,
                    rusqlite::params![id, name],
                )?;
            }
            conn.execute(
                r#"
INSERT INTO jobs (
  id, type, scope, stack_id, service_id, status, allow_arch_mismatch,
  backup_mode, created_by, reason, created_at, finished_at, summary_json
) VALUES ('job-notification', 'update', 'service', 'stack-notification',
          'service-api', 'success', 0, 'inherit', 'user', 'manual',
          '2026-10-01T00:00:00Z', '2026-10-01T00:01:00Z', '{"progress":{"message":"raw progress"}}')
"#,
                [],
            )?;

            for (id, kind, source_job_id, read_at) in [
                ("item-job", "job_finished", Some("job-notification"), None),
                ("item-job-missing", "job_finished", Some("missing-job"), None),
                ("item-job-read", "job_finished", Some("job-notification"), Some("2026-10-01T00:02:00Z")),
                ("item-version", "new_version_discovered", Some("job-notification"), None),
                ("item-version-missing", "new_version_discovered", Some("job-notification"), None),
                ("item-version-digest", "new_version_discovered", Some("job-notification"), None),
                ("item-version-internal", "new_version_discovered", Some("job-notification"), None),
                ("item-version-json", "new_version_discovered", Some("job-notification"), None),
                ("item-ghcr", "ghcr_webhook_anomaly", Some("job-notification"), None),
            ] {
                conn.execute(
                    r#"
INSERT INTO notification_items (
  id, kind, identity_key, title, body, target_url, source_job_id, created_at, read_at
) VALUES (?1, ?2, ?3, '旧标题', '旧正文', '/queue/existing-target', ?4,
          '2026-10-01T00:01:00Z', ?5)
"#,
                    rusqlite::params![id, kind, format!("identity:{id}"), source_job_id, read_at],
                )?;
            }

            for (id, service_id, current_tag, candidate_tag) in [
                ("version-api", "service-api", "1.4.0", "1.5.0"),
                ("version-worker", "service-worker", "2.0.1", "2.0.2"),
                ("version-web", "service-web", "5.1.0", "5.2.0"),
            ] {
                conn.execute(
                    r#"
INSERT INTO new_version_notifications (
  id, service_id, job_id, reason, image_ref, image_tag, current_tag,
  current_display_tag, candidate_tag, candidate_display_tag, candidate_digest,
  status, created_at, notification_item_id
) VALUES (?1, ?2, 'job-notification', 'scheduled', 'ghcr.io/acme/app', 'latest',
          ?3, ?3, ?4, ?4, 'sha256:candidate', 'sent', '2026-10-01T00:01:00Z', 'item-version')
"#,
                    rusqlite::params![id, service_id, current_tag, candidate_tag],
                )?;
            }
            conn.execute(
                r#"
INSERT INTO new_version_notifications (
  id, service_id, job_id, reason, image_ref, image_tag, current_tag,
  current_display_tag, candidate_tag, candidate_display_tag, candidate_digest,
  status, created_at, notification_item_id
) VALUES ('version-digest', 'service-api', 'job-notification', 'scheduled',
          'ghcr.io/acme/app', 'latest',
          '0123456789abcdef0123456789abcdef01234567',
          '0123456789abcdef0123456789abcdef01234567',
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
          'sha256:version-digest', 'sent', '2026-10-01T00:01:00Z', 'item-version-digest')
"#,
                [],
            )?;
            conn.execute(
                r#"
INSERT INTO new_version_notifications (
  id, service_id, job_id, reason, image_ref, image_tag, current_tag,
  current_display_tag, candidate_tag, candidate_display_tag, candidate_digest,
  status, created_at, notification_item_id
) VALUES ('version-internal', 'service-api', 'job-notification', 'scheduled',
          'ghcr.io/acme/app', 'latest', 'service-internal', 'service-internal',
          '2.5.0', '2.5.0', 'sha256:version-internal', 'sent',
          '2026-10-01T00:01:00Z', 'item-version-internal')
"#,
                [],
            )?;
            conn.execute(
                r#"
INSERT INTO new_version_notifications (
  id, service_id, job_id, reason, image_ref, image_tag, current_tag,
  current_display_tag, candidate_tag, candidate_display_tag, candidate_digest,
  status, created_at, notification_item_id
) VALUES ('version-json', 'service-api', 'job-notification', 'scheduled',
          'ghcr.io/acme/app', 'latest', '{"error":"timeout"}',
          '{"error":"timeout"}', '2.5.0', '2.5.0', 'sha256:version-json',
          'sent', '2026-10-01T00:01:00Z', 'item-version-json')
"#,
                [],
            )?;
            for (id, owner, repo, state) in [
                ("anomaly-api", "acme", "api", "missing"),
                ("anomaly-worker", "acme", "worker", "error"),
            ] {
                conn.execute(
                    r#"
INSERT INTO notification_anomaly_occurrences (
  id, owner, repo, state, occurrence_count, batch_id, notification_item_id, created_at
) VALUES (?1, ?2, ?3, ?4, 1, 'batch-notification', 'item-ghcr', '2026-10-01T00:01:00Z')
"#,
                    rusqlite::params![id, owner, repo, state],
                )?;
            }

            let unread_before = conn.query_row(
                "SELECT COUNT(*) FROM notification_items WHERE read_at IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            conn.execute(
                "DELETE FROM schema_migrations WHERE id = '0042_backfill_notification_summaries'",
                [],
            )?;
            super::schema_notification_inbox::apply_migration_0042_backfill_notification_summaries(conn)?;
            super::schema_notification_inbox::apply_migration_0042_backfill_notification_summaries(conn)?;

            let rows = conn
                .prepare(
                    "SELECT id, kind, identity_key, title, body, target_url, source_job_id, created_at, read_at FROM notification_items ORDER BY id",
                )?
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let unread_after = conn.query_row(
                "SELECT COUNT(*) FROM notification_items WHERE read_at IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            let migration_count = conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0042_backfill_notification_summaries'",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            Ok((rows, unread_before, unread_after, migration_count))
        })
        .await
        .unwrap();

    let (rows, unread_before, unread_after, migration_count) = result;
    let row = |id: &str| rows.iter().find(|item| item.0 == id).unwrap();
    assert_eq!(row("item-job").0, "item-job");
    assert_eq!(row("item-job").1, "job_finished");
    assert_eq!(row("item-job").2, "identity:item-job");
    assert_eq!(row("item-job").3, "更新任务已成功");
    assert_eq!(row("item-job").4, "服务「支付 API」的更新操作已完成。");
    assert_eq!(row("item-job").5, "/queue/existing-target");
    assert_eq!(row("item-job").6.as_deref(), Some("job-notification"));
    assert_eq!(row("item-job").7, "2026-10-01T00:01:00Z");
    assert_eq!(row("item-job").8, None);
    assert_eq!(row("item-job-missing").3, "旧标题");
    assert_eq!(row("item-job-missing").4, "旧正文");
    assert_eq!(row("item-job-read").3, "旧标题");
    assert_eq!(row("item-job-read").4, "旧正文");
    assert_eq!(row("item-job-read").5, "/queue/existing-target");
    assert_eq!(row("item-job-read").6.as_deref(), Some("job-notification"));
    assert_eq!(row("item-job-read").7, "2026-10-01T00:01:00Z");
    assert_eq!(
        row("item-job-read").8.as_deref(),
        Some("2026-10-01T00:02:00Z")
    );
    assert_eq!(row("item-version").3, "发现 3 个新版本");
    assert_eq!(
        row("item-version").4,
        "支付 API：1.4.0 -> 1.5.0\n前端：5.1.0 -> 5.2.0\n另有 1 个服务有新版本。"
    );
    assert_eq!(row("item-version-missing").3, "旧标题");
    assert_eq!(row("item-version-missing").4, "旧正文");
    assert_eq!(row("item-version-digest").3, "旧标题");
    assert_eq!(row("item-version-digest").4, "旧正文");
    assert_eq!(row("item-version-internal").3, "旧标题");
    assert_eq!(row("item-version-internal").4, "旧正文");
    assert_eq!(row("item-version-json").3, "旧标题");
    assert_eq!(row("item-version-json").4, "旧正文");
    assert_eq!(row("item-ghcr").3, "GHCR Webhook 有 2 项异常");
    assert_eq!(
        row("item-ghcr").4,
        "acme/api：未在 GHCR 找到关联仓库\nacme/worker：Webhook 检查失败"
    );
    assert_eq!(unread_before, 8);
    assert_eq!(unread_after, unread_before);
    assert_eq!(migration_count, 1);

    drop(db);
    std::fs::remove_file(&db_path).unwrap();
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite3-shm"));
}
