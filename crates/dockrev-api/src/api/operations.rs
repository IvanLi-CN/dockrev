use super::*;

use std::collections::HashSet;

use crate::service_check;

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
    make_job_progress_with_percent_and_plan(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        percent,
        current,
        total,
        percent,
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
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent: percent.min(100),
        planned_current: Some(planned_current),
        planned_total: Some(planned_total),
        planned_percent: Some(planned_percent.min(100)),
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

pub(super) fn new_version_notification_reason(
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

pub(super) fn summary_emits_new_version_notification(summary: &serde_json::Value) -> bool {
    summary
        .get("source")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("github_webhook"))
}

pub(super) fn summary_matched_service_ids(
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

pub(super) async fn complete_check_job(
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
        let mut matched = false;
        for d in parsed {
            for repo in repo_candidates {
                if let Some(rest) = d.strip_prefix(&format!("{repo}@"))
                    && !rest.trim().is_empty()
                {
                    digests.insert(rest.trim().to_string());
                    matched = true;
                }
            }
        }
        if matched && let Some(started_at) = started_at {
            started_ats.insert(started_at);
        }
    }

    if digests.len() == 1 {
        Ok(Some(crate::service_check::RuntimeServiceObservation {
            digest: digests.iter().next().cloned().unwrap_or_default(),
            started_at: started_ats.iter().next().cloned(),
        }))
    } else {
        Ok(None)
    }
}

pub(super) async fn trigger_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<Json<TriggerUpdateResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    match req.scope {
        JobScope::Service => {
            if req.targets.is_some() {
                return Err(ApiError::invalid_argument(
                    "targets is not supported for scope=service; use targetTag/targetDigest/pullTags",
                ));
            }
            if req
                .target_tag
                .as_deref()
                .is_none_or(|t| t.trim().is_empty())
            {
                return Err(ApiError::invalid_argument(
                    "targetTag is required for scope=service",
                ));
            }
            if req
                .target_digest
                .as_deref()
                .is_none_or(|d| d.trim().is_empty())
            {
                return Err(ApiError::invalid_argument(
                    "targetDigest is required for scope=service",
                ));
            }
            if req.pull_tags.is_none() {
                return Err(ApiError::invalid_argument(
                    "pullTags is required for scope=service",
                ));
            }
        }
        JobScope::Stack | JobScope::All => {
            if req.target_tag.is_some() || req.target_digest.is_some() || req.pull_tags.is_some() {
                return Err(ApiError::invalid_argument(
                    "targetTag/targetDigest/pullTags is only supported for scope=service",
                ));
            }
            if req.targets.is_none() {
                return Err(ApiError::invalid_argument(
                    "targets is required for scope=stack/all",
                ));
            }
        }
    }

    let job_id = enqueue_update_job(
        state,
        user.principal,
        req.reason.as_str().to_string(),
        req,
        now,
    )
    .await?;

    Ok(Json(TriggerUpdateResponse { job_id }))
}

pub(super) async fn enqueue_update_job(
    state: Arc<AppState>,
    created_by: String,
    reason: String,
    mut req: TriggerUpdateRequest,
    now: String,
) -> Result<String, ApiError> {
    let stack_ids = resolve_stack_ids_for_update(&state, &req)
        .await
        .map_err(map_internal)?;
    let validated_targets = resolve_validated_update_targets(&state, &req, &stack_ids).await?;
    req.targets = Some(validated_targets);

    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::Update,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );
    job.allow_arch_mismatch = req.allow_arch_mismatch;
    job.backup_mode = req.backup_mode.as_str().to_string();
    job.summary_json = json!({ "mode": req.mode.as_str() });

    let mut job_db = job.to_db();
    job_db.created_by = created_by;
    job_db.reason = reason;
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    state
        .db
        .insert_job_log(
            &job_id,
            &JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: "update started".to_string(),
            },
        )
        .await
        .map_err(map_internal)?;
    let init_progress = make_job_progress(
        "prepare",
        "preparing update job".to_string(),
        0,
        0,
        None,
        now.clone(),
    );
    if let Err(e) = persist_job_progress(&state, &job_id, &init_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial update progress");
    }

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_req = req.clone();
    tokio::spawn(async move {
        let _ = run_update_job(run_state, run_job_id, run_req).await;
    });

    Ok(job_id)
}

fn normalize_digest_for_compare(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(':') {
        return Some(trimmed.to_string());
    }
    Some(format!("sha256:{trimmed}"))
}

