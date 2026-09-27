use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, Transaction, params};

use super::Db;

#[derive(Clone, Debug)]
pub(crate) struct PendingCheckNotificationDispatch {
    pub job_id: String,
    pub reason: String,
    pub finished_at: String,
    pub summary: serde_json::Value,
}

pub(super) fn enqueue_check_notification_tx(
    tx: &Transaction<'_>,
    job_id: &str,
    reason: &str,
    finished_at: &str,
    summary: &serde_json::Value,
) -> anyhow::Result<()> {
    tx.execute(
        r#"
INSERT INTO notification_dispatch_outbox (job_id, reason, finished_at, summary_json)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(job_id) DO NOTHING
"#,
        params![job_id, reason, finished_at, serde_json::to_string(summary)?],
    )?;
    Ok(())
}

impl Db {
    #[allow(dead_code)]
    pub(crate) async fn list_pending_check_notification_dispatches(
        &self,
    ) -> anyhow::Result<Vec<PendingCheckNotificationDispatch>> {
        self.list_pending_check_notification_dispatches_after(None)
            .await
    }

    pub(crate) async fn list_pending_check_notification_dispatches_after(
        &self,
        after: Option<(&str, &str)>,
    ) -> anyhow::Result<Vec<PendingCheckNotificationDispatch>> {
        let after_finished_at = after.map(|(finished_at, _)| finished_at.to_string());
        let after_job_id = after.map(|(_, job_id)| job_id.to_string());
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT outbox.job_id, outbox.reason, outbox.finished_at, outbox.summary_json
FROM notification_dispatch_outbox outbox
WHERE outbox.processed_at IS NULL
  AND (
    ?1 IS NULL
    OR outbox.finished_at > ?1
    OR (outbox.finished_at = ?1 AND outbox.job_id > ?2)
  )
ORDER BY outbox.finished_at ASC, outbox.job_id ASC
LIMIT 256
"#,
            )?;
            let rows = stmt.query_map(params![after_finished_at, after_job_id], |row| {
                let summary_raw: String = row.get(3)?;
                let summary = serde_json::from_str(&summary_raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(PendingCheckNotificationDispatch {
                    job_id: row.get(0)?,
                    reason: row.get(1)?,
                    finished_at: row.get(2)?,
                    summary,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .context("list pending check notification dispatches")
    }

    pub(crate) async fn mark_check_notification_dispatch_processed(
        &self,
        job_id: &str,
        processed_at: &str,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let processed_at = processed_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE notification_dispatch_outbox SET processed_at = ?2 WHERE job_id = ?1 AND processed_at IS NULL",
                params![job_id, processed_at],
            )?;
            Ok(())
        })
        .await
        .context("mark check notification dispatch processed")
    }

    #[allow(dead_code)]
    pub(crate) async fn check_notification_dispatch_pending(
        &self,
        job_id: &str,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT processed_at IS NULL FROM notification_dispatch_outbox WHERE job_id = ?1",
                    params![job_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some_and(|pending| pending != 0))
        })
        .await
        .context("check notification dispatch pending")
    }
}
