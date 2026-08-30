use super::*;

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
