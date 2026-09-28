use std::path::PathBuf;

use anyhow::Context as _;

use crate::rollback_evidence::RollbackEvidenceContext;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_job_with_evidence_archive(
    db: &crate::db::Db,
    job_id: &str,
    status: &str,
    finished_at: &str,
    summary_json: &mut serde_json::Value,
    archive_path: Option<PathBuf>,
    evidence: Option<&RollbackEvidenceContext>,
    settlements: Option<&[crate::db::ServiceAcceptedStateSettlement]>,
    notification: Option<&crate::db::NotificationItemDraft>,
) -> anyhow::Result<Option<bool>> {
    let Some(archive_path) = archive_path else {
        let notification_enabled = db
            .finish_job_with_archive_file_and_settlement_and_notification(
                job_id,
                status,
                finished_at,
                summary_json,
                None,
                settlements,
                notification,
            )
            .await?;
        return Ok(notification_enabled);
    };

    let (notification_enabled, archive_committed) = match db
        .finish_job_with_archive_file_and_settlement_and_notification(
            job_id,
            status,
            finished_at,
            summary_json,
            Some(archive_path),
            settlements,
            notification,
        )
        .await
    {
        Ok(notification_enabled) => {
            let archive_committed = db.rollback_evidence_archive_size(job_id).await?.is_some();
            (notification_enabled, archive_committed)
        }
        Err(error) => {
            record_archive_persistence_failure(summary_json, &error);
            let notification_enabled = db
                .finish_job_with_archive_file_and_settlement_and_notification(
                    job_id,
                    status,
                    finished_at,
                    summary_json,
                    None,
                    settlements,
                    notification,
                )
                .await
                .context("finish job after rollback evidence archive persistence failure")?;
            (notification_enabled, false)
        }
    };
    if archive_committed && let Some(evidence) = evidence {
        evidence.cleanup_after_commit().await;
    }
    Ok(notification_enabled)
}

fn record_archive_persistence_failure(summary_json: &mut serde_json::Value, error: &anyhow::Error) {
    let Some(summary) = summary_json.as_object_mut() else {
        return;
    };
    let evidence = summary
        .entry("rollbackEvidence")
        .or_insert_with(|| serde_json::json!({}));
    let Some(evidence) = evidence.as_object_mut() else {
        return;
    };
    evidence.insert("status".to_string(), serde_json::json!("incomplete"));
    evidence.insert("archiveSizeBytes".to_string(), serde_json::Value::Null);
    let errors = evidence
        .entry("errors")
        .or_insert_with(|| serde_json::json!([]));
    if let Some(errors) = errors.as_array_mut() {
        errors.push(serde_json::json!(bounded_archive_error(error)));
    }
}

fn bounded_archive_error(error: &anyhow::Error) -> String {
    const MAX_ERROR_CHARS: usize = 512;
    let message = format!("archive persistence: {error:#}");
    let mut chars = message.chars();
    let mut bounded = chars.by_ref().take(MAX_ERROR_CHARS).collect::<String>();
    if chars.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}
