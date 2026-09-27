use super::*;

const STATUS_PENDING: &str = "pending";
const STATUS_SENT: &str = "sent";
const STATUS_FAILED: &str = "failed";
const STATUS_SUPERSEDED: &str = "superseded";
const ACTIVE_INDEX_NAME: &str = "idx_new_version_notifications_active_service_digest";
const TARGET_BATCH_SIZE: usize = 200;
const DELIVERY_CLAIM_TTL_MINUTES: i64 = 5;

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
        .filter_map(|(service_id, image_ref, image_tag, candidate_digest)| {
            let candidate_digest = crate::snapshot_worker::normalize_digest(candidate_digest)?;
            let service_id = service_id.trim();
            let image_ref = image_ref.trim();
            let image_tag = image_tag.trim();
            if service_id.is_empty() || image_ref.is_empty() || image_tag.is_empty() {
                return None;
            }
            Some((
                service_id.to_string(),
                image_ref.to_string(),
                image_tag.to_string(),
                candidate_digest,
            ))
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

fn reserve_new_version_notification_tx(
    tx: &rusqlite::Transaction<'_>,
    pending: &NewVersionNotificationPending,
) -> anyhow::Result<NewVersionNotificationReserveResult> {
    let candidate_digest = normalize_candidate_digest(Some(&pending.candidate_digest))
        .ok_or_else(|| rusqlite::Error::InvalidParameterName("candidate_digest".into()))?;
    let previous = tx
        .query_row(
            r#"
SELECT job_id, sent_channels_json
FROM new_version_notifications
WHERE service_id = ?1
  AND candidate_digest = ?2
  AND status = ?3
ORDER BY created_at DESC, id DESC
LIMIT 1
"#,
            params![pending.service_id, candidate_digest, STATUS_FAILED],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (job_id, sent_channels_json) = match previous {
        Some((job_id, raw)) => (
            job_id,
            serde_json::to_string(&serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default())?,
        ),
        None => (pending.job_id.clone(), "[]".to_string()),
    };
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
            job_id,
            pending.reason,
            pending.image_ref,
            pending.image_tag,
            pending.current_tag,
            pending.current_display_tag,
            pending.candidate_tag,
            pending.candidate_display_tag,
            candidate_digest,
            STATUS_PENDING,
            sent_channels_json,
            pending.created_at,
        ],
    );

    match insert {
        Ok(_) => Ok(NewVersionNotificationReserveResult::Reserved(
            pending.id.clone(),
        )),
        Err(err) if is_active_notification_conflict(&err) => {
            let existing = tx
                .query_row(
                    "SELECT id, status FROM new_version_notifications WHERE service_id = ?1 AND candidate_digest = ?2 AND status IN (?3, ?4)",
                    params![pending.service_id, candidate_digest, STATUS_PENDING, STATUS_SENT],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            match existing {
                Some((id, status)) if status == STATUS_PENDING => {
                    Ok(NewVersionNotificationReserveResult::AlreadyPending(id))
                }
                _ => Ok(NewVersionNotificationReserveResult::SkippedDuplicate),
            }
        }
        Err(err) => Err(err.into()),
    }
}

fn normalize_candidate_digest(candidate_digest: Option<&str>) -> Option<String> {
    candidate_digest.and_then(crate::snapshot_worker::normalize_digest)
}

fn delivery_claim_expiry(now: &str) -> anyhow::Result<String> {
    Ok(
        (time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)?
            + time::Duration::minutes(DELIVERY_CLAIM_TTL_MINUTES))
        .format(&time::format_description::well_known::Rfc3339)?,
    )
}

fn try_claim_new_version_notification_tx(
    tx: &rusqlite::Transaction<'_>,
    notification_id: &str,
    claim_token: &str,
    claim_expires_at: &str,
    now: &str,
) -> rusqlite::Result<bool> {
    Ok(tx.execute(
        r#"
UPDATE new_version_notifications
SET delivery_claim_token = ?2, delivery_claim_expires_at = ?3
WHERE id = ?1
  AND status = ?4
  AND (
    delivery_claim_token IS NULL
    OR delivery_claim_expires_at <= ?5
  )
"#,
        params![
            notification_id,
            claim_token,
            claim_expires_at,
            STATUS_PENDING,
            now
        ],
    )? > 0)
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
  AND status IN (?4, ?5, ?6)
  AND (
    image_ref != ?7
    OR image_tag != ?8
    OR candidate_digest != ?9
  )
"#,
            params![
                service_id,
                STATUS_SUPERSEDED,
                now,
                STATUS_PENDING,
                STATUS_SENT,
                STATUS_FAILED,
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
  AND status IN (?4, ?5, ?6)
"#,
            params![
                service_id,
                STATUS_SUPERSEDED,
                now,
                STATUS_PENDING,
                STATUS_SENT,
                STATUS_FAILED,
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

    #[allow(dead_code)]
    pub async fn reserve_new_version_notification(
        &self,
        pending: &NewVersionNotificationPending,
    ) -> anyhow::Result<NewVersionNotificationReserveResult> {
        let pending = pending.clone();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let result = reserve_new_version_notification_tx(&tx, &pending)?;
            tx.commit()?;
            Ok(result)
        })
        .await
        .context("reserve new version notification")
    }

    pub async fn reserve_new_version_notifications_with_item(
        &self,
        pendings: &[NewVersionNotificationPending],
        draft: &super::NotificationItemDraft,
    ) -> anyhow::Result<Option<(Vec<(String, String, String)>, String, u64)>> {
        let pendings = pendings.to_vec();
        let draft = draft.clone();
        let claim_token = crate::ids::new_notification_id();
        let claim_expires_at = delivery_claim_expiry(&draft.created_at)?;
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut reservations = Vec::new();
            let mut claim_blocked = false;
            for pending in &pendings {
                match reserve_new_version_notification_tx(&tx, pending)? {
                    NewVersionNotificationReserveResult::Reserved(id) => {
                        if try_claim_new_version_notification_tx(
                            &tx,
                            &id,
                            &claim_token,
                            &claim_expires_at,
                            &draft.created_at,
                        )? {
                            reservations.push((id, pending.id.clone(), claim_token.clone()));
                        }
                    }
                    NewVersionNotificationReserveResult::AlreadyPending(id) => {
                        let notification_item_identity = tx
                            .query_row(
                                "SELECT item.identity_key FROM new_version_notifications notification JOIN notification_items item ON item.id = notification.notification_item_id WHERE notification.id = ?1",
                                params![id],
                                |row| row.get::<_, String>(0),
                            )
                            .optional()?;
                        if notification_item_identity
                            .as_deref()
                            .is_none_or(|identity| identity == draft.identity_key)
                        {
                            if try_claim_new_version_notification_tx(
                                &tx,
                                &id,
                                &claim_token,
                                &claim_expires_at,
                                &draft.created_at,
                            )? {
                                reservations.push((id, pending.id.clone(), claim_token.clone()));
                            } else {
                                claim_blocked = true;
                            }
                        }
                    }
                    NewVersionNotificationReserveResult::SkippedDuplicate => {}
                }
            }
            if reservations.is_empty() {
                if claim_blocked {
                    return Err(anyhow::anyhow!("new-version notification delivery claim is held"));
                }
                tx.commit()?;
                return Ok(None);
            }

            tx.execute(
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
            )?;
            let item_id = tx.query_row(
                "SELECT id FROM notification_items WHERE identity_key = ?1",
                params![draft.identity_key],
                |row| row.get::<_, String>(0),
            )?;
            let unread_count = tx.query_row(
                "SELECT COUNT(*) FROM notification_items WHERE read_at IS NULL",
                [],
                |row| row.get::<_, u64>(0),
            )?;
            let reserved_ids = reservations
                .iter()
                .map(|(record_id, _, _)| record_id.clone())
                .collect::<Vec<_>>();
            let placeholders = std::iter::repeat_n("?", reserved_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            tx.execute(
                &format!(
                    "UPDATE new_version_notifications SET notification_item_id = ?1 WHERE id IN ({placeholders})"
                ),
                rusqlite::params_from_iter(
                    std::iter::once(&item_id).chain(reserved_ids.iter()),
                ),
            )?;
            tx.commit()?;
            Ok(Some((reservations, item_id, unread_count)))
        })
        .await
        .context("reserve new version notifications with item")
    }

    pub async fn find_new_version_notification_batch_job_id(
        &self,
        candidates: &[(String, String)],
    ) -> anyhow::Result<Option<String>> {
        let candidates = candidates.to_vec();
        self.call(move |conn| {
            if candidates.is_empty() {
                return Ok(None);
            }
            let mut stmt = conn.prepare(
                    "SELECT job_id FROM new_version_notifications WHERE service_id = ?1 AND candidate_digest = ?2 AND status IN (?3, ?4, ?5) ORDER BY created_at ASC, id ASC LIMIT 1",
            )?;
            let mut batch_job_id = None;
            for (service_id, candidate_digest) in candidates {
                let digest = normalize_candidate_digest(Some(&candidate_digest))
                    .ok_or_else(|| rusqlite::Error::InvalidParameterName("candidate_digest".into()))?;
                let Some(job_id) = stmt
                    .query_row(
                        params![service_id, digest, STATUS_PENDING, STATUS_SENT, STATUS_FAILED],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                else {
                    return Ok(None);
                };
                if batch_job_id
                    .as_ref()
                    .is_some_and(|existing| existing != &job_id)
                {
                    return Ok(None);
                }
                batch_job_id = Some(job_id);
            }
            Ok(batch_job_id)
        })
        .await
        .context("find new version notification batch job id")
    }

    #[allow(dead_code)]
    pub async fn finalize_new_version_notification(
        &self,
        notification_id: &str,
        sent_channels: &[String],
        last_error: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        self.finalize_new_version_notification_with_claim(
            notification_id,
            sent_channels,
            last_error,
            now,
            None,
        )
        .await
    }

    pub async fn finalize_new_version_notification_with_claim(
        &self,
        notification_id: &str,
        sent_channels: &[String],
        last_error: Option<&str>,
        now: &str,
        claim_token: Option<&str>,
    ) -> anyhow::Result<bool> {
        let notification_id = notification_id.to_string();
        let sent_channels = sent_channels.to_vec();
        let last_error = last_error.map(ToString::to_string);
        let now = now.to_string();
        let claim_token = claim_token.map(ToString::to_string);
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let service_digest = super::canonical_digest_sql("s.candidate_digest");
            let notification_digest = super::canonical_digest_sql("n.candidate_digest");
            let sql = format!(
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
      AND {service_digest} = {notification_digest}
  )
FROM new_version_notifications n
WHERE n.id = ?1
"#
            );
            let row = tx
                .query_row(&sql, params![notification_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })
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
                    if sent_channels.is_empty() || last_error.is_some() {
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
  last_error = ?6,
  delivery_claim_token = NULL,
  delivery_claim_expires_at = NULL
WHERE id = ?1 AND status IN (?7, ?8)
  AND (?9 IS NULL OR delivery_claim_token = ?9)
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
                    claim_token,
                ],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("finalize new version notification")
    }

    pub async fn list_new_version_notification_sent_channels(
        &self,
        notification_ids: &[String],
    ) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
        let notification_ids = notification_ids.to_vec();
        self.call(move |conn| {
            if notification_ids.is_empty() {
                return Ok(std::collections::HashMap::new());
            }
            let placeholders = std::iter::repeat_n("?", notification_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let mut stmt = conn.prepare(&format!(
                "SELECT id, sent_channels_json FROM new_version_notifications WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(notification_ids.iter()),
                |row| {
                    let sent_channels_json: String = row.get(1)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        serde_json::from_str::<Vec<String>>(&sent_channels_json)
                            .unwrap_or_default(),
                    ))
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
        })
        .await
        .context("list new version notification sent channels")
    }

    #[allow(dead_code)]
    pub async fn list_new_version_notification_job_ids(
        &self,
        notification_ids: &[String],
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let notification_ids = notification_ids.to_vec();
        self.call(move |conn| {
            if notification_ids.is_empty() {
                return Ok(std::collections::HashMap::new());
            }
            let placeholders = std::iter::repeat_n("?", notification_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let mut stmt = conn.prepare(&format!(
                "SELECT id, job_id FROM new_version_notifications WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(notification_ids.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
            Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
        })
        .await
        .context("list new version notification job ids")
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
#[path = "new_version_notifications_tests.rs"]
mod tests;
