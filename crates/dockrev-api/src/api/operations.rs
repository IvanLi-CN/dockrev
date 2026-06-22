use super::*;

use std::collections::HashSet;

use crate::service_check;
mod transitions;
pub(crate) use transitions::*;

pub(super) const CHECK_PROGRESS_LOG_INTERVAL: Duration = Duration::from_millis(500);
pub(super) const CHECK_PARALLELISM: usize = crate::config::FIXED_CHECK_PARALLELISM;
pub(super) const CHECK_SPAWN_STAGGER: Duration = Duration::from_secs(1);
pub(super) const UPDATE_STACK_BASE_PROGRESS: f64 = 0.15;
pub(super) const UPDATE_STACK_APPLY_SPAN: f64 = 0.80;

pub(super) fn progress_percent(current: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / total).min(100)
}

pub(super) fn make_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    make_job_progress_with_percent(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        progress_percent(current, total),
    )
}

pub(super) fn make_job_progress_with_percent(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
    percent: u32,
) -> JobProgress {
    make_job_progress_with_optional_plan(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        percent,
        Some(current),
        Some(total),
        Some(percent),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_job_progress_with_percent_and_plan(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
    percent: u32,
    planned_current: u32,
    planned_total: u32,
    planned_percent: u32,
) -> JobProgress {
    make_job_progress_with_optional_plan(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        percent,
        Some(planned_current),
        Some(planned_total),
        Some(planned_percent),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_job_progress_with_optional_plan(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
    percent: u32,
    planned_current: Option<u32>,
    planned_total: Option<u32>,
    planned_percent: Option<u32>,
) -> JobProgress {
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent: percent.min(100),
        planned_current,
        planned_total,
        planned_percent: planned_percent.map(|value| value.min(100)),
        current_target,
        updated_at,
    }
}

pub(super) fn make_check_job_progress(
    message: String,
    completed: u32,
    planned: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    make_job_progress_with_percent_and_plan(
        "scanning",
        message,
        completed,
        total,
        current_target,
        updated_at,
        progress_percent(completed, total),
        planned,
        total,
        progress_percent(planned, total),
    )
}

pub(super) fn update_progress_percent(
    processed_stacks: u32,
    total_stacks: u32,
    stack_fraction: f64,
) -> u32 {
    if total_stacks == 0 {
        return 0;
    }
    let stack_fraction = stack_fraction.clamp(0.0, 1.0);
    let overall = ((processed_stacks as f64) + stack_fraction) / (total_stacks as f64);
    (overall.clamp(0.0, 1.0) * 100.0).floor() as u32
}

pub(super) fn update_apply_fraction(evt: &updater::UpdateProgressEvent) -> f64 {
    use updater::UpdateProgressStep as S;

    let service_total = evt.service_total.max(1);
    let service_index = evt.service_index.min(service_total.saturating_sub(1));
    let unit = 1.0 / service_total as f64;

    let step_fraction = match evt.step {
        S::ServiceStart => 0.02,
        S::PullStart => 0.08,
        S::PullProgress => {
            let f = evt.pull_fraction.unwrap_or(0.0).clamp(0.0, 1.0);
            0.08 + 0.42 * f
        }
        S::PullDone => 0.52,
        S::UpStart => 0.60,
        S::UpDone => 0.82,
        S::HealthStart => 0.84,
        S::HealthFailed => 0.86,
        S::HealthDone => 0.88,
        S::TargetTagPullStart => 0.90,
        S::TargetTagPullDone => 0.93,
        S::SyncTagStart => 0.95,
        S::SyncTagDone => 0.97,
        S::PullTagsStart => 0.985,
        S::PullTagsDone => 0.995,
        S::ServiceDone => 1.0,
    };

    ((service_index as f64) + step_fraction) * unit
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpdateProgressSemantics {
    Legacy,
    VerifiedOnlyBatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UpdateProgressSnapshot {
    pub percent: u32,
    pub planned_percent: Option<u32>,
}

pub(super) fn update_progress_snapshot(
    evt: &updater::UpdateProgressEvent,
    semantics: UpdateProgressSemantics,
    processed_stacks: u32,
    total_stacks: u32,
    last_percent: u32,
) -> UpdateProgressSnapshot {
    use updater::UpdateProgressStep as S;

    let legacy_percent = update_progress_percent(
        processed_stacks,
        total_stacks,
        UPDATE_STACK_BASE_PROGRESS + UPDATE_STACK_APPLY_SPAN * update_apply_fraction(evt),
    )
    .max(last_percent);

    if semantics == UpdateProgressSemantics::Legacy {
        return UpdateProgressSnapshot {
            percent: legacy_percent,
            planned_percent: Some(legacy_percent),
        };
    }

    let next_percent = match evt.step {
        S::ServiceStart | S::PullStart => last_percent,
        _ => update_progress_percent(
            processed_stacks,
            total_stacks,
            UPDATE_STACK_BASE_PROGRESS + UPDATE_STACK_APPLY_SPAN * update_apply_fraction(evt),
        )
        .max(last_percent),
    };

    let planned_percent = match evt.step {
        S::ServiceStart | S::PullStart => None,
        _ => Some(next_percent),
    };

    UpdateProgressSnapshot {
        percent: next_percent,
        planned_percent,
    }
}

pub(super) async fn persist_job_progress(
    state: &Arc<AppState>,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let evt = json!({
        "type": "job_progress",
        "jobId": job_id,
        "ts": progress.updated_at,
        "phase": progress.phase,
        "message": progress.message,
        "current": progress.current,
        "total": progress.total,
        "percent": progress.percent,
        "plannedCurrent": progress.planned_current,
        "plannedTotal": progress.planned_total,
        "plannedPercent": progress.planned_percent,
        "currentTarget": progress.current_target,
        "updatedAt": progress.updated_at,
    });

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: progress.updated_at.clone(),
                level: "event".to_string(),
                msg: evt.to_string(),
            },
        )
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(
        step: updater::UpdateProgressStep,
        pull_fraction: Option<f64>,
    ) -> updater::UpdateProgressEvent {
        updater::UpdateProgressEvent {
            step,
            service_name: "web".to_string(),
            service_index: 0,
            service_total: 2,
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
        assert_eq!(service_start.planned_percent, None);

        let pull_start = update_progress_snapshot(
            &evt(updater::UpdateProgressStep::PullStart, None),
            UpdateProgressSemantics::VerifiedOnlyBatch,
            0,
            2,
            last_percent,
        );
        assert_eq!(pull_start.percent, last_percent);
        assert_eq!(pull_start.planned_percent, None);

        let pull_progress = update_progress_snapshot(
            &evt(updater::UpdateProgressStep::PullProgress, Some(0.5)),
            UpdateProgressSemantics::VerifiedOnlyBatch,
            0,
            2,
            last_percent,
        );
        assert!(pull_progress.percent > last_percent);
        assert_eq!(pull_progress.planned_percent, Some(pull_progress.percent));
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
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_check_worker_result(
    state: &Arc<AppState>,
    job_id: &str,
    now: &str,
    host_platform: &str,
    joined: Result<CheckWorkerResult, tokio::task::JoinError>,
    total_services: u32,
    planned_services: u32,
    services_checked: &mut u32,
    services_with_candidate: &mut u32,
    discovered_versions: &mut Vec<CheckDiscoveredVersion>,
    latest_target: &mut Option<String>,
    last_progress_logged_at: &mut Option<std::time::Instant>,
    latest_progress: &mut JobProgress,
) -> Result<(), ApiError> {
    let CheckWorkerResult {
        stack_id,
        service_id,
        service_name,
        service_image_ref,
        service_image_tag,
        outcome,
    } = joined.map_err(|e| map_internal(anyhow::anyhow!("check worker join failed: {e}")))?;

    *services_checked = (*services_checked).saturating_add(1);
    *latest_target = Some(format!("{stack_id}/{service_name}"));

    let outcome = outcome.map_err(map_internal)?;
    if outcome.candidate_present {
        *services_with_candidate = (*services_with_candidate).saturating_add(1);
    }
    let current_tag_trim = service_image_tag.trim();
    let candidate_raw_tag = outcome.candidate_tag.as_deref().unwrap_or_default().trim();
    let image_repo = crate::snapshot_worker::image_repo_from_image_ref(&service_image_ref);
    let current_digest = outcome
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest);
    let candidate_digest = outcome
        .candidate_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest);
    let current_snapshot =
        if crate::notify::notification_tag_requires_settle(current_tag_trim, current_tag_trim) {
            if let (Some(image_repo), Some(current_digest)) =
                (image_repo.as_deref(), current_digest.as_deref())
            {
                notification_snapshot_display_for_digest(
                    state.as_ref(),
                    image_repo,
                    current_digest,
                    host_platform,
                    current_tag_trim,
                )
                .await?
            } else {
                NotificationSnapshotDisplay::default()
            }
        } else {
            NotificationSnapshotDisplay::default()
        };
    let current_display_tag = if current_snapshot.ready {
        current_snapshot
            .display_tag
            .unwrap_or_else(|| current_tag_trim.to_string())
    } else if crate::ignore::is_strict_semver(current_tag_trim) {
        preferred_display_tag(&service_image_tag, outcome.current_resolved_tag.as_deref())
    } else {
        current_tag_trim.to_string()
    };
    let candidate_snapshot =
        if crate::notify::notification_tag_requires_settle(candidate_raw_tag, candidate_raw_tag) {
            if let (Some(image_repo), Some(candidate_digest)) =
                (image_repo.as_deref(), candidate_digest.as_deref())
            {
                notification_snapshot_display_for_digest(
                    state.as_ref(),
                    image_repo,
                    candidate_digest,
                    host_platform,
                    candidate_raw_tag,
                )
                .await?
            } else {
                NotificationSnapshotDisplay::default()
            }
        } else {
            NotificationSnapshotDisplay::default()
        };
    let candidate_display_tag = if candidate_snapshot.ready {
        candidate_snapshot
            .display_tag
            .unwrap_or_else(|| candidate_raw_tag.to_string())
    } else {
        preferred_display_tag(candidate_raw_tag, outcome.candidate_resolved_tag.as_deref())
    };
    let current_needs_inference =
        crate::notify::notification_tag_requires_settle(current_tag_trim, &current_display_tag);
    let candidate_needs_inference =
        crate::notify::notification_tag_requires_settle(candidate_raw_tag, &candidate_display_tag);

    if outcome.candidate_present
        && outcome.candidate_digest_changed
        && let (Some(candidate_tag), Some(candidate_digest)) = (
            outcome.candidate_tag.clone(),
            outcome.candidate_digest.clone(),
        )
    {
        discovered_versions.push(CheckDiscoveredVersion {
            stack_id: stack_id.clone(),
            service_id: service_id.clone(),
            service_name: service_name.clone(),
            image_ref: service_image_ref.clone(),
            current_tag: service_image_tag.clone(),
            current_digest: outcome.current_digest.clone(),
            current_display_tag: current_display_tag.clone(),
            candidate_tag,
            candidate_display_tag: candidate_display_tag.clone(),
            candidate_digest,
        });
    }
    if let Some(image_repo) = image_repo {
        if outcome.candidate_digest_changed
            && current_needs_inference
            && let Some(current_digest) = current_digest.as_deref()
            && should_enqueue_new_version_inference(
                state.as_ref(),
                &image_repo,
                current_digest,
                host_platform,
            )
            .await?
        {
            let _ = state
                .snapshot_worker
                .enqueue(
                    &image_repo,
                    current_digest,
                    host_platform,
                    VERSION_INFERENCE_REASON_NEW_VERSION,
                )
                .await;
        }
        if outcome.candidate_digest_changed
            && candidate_needs_inference
            && let Some(candidate_digest) = candidate_digest.as_deref()
            && should_enqueue_new_version_inference(
                state.as_ref(),
                &image_repo,
                candidate_digest,
                host_platform,
            )
            .await?
        {
            let _ = state
                .snapshot_worker
                .enqueue(
                    &image_repo,
                    candidate_digest,
                    host_platform,
                    VERSION_INFERENCE_REASON_NEW_VERSION,
                )
                .await;
        }
    }

    let now_instant = std::time::Instant::now();
    let should_emit = *services_checked == 1
        || *services_checked == total_services
        || last_progress_logged_at
            .map(|ts| now_instant.duration_since(ts) >= CHECK_PROGRESS_LOG_INTERVAL)
            .unwrap_or(true);
    if should_emit {
        *last_progress_logged_at = Some(now_instant);
        let updated_at = now_rfc3339().unwrap_or_else(|_| now.to_string());
        *latest_progress = make_check_job_progress(
            format!("checking services ({}/{total_services})", *services_checked),
            *services_checked,
            planned_services,
            total_services,
            (*latest_target).clone(),
            updated_at.clone(),
        );
        if let Err(e) = persist_job_progress(state, job_id, latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist check progress");
        }
        let _ = state
            .db
            .insert_job_log(
                job_id,
                &JobLogLine {
                    ts: updated_at,
                    level: "info".to_string(),
                    msg: format!(
                        "check progress: {}/{} ({}%) current={}",
                        latest_progress.current,
                        latest_progress.total,
                        latest_progress.percent,
                        latest_progress.current_target.as_deref().unwrap_or("-"),
                    ),
                },
            )
            .await;
    }

    Ok(())
}

#[derive(Debug)]
pub(super) struct CheckWorkerResult {
    stack_id: String,
    service_id: String,
    service_name: String,
    service_image_ref: String,
    service_image_tag: String,
    outcome: anyhow::Result<crate::service_check::ServiceCheckOutcome>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckDiscoveredVersion {
    stack_id: String,
    service_id: String,
    service_name: String,
    image_ref: String,
    current_tag: String,
    current_digest: Option<String>,
    current_display_tag: String,
    candidate_tag: String,
    candidate_display_tag: String,
    candidate_digest: String,
}

pub(super) fn check_job_is_stale(
    existing: &JobListItem,
    now: &str,
    stale_threshold: time::Duration,
) -> bool {
    let started_at = existing
        .started_at
        .as_deref()
        .unwrap_or(existing.created_at.as_str());
    time::OffsetDateTime::parse(started_at, &time::format_description::well_known::Rfc3339)
        .ok()
        .and_then(|started| {
            time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)
                .ok()
                .map(|cur| cur - started)
        })
        .is_some_and(|age| age > stale_threshold)
}

fn preferred_display_tag(raw_tag: &str, resolved_tag: Option<&str>) -> String {
    resolved_tag
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .unwrap_or_else(|| raw_tag.trim())
        .to_string()
}

#[derive(Default)]
struct NotificationSnapshotDisplay {
    display_tag: Option<String>,
    ready: bool,
}

async fn notification_snapshot_display_for_digest(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    raw_tag: &str,
) -> Result<NotificationSnapshotDisplay, ApiError> {
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, checked_at, _updated_at)) = snapshot else {
        return Ok(NotificationSnapshotDisplay::default());
    };
    let Some(snapshot_entry) =
        super::stacks::parse_digest_snapshot_row(&snapshot_json, &checked_at)
    else {
        return Ok(NotificationSnapshotDisplay::default());
    };
    let ready = crate::notify::notification_snapshot_is_ready(
        &snapshot_entry.snapshot,
        snapshot_entry.snapshot.checked_at.as_str(),
    );
    let display_tag = ready
        .then(|| {
            super::stacks::infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, raw_tag)
                .into_iter()
                .next()
        })
        .flatten();
    Ok(NotificationSnapshotDisplay { display_tag, ready })
}

async fn notification_snapshot_ready_for_digest(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> Result<Option<bool>, ApiError> {
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, checked_at, _updated_at)) = snapshot else {
        return Ok(None);
    };
    Ok(Some(
        crate::notify::notification_snapshot_is_ready_from_row(&snapshot_json, &checked_at)
            .unwrap_or(false),
    ))
}