fn normalize_required_update_string(field: &str, value: Option<&str>) -> Result<String, ApiError> {
    let trimmed = value.unwrap_or_default().trim();
    if trimmed.is_empty() {
        return Err(ApiError::invalid_argument(format!("{field} is required")));
    }
    Ok(trimmed.to_string())
}

fn normalize_required_pull_tags(
    field: &str,
    pull_tags: Option<&Vec<String>>,
    target_tag: &str,
) -> Result<Vec<String>, ApiError> {
    let values =
        pull_tags.ok_or_else(|| ApiError::invalid_argument(format!("{field} is required")))?;
    let target_tag = target_tag.trim();
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (idx, tag) in values.iter().enumerate() {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(ApiError::invalid_argument(format!(
                "{field}[{idx}] must not be empty"
            )));
        }
        if trimmed == target_tag {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            normalized.push(trimmed.to_string());
        }
    }
    Ok(normalized)
}

fn normalize_update_service_target(
    target: &UpdateServiceTarget,
    field_prefix: &str,
) -> Result<UpdateServiceTarget, ApiError> {
    let target_tag = normalize_required_update_string(
        &format!("{field_prefix}.targetTag"),
        Some(&target.target_tag),
    )?;
    let target_digest = normalize_required_update_string(
        &format!("{field_prefix}.targetDigest"),
        Some(&target.target_digest),
    )?;
    let normalized_digest = normalize_digest_for_compare(&target_digest).ok_or_else(|| {
        ApiError::invalid_argument(format!("{field_prefix}.targetDigest is required"))
    })?;
    Ok(UpdateServiceTarget {
        service_id: normalize_required_update_string(
            &format!("{field_prefix}.serviceId"),
            Some(&target.service_id),
        )?,
        target_tag: target_tag.clone(),
        target_digest: normalized_digest,
        pull_tags: Some(normalize_required_pull_tags(
            &format!("{field_prefix}.pullTags"),
            target.pull_tags.as_ref(),
            &target_tag,
        )?),
    })
}

fn requested_update_targets(
    req: &TriggerUpdateRequest,
) -> Result<Vec<UpdateServiceTarget>, ApiError> {
    match req.scope {
        JobScope::Service => {
            if let Some(targets) = req.targets.as_ref() {
                let mut normalized = Vec::with_capacity(targets.len());
                for (idx, target) in targets.iter().enumerate() {
                    normalized.push(normalize_update_service_target(
                        target,
                        &format!("targets[{idx}]"),
                    )?);
                }
                return Ok(normalized);
            }

            let target_tag =
                normalize_required_update_string("targetTag", req.target_tag.as_deref())?;
            let target_digest =
                normalize_required_update_string("targetDigest", req.target_digest.as_deref())?;
            let normalized_digest = normalize_digest_for_compare(&target_digest)
                .ok_or_else(|| ApiError::invalid_argument("targetDigest is required"))?;

            Ok(vec![UpdateServiceTarget {
                service_id: normalize_required_update_string(
                    "serviceId",
                    req.service_id.as_deref(),
                )?,
                target_tag: target_tag.clone(),
                target_digest: normalized_digest,
                pull_tags: Some(normalize_required_pull_tags(
                    "pullTags",
                    req.pull_tags.as_ref(),
                    &target_tag,
                )?),
            }])
        }
        JobScope::Stack | JobScope::All => {
            let targets = req.targets.as_ref().ok_or_else(|| {
                ApiError::invalid_argument("targets is required for scope=stack/all")
            })?;
            let mut normalized = Vec::with_capacity(targets.len());
            for (idx, target) in targets.iter().enumerate() {
                normalized.push(normalize_update_service_target(
                    target,
                    &format!("targets[{idx}]"),
                )?);
            }
            Ok(normalized)
        }
    }
}

