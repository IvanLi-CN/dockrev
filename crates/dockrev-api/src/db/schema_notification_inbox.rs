use super::{migration_applied, record_migration_tx};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

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
    apply_migration_0040_backfill_notification_anomaly_occurrences(conn)?;
    apply_migration_0041_add_notification_anomaly_delivery_attempt(conn)?;
    apply_migration_0042_backfill_notification_summaries(conn)?;
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
    tx.execute(
        "UPDATE notification_dispatch_outbox SET event_decision_known = 1",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0040_backfill_notification_anomaly_occurrences(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0040_backfill_notification_anomaly_occurrences";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        r#"
INSERT INTO notification_anomaly_occurrences (
  id,
  owner,
  repo,
  state,
  last_error,
  occurrence_count,
  batch_id,
  notification_pending,
  notification_item_id,
  sent_channels_json,
  created_at
)
SELECT
  lower(hex(randomblob(16))),
  state.owner,
  state.repo,
  state.state,
  state.last_error,
  state.occurrence_count,
  COALESCE(state.notification_batch_id, 'migration-0040:' || state.owner || '/' || state.repo),
  1,
  NULL,
  state.notification_sent_channels_json,
  state.last_seen_at
FROM notification_anomaly_states state
WHERE state.notification_pending = 1
  AND NOT EXISTS (
    SELECT 1
    FROM notification_anomaly_occurrences occurrence
    WHERE occurrence.owner = state.owner
      AND occurrence.repo = state.repo
      AND occurrence.state = state.state
      AND occurrence.occurrence_count = state.occurrence_count
      AND occurrence.notification_pending = 1
  )
"#,
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

pub(super) fn apply_migration_0041_add_notification_anomaly_delivery_attempt(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0041_add_notification_anomaly_delivery_attempt";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
ALTER TABLE notification_anomaly_occurrences ADD COLUMN delivery_attempted_at TEXT;
CREATE INDEX IF NOT EXISTS idx_notification_anomaly_occurrences_replay
  ON notification_anomaly_occurrences(notification_pending, delivery_attempted_at, created_at, id);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0042_backfill_notification_summaries(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0042_backfill_notification_summaries";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let unread_items = {
        let mut stmt = tx.prepare(
            "SELECT id, kind, source_job_id FROM notification_items WHERE read_at IS NULL ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (item_id, kind, source_job_id) in unread_items {
        let summary = match kind.as_str() {
            "job_finished" => {
                let Some(source_job_id) = source_job_id.as_deref() else {
                    continue;
                };
                let context = tx
                    .query_row(
                        r#"
SELECT job.type, job.scope,
       COALESCE(NULLIF(TRIM(service.name), ''), NULLIF(TRIM(stack.name), '')),
       job.status
FROM jobs AS job
LEFT JOIN services AS service ON service.id = job.service_id
LEFT JOIN stacks AS stack ON stack.id = COALESCE(job.stack_id, service.stack_id)
WHERE job.id = ?1
"#,
                        params![source_job_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, String>(3)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((job_type, scope, target_name, status)) = context else {
                    continue;
                };
                if !is_known_notification_status(&status) {
                    continue;
                }
                Some(crate::db::format_job_notification_summary(
                    Some(&job_type),
                    Some(&scope),
                    target_name.as_deref(),
                    &status,
                ))
            }
            "new_version_discovered" => {
                let rows = {
                    let mut stmt = tx.prepare(
                        r#"
SELECT candidate.service_id, service.name,
       candidate.current_display_tag, candidate.candidate_display_tag
FROM new_version_notifications AS candidate
LEFT JOIN services AS service ON service.id = candidate.service_id
WHERE candidate.notification_item_id = ?1
ORDER BY candidate.created_at, candidate.id
"#,
                    )?;
                    let rows = stmt.query_map(params![item_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    })?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()?
                };
                let mut seen_services = std::collections::BTreeSet::new();
                let mut entries = Vec::new();
                let mut reliable = true;
                for (service_id, service_name, current_tag, candidate_tag) in rows {
                    if service_id.trim().is_empty()
                        || !crate::db::notification_version_tag_is_readable(&current_tag)
                        || !crate::db::notification_version_tag_is_readable(&candidate_tag)
                    {
                        reliable = false;
                        break;
                    }
                    if seen_services.insert(service_id) {
                        entries.push(crate::db::NotificationVersionSummaryEntry {
                            service_name,
                            current_tag,
                            candidate_tag,
                        });
                    }
                }
                if !reliable || entries.is_empty() {
                    continue;
                }
                Some((
                    crate::db::format_new_version_notification_title(entries.len()),
                    crate::db::format_new_version_notification_body(&entries),
                ))
            }
            "ghcr_webhook_anomaly" => {
                let rows = {
                    let mut stmt = tx.prepare(
                        r#"
SELECT owner, repo, state
FROM notification_anomaly_occurrences
WHERE notification_item_id = ?1
ORDER BY created_at, id
"#,
                    )?;
                    let rows = stmt.query_map(params![item_id], |row| {
                        Ok(crate::db::NotificationGhcrSummaryEntry {
                            owner: row.get(0)?,
                            repo: row.get(1)?,
                            state: row.get(2)?,
                        })
                    })?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()?
                };
                let mut seen_repos = std::collections::BTreeSet::new();
                let mut entries = Vec::new();
                let mut reliable = true;
                for entry in rows {
                    if entry.owner.trim().is_empty()
                        || entry.repo.trim().is_empty()
                        || !matches!(entry.state.as_str(), "missing" | "conflict" | "error")
                    {
                        reliable = false;
                        break;
                    }
                    if seen_repos.insert((entry.owner.clone(), entry.repo.clone())) {
                        entries.push(entry);
                    }
                }
                if !reliable || entries.is_empty() {
                    continue;
                }
                Some((
                    crate::db::format_ghcr_anomaly_notification_title(entries.len()),
                    crate::db::format_ghcr_anomaly_notification_body(&entries),
                ))
            }
            _ => None,
        };

        if let Some((title, body)) = summary {
            tx.execute(
                "UPDATE notification_items SET title = ?1, body = ?2 WHERE id = ?3 AND read_at IS NULL",
                params![title, body, item_id],
            )?;
        }
    }

    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn is_known_notification_status(status: &str) -> bool {
    matches!(
        status,
        "success"
            | "succeeded"
            | "failed"
            | "error"
            | "cancelled"
            | "canceled"
            | "rolled_back"
            | "stopped"
    )
}
