#[tokio::test]
async fn cleanup_once_keeps_latest_and_removes_due_backup_without_stack_health_gate() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let latest_created_at = test_offset_rfc3339(&now, time::Duration::hours(-1));
    let old_created_at = test_offset_rfc3339(&now, time::Duration::hours(-2));
    let cleanup_after = test_offset_rfc3339(&now, time::Duration::hours(-1));

    insert_update_job_with_summary(
        &state,
        "job-cleanup-latest",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        json!({ "targets": [{"serviceId": service_id}] }),
        &latest_created_at,
    )
    .await;
    insert_update_job_with_summary(
        &state,
        "job-cleanup-old",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        json!({ "targets": [{"serviceId": service_id}] }),
        &old_created_at,
    )
    .await;

    let storage_root = crate::backup_storage::logical_backup_root(&state.config.db_path).unwrap();
    let artifact_dir = storage_root.join(&stack_id);
    tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
    let latest_path = artifact_dir.join("latest.tar.zst");
    let old_path = artifact_dir.join("old.tar.zst");
    tokio::fs::write(&latest_path, b"latest").await.unwrap();
    tokio::fs::write(&old_path, b"old").await.unwrap();

    insert_backup_record(
        &state,
        "backup-cleanup-latest",
        &stack_id,
        "job-cleanup-latest",
        &latest_created_at,
        "success",
        Some(latest_path.to_str().unwrap()),
        Some(6),
        None,
        Some(&cleanup_after),
        None,
    )
    .await;
    insert_backup_record(
        &state,
        "backup-cleanup-old",
        &stack_id,
        "job-cleanup-old",
        &old_created_at,
        "success",
        Some(old_path.to_str().unwrap()),
        Some(3),
        None,
        Some(&cleanup_after),
        None,
    )
    .await;

    crate::backup::cleanup_once(&state).await.unwrap();

    assert!(tokio::fs::try_exists(&latest_path).await.unwrap());
    assert!(!tokio::fs::try_exists(&old_path).await.unwrap());
    let records = state
        .db
        .list_service_backup_records(&stack_id, &service_id)
        .await
        .unwrap();
    let old_record = records
        .iter()
        .find(|record| record.backup_id == "backup-cleanup-old")
        .expect("cleaned backup record should remain auditable");
    assert!(old_record.deleted_at.is_some());

    tokio::fs::remove_dir_all(storage_root).await.unwrap();
}
