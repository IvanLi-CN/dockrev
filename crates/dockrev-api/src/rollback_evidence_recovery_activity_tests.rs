use super::*;

#[tokio::test]
async fn startup_recovery_preserves_spool_held_by_active_update() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-rollback-recovery-active-{}",
        ulid::Ulid::new()
    ));
    tokio::fs::create_dir_all(&root).await.expect("test root");
    let db_path = root.join("dockrev.sqlite");
    let db = crate::db::Db::open(&db_path).await.expect("db");
    let job_id = "job-active-evidence";
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
    let evidence =
        crate::rollback_evidence_finalize::initialize_evidence_context(true, job_id, &db_path)
            .0
            .expect("active evidence context");
    let spool = evidence.job_spool_path();
    let candidate = spool.join("service-a/candidate-a");
    tokio::fs::create_dir_all(&candidate)
        .await
        .expect("candidate spool");
    tokio::fs::write(candidate.join("container.log.part"), b"partial log\xff")
        .await
        .expect("partial log");
    tokio::fs::write(candidate.join("state.json"), b"{}")
        .await
        .expect("state");
    tokio::fs::write(candidate.join("health.log"), b"[]")
        .await
        .expect("health log");
    write_manifest(
        &spool,
        &[EvidenceMetadata {
            service_id: "service-a".to_string(),
            candidate_id: "candidate-a".to_string(),
            health_status: "unhealthy".to_string(),
            logs_truncated: true,
            capture_errors: vec![CAPTURE_INTERRUPTED_REASON.to_string()],
            ..Default::default()
        }],
    )
    .await
    .expect("checkpoint manifest");

    recover_startup_interrupted_evidence(&db, &db_path).await;

    assert!(
        spool.exists(),
        "active spool must not be consumed by recovery"
    );
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("archive lookup")
            .is_none(),
        "active partial logs must not be attached as a completed recovery archive"
    );

    drop(evidence);
    recover_startup_interrupted_evidence(&db, &db_path).await;

    assert!(!spool.exists(), "released spool should be recoverable");
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("recovered archive lookup")
            .is_some()
    );
    let _ = tokio::fs::remove_dir_all(root).await;
}
