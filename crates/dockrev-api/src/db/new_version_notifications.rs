use super::*;

const STATUS_PENDING: &str = "pending";
const STATUS_SENT: &str = "sent";
const STATUS_FAILED: &str = "failed";
const STATUS_SUPERSEDED: &str = "superseded";
const ACTIVE_INDEX_NAME: &str = "idx_new_version_notifications_active_service_digest";
const TARGET_BATCH_SIZE: usize = 200;

pub(crate) type NotificationTargetKey = (String, String, String, String);
pub(super) type StableCandidateDisplayTags = std::collections::BTreeSet<String>;
pub(crate) type StableCandidateDisplayTagsByNotificationTarget =
    std::collections::HashMap<NotificationTargetKey, StableCandidateDisplayTags>;

pub(crate) fn list_stable_candidate_display_tags_for_notification_targets_conn(
    conn: &rusqlite::Connection,
    targets: &[NotificationTargetKey],
) -> rusqlite::Result<StableCandidateDisplayTagsByNotificationTarget> {
    let targets = targets
        .iter()
        .map(|(service_id, image_ref, image_tag, candidate_digest)| {
            (
                service_id.trim(),
                image_ref.trim(),
                image_tag.trim(),
                candidate_digest.trim(),
            )
        })
        .filter(|(service_id, image_ref, image_tag, candidate_digest)| {
            !service_id.is_empty()
                && !image_ref.is_empty()
                && !image_tag.is_empty()
                && !candidate_digest.is_empty()
        })
        .map(|(service_id, image_ref, image_tag, candidate_digest)| {
            (
                service_id.to_string(),
                image_ref.to_string(),
                image_tag.to_string(),
                candidate_digest.to_string(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let target_set = targets
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let service_ids = targets
        .iter()
        .map(|(service_id, _, _, _)| service_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut resolved = StableCandidateDisplayTagsByNotificationTarget::new();
    for chunk in service_ids.chunks(TARGET_BATCH_SIZE) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
SELECT
  service_id,
  image_ref,
  image_tag,
  candidate_digest,
  candidate_tag,
  candidate_display_tag
FROM new_version_notifications
WHERE service_id IN ({placeholders})
"#,
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len());
        for service_id in chunk {
            params.push(service_id);
        }
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (
                service_id,
                image_ref,
                image_tag,
                candidate_digest,
                candidate_tag,
                candidate_display_tag,
            ) = row?;
            let target = (service_id, image_ref, image_tag, candidate_digest);
            if !target_set.contains(&target) {
                continue;
            }
            let Some(stable_display_tag) =
                super::stable_candidate_display_tag(&candidate_tag, &candidate_display_tag)
            else {
                continue;
            };
            resolved
                .entry(target)
                .or_default()
                .insert(super::canonical_visible_version_tag(stable_display_tag));
        }
    }
    Ok(resolved)
}

#[cfg(test)]
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

fn normalize_candidate_digest(candidate_digest: Option<&str>) -> Option<String> {
    candidate_digest
        .map(str::trim)
        .filter(|digest| !digest.is_empty())
        .map(ToString::to_string)
}

pub(super) fn reconcile_service_new_version_notifications_tx(
    tx: &rusqlite::Transaction<'_>,
    service_id: &str,
    image_ref: &str,
    image_tag: &str,
    candidate_digest: Option<&str>,
    now: &str,
) -> rusqlite::Result<usize> {
    let candidate_digest = normalize_candidate_digest(candidate_digest);
    if let Some(candidate_digest) = candidate_digest {
        tx.execute(
            r#"
UPDATE new_version_notifications
SET
  status = ?2,
  superseded_at = COALESCE(superseded_at, ?3)
WHERE service_id = ?1
  AND status IN (?4, ?5)
  AND (
    image_ref != ?6
    OR image_tag != ?7
    OR candidate_digest != ?8
  )
"#,
            params![
                service_id,
                STATUS_SUPERSEDED,
                now,
                STATUS_PENDING,
                STATUS_SENT,
                image_ref,
                image_tag,
                candidate_digest,
            ],
        )
    } else {
        tx.execute(
            r#"
UPDATE new_version_notifications
SET
  status = ?2,
  superseded_at = COALESCE(superseded_at, ?3)
WHERE service_id = ?1
  AND status IN (?4, ?5)
"#,
            params![
                service_id,
                STATUS_SUPERSEDED,
                now,
                STATUS_PENDING,
                STATUS_SENT,
            ],
        )
    }
}

impl Db {
    #[allow(dead_code)] // API hot paths use the query-only OperationalReadModel.
    pub async fn list_stable_candidate_display_tags_for_notification_targets(
        &self,
        targets: &[NotificationTargetKey],
    ) -> anyhow::Result<StableCandidateDisplayTagsByNotificationTarget> {
        let targets = targets.to_vec();
        self.call(move |conn| {
            Ok(list_stable_candidate_display_tags_for_notification_targets_conn(conn, &targets)?)
        })
        .await
        .context("list stable candidate display tags for notification targets")
    }

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
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row = tx
                .query_row(
                    r#"
SELECT
  n.status,
  n.superseded_at,
  EXISTS(
    SELECT 1
    FROM services s
    WHERE s.id = n.service_id
      AND s.image_ref = n.image_ref
      AND s.image_tag = n.image_tag
      AND s.candidate_digest = n.candidate_digest
  )
FROM new_version_notifications n
WHERE n.id = ?1
"#,
                    params![notification_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, i64>(2)? != 0,
                        ))
                    },
                )
                .optional()?;
            let Some((existing_status, existing_superseded_at, still_current)) = row else {
                return Ok(false);
            };
            if existing_status != STATUS_PENDING && existing_status != STATUS_SUPERSEDED {
                return Ok(false);
            }

            let (status, superseded_at) = if existing_status == STATUS_SUPERSEDED {
                (STATUS_SUPERSEDED, existing_superseded_at)
            } else if still_current {
                (
                    if sent_channels.is_empty() {
                        STATUS_FAILED
                    } else {
                        STATUS_SENT
                    },
                    existing_superseded_at,
                )
            } else {
                (
                    STATUS_SUPERSEDED,
                    Some(existing_superseded_at.unwrap_or_else(|| now.clone())),
                )
            };
            let sent_at = (!sent_channels.is_empty()).then_some(now.clone());
            let changed = tx.execute(
                r#"
UPDATE new_version_notifications
SET
  status = ?2,
  sent_channels_json = ?3,
  sent_at = ?4,
  superseded_at = ?5,
  last_error = ?6
WHERE id = ?1 AND status IN (?7, ?8)
"#,
                params![
                    notification_id,
                    status,
                    serde_json::to_string(&sent_channels)?,
                    sent_at,
                    superseded_at,
                    last_error,
                    STATUS_PENDING,
                    STATUS_SUPERSEDED,
                ],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("finalize new version notification")
    }

    #[allow(dead_code)]
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
        let candidate_digest = normalize_candidate_digest(candidate_digest);
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = reconcile_service_new_version_notifications_tx(
                &tx,
                &service_id,
                &image_ref,
                &image_tag,
                candidate_digest.as_deref(),
                &now,
            )?;
            tx.commit()?;
            Ok(changed)
        })
        .await
        .context("reconcile new version notifications")
    }

    pub async fn list_current_new_version_notification_targets(
        &self,
        service_ids: &[String],
    ) -> anyhow::Result<Vec<CurrentNewVersionNotificationTarget>> {
        let service_ids = service_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if service_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.call(move |conn| {
            let placeholders = service_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
SELECT
  id,
  image_ref,
  image_tag,
  candidate_digest
FROM services
WHERE id IN ({placeholders})
"#,
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(service_ids.len());
            for service_id in &service_ids {
                params.push(service_id);
            }
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok(CurrentNewVersionNotificationTarget {
                    service_id: row.get(0)?,
                    image_ref: row.get(1)?,
                    image_tag: row.get(2)?,
                    candidate_digest: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list current new version notification targets")
    }

    #[cfg(test)]
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
    use std::{collections::BTreeMap, path::Path};

    use super::*;
    use crate::{
        api::types::{BackupRetention, ComposeConfig, StackBackupConfig},
        db::ComposeServiceSpec,
        models::{ServiceSeed, StackRecord},
    };

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

    async fn seed_service(db: &Db, service_id: &str, candidate_digest: Option<&str>) {
        let now = "2026-03-09T00:00:00Z";
        let stack = StackRecord {
            id: "stack_1".to_string(),
            name: "demo".to_string(),
            archived: false,
            compose: ComposeConfig {
                kind: "compose".to_string(),
                compose_files: vec!["/tmp/demo.yml".to_string()],
                env_file: None,
            },
            backup: StackBackupConfig {
                targets: Vec::new(),
                retention: BackupRetention::default(),
            },
            services: Vec::new(),
        };
        let seeds = vec![ServiceSeed {
            id: service_id.to_string(),
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: "latest".to_string(),
            homepage: None,
            update_guard: None,
            auto_rollback: false,
            backup_bind_paths: BTreeMap::new(),
            backup_volume_names: BTreeMap::new(),
        }];
        db.insert_stack(&stack, &seeds, now).await.unwrap();
        if let Some(candidate_digest) = candidate_digest {
            db.update_service_check_result(
                service_id,
                Some("sha256:old".to_string()),
                Some("1.0.0".to_string()),
                Some("[\"1.0.0\"]".to_string()),
                Some("latest".to_string()),
                Some("1.1.0".to_string()),
                Some(candidate_digest.to_string()),
                Some("match".to_string()),
                Some("[\"linux/amd64\"]".to_string()),
                None,
                None,
                now,
                now,
            )
            .await
            .unwrap();
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
        seed_service(&db, "svc_1", Some("sha256:new")).await;
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
    async fn reconcile_supersedes_pending_rows_and_finalize_preserves_audit() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let first = pending("nvn_1", "svc_1", "sha256:old");
        let second = pending("nvn_2", "svc_1", "sha256:old");

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
        assert_eq!(changed, 1);

        let finalized = db
            .finalize_new_version_notification(
                "nvn_1",
                &["webhook".to_string()],
                None,
                "2026-03-09T00:03:00Z",
            )
            .await
            .unwrap();
        assert!(finalized);

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
        assert_eq!(
            rows[0].superseded_at.as_deref(),
            Some("2026-03-09T00:02:00Z")
        );
        assert_eq!(rows[0].sent_at.as_deref(), Some("2026-03-09T00:03:00Z"));
        assert_eq!(rows[0].sent_channels, vec!["webhook".to_string()]);
        assert_eq!(rows[1].status, STATUS_PENDING);
    }

    #[tokio::test]
    async fn reconcile_supersedes_old_active_rows() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(&db, "svc_1", Some("sha256:old")).await;
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
        seed_service(&db, "svc_1", Some("sha256:old")).await;
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

    #[tokio::test]
    async fn sync_stack_from_compose_keeps_active_rows_for_unchanged_services() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(&db, "svc_1", Some("sha256:old")).await;
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

        db.sync_stack_from_compose(
            "stack_1",
            &["/tmp/demo.yml".to_string()],
            &[ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                image_tag: "latest".to_string(),
                homepage: None,
                update_guard: None,
                backup_bind_paths: Vec::new(),
                backup_volume_names: Vec::new(),
            }],
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();

        let retried = db.reserve_new_version_notification(&second).await.unwrap();
        assert_eq!(
            retried,
            NewVersionNotificationReserveResult::SkippedDuplicate
        );

        let rows = db
            .list_new_version_notifications_for_service("svc_1")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, STATUS_SENT);
        assert_eq!(rows[0].superseded_at, None);
    }

    #[tokio::test]
    async fn sync_stack_from_compose_supersedes_active_rows_when_baseline_changes() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(&db, "svc_1", Some("sha256:old")).await;
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

        db.sync_stack_from_compose(
            "stack_1",
            &["/tmp/demo.yml".to_string()],
            &[ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web-next".to_string(),
                image_tag: "stable".to_string(),
                homepage: None,
                update_guard: None,
                backup_bind_paths: Vec::new(),
                backup_volume_names: Vec::new(),
            }],
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();

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
        assert_eq!(
            rows[0].superseded_at.as_deref(),
            Some("2026-03-09T00:02:00Z")
        );
        assert_eq!(rows[1].status, STATUS_PENDING);

        let target = db
            .list_current_new_version_notification_targets(&["svc_1".to_string()])
            .await
            .unwrap();
        assert_eq!(target.len(), 1);
        assert_eq!(target[0].image_ref, "ghcr.io/acme/web-next");
        assert_eq!(target[0].image_tag, "stable");
        assert_eq!(target[0].candidate_digest, None);
    }

    #[tokio::test]
    async fn list_stable_candidate_display_tags_batches_large_target_sets() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(&db, "svc_1", Some("sha256:seed")).await;

        let mut targets = Vec::new();
        for idx in 0..450 {
            let digest = format!("sha256:{idx:064x}");
            let mut row = pending(&format!("nvn_{idx}"), "svc_1", &digest);
            row.candidate_display_tag = format!("1.{}.0", idx + 1);
            db.reserve_new_version_notification(&row).await.unwrap();
            targets.push((
                "svc_1".to_string(),
                "ghcr.io/acme/web".to_string(),
                "latest".to_string(),
                digest,
            ));
        }

        let resolved = db
            .list_stable_candidate_display_tags_for_notification_targets(&targets)
            .await
            .unwrap();

        assert_eq!(resolved.len(), 450);
        assert_eq!(
            resolved.get(&(
                "svc_1".to_string(),
                "ghcr.io/acme/web".to_string(),
                "latest".to_string(),
                format!("sha256:{:064x}", 0)
            )),
            Some(&std::collections::BTreeSet::from(["1.1.0".to_string()]))
        );
        assert_eq!(
            resolved.get(&(
                "svc_1".to_string(),
                "ghcr.io/acme/web".to_string(),
                "latest".to_string(),
                format!("sha256:{:064x}", 449)
            )),
            Some(&std::collections::BTreeSet::from(["1.450.0".to_string()]))
        );
    }

    #[tokio::test]
    async fn list_stable_candidate_display_tags_keeps_repo_track_provenance_separate() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(&db, "svc_1", Some("sha256:seed")).await;

        let digest = "sha256:shared".to_string();
        let mut web = pending("nvn_web", "svc_1", &digest);
        web.image_ref = "ghcr.io/acme/web".to_string();
        web.image_tag = "latest".to_string();
        web.candidate_display_tag = "1.16.2".to_string();
        db.reserve_new_version_notification(&web).await.unwrap();

        db.sync_stack_from_compose(
            "stack_1",
            &["/tmp/demo.yml".to_string()],
            &[ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/worker".to_string(),
                image_tag: "stable".to_string(),
                homepage: None,
                update_guard: None,
                backup_bind_paths: Vec::new(),
                backup_volume_names: Vec::new(),
            }],
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();

        let mut worker = pending("nvn_worker", "svc_1", &digest);
        worker.image_ref = "ghcr.io/acme/worker".to_string();
        worker.image_tag = "stable".to_string();
        worker.candidate_display_tag = "2.0.0".to_string();
        db.reserve_new_version_notification(&worker).await.unwrap();

        let mut unrequested = pending("nvn_unrequested", "svc_1", "sha256:unrequested");
        unrequested.image_ref = "ghcr.io/acme/unrequested".to_string();
        unrequested.image_tag = "edge".to_string();
        unrequested.candidate_display_tag = "3.0.0".to_string();
        db.reserve_new_version_notification(&unrequested)
            .await
            .unwrap();

        let resolved = db
            .list_stable_candidate_display_tags_for_notification_targets(&[
                (
                    "svc_1".to_string(),
                    "ghcr.io/acme/web".to_string(),
                    "latest".to_string(),
                    digest.clone(),
                ),
                (
                    "svc_1".to_string(),
                    "ghcr.io/acme/worker".to_string(),
                    "stable".to_string(),
                    digest.clone(),
                ),
            ])
            .await
            .unwrap();

        assert_eq!(resolved.len(), 2);
        assert_eq!(
            resolved.get(&(
                "svc_1".to_string(),
                "ghcr.io/acme/web".to_string(),
                "latest".to_string(),
                digest.clone()
            )),
            Some(&std::collections::BTreeSet::from(["1.16.2".to_string()]))
        );
        assert_eq!(
            resolved.get(&(
                "svc_1".to_string(),
                "ghcr.io/acme/worker".to_string(),
                "stable".to_string(),
                digest
            )),
            Some(&std::collections::BTreeSet::from(["2.0.0".to_string()]))
        );
    }
}