async fn should_enqueue_new_version_inference(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> Result<bool, ApiError> {
    Ok(
        notification_snapshot_ready_for_digest(state, image_repo, digest, host_platform).await?
            != Some(true),
    )
}

pub(crate) fn new_version_notification_reason(
    reason: &str,
    summary: &serde_json::Value,
) -> Option<&'static str> {
    if reason.eq_ignore_ascii_case("schedule") {
        Some("schedule")
    } else if reason.eq_ignore_ascii_case("webhook")
        || summary_emits_new_version_notification(summary)
    {
        Some("webhook")
    } else {
        None
    }
}

pub(crate) fn summary_emits_new_version_notification(summary: &serde_json::Value) -> bool {
    summary
        .get("source")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("github_webhook"))
}

pub(crate) fn summary_matched_service_ids(
    summary: &serde_json::Value,
) -> Option<std::collections::HashSet<String>> {
    let items = summary.get("matchedServiceIds")?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|value| value.as_str())
            .map(ToString::to_string)
            .collect::<std::collections::HashSet<_>>(),
    )
}

pub(super) fn merge_job_summary(
    mut summary: serde_json::Value,
    extra_summary: Option<&serde_json::Value>,
) -> serde_json::Value {
    if !summary.is_object() {
        summary = json!({ "result": summary });
    }

    if let Some(extra) = extra_summary.and_then(|value| value.as_object())
        && let Some(obj) = summary.as_object_mut()
    {
        for (key, value) in extra {
            obj.insert(key.clone(), value.clone());
        }
    }

    summary
}

