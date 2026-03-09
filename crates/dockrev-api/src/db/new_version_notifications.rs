use super::*;

const STATUS_PENDING: &str = "pending";
const STATUS_SENT: &str = "sent";
const STATUS_FAILED: &str = "failed";
const STATUS_SUPERSEDED: &str = "superseded";
const ACTIVE_INDEX_NAME: &str = "idx_new_version_notifications_active_service_digest";

fn map_new_version_notification_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NewVersionNotificationRecord> {
    let sent_channels_json: String = row.get(12)?;
    let sent_channels =
        serde_json::from_str::<Vec<String>>(&sent_channels_json).unwrap_or_default();
    Ok(NewVersionNotificationRecord {
        id: row.get(0)?,
        service_id: row.get(1)?,
        job_id: row.get(2)?,
        reason: row.get(3)?,
        image_ref: row.get(4)?,
        image_tag: row.get(5)?,
        current_tag: row.get(6)?,
        current_display_tag: row.get(7)?,
        candidate_tag: row.get(8)?,
        candidate_display_tag: row.get(9)?,
        candidate_digest: row.get(10)?,
        status: row.get(11)?,
        sent_channels,
        created_at: row.get(13)?,
        sent_at: row.get(14)?,
        superseded_at: row.get(15)?,
        last_error: row.get(16)?,
    })
}

fn is_active_notification_conflict(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(code, msg)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            msg.as_deref().is_some_and(|message| {
                message.contains(ACTIVE_INDEX_NAME)
                    || message.contains(
                        "new_version_notifications.service_id, new_version_notifications.candidate_digest",
                    )
            })
        }
        _ => false,
    }
}

