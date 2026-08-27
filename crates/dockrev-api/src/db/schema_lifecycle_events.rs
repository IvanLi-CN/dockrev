use rusqlite::TransactionBehavior;

use super::{migration_applied, record_migration_tx};

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0014_add_service_lifecycle_events";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS service_lifecycle_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  stack_id TEXT REFERENCES stacks(id) ON DELETE SET NULL,
  operation_group_id TEXT NOT NULL,
  job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
  origin TEXT NOT NULL,
  transition TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  boundary_precision TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '{}',
  details_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  UNIQUE(operation_group_id, service_id, transition)
);
CREATE INDEX IF NOT EXISTS idx_service_lifecycle_events_service_time
  ON service_lifecycle_events(service_id, observed_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_service_lifecycle_events_operation_group
  ON service_lifecycle_events(operation_group_id, id);
CREATE INDEX IF NOT EXISTS idx_service_lifecycle_events_created_at
  ON service_lifecycle_events(created_at);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
