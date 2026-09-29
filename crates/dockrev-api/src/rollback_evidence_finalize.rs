use std::path::PathBuf;

use anyhow::Context as _;

use crate::rollback_evidence::{RollbackEvidenceContext, bounded_summary_errors};

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

    let notification_enabled = match db
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
        Ok(notification_enabled) => notification_enabled,
        Err(error) => {
            return finish_after_archive_error(
                db,
                job_id,
                status,
                finished_at,
                summary_json,
                evidence,
                settlements,
                notification,
                error,
            )
            .await;
        }
    };
    if let Some(evidence) = evidence {
        let archive_lookup = db.rollback_evidence_archive_size(job_id).await;
        cleanup_evidence_after_archive_lookup(job_id, Some(evidence), archive_lookup).await;
    }
    Ok(notification_enabled)
}

#[allow(clippy::too_many_arguments)]
async fn finish_after_archive_error(
    db: &crate::db::Db,
    job_id: &str,
    status: &str,
    finished_at: &str,
    summary_json: &mut serde_json::Value,
    evidence: Option<&RollbackEvidenceContext>,
    settlements: Option<&[crate::db::ServiceAcceptedStateSettlement]>,
    notification: Option<&crate::db::NotificationItemDraft>,
    error: anyhow::Error,
) -> anyhow::Result<Option<bool>> {
    match archive_commit_is_visible(db, job_id, status, finished_at).await {
        Ok(true) => {
            if let Some(evidence) = evidence
                && let Err(cleanup_error) = evidence.cleanup_after_commit().await
            {
                tracing::warn!(
                    job_id = %job_id,
                    error = %cleanup_error,
                    "committed rollback evidence local cleanup failed"
                );
            }
            return Err(
                error.context("job and rollback evidence committed before a post-commit failure")
            );
        }
        Ok(false) => {}
        Err(inspection_error) => {
            return Err(error.context(format!(
                "could not verify rollback evidence commit after finish error: {inspection_error:#}"
            )));
        }
    }

    record_archive_persistence_failure(summary_json, &error);
    db.finish_job_with_archive_file_and_settlement_and_notification(
        job_id,
        status,
        finished_at,
        summary_json,
        None,
        settlements,
        notification,
    )
    .await
    .context("finish job after rollback evidence archive persistence failure")
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
    let mut errors = evidence
        .get("errors")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    errors.insert(0, bounded_archive_error(error));
    evidence.insert(
        "errors".to_string(),
        serde_json::json!(bounded_summary_errors(errors, false)),
    );
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

async fn cleanup_evidence_after_archive_lookup(
    job_id: &str,
    evidence: Option<&RollbackEvidenceContext>,
    archive_lookup: anyhow::Result<Option<u64>>,
) {
    match (evidence, archive_lookup) {
        (Some(evidence), Ok(Some(_))) => {
            if let Err(error) = evidence.cleanup_after_commit().await {
                tracing::warn!(
                    job_id = %job_id,
                    error = %error,
                    "committed rollback evidence local cleanup failed"
                );
            }
        }
        (Some(_), Ok(None)) => tracing::warn!(
            job_id = %job_id,
            "preserving rollback evidence because the committed archive is absent"
        ),
        (Some(_), Err(error)) => tracing::warn!(
            job_id = %job_id,
            error = %error,
            "preserving rollback evidence after post-commit archive lookup failure"
        ),
        (None, _) => {}
    }
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

    #[test]
    fn archive_persistence_failure_keeps_summary_errors_bounded_and_visible() {
        let mut summary = serde_json::json!({
            "rollbackEvidence": {
                "status": "available",
                "archiveSizeBytes": 123,
                "errors": (0..16).map(|index| format!("capture error {index}")).collect::<Vec<_>>()
            }
        });

        record_archive_persistence_failure(
            &mut summary,
            &anyhow::anyhow!("injected archive write failure"),
        );

        assert_eq!(summary["rollbackEvidence"]["status"], "incomplete");
        let errors = summary["rollbackEvidence"]["errors"]
            .as_array()
            .expect("summary errors");
        assert!(errors.len() <= 16);
        assert!(
            errors[0]
                .as_str()
                .expect("new persistence error")
                .starts_with("archive persistence:")
        );
        assert!(errors.iter().any(|error| {
            error.as_str()
                == Some("rollback evidence summary metadata was truncated to remain bounded")
        }));
    }

    #[tokio::test]
    async fn spool_initialization_error_is_reported_in_healthcheck_failure_summary() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-evidence-spool-setup-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let spool_root = root.join("rollback-evidence-spool");
        tokio::fs::write(&spool_root, b"blocks directory creation")
            .await
            .expect("spool blocker");

        let (context, setup_error) =
            initialize_evidence_context(true, "job-spool-setup", &root.join("dockrev.sqlite"));
        assert!(context.is_none());
        let error = setup_error.expect("setup error");
        assert!(error.starts_with("spool setup:"));

        let mut summary = serde_json::json!({"status":"rolled_back"});
        record_spool_setup_failure(&mut summary, Some(&error), true);
        assert_eq!(summary["rollbackEvidence"]["status"], "incomplete");
        assert_eq!(summary["rollbackEvidence"]["errors"][0], error);

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn postcommit_error_keeps_committed_summary_and_cleans_temporary_evidence() {
        let root =
            std::env::temp_dir().join(format!("dockrev-evidence-postcommit-{}", ulid::Ulid::new()));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-evidence-postcommit";
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
        let evidence = RollbackEvidenceContext::new(job_id, &db_path).expect("evidence context");
        tokio::fs::create_dir_all(evidence.job_spool_path())
            .await
            .expect("evidence spool");
        tokio::fs::write(
            evidence.job_spool_path().join("container.log"),
            b"raw candidate log",
        )
        .await
        .expect("candidate log");
        let archive_path = evidence.archive_path();
        tokio::fs::write(&archive_path, b"committed archive")
            .await
            .expect("archive");
        let summary = serde_json::json!({
            "rollbackEvidence": {
                "status": "available",
                "archiveSizeBytes": b"committed archive".len(),
                "errors": []
            }
        });
        db.finish_job_with_archive_file_and_settlement_and_notification(
            job_id,
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &summary,
            Some(archive_path.clone()),
            None,
            None,
        )
        .await
        .expect("commit archive");

        let mut summary_after_error = summary.clone();
        let result = finish_after_archive_error(
            &db,
            job_id,
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &mut summary_after_error,
            Some(&evidence),
            None,
            None,
            anyhow::anyhow!("injected post-commit failure"),
        )
        .await;

        let error = result.expect_err("post-commit failure must propagate");
        assert!(
            error
                .to_string()
                .contains("committed before a post-commit failure")
        );
        let job = db.get_job(job_id).await.expect("load job").expect("job");
        assert_eq!(job.status, "rolled_back");
        assert_eq!(job.summary_json["rollbackEvidence"]["status"], "available");
        assert_eq!(
            db.get_rollback_evidence_archive(job_id)
                .await
                .expect("load committed archive")
                .expect("committed archive"),
            b"committed archive"
        );
        assert!(!evidence.job_spool_path().exists());
        assert!(!archive_path.exists());

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn postcommit_archive_lookup_error_preserves_spool_without_failing_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-evidence-lookup-error-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let evidence =
            RollbackEvidenceContext::new("job-lookup-error", &root.join("dockrev.sqlite"))
                .expect("evidence context");
        tokio::fs::create_dir_all(evidence.job_spool_path())
            .await
            .expect("spool");
        tokio::fs::write(
            evidence.job_spool_path().join("container.log"),
            b"recoverable candidate log",
        )
        .await
        .expect("candidate log");
        tokio::fs::write(evidence.archive_path(), b"recoverable archive")
            .await
            .expect("archive");

        cleanup_evidence_after_archive_lookup(
            "job-lookup-error",
            Some(&evidence),
            Err(anyhow::anyhow!("injected database lookup failure")),
        )
        .await;

        assert!(evidence.job_spool_path().exists());
        assert!(evidence.archive_path().exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
