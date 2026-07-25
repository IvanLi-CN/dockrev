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
  PRIMARY KEY (job_id, service_id)
);
CREATE INDEX idx_job_service_targets_service_job ON job_service_targets(service_id, job_id);
CREATE INDEX idx_jobs_created_at_id ON jobs(created_at DESC, id DESC);
CREATE INDEX idx_jobs_stack_created_at_id ON jobs(stack_id, created_at DESC, id DESC);
CREATE INDEX idx_jobs_terminal_finished_at ON jobs(status, finished_at, created_at);

INSERT OR IGNORE INTO job_service_targets (job_id, service_id)
SELECT id, service_id FROM jobs WHERE service_id IS NOT NULL;
INSERT OR IGNORE INTO job_service_targets (job_id, service_id)
SELECT j.id, json_extract(t.value, '$.serviceId')
FROM jobs j
JOIN json_each(COALESCE(json_extract(j.summary_json, '$.targets'), '[]')) AS t
WHERE json_type(t.value, '$.serviceId') = 'text';
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