pub(super) async fn resolve_stack_ids_for_update(
    state: &AppState,
    req: &TriggerUpdateRequest,
) -> anyhow::Result<Vec<String>> {
    let stack_ids = match req.scope {
        JobScope::All => state.db.list_stack_ids().await?,
        JobScope::Stack => req.stack_id.clone().into_iter().collect(),
        JobScope::Service => {
            let service_id = req.service_id.clone().unwrap_or_default();
            state
                .db
                .get_service_stack_id(&service_id)
                .await?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };
    Ok(stack_ids)
}

fn validate_target_against_service(
    svc: &crate::api::types::Service,
    target: &UpdateServiceTarget,
    allow_arch_mismatch: bool,
) -> Result<(), ApiError> {
    if target.target_tag.trim() != svc.image.tag.trim() {
        return Err(ApiError::invalid_argument(
            "cross-tag updates are not supported (targetTag must match service image tag)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedTag": svc.image.tag,
            "gotTag": target.target_tag,
        })));
    }

    let expected_digest = svc
        .candidate
        .as_ref()
        .and_then(|candidate| normalize_digest_for_compare(&candidate.digest));
    let got_digest = normalize_digest_for_compare(&target.target_digest);
    let (Some(expected_digest), Some(got_digest)) = (expected_digest.clone(), got_digest) else {
        return Err(ApiError::conflict(
            "target digest no longer matches latest scan (rescan required)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedDigest": expected_digest,
            "gotDigest": target.target_digest,
        })));
    };
    if expected_digest != got_digest {
        return Err(ApiError::conflict(
            "target digest no longer matches latest scan (rescan required)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedDigest": expected_digest,
            "gotDigest": got_digest,
        })));
    }

    if !allow_arch_mismatch
        && svc.candidate.as_ref().is_some_and(|candidate| {
            matches!(candidate.arch_match, crate::api::types::ArchMatch::Mismatch)
        })
    {
        return Err(ApiError::invalid_argument(
            "arch mismatch: re-run with allowArchMismatch=true to force update",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "archMatch": "mismatch",
        })));
    }

    Ok(())
}

pub(super) async fn resolve_validated_update_targets(
    state: &AppState,
    req: &TriggerUpdateRequest,
    stack_ids: &[String],
) -> Result<Vec<UpdateServiceTarget>, ApiError> {
    let normalized_targets = requested_update_targets(req)?;
    let mut requested_by_service = std::collections::BTreeMap::<String, UpdateServiceTarget>::new();
    for target in normalized_targets {
        if requested_by_service
            .insert(target.service_id.clone(), target)
            .is_some()
        {
            return Err(ApiError::invalid_argument(
                "targets contains duplicate serviceId",
            ));
        }
    }

    if req.scope == JobScope::Service {
        let service_id = req.service_id.as_deref().unwrap_or_default().trim();
        if requested_by_service.len() != 1 {
            return Err(ApiError::invalid_argument(
                "exactly one target is required for scope=service",
            ));
        }
        let Some(target) = requested_by_service.get(service_id) else {
            return Err(ApiError::invalid_argument(
                "scope=service target serviceId must match request serviceId",
            ));
        };

        let mut found_service = false;
        for stack_id in stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
                continue;
            };
            for svc in &stack.services {
                if svc.id != service_id {
                    continue;
                }
                validate_target_against_service(svc, target, req.allow_arch_mismatch)?;
                found_service = true;
            }
        }

        if !found_service {
            return Ok(requested_by_service.into_values().collect());
        }

        return Ok(requested_by_service.into_values().collect());
    }

    let mut expected_service_ids = std::collections::BTreeSet::<String>::new();
    let mut actionable_services =
        std::collections::BTreeMap::<String, crate::api::types::Service>::new();
    for stack_id in stack_ids {
        let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
            continue;
        };

        let selection = updater::select_update_services(
            &stack,
            &req.scope,
            req.service_id.as_deref(),
            req.allow_arch_mismatch,
            req.reason.as_str(),
            Some(state.config.dockrev_image_repo.as_str()),
        );

        for svc in selection.services {
            expected_service_ids.insert(svc.id.clone());
            actionable_services.insert(svc.id.clone(), svc.clone());
        }
    }

    let missing_service_ids = expected_service_ids
        .iter()
        .filter(|service_id| !requested_by_service.contains_key(*service_id))
        .cloned()
        .collect::<Vec<_>>();
    let extra_service_ids = requested_by_service
        .keys()
        .filter(|service_id| !expected_service_ids.contains(*service_id))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_service_ids.is_empty() || !extra_service_ids.is_empty() {
        return Err(ApiError::invalid_argument(
            "targets must exactly cover the selected services for this scope",
        )
        .with_details(json!({
            "missingServiceIds": missing_service_ids,
            "extraServiceIds": extra_service_ids,
        })));
    }

    for (service_id, target) in &requested_by_service {
        if let Some(svc) = actionable_services.get(service_id) {
            validate_target_against_service(svc, target, req.allow_arch_mismatch)?;
        }
    }

    Ok(requested_by_service.into_values().collect())
}

