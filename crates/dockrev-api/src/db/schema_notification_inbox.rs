use super::{migration_applied, record_migration_tx};
use rusqlite::TransactionBehavior;

pub(super) fn apply_migrations(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    apply_migration_0027_add_notification_items(conn)?;
    apply_migration_0028_add_notification_anomaly_states(conn)?;
    apply_migration_0029_add_notification_anomaly_pending(conn)?;
    Ok(())
}

pub(super) fn apply_migration_0027_add_notification_items(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0027_add_notification_items";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS notification_items (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  identity_key TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  target_url TEXT NOT NULL,
  source_job_id TEXT,
  created_at TEXT NOT NULL,
  read_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_notification_items_created
  ON notification_items(created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_notification_items_unread
  ON notification_items(read_at, created_at DESC, id DESC);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0028_add_notification_anomaly_states(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0028_add_notification_anomaly_states";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS notification_anomaly_states (
  owner TEXT NOT NULL,
  repo TEXT NOT NULL,
  state TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  occurrence_count INTEGER NOT NULL DEFAULT 1,
  last_error TEXT,
  last_seen_at TEXT NOT NULL,
  PRIMARY KEY (owner, repo)
);
CREATE INDEX IF NOT EXISTS idx_notification_anomaly_states_active
  ON notification_anomaly_states (active, last_seen_at);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0029_add_notification_anomaly_pending(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0029_add_notification_anomaly_pending";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_anomaly_states ADD COLUMN notification_pending INTEGER NOT NULL DEFAULT 0",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