impl Db {
    pub async fn reserve_new_version_notification(
        &self,
        pending: &NewVersionNotificationPending,
    ) -> anyhow::Result<NewVersionNotificationReserveResult> {
        let pending = pending.clone();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let insert = tx.execute(
                r#"
INSERT INTO new_version_notifications (
  id,
  service_id,
  job_id,
  reason,
  image_ref,
  image_tag,
  current_tag,
  current_display_tag,
  candidate_tag,
  candidate_display_tag,
  candidate_digest,
  status,
  sent_channels_json,
  created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
"#,
                params![
                    pending.id,
                    pending.service_id,
                    pending.job_id,
                    pending.reason,
                    pending.image_ref,
                    pending.image_tag,
                    pending.current_tag,
                    pending.current_display_tag,
                    pending.candidate_tag,
                    pending.candidate_display_tag,
                    pending.candidate_digest,
                    STATUS_PENDING,
                    "[]",
                    pending.created_at,
                ],
            );

            match insert {
                Ok(_) => {
                    tx.commit()?;
                    Ok(NewVersionNotificationReserveResult::Reserved(pending.id))
                }
                Err(err) if is_active_notification_conflict(&err) => {
                    Ok(NewVersionNotificationReserveResult::SkippedDuplicate)
                }
                Err(err) => Err(err.into()),
            }
        })
        .await
        .context("reserve new version notification")
    }

    pub async fn finalize_new_version_notification(
        &self,
        notification_id: &str,
        sent_channels: &[String],
        last_error: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let notification_id = notification_id.to_string();
        let sent_channels = sent_channels.to_vec();
        let last_error = last_error.map(ToString::to_string);
        let now = now.to_string();
        self.call(move |conn| {
            let status = if sent_channels.is_empty() {
                STATUS_FAILED
            } else {
                STATUS_SENT
            };
            let sent_at = (!sent_channels.is_empty()).then_some(now.clone());
            let changed = conn.execute(
                r#"
UPDATE new_version_notifications
SET
  status = ?2,
  sent_channels_json = ?3,
  sent_at = ?4,
  last_error = ?5
WHERE id = ?1 AND status = ?6
"#,
                params![
                    notification_id,
                    status,
                    serde_json::to_string(&sent_channels)?,
                    sent_at,
                    last_error,
                    STATUS_PENDING,
                ],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("finalize new version notification")
    }

    pub async fn reconcile_service_new_version_notifications(
        &self,
        service_id: &str,
        image_ref: &str,
        image_tag: &str,
        candidate_digest: Option<&str>,
        now: &str,
    ) -> anyhow::Result<usize> {
        let service_id = service_id.to_string();
        let image_ref = image_ref.to_string();
        let image_tag = image_tag.to_string();
        let candidate_digest = candidate_digest
            .map(str::trim)
            .filter(|digest| !digest.is_empty())
            .map(ToString::to_string);
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = if let Some(candidate_digest) = candidate_digest {
                tx.execute(
                    r#"
UPDATE new_version_notifications
SET
  status = ?2,
  superseded_at = ?3
WHERE service_id = ?1
  AND status = ?4
  AND (
    image_ref != ?5
    OR image_tag != ?6
    OR candidate_digest != ?7
  )
"#,
                    params![
                        service_id,
                        STATUS_SUPERSEDED,
                        now,
                        STATUS_SENT,
                        image_ref,
                        image_tag,
                        candidate_digest,
                    ],
                )?
            } else {
                tx.execute(
                    r#"
UPDATE new_version_notifications
SET
  status = ?2,
  superseded_at = ?3
WHERE service_id = ?1
  AND status = ?4
"#,
                    params![service_id, STATUS_SUPERSEDED, now, STATUS_SENT],
                )?
            };
            tx.commit()?;
            Ok(changed)
        })
        .await
        .context("reconcile new version notifications")
    }

    pub async fn list_new_version_notifications_for_service(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Vec<NewVersionNotificationRecord>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  service_id,
  job_id,
  reason,
  image_ref,
  image_tag,
  current_tag,
  current_display_tag,
  candidate_tag,
  candidate_display_tag,
  candidate_digest,
  status,
  sent_channels_json,
  created_at,
  sent_at,
  superseded_at,
  last_error
FROM new_version_notifications
WHERE service_id = ?1
ORDER BY created_at ASC, id ASC
"#,
            )?;
            let rows = stmt.query_map(params![service_id], map_new_version_notification_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list new version notifications for service")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn pending(id: &str, service_id: &str, digest: &str) -> NewVersionNotificationPending {
        NewVersionNotificationPending {
            id: id.to_string(),
            service_id: service_id.to_string(),
            job_id: "chk_1".to_string(),
            reason: "schedule".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: "latest".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.1.0".to_string(),
            candidate_digest: digest.to_string(),
            created_at: "2026-03-09T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn reserve_skips_duplicate_active_digest() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:new");
        let second = pending("nvn_2", "svc_1", "sha256:new");

        let first_result = db.reserve_new_version_notification(&first).await.unwrap();
        let second_result = db.reserve_new_version_notification(&second).await.unwrap();

        assert_eq!(
            first_result,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        assert_eq!(
            second_result,
            NewVersionNotificationReserveResult::SkippedDuplicate
        );
    }

    #[tokio::test]
    async fn concurrent_reserve_only_allows_one_active_record() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:new");
        let second = pending("nvn_2", "svc_1", "sha256:new");

        let db_a = db.clone();
        let db_b = db.clone();
        let (first_result, second_result) = tokio::join!(
            db_a.reserve_new_version_notification(&first),
            db_b.reserve_new_version_notification(&second),
        );

        let first_result = first_result.unwrap();
        let second_result = second_result.unwrap();
        let winners = [first_result, second_result]
            .into_iter()
            .filter(|result| matches!(result, NewVersionNotificationReserveResult::Reserved(_)))
            .count();
        assert_eq!(winners, 1);
    }

    #[tokio::test]
    async fn failed_record_does_not_hold_active_slot() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:new");
        let second = pending("nvn_2", "svc_1", "sha256:new");

        let reserved = db.reserve_new_version_notification(&first).await.unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        let finalized = db
            .finalize_new_version_notification(
                "nvn_1",
                &[],
                Some("webhook failed"),
                "2026-03-09T00:01:00Z",
            )
            .await
            .unwrap();
        assert!(finalized);

        let retried = db.reserve_new_version_notification(&second).await.unwrap();
        assert_eq!(
            retried,
            NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
        );
    }

    #[tokio::test]
    async fn reconcile_keeps_pending_rows_until_send_finishes() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:old");

        let reserved = db.reserve_new_version_notification(&first).await.unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );

        let changed = db
            .reconcile_service_new_version_notifications(
                "svc_1",
                "ghcr.io/acme/web",
                "latest",
                Some("sha256:new"),
                "2026-03-09T00:02:00Z",
            )
            .await
            .unwrap();
        assert_eq!(changed, 0);

        let rows = db
            .list_new_version_notifications_for_service("svc_1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, STATUS_PENDING);
        assert_eq!(rows[0].superseded_at, None);
    }

    #[tokio::test]
    async fn reconcile_supersedes_old_active_rows() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:old");

        let reserved = db.reserve_new_version_notification(&first).await.unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        let finalized = db
            .finalize_new_version_notification(
                "nvn_1",
                &["webhook".to_string()],
                None,
                "2026-03-09T00:01:00Z",
            )
            .await
            .unwrap();
        assert!(finalized);

        let changed = db
            .reconcile_service_new_version_notifications(
                "svc_1",
                "ghcr.io/acme/web",
                "latest",
                Some("sha256:new"),
                "2026-03-09T00:02:00Z",
            )
            .await
            .unwrap();
        assert_eq!(changed, 1);

        let rows = db
            .list_new_version_notifications_for_service("svc_1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, STATUS_SUPERSEDED);
        assert_eq!(
            rows[0].superseded_at.as_deref(),
            Some("2026-03-09T00:02:00Z")
        );
    }

    #[tokio::test]
    async fn reconcile_without_candidate_supersedes_active_rows_and_allows_reuse() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:old");
        let second = pending("nvn_2", "svc_1", "sha256:old");

        let reserved = db.reserve_new_version_notification(&first).await.unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        let finalized = db
            .finalize_new_version_notification(
                "nvn_1",
                &["webhook".to_string()],
                None,
                "2026-03-09T00:01:00Z",
            )
            .await
            .unwrap();
        assert!(finalized);

        let changed = db
            .reconcile_service_new_version_notifications(
                "svc_1",
                "ghcr.io/acme/web",
                "latest",
                None,
                "2026-03-09T00:02:00Z",
            )
            .await
            .unwrap();
        assert_eq!(changed, 1);

        let retried = db.reserve_new_version_notification(&second).await.unwrap();
        assert_eq!(
            retried,
            NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
        );

        let rows = db
            .list_new_version_notifications_for_service("svc_1")
            .await
            .unwrap();
        assert_eq!(rows[0].status, STATUS_SUPERSEDED);
        assert_eq!(rows[1].status, STATUS_PENDING);
    }
}