pub(super) async fn maybe_notify_check_new_versions(
    state: &Arc<AppState>,
    job_id: &str,
    reason: &str,
    finished_at: &str,
    summary: &serde_json::Value,
) -> anyhow::Result<()> {
    let Some(notification_reason) = new_version_notification_reason(reason, summary) else {
        return Ok(());
    };

    let mut discovered_services = notify::extract_new_versions_discovered(summary);
    if discovered_services.is_empty() {
        return Ok(());
    }

    if summary_emits_new_version_notification(summary)
        && let Some(matched_service_ids) = summary_matched_service_ids(summary)
    {
        discovered_services.retain(|service| matched_service_ids.contains(&service.service_id));
        if discovered_services.is_empty() {
            return Ok(());
        }
    }

    let services_checked = summary
        .get("servicesChecked")
        .and_then(|v| v.as_u64())
        .unwrap_or_default()
        .min(u32::MAX as u64) as u32;
    notify::notify_new_versions_discovered(
        state.as_ref(),
        job_id,
        notification_reason,
        finished_at,
        services_checked,
        &discovered_services,
    )
    .await
}

pub(crate) async fn complete_check_job(
    state: &Arc<AppState>,
    job_id: &str,
    reason: &str,
    finished_at: &str,
    outcome: Result<serde_json::Value, ApiError>,
    failure_log_prefix: &str,
    extra_summary: Option<serde_json::Value>,
) {
    match outcome {
        Ok(summary) => {
            let summary = merge_job_summary(summary, extra_summary.as_ref());
            if let Err(e) = state
                .db
                .finish_job(job_id, "success", finished_at, &summary)
                .await
            {
                tracing::error!(job_id = %job_id, error = %e, "failed to finish check job");
            } else {
                let notify_summary = state
                    .db
                    .get_job(job_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|job| job.summary_json)
                    .unwrap_or_else(|| summary.clone());
                if let Err(e) = maybe_notify_check_new_versions(
                    state,
                    job_id,
                    reason,
                    finished_at,
                    &notify_summary,
                )
                .await
                {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "failed to send discovered-version notification"
                    );
                }
                if let Err(e) = crate::auto_update::handle_completed_check(
                    state,
                    job_id,
                    reason,
                    finished_at,
                    &notify_summary,
                )
                .await
                {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "failed to evaluate auto update policies"
                    );
                }
            }
        }
        Err(e) => {
            if let Err(err) = state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: finished_at.to_string(),
                        level: "error".to_string(),
                        msg: format!("{failure_log_prefix}: {e:?}"),
                    },
                )
                .await
            {
                tracing::warn!(job_id = %job_id, error = %err, "failed to insert check failure log");
            }
            let summary =
                merge_job_summary(json!({"error": format!("{e:?}")}), extra_summary.as_ref());
            if let Err(err) = state
                .db
                .finish_job(job_id, "failed", finished_at, &summary)
                .await
            {
                tracing::error!(job_id = %job_id, error = %err, "failed to finish failed check job");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_check_job_task(
    state: Arc<AppState>,
    job_id: String,
    scope: JobScope,
    stack_id: Option<String>,
    service_id: Option<String>,
    host_platform: String,
    started_at: String,
    reason: String,
    start_log_message: String,
    failure_log_prefix: String,
    extra_summary: Option<serde_json::Value>,
) {
    tokio::spawn(async move {
        if let Err(e) = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: started_at.clone(),
                    level: "info".to_string(),
                    msg: start_log_message,
                },
            )
            .await
        {
            tracing::warn!(job_id = %job_id, error = %e, "failed to insert check started log");
        }

        let outcome = run_check_for_job(
            &state,
            &job_id,
            &scope,
            stack_id.as_deref(),
            service_id.as_deref(),
            &host_platform,
            &started_at,
        )
        .await;

        let finished_at = match now_rfc3339() {
            Ok(ts) => ts,
            Err(err) => {
                tracing::warn!(
                    job_id = %job_id,
                    error = %err,
                    "failed to format finished_at as RFC3339; falling back to started_at"
                );
                started_at.clone()
            }
        };

        complete_check_job(
            &state,
            &job_id,
            &reason,
            &finished_at,
            outcome,
            &failure_log_prefix,
            extra_summary,
        )
        .await;
    });
}

