use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::Db;

pub(crate) const NOTIFICATION_KIND_JOB_FINISHED: &str = "job_finished";
pub(crate) const NOTIFICATION_KIND_NEW_VERSION: &str = "new_version_discovered";
pub(crate) const NOTIFICATION_KIND_GHCR_ANOMALY: &str = "ghcr_webhook_anomaly";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationItemDraft {
    pub id: String,
    pub kind: String,
    pub identity_key: String,
    pub title: String,
    pub body: String,
    pub target_url: String,
    pub source_job_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationItemRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub target_url: String,
    pub source_job_id: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct NotificationItemUpsert {
    pub item: NotificationItemRow,
    pub created: bool,
    pub unread_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct NotificationInboxPage {
    pub items: Vec<NotificationItemRow>,
    pub next_cursor: Option<String>,
    pub unread_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct NotificationReadResult {
    pub notification_id: String,
    pub read_at: String,
    pub unread_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct NotificationReadAllResult {
    pub read_at: String,
    pub unread_count: u64,
}

impl Db {
    pub(crate) async fn ensure_notification_item(
        &self,
        draft: &NotificationItemDraft,
        now: &str,
    ) -> anyhow::Result<NotificationItemUpsert> {
        let draft = draft.clone();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let created = tx.execute(
                r#"
INSERT INTO notification_items (
  id, kind, identity_key, title, body, target_url, source_job_id, created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(identity_key) DO NOTHING
"#,
                params![
                    draft.id,
                    draft.kind,
                    draft.identity_key,
                    draft.title,
                    draft.body,
                    draft.target_url,
                    draft.source_job_id,
                    draft.created_at,
                ],
            )? > 0;

            let item = select_item(&tx, &draft.identity_key)?
                .context("notification item disappeared after insert")?;
            let unread_count = count_unread(&tx)?;
            tx.commit()?;

            let _ = now;
            Ok(NotificationItemUpsert {
                item,
                created,
                unread_count,
            })
        })
        .await
        .context("ensure notification item")
    }

    pub(crate) async fn list_notification_items(
        &self,
        limit: usize,
        cursor: Option<&str>,
        now: &str,
    ) -> anyhow::Result<NotificationInboxPage> {
        let limit = limit.clamp(1, 50);
        let cursor = cursor.map(decode_cursor).transpose()?;
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            purge_read_items(&tx, &retention_cutoff(&now)?)?;
            let mut stmt = tx.prepare(
                r#"
SELECT id, kind, title, body, target_url, source_job_id, created_at, read_at
FROM notification_items
WHERE (?1 IS NULL OR created_at < ?1 OR (created_at = ?1 AND id < ?2))
ORDER BY created_at DESC, id DESC
LIMIT ?3
"#,
            )?;
            let rows = stmt
                .query_map(
                    params![
                        cursor.as_ref().map(|value| value.0.as_str()),
                        cursor.as_ref().map(|value| value.1.as_str()),
                        (limit + 1) as i64,
                    ],
                    row_to_item,
                )?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            let mut items = rows;
            let next_cursor = if items.len() > limit {
                let next = items.pop().expect("cursor row exists");
                Some(encode_cursor(&next.created_at, &next.id))
            } else {
                None
            };
            let unread_count = count_unread(&tx)?;
            tx.commit()?;
            Ok(NotificationInboxPage {
                items,
                next_cursor,
                unread_count,
            })
        })
        .await
        .context("list notification items")
    }

    pub(crate) async fn count_notification_items(&self, now: &str) -> anyhow::Result<u64> {
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            purge_read_items(&tx, &retention_cutoff(&now)?)?;
            let count = count_unread(&tx)?;
            tx.commit()?;
            Ok(count)
        })
        .await
        .context("count unread notification items")
    }

    pub(crate) async fn mark_notification_item_read(
        &self,
        notification_id: &str,
        now: &str,
    ) -> anyhow::Result<Option<NotificationReadResult>> {
        let notification_id = notification_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            purge_read_items(&tx, &retention_cutoff(&now)?)?;
            tx.execute(
                "UPDATE notification_items SET read_at = COALESCE(read_at, ?2) WHERE id = ?1",
                params![notification_id, now],
            )?;
            let Some(read_at) = tx
                .query_row(
                    "SELECT read_at FROM notification_items WHERE id = ?1",
                    params![notification_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten()
            else {
                tx.commit()?;
                return Ok(None);
            };
            let unread_count = count_unread(&tx)?;
            tx.commit()?;
            Ok(Some(NotificationReadResult {
                notification_id,
                read_at,
                unread_count,
            }))
        })
        .await
        .context("mark notification item read")
    }

    pub(crate) async fn mark_all_notification_items_read(
        &self,
        now: &str,
    ) -> anyhow::Result<NotificationReadAllResult> {
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            purge_read_items(&tx, &retention_cutoff(&now)?)?;
            tx.execute(
                "UPDATE notification_items SET read_at = ?1 WHERE read_at IS NULL",
                params![now],
            )?;
            let unread_count = count_unread(&tx)?;
            tx.commit()?;
            Ok(NotificationReadAllResult {
                read_at: now,
                unread_count,
            })
        })
        .await
        .context("mark all notification items read")
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotificationItemRow> {
    Ok(NotificationItemRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        target_url: row.get(4)?,
        source_job_id: row.get(5)?,
        created_at: row.get(6)?,
        read_at: row.get(7)?,
    })
}

