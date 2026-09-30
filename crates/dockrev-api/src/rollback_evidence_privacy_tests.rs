use super::*;

#[tokio::test]
async fn finalized_summary_hides_state_error_but_archive_keeps_it() {
    let root =
        std::env::temp_dir().join(format!("dockrev-state-error-privacy-{}", ulid::Ulid::new()));
    fs::create_dir_all(&root).expect("test root");
    let context =
        RollbackEvidenceContext::new("job-state-error-privacy", &root.join("dockrev.sqlite"))
            .expect("evidence context");
    let state_error = "docker state error: credential=private-marker";
    let record = EvidenceMetadata {
        service_id: "service-a".to_string(),
        candidate_id: "candidate-a".to_string(),
        health_status: "unhealthy".to_string(),
        state_error: Some(state_error.to_string()),
        ..Default::default()
    };
    context.upsert_metadata("service-a", "candidate-a", record);
    let candidate_dir = context
        .job_spool_path()
        .join("service-a")
        .join("candidate-a");
    tokio::fs::create_dir_all(&candidate_dir)
        .await
        .expect("candidate directory");
    tokio::fs::write(candidate_dir.join("container.log"), b"candidate output")
        .await
        .expect("candidate log");

    let summary = context.finalize().await;
    let summary_json = serde_json::to_string(&summary).expect("summary JSON");
    let manifest: Vec<Value> = serde_json::from_slice(
        &test_support::archive_member(&context.archive_path(), "./manifest.json").await,
    )
    .expect("archive manifest");

    assert!(!summary_json.contains(state_error));
    assert!(summary.services[0].state_error.is_none());
    assert_eq!(manifest[0]["stateError"], state_error);
    let _ = tokio::fs::remove_dir_all(root).await;
}