pub(super) async fn trigger_check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerCheckRequest>,
) -> Result<Json<TriggerCheckResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    // Prevent accidental parallel checks from UI double-clicks / multiple tabs.
    // If we detect a stale running check (likely orphaned by a restart), we terminate it and proceed.
    let stale_threshold = time::Duration::hours(2);
    if let Ok(Some(existing)) = state
        .db
        .find_latest_running_check_job(
            &req.scope,
            req.stack_id.as_deref(),
            req.service_id.as_deref(),
        )
        .await
    {
        if check_job_is_stale(&existing, &now, stale_threshold) {
            let _ = state
                .db
                .terminate_job_as_failed(&existing.id, &now, "stale_check")
                .await;
        } else {
            return Err(
                ApiError::conflict("check already running").with_details(json!({
                    "existingJobId": existing.id,
                })),
            );
        }
    }

    let check_id = ids::new_check_id();
    let job = JobRecord::new_running(
        check_id.clone(),
        JobType::Check,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = user.principal.clone();
    job_db.reason = req.reason.as_str().to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    // Run the check job in the background so it is not tied to the HTTP request lifecycle.
    // This avoids orphaned `running` jobs when the client disconnects or the gateway times out.
    spawn_check_job_task(
        state.clone(),
        check_id.clone(),
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        host_platform,
        now.clone(),
        req.reason.as_str().to_string(),
        "check started".to_string(),
        "check failed".to_string(),
        None,
    );

    Ok(Json(TriggerCheckResponse { check_id }))
}

