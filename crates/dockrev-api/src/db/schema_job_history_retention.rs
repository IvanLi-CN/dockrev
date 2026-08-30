use rusqlite::TransactionBehavior;

use super::{migration_applied, record_migration_tx};

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0013_job_history_retention";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE backups_new (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  artifact_path TEXT,
  size_bytes INTEGER,
  error TEXT,
  cleanup_after TEXT,
  deleted_at TEXT
);
INSERT INTO backups_new (
  id, stack_id, job_id, status, created_at, finished_at, artifact_path,
  size_bytes, error, cleanup_after, deleted_at
)
SELECT id, stack_id, job_id, status, created_at, finished_at, artifact_path,
  size_bytes, error, cleanup_after, deleted_at
FROM backups;
DROP TABLE backups;
ALTER TABLE backups_new RENAME TO backups;
CREATE INDEX idx_backups_stack_id ON backups(stack_id);
CREATE INDEX idx_backups_cleanup_after ON backups(cleanup_after);

CREATE TABLE job_service_targets (
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  opened_generation INTEGER,
  baseline_snapshot_json TEXT,
  PRIMARY KEY (job_id, service_id)
);
CREATE INDEX idx_job_service_targets_service_job ON job_service_targets(service_id, job_id);
CREATE INDEX idx_jobs_created_at_id ON jobs(created_at DESC, id DESC);
CREATE INDEX idx_jobs_stack_created_at_id ON jobs(stack_id, created_at DESC, id DESC);
CREATE INDEX idx_jobs_terminal_finished_at ON jobs(status, finished_at, created_at);

INSERT OR IGNORE INTO job_service_targets (job_id, service_id)
SELECT j.id, j.service_id
FROM jobs j
JOIN services s ON s.id = j.service_id;
INSERT OR IGNORE INTO job_service_targets (job_id, service_id)
SELECT j.id, json_extract(t.value, '$.serviceId')
FROM jobs j
JOIN json_each(COALESCE(json_extract(j.summary_json, '$.targets'), '[]')) AS t
JOIN services s ON s.id = json_extract(t.value, '$.serviceId')
WHERE json_type(t.value, '$.serviceId') = 'text';
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    #[test]
    fn backfill_skips_targets_for_deleted_services() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
PRAGMA foreign_keys = ON;
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE stacks (id TEXT PRIMARY KEY NOT NULL);
CREATE TABLE services (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE
);
CREATE TABLE jobs (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT,
  service_id TEXT,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  summary_json TEXT NOT NULL
);
CREATE TABLE backups (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  artifact_path TEXT,
  size_bytes INTEGER,
  error TEXT,
  cleanup_after TEXT,
  deleted_at TEXT
);
"#,
        )
        .unwrap();
        conn.execute("INSERT INTO stacks (id) VALUES ('stack_1')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO services (id, stack_id) VALUES ('service_live', 'stack_1')",
            [],
        )
        .unwrap();
        for (id, service_id, summary_json) in [
            ("direct_live", Some("service_live"), "{}"),
            ("direct_deleted", Some("service_deleted"), "{}"),
            (
                "targets_mixed",
                None,
                r#"{"targets":[{"serviceId":"service_live"},{"serviceId":"service_deleted"}]}"#,
            ),
        ] {
            conn.execute(
                r#"
INSERT INTO jobs (id, service_id, status, created_at, summary_json)
VALUES (?1, ?2, 'success', '2026-01-01T00:00:00Z', ?3)
"#,
                params![id, service_id, summary_json],
            )
            .unwrap();
        }

        apply(&mut conn).unwrap();

        let mut stmt = conn
            .prepare("SELECT job_id, service_id FROM job_service_targets ORDER BY job_id")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("direct_live".to_string(), "service_live".to_string()),
                ("targets_mixed".to_string(), "service_live".to_string()),
            ]
        );
    }
}