type UpdateStackSummaries = Vec<serde_json::Value>;
type UpdateBackupsToCleanup = Vec<(String, u32)>;
type UpdateJobOutcome = (
    String,
    UpdateStackSummaries,
    UpdateBackupsToCleanup,
    JobProgress,
);

fn extract_changed_service_ids(update: &serde_json::Value) -> Option<Vec<String>> {
    let ids = update
        .get("newDigests")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect::<Vec<_>>())?;
    if ids.is_empty() { None } else { Some(ids) }
}

pub(super) async fn run_update_job(
    state: Arc<AppState>,
    job_id: String,
    req: TriggerUpdateRequest,
) -> anyhow::Result<()> {
    let outcome: anyhow::Result<UpdateJobOutcome> = async {
        let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
        let backup_settings = state.db.get_backup_settings().await?;
        let stack_ids = resolve_stack_ids_for_update(state.as_ref(), &req).await?;
        let manifest_digest_cache = service_check::new_manifest_digest_cache();
        let repo_tags_cache = service_check::new_repo_tags_cache();
        let total_stacks = stack_ids.len() as u32;

        let mut final_status = "success".to_string();
        let mut stack_summaries = Vec::new();
        let mut backups_to_cleanup: Vec<(String, u32)> = Vec::new();
        let mut processed_stacks = 0u32;
        let mut latest_progress = make_job_progress(
            "prepare",
            format!("preparing update targets ({total_stacks} stacks)"),
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        for stack_id in &stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await? else {
                processed_stacks = processed_stacks.saturating_add(1);
                latest_progress = make_job_progress(
                    "apply",
                    format!("skipped missing stack {stack_id}"),
                    processed_stacks,
                    total_stacks,
                    Some(stack_id.clone()),
                    now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                );
                if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "failed to persist update progress"
                    );
                }
                continue;
            };
            latest_progress = make_job_progress_with_percent(
                "backup",
                format!("processing stack {stack_id}"),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, 0.08),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let logging_runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
            };

            let mut stack_summary = serde_json::Map::new();
            stack_summary.insert("stackId".to_string(), json!(stack_id));
            let planned_selection = updater::select_update_services(
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
                Some(state.config.dockrev_image_repo.as_str()),
            );
            let skipped_version_anomaly = planned_selection.skipped_version_anomaly.clone();
            let planned_service_ids = planned_selection
                .services
                .iter()
                .map(|svc| svc.id.clone())
                .collect::<Vec<_>>();
            let no_actionable_services =
                req.mode.as_str() == "apply" && planned_selection.services.is_empty();
            let no_actionable_services_after_anomaly_skip = no_actionable_services
                && !req.reason.as_str().eq_ignore_ascii_case("ui")
                && !skipped_version_anomaly.is_empty();

            let mut backup_id_for_cleanup: Option<(String, u32)> = None;
            if req.mode.as_str() == "apply"
                && !no_actionable_services
                && backup::should_run_backup(&backup_settings, req.backup_mode.as_str())
            {
                let backup_id = ids::new_backup_id();
                let now = now_rfc3339()?;
                state
                    .db
                    .insert_backup(&backup_id, stack_id, &job_id, &now)
                    .await?;
                state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: now.clone(),
                            level: "info".to_string(),
                            msg: format!("backup started: {backup_id}"),
                        },
                    )
                    .await?;

                match backup::run_pre_update_backup(
                    &logging_runner,
                    &backup_settings,
                    &stack,
                    &req.scope,
                    req.service_id.as_deref(),
                    &now,
                )
                .await
                {
                    Ok(res) => {
                        for msg in &res.log_lines {
                            let _ = state
                                .db
                                .insert_job_log(
                                    &job_id,
                                    &JobLogLine {
                                        ts: now.clone(),
                                        level: "info".to_string(),
                                        msg: msg.clone(),
                                    },
                                )
                                .await;
                        }

                        let _ = state
                            .db
                            .finish_backup(
                                &backup_id,
                                &res.status,
                                &now,
                                res.artifact_path.as_deref(),
                                res.size_bytes,
                                None,
                            )
                            .await;

                        stack_summary.insert("backup".to_string(), res.summary_json);

                        if res.status == "success" {
                            backup_id_for_cleanup = Some((
                                backup_id,
                                stack.backup.retention.delete_after_stable_seconds,
                            ));
                        }
                    }
                    Err(e) => {
                        let err = e.to_string();
                        let _ = state
                            .db
                            .finish_backup(&backup_id, "failed", &now, None, None, Some(&err))
                            .await;
                        let _ = state
                            .db
                            .insert_job_log(
                                &job_id,
                                &JobLogLine {
                                    ts: now.clone(),
                                    level: "warn".to_string(),
                                    msg: format!("backup failed: {err}"),
                                },
                            )
                            .await;

                        stack_summary
                            .insert("backup".to_string(), json!({"status":"failed","error":err}));

                        if backup_settings.require_success {
                            final_status = "failed".to_string();
                            stack_summaries.push(serde_json::Value::Object(stack_summary));
                            processed_stacks = processed_stacks.saturating_add(1);
                            latest_progress = make_job_progress(
                                "apply",
                                format!("processed stacks ({processed_stacks}/{total_stacks})"),
                                processed_stacks,
                                total_stacks,
                                Some(stack_id.clone()),
                                now_rfc3339().unwrap_or_else(|_| {
                                    time::OffsetDateTime::now_utc().to_string()
                                }),
                            );
                            if let Err(err) =
                                persist_job_progress(&state, &job_id, &latest_progress).await
                            {
                                tracing::warn!(
                                    job_id = %job_id,
                                    error = %err,
                                    "failed to persist update progress"
                                );
                            }
                            break;
                        }
                    }
                }
            } else {
                stack_summary.insert(
                    "backup".to_string(),
                    if no_actionable_services_after_anomaly_skip {
                        json!({"status":"skipped","reason":"no_actionable_services_after_anomaly_skip"})
                    } else if no_actionable_services {
                        json!({"status":"skipped","reason":"no_actionable_services"})
                    } else if req.mode.as_str() != "apply" {
                        json!({"status":"skipped","reason":"dry_run"})
                    } else {
                        json!({"status":"skipped","reason":"disabled"})
                    },
                );
            }

            latest_progress = make_job_progress_with_percent(
                "apply",
                format!("applying updates for stack {stack_id}"),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, UPDATE_STACK_BASE_PROGRESS),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::unbounded_channel::<updater::UpdateProgressEvent>();
            let progress_state = state.clone();
            let progress_job_id = job_id.clone();
            let progress_stack_id = stack_id.clone();
            let processed_stacks_for_progress = processed_stacks;
            let total_stacks_for_progress = total_stacks;
            let progress_task = tokio::spawn(async move {
                let mut last_percent = update_progress_percent(
                    processed_stacks_for_progress,
                    total_stacks_for_progress,
                    UPDATE_STACK_BASE_PROGRESS,
                );
                let mut last_emit = std::time::Instant::now()
                    .checked_sub(Duration::from_secs(5))
                    .unwrap_or_else(std::time::Instant::now);

                while let Some(evt) = progress_rx.recv().await {
                    let apply_fraction = update_apply_fraction(&evt);
                    let stack_fraction =
                        UPDATE_STACK_BASE_PROGRESS + UPDATE_STACK_APPLY_SPAN * apply_fraction;
                    let next_percent = update_progress_percent(
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        stack_fraction,
                    )
                    .max(last_percent);

                    let force_emit = matches!(
                        evt.step,
                        updater::UpdateProgressStep::PullDone
                            | updater::UpdateProgressStep::UpDone
                            | updater::UpdateProgressStep::HealthDone
                            | updater::UpdateProgressStep::TargetTagPullDone
                            | updater::UpdateProgressStep::SyncTagDone
                            | updater::UpdateProgressStep::PullTagsDone
                            | updater::UpdateProgressStep::ServiceDone
                    );
                    let should_emit = force_emit
                        || next_percent > last_percent
                        || last_emit.elapsed() >= Duration::from_millis(600);
                    if !should_emit {
                        continue;
                    }

                    last_percent = next_percent;
                    last_emit = std::time::Instant::now();
                    let updated_at = now_rfc3339()
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                    let progress_message = if evt.message.contains(&evt.service_name) {
                        evt.message
                    } else {
                        format!("{} · {}", evt.service_name, evt.message)
                    };
                    let progress = make_job_progress_with_percent(
                        "apply",
                        progress_message,
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        Some(progress_stack_id.clone()),
                        updated_at,
                        next_percent,
                    );
                    if let Err(e) =
                        persist_job_progress(&progress_state, &progress_job_id, &progress).await
                    {
                        tracing::warn!(
                            job_id = %progress_job_id,
                            error = %e,
                            "failed to persist streamed update progress"
                        );
                    }
                }
            });

            let update_outcome = updater::run_update_job(
                &logging_runner,
                &state.config.compose_bin,
                state.config.docker_config_path.as_deref(),
                updater::IdempotentRetryPolicy {
                    max_attempts: state.config.update_idempotent_retry_max_attempts,
                    base_ms: state.config.update_idempotent_retry_base_ms,
                    max_ms: state.config.update_idempotent_retry_max_ms,
                },
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.mode.as_str(),
                req.targets.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
                Some(state.config.dockrev_image_repo.as_str()),
                Some(progress_tx),
            )
            .await;
            let _ = progress_task.await;
            match update_outcome {
                Ok(outcome) => {
                    if outcome.status == "success"
                        && !planned_service_ids.is_empty()
                        && let Some(project) = state.db.get_stack_compose_project(stack_id).await?
                    {
                        let settled_at = now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                        let mut settled_services = 0usize;
                        for changed_service_id in &planned_service_ids {
                            let Some(svc) = stack.services.iter().find(|svc| svc.id == *changed_service_id) else {
                                continue;
                            };
                            let Ok(img) = registry::ImageRef::parse(&svc.image.reference) else {
                                continue;
                            };
                            let runtime_observation = docker_compose_service_runtime_digest(
                                state.as_ref(),
                                &project,
                                &svc.name,
                                &repo_candidates(&img),
                            )
                            .await
                            .ok()
                            .flatten();
                            let Some(runtime_observation) = runtime_observation else {
                                continue;
                            };

                            let svc_for_check = crate::db::ServiceForCheck {
                                id: svc.id.clone(),
                                name: svc.name.clone(),
                                image_ref: svc.image.reference.clone(),
                                image_tag: svc.image.tag.clone(),
                                current_digest: svc.image.digest.clone(),
                                current_runtime_started_at: None,
                                current_resolved_tag: svc.image.resolved_tag.clone(),
                                current_resolved_tags_json: svc
                                    .image
                                    .resolved_tags
                                    .as_ref()
                                    .and_then(|tags| serde_json::to_string(tags).ok()),
                                candidate_digest: svc
                                    .candidate
                                    .as_ref()
                                    .map(|candidate| candidate.digest.clone()),
                                candidate_resolved_tag: svc
                                    .candidate
                                    .as_ref()
                                    .and_then(|candidate| candidate.resolved_tag.clone()),
                            };
                            let mut settle_outcome = service_check::check_service_and_persist(
                                &state,
                                &job_id,
                                &svc_for_check,
                                Some(runtime_observation.clone()),
                                &host_platform,
                                &settled_at,
                                &manifest_digest_cache,
                                &repo_tags_cache,
                            )
                            .await?;
                            let mut inference_ok = true;
                            if settle_outcome.current_digest.is_none() {
                                inference_ok = false;
                                service_check::persist_runtime_fallback_result(
                                    &state.db,
                                    &svc.id,
                                    &svc.image.reference,
                                    &svc.image.tag,
                                    &runtime_observation,
                                    &settled_at,
                                )
                                .await?;
                                settle_outcome.current_digest =
                                    Some(runtime_observation.digest.clone());
                                settle_outcome.current_resolved_tag = None;
                                settle_outcome.current_resolved_tags_json = None;
                                settle_outcome.candidate_tag = None;
                                settle_outcome.candidate_resolved_tag = None;
                                settle_outcome.candidate_digest = None;
                                settle_outcome.candidate_arch_match = None;
                                settle_outcome.candidate_arch_json = None;
                                settle_outcome.ignore_rule_id = None;
                                settle_outcome.ignore_reason = None;
                                settle_outcome.candidate_present = false;
                            }
                            let evt = json!({
                                "type": "update_state_settled",
                                "jobId": job_id,
                                "ts": settled_at,
                                "stackId": stack_id,
                                "serviceId": svc.id,
                                "serviceName": svc.name,
                                "runtimeDigest": runtime_observation.digest,
                                "runtimeStartedAt": runtime_observation.started_at,
                                "candidatePresent": settle_outcome.candidate_present,
                                "inferenceOk": inference_ok,
                            });
                            state
                                .db
                                .insert_job_log(
                                    &job_id,
                                    &JobLogLine {
                                        ts: settled_at.clone(),
                                        level: "event".to_string(),
                                        msg: evt.to_string(),
                                    },
                                )
                                .await?;
                            settled_services += 1;

                            enqueue_snapshot_for_image_ref(
                                &state,
                                &svc.image.reference,
                                &runtime_observation.digest,
                                &host_platform,
                                "update_digest_changed",
                            )
                            .await;
                        }
                        if settled_services > 0 {
                            state.db.update_stack_last_check_at(stack_id, &settled_at).await?;
                        }
                    }
                    final_status = outcome.status.clone();
                    stack_summary.insert("update".to_string(), outcome.summary_json);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        format!("processed stacks ({processed_stacks}/{total_stacks})"),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to persist update progress"
                        );
                    }

                    if final_status != "success" {
                        break;
                    }

                    if let Some(b) = backup_id_for_cleanup.take() {
                        backups_to_cleanup.push(b);
                    }
                }
                Err(e) => {
                    final_status = "failed".to_string();
                    let mut update_summary = json!({"error": e.to_string()});
                    if let Some(step_failure) = e.downcast_ref::<updater::UpdateStepFailure>()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        if let Some(partial) = step_failure.partial_summary.as_ref()
                            && let Some(partial_obj) = partial.as_object()
                        {
                            for (key, value) in partial_obj {
                                obj.entry(key.clone()).or_insert_with(|| value.clone());
                            }
                        }
                        obj.insert(
                            "failureStep".to_string(),
                            json!(step_failure.step.clone()),
                        );
                        obj.insert("retry".to_string(), json!(step_failure.retry.clone()));
                        obj.insert(
                            "lastError".to_string(),
                            json!(step_failure.last_error.clone()),
                        );
                    }
                    if !skipped_version_anomaly.is_empty()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        obj.insert(
                            "skippedVersionAnomaly".to_string(),
                            serde_json::Value::Array(skipped_version_anomaly.clone()),
                        );
                    }
                    stack_summary.insert("update".to_string(), update_summary);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        format!("processed stacks ({processed_stacks}/{total_stacks})"),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(err) = persist_job_progress(&state, &job_id, &latest_progress).await
                    {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %err,
                            "failed to persist update progress"
                        );
                    }
                    break;
                }
            }
        }

        latest_progress = make_job_progress(
            "done",
            if final_status == "success" {
                "update finished".to_string()
            } else {
                "update finished with failures".to_string()
            },
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        Ok((
            final_status,
            stack_summaries,
            backups_to_cleanup,
            latest_progress,
        ))
    }
    .await;

    let (final_status, stack_summaries, backups_to_cleanup, final_summary, finished_at) =
        match outcome {
            Ok((final_status, stack_summaries, backups_to_cleanup, progress)) => {
                let progress_json = serde_json::to_value(&progress)?;
                let final_summary = json!({
                    "mode": req.mode.as_str(),
                    "stacks": stack_summaries.clone(),
                    "progress": progress_json,
                });
                let finished_at = now_rfc3339()?;
                (
                    final_status,
                    stack_summaries,
                    backups_to_cleanup,
                    final_summary,
                    finished_at,
                )
            }
            Err(err) => {
                let finished_at = now_rfc3339()?;
                let progress = make_job_progress(
                    "done",
                    "update failed".to_string(),
                    0,
                    0,
                    None,
                    finished_at.clone(),
                );
                let progress_json = serde_json::to_value(&progress)?;
                let _ = persist_job_progress(&state, &job_id, &progress).await;
                let _ = state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: format!("update failed: {err}"),
                        },
                    )
                    .await;
                let final_summary = json!({
                    "mode": req.mode.as_str(),
                    "error": err.to_string(),
                    "progress": progress_json,
                });
                (
                    "failed".to_string(),
                    Vec::new(),
                    Vec::new(),
                    final_summary,
                    finished_at,
                )
            }
        };

    let force_notify = final_status != "success";
    let mut should_notify = true;
    let mut notify_summary = final_summary.clone();
    let mut notify_skip_reason: Option<String> = None;
    if !force_notify {
        match req.scope {
            JobScope::Service => {
                if let Some(service_id) = req.service_id.as_deref()
                    && let Some(true) = state.db.is_service_archived(service_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived service".to_string());
                }
                if should_notify
                    && let Some(service_id) = req.service_id.as_deref()
                    && let Some(stack_id) = state.db.get_service_stack_id(service_id).await?
                    && let Some(true) = state.db.is_stack_archived(&stack_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived stack".to_string());
                }
            }
            JobScope::Stack | JobScope::All => {
                let mut filtered = Vec::<serde_json::Value>::new();
                for s in &stack_summaries {
                    let Some(stack_id) = s.get("stackId").and_then(|v| v.as_str()) else {
                        continue;
                    };

                    if let Some(true) = state.db.is_stack_archived(stack_id).await? {
                        continue;
                    }

                    let include = if let Some(update) = s.get("update")
                        && let Some(changed_ids) = extract_changed_service_ids(update)
                    {
                        state.db.has_unarchived_services(&changed_ids).await?
                    } else {
                        state.db.has_unarchived_services_in_stack(stack_id).await?
                    };

                    if include {
                        filtered.push(s.clone());
                    }
                }

                if filtered.is_empty() {
                    should_notify = false;
                    notify_skip_reason =
                        Some("all stacks archived or only archived services touched".to_string());
                } else {
                    notify_summary = json!({
                        "mode": req.mode.as_str(),
                        "stacks": filtered,
                    });
                }
            }
        }
    }

    if !should_notify {
        let _ = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: finished_at.clone(),
                    level: "info".to_string(),
                    msg: format!(
                        "notify skipped ({})",
                        notify_skip_reason.as_deref().unwrap_or("filtered")
                    ),
                },
            )
            .await;
    }

    state
        .db
        .finish_job(&job_id, &final_status, &finished_at, &final_summary)
        .await?;

    if final_status == "success"
        && let Ok(now_dt) = time::OffsetDateTime::parse(
            &finished_at,
            &time::format_description::well_known::Rfc3339,
        )
    {
        for (backup_id, after_seconds) in backups_to_cleanup {
            let cleanup_after = now_dt + time::Duration::seconds(after_seconds as i64);
            if let Ok(cleanup_after) =
                cleanup_after.format(&time::format_description::well_known::Rfc3339)
            {
                let _ = state
                    .db
                    .schedule_backup_cleanup(&backup_id, &cleanup_after)
                    .await;
            }
        }
    }

    if should_notify {
        let notify_state = state.clone();
        let notify_job_id = job_id.clone();
        let notify_status = final_status.clone();
        let notify_now = finished_at.clone();
        let notify_summary = notify_summary.clone();
        tokio::spawn(async move {
            let _ = notify::notify_job_updated(
                notify_state.as_ref(),
                &notify_job_id,
                &notify_status,
                &notify_now,
                &notify_summary,
            )
            .await;
        });
    }

    Ok(())
}