pub(super) async fn trigger_runtime_scan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerRuntimeScanRequest>,
) -> Result<Json<TriggerRuntimeScanResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    // Prevent accidental parallel scans from UI double-clicks / multiple tabs.
    if let Ok(Some(existing)) = state
        .db
        .find_latest_running_runtime_scan_job(
            &req.scope,
            req.stack_id.as_deref(),
            req.service_id.as_deref(),
        )
        .await
    {
        return Err(
            ApiError::conflict("runtime scan already running").with_details(json!({
                "existingJobId": existing.id,
            })),
        );
    }

    let job_id = ids::new_job_id();
    let job = JobRecord::new_running(
        job_id.clone(),
        JobType::RuntimeScan,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = user.principal.clone();
    job_db.reason = req.reason.as_str().to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    // Run the scan job in the background so it is not tied to the HTTP request lifecycle.
    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_scope = req.scope.clone();
    let run_stack_id = req.stack_id.clone();
    let run_service_id = req.service_id.clone();
    let run_host_platform = host_platform.clone();
    let run_started_at = now.clone();
    let run_reason = req.reason.as_str().to_string();
    tokio::spawn(async move {
        runtime_scan::run_job(
            run_state,
            runtime_scan::RuntimeScanJobArgs {
                job_id: run_job_id,
                scope: run_scope,
                stack_id: run_stack_id,
                service_id: run_service_id,
                host_platform: run_host_platform,
                started_at: run_started_at,
                reason: run_reason,
            },
        )
        .await;
    });

    Ok(Json(TriggerRuntimeScanResponse { job_id }))
}

