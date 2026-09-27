use super::{migration_applied, record_migration_tx};
use rusqlite::TransactionBehavior;

pub(super) fn apply_migrations(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    apply_migration_0027_add_notification_items(conn)?;
    apply_migration_0028_add_notification_anomaly_states(conn)?;
    apply_migration_0029_add_notification_anomaly_pending(conn)?;
    apply_migration_0030_add_notification_anomaly_batch(conn)?;
    apply_migration_0031_add_new_version_notification_item(conn)?;
    apply_migration_0032_add_notification_anomaly_delivery(conn)?;
    apply_migration_0033_add_notification_anomaly_occurrences(conn)?;
    apply_migration_0034_add_notification_dispatch_outbox(conn)?;
    apply_migration_0035_add_new_version_delivery_claim(conn)?;
    apply_migration_0036_add_notification_dispatch_reason(conn)?;
    apply_migration_0037_add_notification_anomaly_delivery_claim(conn)?;
    apply_migration_0038_add_notification_delivery_guards(conn)?;
    apply_migration_0039_add_notification_dispatch_decision_marker(conn)?;
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

pub(super) fn apply_migration_0030_add_notification_anomaly_batch(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0030_add_notification_anomaly_batch";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_anomaly_states ADD COLUMN notification_batch_id TEXT",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0031_add_new_version_notification_item(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0031_add_new_version_notification_item";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE new_version_notifications ADD COLUMN notification_item_id TEXT",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0032_add_notification_anomaly_delivery(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0032_add_notification_anomaly_delivery";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_anomaly_states ADD COLUMN notification_sent_channels_json TEXT NOT NULL DEFAULT '[]'",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0033_add_notification_anomaly_occurrences(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0033_add_notification_anomaly_occurrences";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS notification_anomaly_occurrences (
  id TEXT PRIMARY KEY NOT NULL,
  owner TEXT NOT NULL,
  repo TEXT NOT NULL,
  state TEXT NOT NULL,
  last_error TEXT,
  occurrence_count INTEGER NOT NULL,
  batch_id TEXT NOT NULL,
  notification_pending INTEGER NOT NULL DEFAULT 1,
  notification_item_id TEXT,
  sent_channels_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notification_anomaly_occurrences_pending
  ON notification_anomaly_occurrences (notification_pending, owner, repo, created_at, id);
CREATE INDEX IF NOT EXISTS idx_notification_anomaly_occurrences_item
  ON notification_anomaly_occurrences (notification_item_id);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0034_add_notification_dispatch_outbox(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0034_add_notification_dispatch_outbox";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS notification_dispatch_outbox (
  job_id TEXT PRIMARY KEY NOT NULL,
  finished_at TEXT NOT NULL,
  summary_json TEXT NOT NULL,
  processed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_notification_dispatch_outbox_pending
  ON notification_dispatch_outbox(processed_at, finished_at, job_id);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0038_add_notification_delivery_guards(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0038_add_notification_delivery_guards";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_dispatch_outbox ADD COLUMN event_enabled INTEGER NOT NULL DEFAULT 1",
        [],
    )?;
    tx.execute(
        "ALTER TABLE notification_anomaly_occurrences ADD COLUMN source_job_id TEXT",
        [],
    )?;
    tx.execute(
        "ALTER TABLE notification_anomaly_occurrences ADD COLUMN source_status TEXT NOT NULL DEFAULT 'success'",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0039_add_notification_dispatch_decision_marker(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0039_add_notification_dispatch_decision_marker";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_dispatch_outbox ADD COLUMN event_decision_known INTEGER NOT NULL DEFAULT 0",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0035_add_new_version_delivery_claim(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0035_add_new_version_delivery_claim";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
ALTER TABLE new_version_notifications ADD COLUMN delivery_claim_token TEXT;
ALTER TABLE new_version_notifications ADD COLUMN delivery_claim_expires_at TEXT;
CREATE INDEX IF NOT EXISTS idx_new_version_notifications_delivery_claim
  ON new_version_notifications(delivery_claim_expires_at);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0036_add_notification_dispatch_reason(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0036_add_notification_dispatch_reason";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "ALTER TABLE notification_dispatch_outbox ADD COLUMN reason TEXT NOT NULL DEFAULT ''",
        [],
    )?;
    tx.execute(
        "UPDATE notification_dispatch_outbox SET reason = COALESCE((SELECT reason FROM jobs WHERE jobs.id = notification_dispatch_outbox.job_id), '') WHERE reason = ''",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0037_add_notification_anomaly_delivery_claim(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0037_add_notification_anomaly_delivery_claim";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
ALTER TABLE notification_anomaly_occurrences ADD COLUMN delivery_claim_token TEXT;
ALTER TABLE notification_anomaly_occurrences ADD COLUMN delivery_claim_expires_at TEXT;
CREATE INDEX IF NOT EXISTS idx_notification_anomaly_occurrences_delivery_claim
  ON notification_anomaly_occurrences(delivery_claim_expires_at);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