fn select_item(
    conn: &rusqlite::Connection,
    identity_key: &str,
) -> rusqlite::Result<Option<NotificationItemRow>> {
    conn.query_row(
        r#"
SELECT id, kind, title, body, target_url, source_job_id, created_at, read_at
FROM notification_items
WHERE identity_key = ?1
"#,
        params![identity_key],
        row_to_item,
    )
    .optional()
}

fn count_unread(conn: &rusqlite::Connection) -> rusqlite::Result<u64> {
    conn.query_row(
        "SELECT COUNT(*) FROM notification_items WHERE read_at IS NULL",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value.max(0) as u64)
}

fn purge_read_items(conn: &rusqlite::Connection, cutoff: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM notification_items WHERE rowid IN (SELECT rowid FROM notification_items WHERE read_at IS NOT NULL AND read_at < ?1 LIMIT 500)",
        params![cutoff],
    )
}

fn retention_cutoff(now: &str) -> anyhow::Result<String> {
    let parsed = time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)?;
    Ok((parsed - time::Duration::days(90))
        .format(&time::format_description::well_known::Rfc3339)?)
}

fn encode_cursor(created_at: &str, id: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{created_at}\n{id}"))
}

fn decode_cursor(cursor: &str) -> anyhow::Result<(String, String)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor)?;
    let value = String::from_utf8(bytes)?;
    let (created_at, id) = value
        .split_once('\n')
        .context("invalid notification cursor")?;
    if created_at.is_empty() || id.is_empty() {
        anyhow::bail!("invalid notification cursor")
    }
    Ok((created_at.to_string(), id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip_is_opaque() {
        let cursor = encode_cursor("2026-09-26T00:00:00Z", "ntf_123");
        assert_ne!(cursor, "2026-09-26T00:00:00Z|ntf_123");
        assert_eq!(
            decode_cursor(&cursor).unwrap(),
            ("2026-09-26T00:00:00Z".to_string(), "ntf_123".to_string())
        );
    }

    #[tokio::test]
    async fn notification_items_are_idempotent_and_reads_are_absolute() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        let draft = NotificationItemDraft {
            id: "ntf_1".to_string(),
            kind: NOTIFICATION_KIND_JOB_FINISHED.to_string(),
            identity_key: "job_finished:job_1".to_string(),
            title: "任务已成功".to_string(),
            body: "完成".to_string(),
            target_url: "/queue/job_1".to_string(),
            source_job_id: Some("job_1".to_string()),
            created_at: "2026-09-26T00:00:00Z".to_string(),
        };

        let first = db
            .ensure_notification_item(&draft, "2026-09-26T00:00:00Z")
            .await
            .unwrap();
        assert!(first.created);
        assert_eq!(first.unread_count, 1);

        let duplicate = db
            .ensure_notification_item(
                &NotificationItemDraft {
                    id: "ntf_other".to_string(),
                    ..draft
                },
                "2026-09-26T00:00:01Z",
            )
            .await
            .unwrap();
        assert!(!duplicate.created);
        assert_eq!(duplicate.item.id, "ntf_1");
        assert_eq!(duplicate.unread_count, 1);

        let read = db
            .mark_notification_item_read("ntf_1", "2026-09-26T00:00:02Z")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read.unread_count, 0);
        let repeated = db
            .mark_notification_item_read("ntf_1", "2026-09-26T00:00:03Z")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(repeated.unread_count, 0);

        let page = db
            .list_notification_items(50, None, "2026-09-26T00:00:04Z")
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.unread_count, 0);
    }

    #[tokio::test]
    async fn mark_all_reads_only_unread_items() {
        let db = Db::open(std::path::Path::new(":memory:")).await.unwrap();
        for index in 1..=2 {
            db.ensure_notification_item(
                &NotificationItemDraft {
                    id: format!("ntf_{index}"),
                    kind: NOTIFICATION_KIND_JOB_FINISHED.to_string(),
                    identity_key: format!("job_finished:job_{index}"),
                    title: "任务".to_string(),
                    body: "完成".to_string(),
                    target_url: format!("/queue/job_{index}"),
                    source_job_id: Some(format!("job_{index}")),
                    created_at: format!("2026-09-26T00:00:0{index}Z"),
                },
                "2026-09-26T00:00:02Z",
            )
            .await
            .unwrap();
        }
        db.mark_notification_item_read("ntf_1", "2026-09-26T00:00:03Z")
            .await
            .unwrap();
        let result = db
            .mark_all_notification_items_read("2026-09-26T00:00:04Z")
            .await
            .unwrap();
        assert_eq!(result.unread_count, 0);
    }
}