pub(crate) async fn run_check_for_job(
    state: &Arc<AppState>,
    job_id: &str,
    scope: &JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    host_platform: &str,
    now: &str,
) -> Result<serde_json::Value, ApiError> {
    #[derive(Debug)]
    struct CheckUnit {
        stack_id: String,
        compose_project: Option<String>,
        service: crate::db::ServiceForCheck,
    }

    let stack_ids = match scope {
        JobScope::All => state.db.list_stack_ids().await.map_err(map_internal)?,
        JobScope::Stack => stack_id.map(|s| vec![s.to_string()]).unwrap_or_default(),
        JobScope::Service => {
            let service_id = service_id.unwrap_or_default().to_string();
            state
                .db
                .get_service_stack_id(&service_id)
                .await
                .map_err(map_internal)?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };

    let target_service_id =
        matches!(scope, JobScope::Service).then(|| service_id.unwrap_or_default().to_string());
    let mut units: std::collections::VecDeque<CheckUnit> = std::collections::VecDeque::new();

    for stack_id in &stack_ids {
        let compose_project = state
            .db
            .get_stack_compose_project(stack_id)
            .await
            .map_err(map_internal)?;

        let services = state
            .db
            .list_services_for_check(stack_id)
            .await
            .map_err(map_internal)?;

        for svc in services {
            if target_service_id
                .as_deref()
                .is_some_and(|target| target != svc.id)
            {
                continue;
            }
            units.push_back(CheckUnit {
                stack_id: stack_id.clone(),
                compose_project: compose_project.clone(),
                service: svc,
            });
        }
    }

    let total_services = units.len() as u32;
    let started_ts = now_rfc3339().unwrap_or_else(|_| now.to_string());
    let mut latest_progress = make_job_progress(
        "prepare",
        format!("preparing check targets ({total_services} services)"),
        0,
        total_services,
        None,
        started_ts,
    );
    if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial check progress");
    }

    let mut join_set: JoinSet<CheckWorkerResult> = JoinSet::new();

    let mut planned_services = 0u32;
    let mut services_checked = 0u32;
    let mut services_with_candidate = 0u32;
    let mut discovered_versions: Vec<CheckDiscoveredVersion> = Vec::new();
    let mut last_progress_logged_at: Option<std::time::Instant> = None;
    let mut latest_target: Option<String> = None;
    let mut next_spawn_not_before: Option<std::time::Instant> = None;
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    while services_checked < total_services {
        // Drain any ready workers first so completed progress stays responsive.
        while let Some(joined) = join_set.try_join_next() {
            handle_check_worker_result(
                state,
                job_id,
                now,
                host_platform,
                joined,
                total_services,
                planned_services,
                &mut services_checked,
                &mut services_with_candidate,
                &mut discovered_versions,
                &mut latest_target,
                &mut last_progress_logged_at,
                &mut latest_progress,
            )
            .await?;
        }

        if join_set.len() >= CHECK_PARALLELISM {
            if let Some(joined) = join_set.join_next().await {
                handle_check_worker_result(
                    state,
                    job_id,
                    now,
                    host_platform,
                    joined,
                    total_services,
                    planned_services,
                    &mut services_checked,
                    &mut services_with_candidate,
                    &mut discovered_versions,
                    &mut latest_target,
                    &mut last_progress_logged_at,
                    &mut latest_progress,
                )
                .await?;
            }
            continue;
        }

        let Some(unit) = units.pop_front() else {
            if let Some(joined) = join_set.join_next().await {
                handle_check_worker_result(
                    state,
                    job_id,
                    now,
                    host_platform,
                    joined,
                    total_services,
                    planned_services,
                    &mut services_checked,
                    &mut services_with_candidate,
                    &mut discovered_versions,
                    &mut latest_target,
                    &mut last_progress_logged_at,
                    &mut latest_progress,
                )
                .await?;
            }
            continue;
        };

        if let Some(not_before) = next_spawn_not_before
            && let Some(wait) = not_before.checked_duration_since(std::time::Instant::now())
        {
            if join_set.is_empty() {
                tokio::time::sleep(wait).await;
            } else {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    Some(joined) = join_set.join_next() => {
                        handle_check_worker_result(
                            state,
                            job_id,
                            now,
                            host_platform,
                            joined,
                            total_services,
                            planned_services,
                            &mut services_checked,
                            &mut services_with_candidate,
                            &mut discovered_versions,
                            &mut latest_target,
                            &mut last_progress_logged_at,
                            &mut latest_progress,
                        ).await?;
                        units.push_front(unit);
                        continue;
                    }
                }
            }
        }

        let spawn_state = state.clone();
        let spawn_job_id = job_id.to_string();
        let spawn_host_platform = host_platform.to_string();
        let spawn_now = now.to_string();
        let spawn_manifest_digest_cache = manifest_digest_cache.clone();
        let spawn_repo_tags_cache = repo_tags_cache.clone();
        join_set.spawn(async move {
            let stack_id = unit.stack_id.clone();
            let service_id = unit.service.id.clone();
            let service_name = unit.service.name.clone();
            let service_image_ref = unit.service.image_ref.clone();
            let service_image_tag = unit.service.image_tag.clone();
            let runtime_observation = match (
                unit.compose_project.as_deref(),
                registry::ImageRef::parse(&unit.service.image_ref),
            ) {
                (Some(project), Ok(img)) => docker_compose_service_runtime_digest(
                    spawn_state.as_ref(),
                    project,
                    &unit.service.name,
                    &repo_candidates(&img),
                )
                .await
                .ok()
                .flatten(),
                _ => None,
            };
            let outcome = crate::service_check::check_service_and_persist(
                &spawn_state,
                &spawn_job_id,
                &unit.service,
                runtime_observation,
                &spawn_host_platform,
                &spawn_now,
                &spawn_manifest_digest_cache,
                &spawn_repo_tags_cache,
            )
            .await;
            CheckWorkerResult {
                stack_id,
                service_id,
                service_name,
                service_image_ref,
                service_image_tag,
                outcome,
            }
        });

        planned_services = planned_services.saturating_add(1);
        next_spawn_not_before = Some(std::time::Instant::now() + CHECK_SPAWN_STAGGER);
        let updated_at = now_rfc3339().unwrap_or_else(|_| now.to_string());
        latest_progress = make_check_job_progress(
            format!("scheduled checks ({planned_services}/{total_services})"),
            services_checked,
            planned_services,
            total_services,
            latest_target.clone(),
            updated_at,
        );
        if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist check scheduling progress");
        }
    }

    for stack_id in &stack_ids {
        state
            .db
            .update_stack_last_check_at(stack_id, now)
            .await
            .map_err(map_internal)?;
    }

    let finished_ts = now_rfc3339().unwrap_or_else(|_| now.to_string());
    latest_progress = make_job_progress_with_percent_and_plan(
        "done",
        "check finished".to_string(),
        services_checked,
        total_services,
        latest_target,
        finished_ts.clone(),
        progress_percent(services_checked, total_services),
        planned_services,
        total_services,
        progress_percent(planned_services, total_services),
    );
    if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist final check progress");
    }

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: finished_ts,
                level: "info".to_string(),
                msg: format!(
                    "check finished: servicesChecked={services_checked} servicesWithCandidate={services_with_candidate}"
                ),
            },
        )
        .await
        .map_err(map_internal)?;

    let progress_json = serde_json::to_value(&latest_progress)
        .map_err(anyhow::Error::from)
        .map_err(map_internal)?;
    let new_versions_json = discovered_versions
        .iter()
        .map(|item| {
            json!({
                "stackId": item.stack_id,
                "serviceId": item.service_id,
                "serviceName": item.service_name,
                "imageRef": item.image_ref,
                "currentTag": item.current_tag,
                "currentDigest": item.current_digest,
                "currentDisplayTag": item.current_display_tag,
                "candidateTag": item.candidate_tag,
                "candidateDisplayTag": item.candidate_display_tag,
                "candidateDigest": item.candidate_digest,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "hostPlatform": host_platform,
        "scope": scope.as_str(),
        "stackIds": stack_ids,
        "servicesChecked": services_checked,
        "servicesWithCandidate": services_with_candidate,
        "newVersions": {
            "count": new_versions_json.len(),
            "services": new_versions_json,
        },
        "progress": progress_json,
    }))
}

