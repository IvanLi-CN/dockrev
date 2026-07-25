use super::*;

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
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let deleted_at = deleted_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET deleted_at = ?2 WHERE id = ?1",
                params![backup_id, deleted_at],
            )?;
            Ok(())
        })
        .await
        .context("mark backup deleted")
    }

    pub async fn list_due_backup_cleanups(
        &self,
        now: &str,
    ) -> anyhow::Result<Vec<BackupCleanupItem>> {
        let now = now.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, stack_id, job_id, artifact_path
FROM backups
WHERE
  status = 'success'
  AND deleted_at IS NULL
  AND artifact_path IS NOT NULL
  AND cleanup_after IS NOT NULL
  AND cleanup_after <= ?1
ORDER BY cleanup_after ASC
LIMIT 50
"#,
            )?;
            let rows = stmt.query_map(params![now], |row| {
                Ok(BackupCleanupItem {
                    id: row.get(0)?,
                    stack_id: row.get(1)?,
                    job_id: row.get(2)?,
                    artifact_path: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
SELECT id
FROM backups
WHERE stack_id = ?1 AND status = 'success' AND deleted_at IS NULL
ORDER BY created_at DESC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
  b.error,
  COALESCE(j.summary_json, '{}')
FROM backups b
LEFT JOIN jobs j ON j.id = b.job_id
WHERE b.stack_id = ?1
  AND (
    (j.scope = 'service' AND j.service_id = ?2)
    OR (
      EXISTS (
        SELECT 1
        FROM json_each(COALESCE(json_extract(j.summary_json, '$.stacks'), '[]')) AS s
        WHERE json_extract(s.value, '$.stackId') = ?1
          AND EXISTS (
            SELECT 1
            FROM json_each(
              COALESCE(
                json_extract(s.value, '$.update.newDigests'),
                json_extract(s.value, '$.update.oldDigests'),
                json_extract(s.value, '$.rollback.newDigests'),
                json_extract(s.value, '$.rollback.oldDigests'),
                '{}'
              )
            ) AS d
            WHERE d.key = ?2
          )
      )
    )
  )
ORDER BY b.created_at DESC, b.id DESC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id, service_id], |row| {
                let summary_json: String = row.get(11)?;
                let job_summary_json = serde_json::from_str(&summary_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        11,
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
                    error: row.get(10)?,
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
}