pub(super) struct DbLoggingRunner {
    db: crate::db::Db,
    inner: Arc<dyn crate::runner::CommandRunner>,
    job_id: String,
}

#[async_trait::async_trait]
impl crate::runner::CommandRunner for DbLoggingRunner {
    async fn run(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let start = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!("$ {} {}", spec.program, spec.args.join(" "));
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts: start,
                    level: "info".to_string(),
                    msg,
                },
            )
            .await;

        let out = self.inner.run(spec, timeout).await?;
        let ts = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status,
            truncate(&out.stdout, 2000),
            truncate(&out.stderr, 2000)
        );
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts,
                    level: if out.status == 0 {
                        "info".to_string()
                    } else {
                        "warn".to_string()
                    },
                    msg,
                },
            )
            .await;
        Ok(out)
    }

    async fn run_stream(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
        on_stdout: &mut (dyn FnMut(String) + Send),
        on_stderr: &mut (dyn FnMut(String) + Send),
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let start = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!("$ {} {}", spec.program, spec.args.join(" "));
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts: start,
                    level: "info".to_string(),
                    msg,
                },
            )
            .await;

        let mut captured_stdout = String::new();
        let mut captured_stderr = String::new();
        let mut tap_stdout = |chunk: String| {
            captured_stdout.push_str(&chunk);
            on_stdout(chunk);
        };
        let mut tap_stderr = |chunk: String| {
            captured_stderr.push_str(&chunk);
            on_stderr(chunk);
        };

        let out = self
            .inner
            .run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr)
            .await?;
        if captured_stdout.is_empty() {
            captured_stdout = out.stdout.clone();
        }
        if captured_stderr.is_empty() {
            captured_stderr = out.stderr.clone();
        }

        let ts = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status,
            truncate(&captured_stdout, 2000),
            truncate(&captured_stderr, 2000)
        );
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts,
                    level: if out.status == 0 {
                        "info".to_string()
                    } else {
                        "warn".to_string()
                    },
                    msg,
                },
            )
            .await;

        Ok(crate::runner::CommandOutput {
            status: out.status,
            stdout: captured_stdout,
            stderr: captured_stderr,
        })
    }
}

pub(super) fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_string();
    }
    format!("{}...(truncated)", &input[..max])
}
