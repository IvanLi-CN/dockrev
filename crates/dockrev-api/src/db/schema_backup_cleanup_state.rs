use rusqlite::TransactionBehavior;

use super::{migration_applied, record_migration_tx};

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0014_backup_cleanup_state";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
ALTER TABLE backups ADD COLUMN last_cleanup_attempt_at TEXT;
ALTER TABLE backups ADD COLUMN last_cleanup_error TEXT;
ALTER TABLE backups ADD COLUMN missing_at TEXT;
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
    fn adds_cleanup_state_columns_to_existing_backups_table() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE backups (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  artifact_path TEXT,
  size_bytes INTEGER,
  error TEXT,
  cleanup_after TEXT,
  deleted_at TEXT
);
INSERT INTO backups (id, stack_id, job_id, status, created_at)
VALUES ('backup_1', 'stack_1', 'job_1', 'success', '2026-01-01T00:00:00Z');
"#,
        )
        .unwrap();

        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();

        let columns = conn
            .prepare("PRAGMA table_info(backups)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            columns
                .iter()
                .any(|column| column == "last_cleanup_attempt_at")
        );
        assert!(columns.iter().any(|column| column == "last_cleanup_error"));
        assert!(columns.iter().any(|column| column == "missing_at"));

        let row = conn
            .query_row(
                "SELECT status, last_cleanup_attempt_at, last_cleanup_error, missing_at FROM backups WHERE id = 'backup_1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row, ("success".to_string(), None, None, None));
    }
}
