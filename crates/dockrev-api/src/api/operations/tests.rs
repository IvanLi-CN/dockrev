use super::*;

fn evt(
    step: updater::UpdateProgressStep,
    pull_fraction: Option<f64>,
) -> updater::UpdateProgressEvent {
    evt_with_index(step, pull_fraction, 0, 2)
}

fn evt_with_index(
    step: updater::UpdateProgressStep,
    pull_fraction: Option<f64>,
    service_index: u32,
    service_total: u32,
) -> updater::UpdateProgressEvent {
    updater::UpdateProgressEvent {
        step,
        service_name: "web".to_string(),
        service_index,
        service_total,
        pull_fraction,
        message: "mock".to_string(),
    }
}

#[test]
fn batch_update_progress_stays_verified_only_until_pull_has_evidence() {
    let last_percent = update_progress_percent(0, 2, UPDATE_STACK_BASE_PROGRESS);

    let service_start = update_progress_snapshot(
        &evt(updater::UpdateProgressStep::ServiceStart, None),
        UpdateProgressSemantics::VerifiedOnlyBatch,
        0,
        2,
        last_percent,
    );
    assert_eq!(service_start.percent, last_percent);
    assert_eq!(service_start.planned_percent, Some(None));

    let pull_start = update_progress_snapshot(
        &evt(updater::UpdateProgressStep::PullStart, None),
        UpdateProgressSemantics::VerifiedOnlyBatch,
        0,
        2,
        last_percent,
    );
    assert_eq!(pull_start.percent, last_percent);
    assert_eq!(pull_start.planned_percent, Some(None));

    let pull_progress = update_progress_snapshot(
        &evt(updater::UpdateProgressStep::PullProgress, Some(0.5)),
        UpdateProgressSemantics::VerifiedOnlyBatch,
        0,
        2,
        last_percent,
    );
    assert!(pull_progress.percent > last_percent);
    assert_eq!(
        pull_progress.planned_percent,
        Some(Some(pull_progress.percent))
    );
}

#[test]
fn batch_pull_progress_does_not_jump_with_later_service_indexes() {
    let last_percent = update_progress_percent(0, 1, UPDATE_STACK_BASE_PROGRESS);

    let first_service_progress = update_progress_snapshot(
        &evt_with_index(updater::UpdateProgressStep::PullProgress, Some(0.5), 0, 3),
        UpdateProgressSemantics::VerifiedOnlyBatch,
        0,
        1,
        last_percent,
    );
    let later_service_progress = update_progress_snapshot(
        &evt_with_index(updater::UpdateProgressStep::PullProgress, Some(0.5), 2, 3),
        UpdateProgressSemantics::VerifiedOnlyBatch,
        0,
        1,
        last_percent,
    );

    assert_eq!(
        first_service_progress.percent,
        later_service_progress.percent
    );
    assert_eq!(
        first_service_progress.planned_percent,
        later_service_progress.planned_percent
    );
}

#[test]
fn optional_planned_progress_serializes_explicit_nulls() {
    let progress = make_job_progress_with_optional_plan(
        "apply",
        "mock".to_string(),
        2,
        5,
        Some("svc-web".to_string()),
        "2026-06-22T00:00:00Z".to_string(),
        40,
        Some(2),
        Some(5),
        None,
    );
    let value = serde_json::to_value(progress).unwrap();
    assert!(value["plannedCurrent"].is_number());
    assert!(value["plannedTotal"].is_number());
    assert!(value["plannedPercent"].is_null());
}
