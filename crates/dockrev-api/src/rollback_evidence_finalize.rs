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
            match archive_commit_is_visible(db, job_id, status, finished_at).await {
                Ok(true) => {
                    if let Some(evidence) = evidence {
                        evidence.cleanup_after_commit().await;
                    }
                    return Err(error.context(
                        "job and rollback evidence committed before a post-commit failure",
                    ));
                }
                Ok(false) => {}
                Err(inspection_error) => {
                    return Err(error.context(format!(
                        "could not verify rollback evidence commit after finish error: {inspection_error:#}"
                    )));
                }
            }
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

async fn archive_commit_is_visible(
    db: &crate::db::Db,
    job_id: &str,
    expected_status: &str,
    expected_finished_at: &str,
) -> anyhow::Result<bool> {
    let Some(job) = db.get_job(job_id).await? else {
        return Ok(false);
    };
    if job.status != expected_status
        || job.finished_at.as_deref() != Some(expected_finished_at)
        || job.summary_json["rollbackEvidence"]["status"] != "available"
    {
        return Ok(false);
    }
    let Some(archive_size) = db.rollback_evidence_archive_size(job_id).await? else {
        return Ok(false);
    };
    Ok(job.summary_json["rollbackEvidence"]["archiveSizeBytes"].as_u64() == Some(archive_size))
}

pub(crate) fn initialize_evidence_context(
    enabled: bool,
    job_id: &str,
    db_path: &std::path::Path,
) -> (Option<RollbackEvidenceContext>, Option<String>) {
    if !enabled {
        return (None, None);
    }
    match RollbackEvidenceContext::new(job_id, db_path) {
        Ok(context) => (Some(context), None),
        Err(error) => {
            tracing::warn!(job_id = %job_id, error = %error, "rollback evidence spool unavailable");
            (None, Some(bounded_error("spool setup", &error)))
        }
    }
}

pub(crate) fn record_spool_setup_failure(
    summary_json: &mut serde_json::Value,
    error: Option<&str>,
    is_healthcheck_apply_failure: bool,
) {
    let (Some(error), true, Some(summary)) = (
        error,
        is_healthcheck_apply_failure,
        summary_json.as_object_mut(),
    ) else {
        return;
    };
    summary.insert(
        "rollbackEvidence".to_string(),
        serde_json::json!({
            "status": "incomplete",
            "failedCandidates": 0,
            "archiveFormat": "tar",
            "compression": "zstd",
            "archiveSizeBytes": null,
            "services": [],
            "errors": [error]
        }),
    );
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
    bounded_error("archive persistence", error)
}

fn bounded_error(prefix: &str, error: &anyhow::Error) -> String {
    const MAX_ERROR_CHARS: usize = 512;
    let message = format!("{prefix}: {error:#}");
    let mut chars = message.chars();
    let mut bounded = chars.by_ref().take(MAX_ERROR_CHARS).collect::<String>();
    if chars.next().is_some() {
        bounded.push_str("...");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn committed_archive_is_detectable_after_job_finish() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-evidence-commit-check-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-archive-commit-check";
        db.insert_job(
            crate::api::types::JobRecord::new_running(
                job_id.to_string(),
                crate::api::types::JobType::Update,
                crate::api::types::JobScope::Service,
                None,
                None,
                "2026-08-28T00:00:00Z",
            )
            .to_db(),
        )
        .await
        .expect("insert job");
        let archive = root.join("evidence.tar.zst");
        tokio::fs::write(&archive, b"archive bytes")
            .await
            .expect("archive");
        db.finish_job_with_archive_file_and_settlement_and_notification(
            job_id,
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "available",
                    "archiveSizeBytes": 13,
                    "errors": []
                }
            }),
            Some(archive),
            None,
            None,
        )
        .await
        .expect("commit archive");

        assert!(
            archive_commit_is_visible(&db, job_id, "rolled_back", "2026-08-28T00:05:00Z")
                .await
                .expect("inspect committed archive")
        );
        assert!(
            !archive_commit_is_visible(&db, job_id, "failed", "2026-08-28T00:05:00Z")
                .await
                .expect("reject mismatched terminal state")
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn spool_setup_failure_is_only_added_for_healthcheck_apply_failure() {
        let error = "spool setup: permission denied";
        let mut summary = serde_json::json!({"status":"rolled_back"});

        record_spool_setup_failure(&mut summary, Some(error), false);
        assert!(summary.get("rollbackEvidence").is_none());

        record_spool_setup_failure(&mut summary, Some(error), true);
        assert_eq!(summary["rollbackEvidence"]["status"], "incomplete");
        assert_eq!(
            summary["rollbackEvidence"]["archiveSizeBytes"],
            serde_json::Value::Null
        );
        assert_eq!(
            summary["rollbackEvidence"]["services"],
            serde_json::json!([])
        );
        assert_eq!(summary["rollbackEvidence"]["errors"][0], error);
    }
}