pub(super) fn repo_candidates(img: &registry::ImageRef) -> Vec<String> {
    let mut out = Vec::<String>::new();
    out.push(format!("{}/{}", img.registry, img.name));
    if img.registry == "docker.io" {
        out.push(img.name.clone());
        if let Some(short) = img.name.strip_prefix("library/") {
            out.push(short.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

pub(super) async fn docker_compose_service_runtime_digest(
    state: &AppState,
    compose_project: &str,
    compose_service: &str,
    repo_candidates: &[String],
) -> anyhow::Result<Option<crate::service_check::RuntimeServiceObservation>> {
    use crate::runner::CommandSpec;

    let ps = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.project={compose_project}"),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.service={compose_service}"),
                ],
                env: Vec::new(),
            },
            std::time::Duration::from_secs(8),
        )
        .await?;

    if ps.status != 0 {
        return Err(anyhow::anyhow!(
            "docker ps failed status={} stderr={}",
            ps.status,
            ps.stderr
        ));
    }

    let container_ids = ps
        .stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if container_ids.is_empty() {
        return Ok(None);
    }

    let mut digests = std::collections::BTreeSet::<String>::new();
    let mut started_ats = std::collections::BTreeSet::<String>::new();
    for id in container_ids {
        let inspect_container = state
            .runner
            .run(
                CommandSpec {
                    program: "docker".to_string(),
                    args: vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{.Image}}\t{{.State.StartedAt}}".to_string(),
                        id,
                    ],
                    env: Vec::new(),
                },
                std::time::Duration::from_secs(10),
            )
            .await?;
        if inspect_container.status != 0 {
            continue;
        }
        let container_output = inspect_container.stdout.trim();
        if container_output.is_empty() {
            continue;
        }
        let mut parts = container_output.splitn(2, '\t');
        let Some(img_id) = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let started_at =
            crate::service_check::normalize_runtime_started_at(parts.next().map(str::trim));
        if img_id.is_empty() {
            continue;
        }

        let inspect = state
            .runner
            .run(
                CommandSpec {
                    program: "docker".to_string(),
                    args: vec![
                        "image".to_string(),
                        "inspect".to_string(),
                        img_id.to_string(),
                        "--format".to_string(),
                        "{{json .RepoDigests}}".to_string(),
                    ],
                    env: Vec::new(),
                },
                std::time::Duration::from_secs(10),
            )
            .await?;
        if inspect.status != 0 {
            continue;
        }

        let parsed = serde_json::from_str::<Vec<String>>(inspect.stdout.trim()).unwrap_or_default();
        crate::runtime_scan::insert_runtime_digests_for_image(
            &mut digests,
            &parsed,
            repo_candidates,
            img_id,
        );
        if let Some(started_at) = started_at {
            started_ats.insert(started_at);
        }
    }

    if digests.len() == 1 {
        let (started_at, started_at_inferred) =
            crate::service_check::aggregate_runtime_started_at(&started_ats);
        Ok(Some(crate::service_check::RuntimeServiceObservation {
            digest: digests.iter().next().cloned().unwrap_or_default(),
            started_at,
            started_at_inferred,
        }))
    } else {
        Ok(None)
    }
}
