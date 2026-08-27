use super::*;

fn summary_artifact_path(summary_json: &str, stack_id: &str) -> Option<String> {
    let summary = serde_json::from_str::<serde_json::Value>(summary_json).ok()?;
    summary
        .get("stacks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|stack| stack.get("stackId").and_then(serde_json::Value::as_str) == Some(stack_id))
        .and_then(|stack| stack.get("backup"))
        .and_then(|backup| backup.get("artifactPath"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
}

impl Db {
    pub async fn insert_backup(
        &self,
        backup_id: &str,
        stack_id: &str,
        job_id: &str,
        created_at: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let stack_id = stack_id.to_string();
        let job_id = job_id.to_string();
        let created_at = created_at.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO backups (id, stack_id, job_id, status, created_at)
VALUES (?1, ?2, ?3, 'running', ?4)
"#,
                params![backup_id, stack_id, job_id, created_at],
            )?;
            Ok(())
        })
        .await
        .context("insert backup")
    }

    pub async fn finish_backup(
        &self,
        backup_id: &str,
        status: &str,
        finished_at: &str,
        artifact_path: Option<&str>,
        size_bytes: Option<u64>,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let status = status.to_string();
        let finished_at = finished_at.to_string();
        let artifact_path = artifact_path.map(|s| s.to_string());
        let size_bytes = size_bytes.map(|v| v as i64);
        let error = error.map(|s| s.to_string());
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE backups
SET
  status = ?2,
  finished_at = ?3,
  artifact_path = ?4,
  size_bytes = ?5,
  error = ?6
WHERE id = ?1
"#,
                params![
                    backup_id,
                    status,
                    finished_at,
                    artifact_path,
                    size_bytes,
                    error
                ],
            )?;
            Ok(())
        })
        .await
        .context("finish backup")
    }

    pub async fn schedule_backup_cleanup(
        &self,
        backup_id: &str,
        cleanup_after: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let cleanup_after = cleanup_after.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET cleanup_after = ?2 WHERE id = ?1",
                params![backup_id, cleanup_after],
            )?;
            Ok(())
        })
        .await
        .context("schedule backup cleanup")
    }

    pub async fn mark_backup_deleted(
        &self,
        backup_id: &str,
        deleted_at: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let deleted_at = deleted_at.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET deleted_at = ?2, missing_at = NULL, last_cleanup_attempt_at = ?2, last_cleanup_error = NULL WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL",
                params![backup_id, deleted_at],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup deleted")
    }

    pub async fn mark_backup_cleanup_attempt(
        &self,
        backup_id: &str,
        attempted_at: &str,
        stale_before: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let attempted_at = attempted_at.to_string();
        let stale_before = stale_before.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET last_cleanup_attempt_at = ?2 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND (last_cleanup_error IS NULL OR last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_intent__:*' OR last_cleanup_attempt_at IS NULL OR last_cleanup_attempt_at < ?3)",
                params![backup_id, attempted_at, stale_before],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup cleanup attempt")
    }

    #[cfg(test)]
    pub async fn mark_backup_missing(
        &self,
        backup_id: &str,
        missing_at: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let missing_at = missing_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET missing_at = ?2, deleted_at = NULL, last_cleanup_attempt_at = ?2, last_cleanup_error = NULL WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL",
                params![backup_id, missing_at],
            )?;
            Ok(())
        })
        .await
        .context("mark backup missing")
    }

    pub async fn mark_backup_missing_after_cleanup(
        &self,
        backup_id: &str,
        missing_at: &str,
        stale_before: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let missing_at = missing_at.to_string();
        let stale_before = stale_before.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET missing_at = ?2, deleted_at = NULL, last_cleanup_attempt_at = ?2, last_cleanup_error = NULL WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND (last_cleanup_error IS NULL OR last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_completed__:*') AND (last_cleanup_error IS NULL OR last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_intent__:*' OR last_cleanup_attempt_at IS NULL OR last_cleanup_attempt_at < ?3)",
                params![backup_id, missing_at, stale_before],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup missing after cleanup")
    }

    pub async fn mark_backup_missing_for_intent(
        &self,
        backup_id: &str,
        missing_at: &str,
        intent: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let missing_at = missing_at.to_string();
        let intent = intent.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET missing_at = ?2, deleted_at = NULL, last_cleanup_attempt_at = ?2, last_cleanup_error = NULL WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND last_cleanup_error = ?3",
                params![backup_id, missing_at, intent],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup missing for intent")
    }

    pub async fn mark_backup_cleanup_delete_started(
        &self,
        backup_id: &str,
        attempted_at: &str,
        intent: &str,
        stale_before: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let attempted_at = attempted_at.to_string();
        let intent = intent.to_string();
        let stale_before = stale_before.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET last_cleanup_attempt_at = ?2, last_cleanup_error = ?3 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND (last_cleanup_error IS NULL OR last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_completed__:*') AND (last_cleanup_error IS NULL OR last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_intent__:*' OR last_cleanup_attempt_at IS NULL OR last_cleanup_attempt_at < ?4)",
                params![backup_id, attempted_at, intent, stale_before],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup cleanup delete started")
    }

    #[cfg(test)]
    pub async fn mark_backup_cleanup_failed(
        &self,
        backup_id: &str,
        attempted_at: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let attempted_at = attempted_at.to_string();
        let error = error.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET last_cleanup_attempt_at = ?2, last_cleanup_error = ?3 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND (last_cleanup_error IS NULL OR (last_cleanup_error <> '__dockrev_cleanup_delete_intent__' AND last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_intent__:*' AND last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_completed__:*'))",
                params![backup_id, attempted_at, error],
            )?;
            Ok(())
        })
        .await
        .context("mark backup cleanup failed")
    }

    pub async fn mark_backup_cleanup_failed_retriable(
        &self,
        backup_id: &str,
        attempted_at: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let attempted_at = attempted_at.to_string();
        let error = error.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET last_cleanup_attempt_at = ?2, last_cleanup_error = ?3 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND (last_cleanup_error IS NULL OR (last_cleanup_error <> '__dockrev_cleanup_delete_intent__' AND last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_completed__:*' AND last_cleanup_error NOT GLOB '__dockrev_cleanup_delete_intent__:*'))",
                params![backup_id, attempted_at, error],
            )?;
            Ok(())
        })
        .await
        .context("mark retriable backup cleanup failure")
    }

    pub async fn mark_backup_cleanup_failed_for_intent(
        &self,
        backup_id: &str,
        attempted_at: &str,
        intent: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let attempted_at = attempted_at.to_string();
        let intent = intent.to_string();
        let error = error.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET last_cleanup_attempt_at = ?2, last_cleanup_error = ?4 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND last_cleanup_error = ?3",
                params![backup_id, attempted_at, intent, error],
            )?;
            Ok(())
        })
        .await
        .context("mark backup cleanup failure for intent")
    }

    pub async fn mark_backup_cleanup_delete_completed(
        &self,
        backup_id: &str,
        intent: &str,
        completed: &str,
    ) -> anyhow::Result<bool> {
        let backup_id = backup_id.to_string();
        let intent = intent.to_string();
        let completed = completed.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                "UPDATE backups SET last_cleanup_error = ?3 WHERE id = ?1 AND deleted_at IS NULL AND missing_at IS NULL AND last_cleanup_error = ?2",
                params![backup_id, intent, completed],
            )?;
            Ok(changed == 1)
        })
        .await
        .context("mark backup cleanup delete completed")
    }

    pub async fn list_due_backup_cleanups(
        &self,
        now: &str,
    ) -> anyhow::Result<Vec<BackupCleanupItem>> {
        let now = now.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT b.id, b.stack_id, b.job_id, b.artifact_path, b.last_cleanup_error,
       COALESCE(j.summary_json, '{}')
FROM backups b
LEFT JOIN jobs j ON j.id = b.job_id
WHERE
  b.status = 'success'
  AND b.deleted_at IS NULL
  AND b.missing_at IS NULL
  AND b.cleanup_after IS NOT NULL
  AND b.cleanup_after <= ?1
ORDER BY b.cleanup_after ASC
LIMIT 50
"#,
            )?;
            let rows = stmt.query_map(params![now], |row| {
                let artifact_path = row
                    .get::<_, Option<String>>(3)?
                    .filter(|path| !path.trim().is_empty())
                    .or_else(|| {
                        row.get::<_, String>(5).ok().and_then(|summary| {
                            summary_artifact_path(&summary, &row.get::<_, String>(1).ok()?)
                        })
                    });
                let Some(artifact_path) = artifact_path else {
                    return Ok(None);
                };
                Ok(Some(BackupCleanupItem {
                    id: row.get(0)?,
                    stack_id: row.get(1)?,
                    job_id: row.get(2)?,
                    artifact_path,
                    last_cleanup_error: row.get(4)?,
                }))
            })?;
            Ok(rows
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect())
        })
        .await
        .context("list due backup cleanups")
    }

    pub async fn list_success_backup_ids_for_stack(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT b.id, b.artifact_path, COALESCE(j.summary_json, '{}')
FROM backups b
LEFT JOIN jobs j ON j.id = b.job_id
WHERE b.stack_id = ?1 AND b.status = 'success' AND b.deleted_at IS NULL AND b.missing_at IS NULL
ORDER BY b.created_at DESC, b.id DESC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id.clone()], |row| {
                let artifact_path = row
                    .get::<_, Option<String>>(1)?
                    .filter(|path| !path.trim().is_empty())
                    .or_else(|| {
                        row.get::<_, String>(2)
                            .ok()
                            .and_then(|summary| summary_artifact_path(&summary, &stack_id))
                    });
                artifact_path.map(|_| row.get::<_, String>(0)).transpose()
            })?;
            Ok(rows
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect())
        })
        .await
        .context("list success backups for stack")
    }

    pub async fn list_service_backup_records(
        &self,
        stack_id: &str,
        service_id: &str,
    ) -> anyhow::Result<Vec<ServiceBackupRecordRow>> {
        let stack_id = stack_id.to_string();
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  b.id,
  b.job_id,
  COALESCE(j.scope, 'unassociated'),
  b.status,
  b.created_at,
  b.finished_at,
  b.artifact_path,
  b.size_bytes,
  b.cleanup_after,
  b.deleted_at,
  b.last_cleanup_attempt_at,
  CASE
    WHEN b.last_cleanup_error = '__dockrev_cleanup_delete_intent__'
      OR b.last_cleanup_error GLOB '__dockrev_cleanup_delete_intent__:*'
      OR b.last_cleanup_error GLOB '__dockrev_cleanup_delete_completed__:*' THEN NULL
    ELSE b.last_cleanup_error
  END,
  b.missing_at,
  b.error,
  COALESCE(j.summary_json, '{}')
FROM backups b
LEFT JOIN jobs j ON j.id = b.job_id
WHERE b.stack_id = ?1
  AND EXISTS (
    SELECT 1
    FROM json_each(COALESCE(json_extract(j.summary_json, '$.targets'), '[]')) AS t
    JOIN services target_service
      ON target_service.id = json_extract(t.value, '$.serviceId')
    WHERE target_service.id = ?2
      AND target_service.stack_id = ?1
  )
ORDER BY b.created_at DESC, b.id DESC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id, service_id], |row| {
                let summary_json: String = row.get(14)?;
                let job_summary_json = serde_json::from_str(&summary_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        14,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Ok(ServiceBackupRecordRow {
                    backup_id: row.get(0)?,
                    job_id: row.get(1)?,
                    scope: row.get(2)?,
                    status: row.get(3)?,
                    created_at: row.get(4)?,
                    finished_at: row.get(5)?,
                    artifact_path: row.get(6)?,
                    size_bytes: row
                        .get::<_, Option<i64>>(7)?
                        .map(|value| value.max(0) as u64),
                    cleanup_after: row.get(8)?,
                    deleted_at: row.get(9)?,
                    last_cleanup_attempt_at: row.get(10)?,
                    last_cleanup_error: row.get(11)?,
                    missing_at: row.get(12)?,
                    error: row.get(13)?,
                    job_summary_json,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service backup records")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn purged_job_backup_is_not_listed_for_any_service_history() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute_batch(
                r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES ('stack_1', 'Stack', 'compose', '[]', '[]', 0, 0,
  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag, auto_rollback,
  backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at
) VALUES
  ('service_a', 'stack_1', 'a', 'example/a', 'latest', 1, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('service_b', 'stack_1', 'b', 'example/b', 'latest', 1, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO jobs (
  id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode,
  created_by, reason, created_at, finished_at, summary_json
) VALUES (
  'job_1', 'update', 'service', 'stack_1', 'service_a', 'success', 0, 'inherit',
  'test', 'test', '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z', '{}'
);
INSERT INTO backups (
  id, stack_id, job_id, status, created_at, finished_at, artifact_path, size_bytes
) VALUES (
  'backup_1', 'stack_1', 'job_1', 'success', '2026-01-01T00:00:00Z',
  '2026-01-01T00:01:00Z', '/backups/backup_1.tar', 42
);
"#,
            )?;
            Ok(())
        })
        .await
        .unwrap();

        assert_eq!(
            db.purge_expired_terminal_jobs("2026-02-01T00:00:00Z", 100)
                .await
                .unwrap(),
            1
        );
        let backup_job_id = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT job_id FROM backups WHERE id = 'backup_1'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(backup_job_id, None);
        assert!(
            db.list_service_backup_records("stack_1", "service_a")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            db.list_service_backup_records("stack_1", "service_b")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn cleanup_state_is_persisted_and_missing_records_leave_candidates() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute_batch(
                r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES ('stack_1', 'Stack', 'compose', '[]', '[]', 0, 0,
  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO backups (id, stack_id, status, created_at, artifact_path, cleanup_after)
VALUES
  ('backup_due', 'stack_1', 'success', '2026-01-01T00:00:00Z', '/backups/due.tar.zst', '2026-01-02T00:00:00Z'),
  ('backup_missing', 'stack_1', 'success', '2025-12-01T00:00:00Z', '/backups/missing.tar.zst', '2025-12-02T00:00:00Z');
"#,
            )?;
            Ok(())
        })
        .await
        .unwrap();

        db.mark_backup_cleanup_attempt(
            "backup_due",
            "2026-01-03T00:00:00Z",
            "2026-01-02T23:00:00Z",
        )
        .await
        .unwrap();
        db.mark_backup_cleanup_failed("backup_due", "2026-01-03T00:00:00Z", "storage unavailable")
            .await
            .unwrap();
        db.mark_backup_missing("backup_missing", "2026-01-03T00:00:00Z")
            .await
            .unwrap();

        let due = db
            .list_due_backup_cleanups("2026-01-04T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            due.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            ["backup_due"]
        );

        let state = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT last_cleanup_attempt_at, last_cleanup_error FROM backups WHERE id = 'backup_due'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                        ))
                    },
                )?)
            })
            .await
            .unwrap();
        assert_eq!(state.0.as_deref(), Some("2026-01-03T00:00:00Z"));
        assert_eq!(state.1.as_deref(), Some("storage unavailable"));

        let missing = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT missing_at, last_cleanup_error FROM backups WHERE id = 'backup_missing'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                        ))
                    },
                )?)
            })
            .await
            .unwrap();
        assert_eq!(missing.0.as_deref(), Some("2026-01-03T00:00:00Z"));
        assert_eq!(missing.1, None);
    }

    #[tokio::test]
    async fn cleanup_terminal_states_are_exclusive_and_delete_intent_is_claimed() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack_1', 'Stack', 'compose', '[]', '[]', 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO backups (id, stack_id, status, created_at, artifact_path, cleanup_after) VALUES ('backup_state', 'stack_1', 'success', '2026-01-01T00:00:00Z', '/backups/state.tar.zst', '2026-01-02T00:00:00Z')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        assert!(
            db.mark_backup_cleanup_delete_started(
                "backup_state",
                "2026-01-02T00:00:00Z",
                "__dockrev_cleanup_delete_intent__:test",
                "2026-01-01T23:00:00Z",
            )
            .await
            .unwrap()
        );
        assert!(
            !db.mark_backup_cleanup_delete_started(
                "backup_state",
                "2026-01-02T00:01:00Z",
                "__dockrev_cleanup_delete_intent__:other",
                "2026-01-01T23:00:00Z",
            )
            .await
            .unwrap()
        );
        let due = db
            .list_due_backup_cleanups("2026-01-03T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            due[0].last_cleanup_error.as_deref(),
            Some("__dockrev_cleanup_delete_intent__:test")
        );

        db.mark_backup_missing("backup_state", "2026-01-03T00:00:00Z")
            .await
            .unwrap();
        db.mark_backup_deleted("backup_state", "2026-01-04T00:00:00Z")
            .await
            .unwrap();
        let state = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT deleted_at, missing_at FROM backups WHERE id = 'backup_state'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                        ))
                    },
                )?)
            })
            .await
            .unwrap();
        assert_eq!(state.0, None);
        assert_eq!(state.1.as_deref(), Some("2026-01-03T00:00:00Z"));
    }

    #[tokio::test]
    async fn cleanup_delete_completion_marker_is_persisted_before_terminal_write() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack_1', 'Stack', 'compose', '[]', '[]', 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )?;
            conn.execute(
                "INSERT INTO backups (id, stack_id, status, created_at, artifact_path, cleanup_after) VALUES ('backup_state', 'stack_1', 'success', '2026-01-01T00:00:00Z', '/backups/state.tar.zst', '2026-01-02T00:00:00Z')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let intent = "__dockrev_cleanup_delete_intent__:test";
        let completed = "__dockrev_cleanup_delete_completed__:test";
        assert!(
            db.mark_backup_cleanup_delete_started(
                "backup_state",
                "2026-01-02T00:00:00Z",
                intent,
                "2026-01-01T23:00:00Z",
            )
            .await
            .unwrap()
        );
        db.mark_backup_cleanup_delete_completed("backup_state", intent, completed)
            .await
            .unwrap();
        let due = db
            .list_due_backup_cleanups("2026-01-03T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(due[0].last_cleanup_error.as_deref(), Some(completed));
        db.mark_backup_deleted("backup_state", "2026-01-03T00:00:00Z")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn service_backup_records_require_current_service_summary_targets() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute_batch(
                r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES ('stack_1', 'Stack', 'compose', '[]', '[]', 0, 0,
  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES ('stack_2', 'Other Stack', 'compose', '[]', '[]', 0, 0,
  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag, auto_rollback,
  backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at
) VALUES
  ('service_a', 'stack_1', 'a', 'example/a', 'latest', 1, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('service_b', 'stack_1', 'b', 'example/b', 'latest', 1, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
  ('service_c', 'stack_2', 'c', 'example/c', 'latest', 1, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
INSERT INTO jobs (
  id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode,
  created_by, reason, created_at, finished_at, summary_json
) VALUES
  ('job_relation', 'update', 'stack', 'stack_1', 'service_a', 'success', 0, 'inherit',
   'test', 'test', '2026-01-03T00:00:00Z', '2026-01-03T00:01:00Z',
   '{"targets":[{"serviceId":"service_b"}]}'),
  ('job_summary', 'update', 'stack', 'stack_1', NULL, 'success', 0, 'inherit',
   'test', 'test', '2026-01-02T00:00:00Z', '2026-01-02T00:01:00Z',
   '{"targets":[{"serviceId":"service_a"}]}'),
  ('job_other', 'update', 'stack', 'stack_1', NULL, 'success', 0, 'inherit',
   'test', 'test', '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z',
   '{"targets":[{"serviceId":"service_b"}]}'),
  ('job_cross_stack', 'update', 'stack', 'stack_2', NULL, 'success', 0, 'inherit',
   'test', 'test', '2025-12-31T00:00:00Z', '2025-12-31T00:01:00Z',
   '{"targets":[{"serviceId":"service_a"},{"serviceId":"service_c"}]}');
INSERT INTO job_service_targets (job_id, service_id)
VALUES ('job_relation', 'service_a');
INSERT INTO backups (
  id, stack_id, job_id, status, created_at, finished_at, artifact_path, size_bytes
) VALUES
  ('backup_relation', 'stack_1', 'job_relation', 'success', '2026-01-03T00:00:00Z',
   '2026-01-03T00:01:00Z', '/backups/relation.tar', 42),
  ('backup_summary', 'stack_1', 'job_summary', 'success', '2026-01-02T00:00:00Z',
   '2026-01-02T00:01:00Z', '/backups/summary.tar', 42),
  ('backup_other', 'stack_1', 'job_other', 'success', '2026-01-01T00:00:00Z',
   '2026-01-01T00:01:00Z', '/backups/other.tar', 42),
  ('backup_cross_stack', 'stack_2', 'job_cross_stack', 'success', '2025-12-31T00:00:00Z',
   '2025-12-31T00:01:00Z', '/backups/cross-stack.tar', 42);
"#,
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let service_a = db
            .list_service_backup_records("stack_1", "service_a")
            .await
            .unwrap();
        assert_eq!(
            service_a
                .iter()
                .map(|record| record.backup_id.as_str())
                .collect::<Vec<_>>(),
            ["backup_summary"]
        );

        let service_b = db
            .list_service_backup_records("stack_1", "service_b")
            .await
            .unwrap();
        assert_eq!(
            service_b
                .iter()
                .map(|record| record.backup_id.as_str())
                .collect::<Vec<_>>(),
            ["backup_relation", "backup_other"]
        );

        let service_c = db
            .list_service_backup_records("stack_2", "service_c")
            .await
            .unwrap();
        assert_eq!(
            service_c
                .iter()
                .map(|record| record.backup_id.as_str())
                .collect::<Vec<_>>(),
            ["backup_cross_stack"]
        );

        assert!(
            db.list_service_backup_records("stack_2", "service_a")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
