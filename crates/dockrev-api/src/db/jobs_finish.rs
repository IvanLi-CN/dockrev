use super::*;
use std::path::{Path, PathBuf};

enum ArchiveSource {
    Bytes(Vec<u8>),
    File(PathBuf),
}

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub async fn finish_job_with_archive_and_settlement_and_notification(
        &self,
        job_id: &str,
        status: &str,
        finished_at: &str,
        summary_json: &serde_json::Value,
        archive: Option<Vec<u8>>,
        settlements: Option<&[ServiceAcceptedStateSettlement]>,
        notification: Option<&NotificationItemDraft>,
    ) -> anyhow::Result<Option<bool>> {
        self.finish_job_with_archive_source(
            job_id,
            status,
            finished_at,
            summary_json,
            archive.map(ArchiveSource::Bytes),
            settlements,
            notification,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finish_job_with_archive_file_and_settlement_and_notification(
        &self,
        job_id: &str,
        status: &str,
        finished_at: &str,
        summary_json: &serde_json::Value,
        archive_path: Option<PathBuf>,
        settlements: Option<&[ServiceAcceptedStateSettlement]>,
        notification: Option<&NotificationItemDraft>,
    ) -> anyhow::Result<Option<bool>> {
        self.finish_job_with_archive_source(
            job_id,
            status,
            finished_at,
            summary_json,
            archive_path.map(ArchiveSource::File),
            settlements,
            notification,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_job_with_archive_source(
        &self,
        job_id: &str,
        status: &str,
        finished_at: &str,
        summary_json: &serde_json::Value,
        archive: Option<ArchiveSource>,
        settlements: Option<&[ServiceAcceptedStateSettlement]>,
        notification: Option<&NotificationItemDraft>,
    ) -> anyhow::Result<Option<bool>> {
        let job_id = job_id.to_string();
        let status = status.to_string();
        let finished_at = finished_at.to_string();
        let mut summary_json = summary_json.clone();
        let settlements = settlements.map(|items| items.to_vec());
        let notification = notification.cloned();
        let projection_finished_at = finished_at.clone();
        let completed = self
            .call(move |conn| {
                let previous = conn
                    .query_row(
                        r#"
	SELECT type, reason, summary_json
FROM jobs
WHERE id = ?1
"#,
                        params![&job_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .optional()?;
                if !summary_json.is_object() {
                    summary_json = serde_json::json!({ "result": summary_json });
                }
                if let Some((_, _, previous_summary_raw)) = previous.as_ref() {
                    let previous_summary: serde_json::Value =
                        serde_json::from_str(previous_summary_raw)
                            .unwrap_or_else(|_| serde_json::json!({}));
                    if let Some(previous) = previous_summary.as_object()
                        && let Some(obj) = summary_json.as_object_mut()
                    {
                        for (key, value) in previous {
                            obj.entry(key.clone()).or_insert_with(|| value.clone());
                        }
                    }
                }

                let summary_json_str = serde_json::to_string(&summary_json)?;
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if let Some(settlements) = settlements.as_deref() {
                    Db::settle_service_operation_accepted_states_tx(
                        &tx,
                        &job_id,
                        settlements,
                        &finished_at,
                    )?;
                }
                let archive_bytes = match archive {
                    Some(ArchiveSource::Bytes(bytes)) => Some(bytes),
                    Some(ArchiveSource::File(path)) => {
                        write_archive_file_tx(&tx, &job_id, &path)?;
                        None
                    }
                    None => None,
                };
                let updated = tx.execute(
                    r#"
UPDATE jobs
SET status = ?2, finished_at = ?3, summary_json = ?4,
    rollback_evidence_tar_zstd = COALESCE(?5, rollback_evidence_tar_zstd)
WHERE id = ?1
"#,
                    params![job_id, status, finished_at, summary_json_str, archive_bytes],
                )?;
                if updated == 0 {
                    return Ok(None);
                }
                tx.execute(
                    r#"
UPDATE services
SET accepted_state_generation = (
  SELECT target.opened_generation + 1
  FROM job_service_targets target
  WHERE target.job_id = ?1
    AND target.service_id = services.id
)
WHERE id IN (
  SELECT target.service_id
  FROM job_service_targets target
  WHERE target.job_id = ?1
    AND target.opened_generation IS NOT NULL
)
  AND accepted_state_generation % 2 = 1
  AND accepted_state_generation = (
    SELECT target.opened_generation
    FROM job_service_targets target
    WHERE target.job_id = ?1
      AND target.service_id = services.id
  )
"#,
                    params![job_id],
                )?;
                let (job_type, scope, stack_id, service_id) = tx.query_row(
                    "SELECT type, scope, stack_id, service_id FROM jobs WHERE id = ?1",
                    params![&job_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )?;
                let direct_service_id = tx
                    .query_row(
                        "SELECT service_id FROM jobs WHERE id = ?1",
                        params![&job_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .flatten();
                replace_job_service_targets_tx(
                    &tx,
                    &job_id,
                    direct_service_id.as_deref(),
                    &summary_json,
                )?;
                let target_service_ids = {
                    let mut statement = tx.prepare(
                        "SELECT service_id FROM job_service_targets WHERE job_id = ?1 ORDER BY service_id",
                    )?;
                    statement
                        .query_map(params![&job_id], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?
                };
                let changed_stack_ids = summary_stack_ids(&summary_json);
                if status == "success"
                    && previous
                        .as_ref()
                        .is_some_and(|(job_type, _, _)| job_type == "check")
                {
                    version_update_observations::record_from_successful_check_summary_tx(
                        &tx,
                        &summary_json,
                    )?;
                    let event_enabled = match tx.query_row(
                        "SELECT event_new_version_enabled FROM notification_settings LIMIT 1",
                        [],
                        |row| row.get::<_, i64>(0),
                    ) {
                        Ok(value) => Some(value != 0),
                        Err(rusqlite::Error::QueryReturnedNoRows) => Some(true),
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to read new-version notification setting");
                            None
                        }
                    };
                    new_version_discoveries::record_new_version_discoveries_from_summary_conn(
                        &tx,
                        &job_id,
                        &finished_at,
                        &summary_json,
                    )?;
                    super::notification_outbox::enqueue_check_notification_tx(
                        &tx,
                        &job_id,
                        previous
                        .as_ref()
                        .map(|(_, reason, _)| reason.as_str())
                        .unwrap_or_default(),
                        &finished_at,
                        &summary_json,
                        event_enabled,
                    )?;
                }
                let notification_event_enabled = if let Some(notification) = notification.as_ref() {
                    let event_enabled = match tx.query_row(
                        "SELECT event_update_enabled FROM notification_settings LIMIT 1",
                        [],
                        |row| row.get::<_, i64>(0),
                    ) {
                        Ok(value) => value != 0,
                        Err(rusqlite::Error::QueryReturnedNoRows) => true,
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to read update notification setting; preserving notification");
                            true
                        }
                    };
                    if event_enabled {
                        super::notification_items::insert_notification_item_tx(&tx, notification)?;
                    }
                    Some(event_enabled)
                } else {
                    None
                };
                tx.commit()?;
                Ok(Some((
                    job_id,
                    status,
                    job_type,
                    scope,
                    stack_id,
                    service_id,
                    target_service_ids,
                    changed_stack_ids,
                    notification_event_enabled,
                )))
            })
            .await
            .context("finish job")?;

        if let Some((job_id, _, job_type, ..)) = completed.as_ref()
            && job_type == "update"
        {
            self.sync_auto_update_candidate_policy_for_job(job_id, &projection_finished_at)
                .await?;
        }
        let notification_event_enabled = completed.as_ref().and_then(|completed| completed.8);
        if let Some((
            job_id,
            status,
            job_type,
            scope,
            stack_id,
            service_id,
            target_service_ids,
            changed_stack_ids,
            _notification_event_enabled,
        )) = completed
        {
            let mut entities = vec![crate::management_events::ManagementEventEntity {
                entity_type: "job".to_string(),
                id: job_id.clone(),
            }];
            if let Some(stack_id) = stack_id.as_ref() {
                super::append_management_entity_if_missing(&mut entities, "stack", stack_id);
            }
            for stack_id in &changed_stack_ids {
                super::append_management_entity_if_missing(&mut entities, "stack", stack_id);
            }
            if let Some(service_id) = service_id.as_ref() {
                super::append_management_entity_if_missing(&mut entities, "service", service_id);
            }
            for service_id in &target_service_ids {
                super::append_management_entity_if_missing(&mut entities, "service", service_id);
            }
            self.management_events
                .publish_immediate(
                    "jobs",
                    entities,
                    serde_json::json!({
                        "jobId": job_id,
                        "status": status,
                        "jobType": job_type,
                        "scope": scope,
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "serviceIds": target_service_ids,
                        "changedStackIds": changed_stack_ids,
                        "terminal": true,
                    }),
                )
                .await;
        }
        Ok(notification_event_enabled)
    }
}

pub(super) fn write_archive_file_tx(
    tx: &rusqlite::Transaction<'_>,
    job_id: &str,
    archive_path: &Path,
) -> anyhow::Result<bool> {
    use std::io::Write as _;

    let row_id = tx
        .query_row(
            "SELECT rowid FROM jobs WHERE id = ?1",
            params![job_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(row_id) = row_id else {
        return Ok(false);
    };

    let mut archive = std::fs::File::open(archive_path)?;
    let archive_size = archive.metadata()?.len();
    let archive_size_sql = i64::try_from(archive_size)?;
    tx.execute(
        "UPDATE jobs SET rollback_evidence_tar_zstd = zeroblob(?2) WHERE id = ?1",
        params![job_id, archive_size_sql],
    )?;
    let mut blob = tx.blob_open(
        rusqlite::MAIN_DB,
        "jobs",
        "rollback_evidence_tar_zstd",
        row_id,
        false,
    )?;
    let copied = std::io::copy(&mut archive, &mut blob)?;
    if copied != archive_size {
        anyhow::bail!("rollback evidence archive changed while being stored");
    }
    blob.flush()?;
    blob.close()?;
    Ok(true)
}
