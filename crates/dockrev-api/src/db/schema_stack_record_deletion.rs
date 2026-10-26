use rusqlite::TransactionBehavior;

use super::{migration_applied, record_migration_tx};

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0043_stack_record_deletion";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS stack_record_deletion_audits (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL,
  actor_principal TEXT NOT NULL,
  outcome TEXT NOT NULL,
  deleted_at TEXT NOT NULL,
  backup_artifacts_json TEXT NOT NULL DEFAULT '[]',
  pending_artifact_cleanup INTEGER NOT NULL DEFAULT 0,
  cleanup_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_stack_record_deletion_audits_stack_deleted
  ON stack_record_deletion_audits(stack_id, deleted_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_stack_record_deletion_audits_success
  ON stack_record_deletion_audits(stack_id)
  WHERE outcome = 'deleted';

UPDATE discovered_compose_projects
SET status = 'missing',
    last_error = 'compose_files_missing',
    archived = 0,
    archived_at = NULL,
    archived_reason = NULL
WHERE archived_reason = 'auto_archive_compose_files_missing';

UPDATE stacks
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE archived_reason IN (
  'auto_archive_on_restart',
  'auto_archive_compose_files_missing'
);

UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE archived_reason = 'auto_archive_on_restart';
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_missing_archive_to_missing_state_without_promoting_stopped_records() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE stacks (
  id TEXT PRIMARY KEY,
  archived INTEGER NOT NULL,
  archived_at TEXT,
  archived_reason TEXT
);
CREATE TABLE discovered_compose_projects (
  project TEXT PRIMARY KEY,
  stack_id TEXT,
  status TEXT NOT NULL,
  last_error TEXT,
  archived INTEGER NOT NULL,
  archived_at TEXT,
  archived_reason TEXT
);
INSERT INTO stacks VALUES
  ('missing-stack', 1, 'now', 'auto_archive_compose_files_missing'),
  ('stopped-stack', 1, 'now', 'auto_archive_on_restart');
INSERT INTO discovered_compose_projects VALUES
  ('missing-project', 'missing-stack', 'active', NULL, 1, 'now', 'auto_archive_compose_files_missing'),
  ('stopped-project', 'stopped-stack', 'stopped', NULL, 1, 'now', 'auto_archive_on_restart');
"#,
        )
        .unwrap();

        apply(&mut conn).unwrap();

        let rows = conn
            .prepare(
                "SELECT project, status, last_error, archived, archived_reason FROM discovered_compose_projects ORDER BY project",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            vec![
                (
                    "missing-project".to_string(),
                    "missing".to_string(),
                    Some("compose_files_missing".to_string()),
                    0,
                    None,
                ),
                (
                    "stopped-project".to_string(),
                    "stopped".to_string(),
                    None,
                    0,
                    None,
                ),
            ]
        );
    }
}
