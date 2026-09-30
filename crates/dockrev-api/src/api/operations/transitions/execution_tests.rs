use super::*;
use crate::rollback_evidence::RollbackEvidenceContext;

fn update_req(mode: UpdateMode) -> TriggerUpdateRequest {
    TriggerUpdateRequest {
        scope: JobScope::Service,
        stack_id: Some("stack_1".to_string()),
        service_id: Some("svc_1".to_string()),
        target_tag: Some("5.2".to_string()),
        target_digest: Some("sha256:abc".to_string()),
        pull_tags: Some(Vec::new()),
        targets: None,
        mode,
        allow_arch_mismatch: false,
        backup_mode: BackupMode::Inherit,
        reason: UpdateReason::Ui,
    }
}

#[test]
fn tag_history_is_recorded_only_for_successful_apply_updates() {
    assert!(should_record_update_tag_history(
        &update_req(UpdateMode::Apply),
        "success"
    ));
    assert!(!should_record_update_tag_history(
        &update_req(UpdateMode::DryRun),
        "success"
    ));
    assert!(!should_record_update_tag_history(
        &update_req(UpdateMode::Apply),
        "failed"
    ));
}

#[test]
fn backup_and_pull_progress_are_weighted_without_terminal_jump() {
    let pull = AtomicU32::new(5_000);
    let backup = AtomicU32::new(2_500);
    assert_eq!(combined_backup_pull_percent(0, 1, &pull, &backup), 27);

    pull.store(10_000, Ordering::Relaxed);
    backup.store(10_000, Ordering::Relaxed);
    assert_eq!(combined_backup_pull_percent(0, 1, &pull, &backup), 75);
    assert_eq!(combined_backup_pull_percent(1, 2, &pull, &backup), 87);
}

#[test]
fn evidence_setup_failure_checks_healthchecks_across_all_stack_summaries() {
    let summaries = vec![
        serde_json::json!({"update":{"failureStep":"pull_services"}}),
        serde_json::json!({"update":{"failureStep":"healthcheck"}}),
    ];

    assert_eq!(
        transition_failure_step(TransitionJobKind::Update, &summaries),
        Some("pull_services")
    );
    assert_eq!(
        transition_failed_candidate_count(TransitionJobKind::Update, "apply", &summaries, 0),
        1
    );

    let mut summary = serde_json::json!({"status":"rolled_back"});
    let failed_candidates =
        transition_failed_candidate_count(TransitionJobKind::Update, "apply", &summaries, 0);
    crate::rollback_evidence_finalize::record_spool_setup_failure(
        &mut summary,
        Some("spool setup: permission denied"),
        failed_candidates,
    );
    assert_eq!(summary["rollbackEvidence"]["status"], "incomplete");
    assert_eq!(summary["rollbackEvidence"]["failedCandidates"], 0);
}

#[test]
fn evidence_setup_failure_detects_later_healthcheck_failure_in_same_stack() {
    let summaries = vec![serde_json::json!({
        "update": {"failureStep":"pull_target_tag"}
    })];

    assert_eq!(
        transition_failed_candidate_count(TransitionJobKind::Update, "apply", &summaries, 0,),
        0
    );
    assert_eq!(
        transition_failed_candidate_count(TransitionJobKind::Update, "apply", &summaries, 1,),
        1
    );
    assert_eq!(
        transition_failed_candidate_count(TransitionJobKind::Update, "dry-run", &summaries, 1,),
        0
    );
}

#[test]
fn archive_metadata_read_error_keeps_evidence_summary_bounded() {
    let mut summary = crate::rollback_evidence::EvidenceSummary {
        status: "available",
        failed_candidates: 1,
        archive_format: "tar",
        compression: "zstd",
        archive_size_bytes: Some(42),
        services: Vec::new(),
        errors: (0..16)
            .map(|index| format!("capture error {index}"))
            .collect(),
    };

    mark_archive_metadata_unavailable(&mut summary, "x".repeat(600));

    assert_eq!(summary.status, "incomplete");
    assert_eq!(summary.archive_size_bytes, None);
    assert_eq!(summary.errors.len(), 16);
    assert!(summary.errors[0].starts_with("archive read: "));
    assert!(
        summary
            .errors
            .iter()
            .all(|error| error.chars().count() <= 512)
    );
    assert!(summary.errors.last().unwrap().contains("truncated"));
}

