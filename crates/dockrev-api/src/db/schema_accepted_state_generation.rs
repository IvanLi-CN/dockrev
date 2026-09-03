use anyhow::Context as _;
use rusqlite::TransactionBehavior;

use super::{migration_applied, record_migration_tx};

fn column_names(conn: &rusqlite::Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("inspect {table} columns"))?;
    statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0015_accepted_state_generation";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let service_columns = column_names(conn, "services")?;
    let target_columns = column_names(conn, "job_service_targets")?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !service_columns
        .iter()
        .any(|column| column == "accepted_state_generation")
    {
        tx.execute_batch(
            "ALTER TABLE services ADD COLUMN accepted_state_generation INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    if !target_columns
        .iter()
        .any(|column| column == "opened_generation")
    {
        tx.execute_batch("ALTER TABLE job_service_targets ADD COLUMN opened_generation INTEGER")?;
    }
    if !target_columns
        .iter()
        .any(|column| column == "baseline_snapshot_json")
    {
        tx.execute_batch("ALTER TABLE job_service_targets ADD COLUMN baseline_snapshot_json TEXT")?;
    }
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_generation_and_operation_snapshot_columns_to_existing_tables() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE schema_migrations (id TEXT PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
CREATE TABLE services (id TEXT PRIMARY KEY NOT NULL);
CREATE TABLE job_service_targets (
  job_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  PRIMARY KEY (job_id, service_id)
);
INSERT INTO services (id) VALUES ('service_1');
INSERT INTO job_service_targets (job_id, service_id) VALUES ('job_1', 'service_1');
"#,
        )
        .unwrap();

        apply(&mut conn).unwrap();

        assert_eq!(
            conn.query_row(
                "SELECT accepted_state_generation FROM services WHERE id = 'service_1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        let target = conn
            .query_row(
                "SELECT opened_generation, baseline_snapshot_json FROM job_service_targets WHERE job_id = 'job_1'",
                [],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .unwrap();
        assert_eq!(target, (None, None));
    }
}