#[tokio::test]
async fn evidence_archive_persistence_failure_still_finishes_job_without_deleting_evidence() {
    let root =
        std::env::temp_dir().join(format!("dockrev-evidence-fallback-{}", ulid::Ulid::new()));
    tokio::fs::create_dir_all(&root).await.expect("test root");
    let db_path = root.join("dockrev.sqlite");
    let db = crate::db::Db::open(&db_path).await.expect("db");
    let job_id = "job-evidence-fallback";
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

    let evidence = RollbackEvidenceContext::new(job_id, &root.join("dockrev.sqlite"))
        .expect("evidence context");
    let candidate_dir = evidence
        .job_spool_path()
        .join("service-a")
        .join("candidate-a");
    tokio::fs::create_dir_all(&candidate_dir)
        .await
        .expect("evidence spool");
    tokio::fs::write(candidate_dir.join("container.log"), b"partial evidence")
        .await
        .expect("evidence log");
    tokio::fs::write(candidate_dir.join("state.json"), b"{}")
        .await
        .expect("state");
    tokio::fs::write(candidate_dir.join("health.log"), b"[]")
        .await
        .expect("health log");
    let manifest = vec![crate::rollback_evidence::EvidenceMetadata {
        service_id: "service-a".to_string(),
        candidate_id: "candidate-a".to_string(),
        health_status: "unhealthy".to_string(),
        logs_truncated: true,
        ..Default::default()
    }];
    tokio::fs::write(
        evidence.job_spool_path().join("manifest.json"),
        serde_json::to_vec(&manifest).expect("manifest JSON"),
    )
    .await
    .expect("manifest");
    let archive_path = evidence.archive_path();
    tokio::fs::write(&archive_path, b"valid archive bytes")
        .await
        .expect("archive file");
    let trigger_conn = rusqlite::Connection::open(&db_path).expect("trigger connection");
    trigger_conn
        .execute_batch(
            r#"
CREATE TRIGGER fail_evidence_summary_after_blob_write
BEFORE UPDATE OF summary_json ON jobs
WHEN NEW.rollback_evidence_tar_zstd IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'injected summary update failure');
END;
"#,
        )
        .expect("install post-blob failure trigger");
    let mut summary = serde_json::json!({
        "rollbackEvidence": {
            "status": "available",
            "archiveSizeBytes": 16,
            "errors": []
        }
    });

    let notification_enabled = crate::rollback_evidence_finalize::finish_job_with_evidence_archive(
        &db,
        job_id,
        "rolled_back",
        "2026-08-28T00:05:00Z",
        &mut summary,
        Some(archive_path),
        Some(&evidence),
        None,
        None,
    )
    .await
    .expect("job should finish without archive");

    let job = db.get_job(job_id).await.expect("load job").expect("job");
    assert_eq!(job.status, "rolled_back");
    assert_eq!(job.summary_json["rollbackEvidence"]["status"], "incomplete");
    assert_eq!(
        job.summary_json["rollbackEvidence"]["archiveSizeBytes"],
        serde_json::Value::Null
    );
    assert!(
        job.summary_json["rollbackEvidence"]["errors"]
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error
                .as_str()
                .is_some_and(|error| error.starts_with("archive persistence:")))
    );
    assert_eq!(notification_enabled, None);
    assert!(candidate_dir.join("container.log").exists());
    assert!(evidence.job_spool_path().exists());
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load archive")
            .is_none()
    );
    trigger_conn
        .execute_batch("DROP TRIGGER fail_evidence_summary_after_blob_write")
        .expect("remove post-blob failure trigger");
    drop(trigger_conn);

    crate::rollback_evidence::recover_orphaned_evidence(&db, &db_path).await;
    let recovered_job = db
        .get_job(job_id)
        .await
        .expect("load recovered job")
        .unwrap();
    assert_eq!(recovered_job.status, "rolled_back");
    assert_eq!(
        recovered_job.summary_json["rollbackEvidence"]["status"],
        "available"
    );
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load recovered archive")
            .is_some()
    );
    assert!(!evidence.job_spool_path().exists());

    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn committed_evidence_is_cleaned_when_notifications_are_disabled() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-evidence-no-notification-{}",
        ulid::Ulid::new()
    ));
    tokio::fs::create_dir_all(&root).await.expect("test root");
    let db_path = root.join("dockrev.sqlite");
    let db = crate::db::Db::open(&db_path).await.expect("db");
    let job_id = "job-evidence-no-notification";
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
    tokio::fs::write(evidence.job_spool_path().join("marker"), b"spool")
        .await
        .expect("spool marker");
    tokio::fs::write(evidence.archive_path(), b"archive bytes")
        .await
        .expect("archive file");
    let mut summary = serde_json::json!({
        "rollbackEvidence": {
            "status": "available",
            "archiveSizeBytes": 13,
            "errors": []
        }
    });

    let notification_enabled = crate::rollback_evidence_finalize::finish_job_with_evidence_archive(
        &db,
        job_id,
        "rolled_back",
        "2026-08-28T00:05:00Z",
        &mut summary,
        Some(evidence.archive_path()),
        Some(&evidence),
        None,
        None,
    )
    .await
    .expect("job should finish with archive");

    assert_eq!(notification_enabled, None);
    assert_eq!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load archive")
            .expect("archive attached"),
        b"archive bytes"
    );
    assert!(!evidence.job_spool_path().exists());
    assert!(!evidence.archive_path().exists());

    let _ = tokio::fs::remove_dir_all(root).await;
}
