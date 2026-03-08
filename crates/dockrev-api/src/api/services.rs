use super::*;

pub(super) async fn get_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    Ok(Json(ServiceSettingsResponse {
        auto_rollback: settings.auto_rollback,
        backup_targets: settings.backup_targets,
    }))
}

pub(super) async fn trigger_service_version_inference_refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stack_id) = stack_id else {
        return Err(ApiError::not_found("service not found"));
    };
    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };
    let Some(service) = stack.services.iter().find(|svc| svc.id == service_id) else {
        return Err(ApiError::not_found("service not found"));
    };
    let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&service.image.reference)
    else {
        return Err(ApiError::invalid_argument("invalid service image ref"));
    };
    let mut digests: Vec<String> = Vec::new();
    if let Some(current_digest) = service
        .image
        .digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
    {
        digests.push(current_digest);
    }
    if let Some(candidate_digest) = service
        .candidate
        .as_ref()
        .and_then(|candidate| snapshot_worker::normalize_digest(&candidate.digest))
        && !digests.iter().any(|digest| digest == &candidate_digest)
    {
        digests.push(candidate_digest);
    }
    if digests.is_empty() {
        return Err(ApiError::invalid_argument("service digest is missing"));
    }

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut inserted = false;
    for digest in digests {
        let enqueued = state
            .snapshot_worker
            .enqueue(
                &image_repo,
                &digest,
                &host_platform,
                VERSION_INFERENCE_REASON_FORCE,
            )
            .await;
        inserted = inserted || enqueued;
    }
    let reason = if inserted {
        VERSION_INFERENCE_REASON_FORCE
    } else {
        VERSION_INFERENCE_REASON_RUNNING
    };
    let resp = TriggerVersionInferenceRefreshResponse {
        status: "pending".to_string(),
        service_id,
        image_repo,
        reason: reason.to_string(),
    };
    Ok((StatusCode::ACCEPTED, Json(resp)).into_response())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VersionInferenceOverviewQuery {
    q: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VersionInferenceEventsQuery {
    #[serde(default)]
    after_id: i64,
}

#[derive(Debug, Clone)]
pub(super) struct VersionInferenceOverviewRowAccum {
    image_repo: String,
    host_platform: String,
    service_count: u32,
    has_snapshot: bool,
    has_stale: bool,
    all_failed_only: bool,
    checked_at: Option<String>,
    updated_at: Option<String>,
    task: Option<snapshot_worker::SnapshotTaskSnapshot>,
}

pub(super) fn normalize_version_inference_status_filter(
    input: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if raw == "all" {
        return Ok(None);
    }
    match raw {
        "queued" | "running" | "ready" | "stale" | "all_failed" => Ok(Some(raw.to_string())),
        other => Err(ApiError::invalid_argument(format!(
            "invalid status filter: {other}"
        ))),
    }
}

pub(super) fn map_task_progress_state(
    progress: Option<snapshot_worker::SnapshotTaskProgress>,
) -> Option<VersionInferenceTaskProgressState> {
    progress.map(|p| VersionInferenceTaskProgressState {
        phase: p.phase,
        message: p.message,
        current: p.current,
        total: p.total,
        percent: p.percent,
        assigned_current: p.assigned_current,
        assigned_total: p.assigned_total,
        assigned_percent: p.assigned_percent,
        result_current: p.result_current,
        result_total: p.result_total,
        result_percent: p.result_percent,
        updated_at: p.updated_at,
    })
}

pub(super) fn derive_overview_row_status(
    row: &VersionInferenceOverviewRowAccum,
) -> (
    String,
    Option<String>,
    Option<VersionInferenceTaskProgressState>,
) {
    if let Some(task) = row.task.as_ref() {
        if task.status == "running" {
            return (
                "running".to_string(),
                Some(task.reason.clone()),
                map_task_progress_state(task.progress.clone()),
            );
        }
        if task.status == "queued" {
            return ("queued".to_string(), Some(task.reason.clone()), None);
        }
    }

    if row.has_snapshot {
        if row.has_stale {
            return (
                "stale".to_string(),
                Some(VERSION_INFERENCE_REASON_CACHE_STALE.to_string()),
                None,
            );
        }
        if row.all_failed_only {
            return (
                "all_failed".to_string(),
                Some(VERSION_INFERENCE_REASON_ALL_FAILED.to_string()),
                None,
            );
        }
        return ("ready".to_string(), None, None);
    }

    // Rows are constructed from cached snapshots and in-flight tasks only.
    (
        "queued".to_string(),
        Some(VERSION_INFERENCE_REASON_CACHE_MISS.to_string()),
        None,
    )
}

pub(super) fn version_inference_status_rank(status: &str) -> u8 {
    match status {
        "running" => 0,
        "queued" => 1,
        "stale" => 2,
        "all_failed" => 3,
        "ready" => 4,
        _ => 9,
    }
}

pub(super) async fn get_version_inference_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceOverviewQuery>,
) -> Result<Json<VersionInferenceOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let status_filter = normalize_version_inference_status_filter(q.status.as_deref())?;
    let search =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());

    let worker_snapshot = state.snapshot_worker.worker_stats().await;
    let gc_snapshot = state.snapshot_worker.gc_status().await;
    let task_snapshots = state.snapshot_worker.snapshot_tasks().await;
    let snapshot_rows = state
        .db
        .list_image_digest_tags_snapshots()
        .await
        .map_err(map_internal)?;
    let service_targets = state
        .db
        .list_version_inference_service_targets()
        .await
        .map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut service_count_by_key = BTreeMap::<String, u32>::new();
    for service in service_targets {
        if !needs_version_inference_for_tags(&service.image_tag, service.candidate_tag.as_deref()) {
            continue;
        }
        let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&service.image_ref)
        else {
            continue;
        };
        let key = format!("{image_repo}@{host_platform}");
        let count = service_count_by_key.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    }

    let mut rows_by_key = BTreeMap::<String, VersionInferenceOverviewRowAccum>::new();

    for snapshot in snapshot_rows {
        let key = format!("{}@{}", snapshot.image_repo, snapshot.host_platform);
        let entry =
            rows_by_key
                .entry(key.clone())
                .or_insert_with(|| VersionInferenceOverviewRowAccum {
                    image_repo: snapshot.image_repo.clone(),
                    host_platform: snapshot.host_platform.clone(),
                    service_count: *service_count_by_key.get(&key).unwrap_or(&0),
                    has_snapshot: false,
                    has_stale: false,
                    all_failed_only: true,
                    checked_at: None,
                    updated_at: None,
                    task: None,
                });
        entry.has_snapshot = true;
        entry.checked_at =
            checked_at_latest(entry.checked_at.clone(), Some(snapshot.checked_at.as_str()));
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(snapshot.updated_at.as_str()));
        if checked_at_is_stale(&snapshot.checked_at) {
            entry.has_stale = true;
        }
        let all_failed = parse_digest_snapshot_row(&snapshot.snapshot_json, &snapshot.checked_at)
            .is_some_and(|parsed| snapshot_worker::snapshot_is_all_failed(&parsed.snapshot));
        entry.all_failed_only = entry.all_failed_only && all_failed;
    }

    for task in task_snapshots.iter() {
        let image_key = format!("{}@{}", task.image_repo, task.host_platform);
        let entry = rows_by_key.entry(image_key.clone()).or_insert_with(|| {
            VersionInferenceOverviewRowAccum {
                image_repo: task.image_repo.clone(),
                host_platform: task.host_platform.clone(),
                service_count: *service_count_by_key.get(&image_key).unwrap_or(&0),
                has_snapshot: false,
                has_stale: false,
                all_failed_only: false,
                checked_at: None,
                updated_at: Some(task.updated_at.clone()),
                task: None,
            }
        });
        let replace = entry.task.as_ref().is_none_or(|existing| {
            version_inference_status_rank(&task.status)
                < version_inference_status_rank(&existing.status)
                || (task.status == existing.status && task.updated_at > existing.updated_at)
        });
        if replace {
            entry.task = Some(task.clone());
        }
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(task.updated_at.as_str()));
    }

    let mut all_rows = rows_by_key
        .into_values()
        .map(|row| {
            let (status, reason, progress) = derive_overview_row_status(&row);
            VersionInferenceOverviewRow {
                key: format!("{}@{}", row.image_repo, row.host_platform),
                image_repo: row.image_repo,
                host_platform: row.host_platform,
                status,
                service_count: row.service_count,
                reason,
                checked_at: row.checked_at,
                updated_at: row.updated_at,
                progress,
            }
        })
        .collect::<Vec<_>>();

    all_rows.sort_by(|a, b| {
        version_inference_status_rank(&a.status)
            .cmp(&version_inference_status_rank(&b.status))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.image_repo.cmp(&b.image_repo))
            .then_with(|| a.host_platform.cmp(&b.host_platform))
    });

    let snapshots_total = all_rows
        .iter()
        .filter(|row| row.checked_at.is_some())
        .count() as u32;
    let mut summary = VersionInferenceOverviewSummary {
        snapshots_total,
        queued: 0,
        running: 0,
        ready: 0,
        stale: 0,
        all_failed: 0,
    };
    for row in &all_rows {
        match row.status.as_str() {
            "queued" => summary.queued = summary.queued.saturating_add(1),
            "running" => summary.running = summary.running.saturating_add(1),
            "ready" => summary.ready = summary.ready.saturating_add(1),
            "stale" => summary.stale = summary.stale.saturating_add(1),
            "all_failed" => summary.all_failed = summary.all_failed.saturating_add(1),
            _ => {}
        }
    }

    let mut filtered_rows = all_rows;
    if let Some(status_filter) = status_filter.as_deref() {
        filtered_rows.retain(|row| row.status == status_filter);
    }
    if let Some(search) = search.as_deref() {
        filtered_rows.retain(|row| {
            row.image_repo.to_ascii_lowercase().contains(search)
                || row.key.to_ascii_lowercase().contains(search)
        });
    }

    let total = filtered_rows.len() as u32;
    let start = page.saturating_sub(1).saturating_mul(per_page) as usize;
    let rows = filtered_rows
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect::<Vec<_>>();

    let tasks = task_snapshots
        .into_iter()
        .map(|task| VersionInferenceTaskState {
            key: task.key,
            image_repo: task.image_repo,
            host_platform: task.host_platform,
            status: task.status,
            reason: task.reason,
            enqueued_at: task.enqueued_at,
            started_at: task.started_at,
            updated_at: task.updated_at,
            progress: map_task_progress_state(task.progress),
        })
        .collect::<Vec<_>>();

    Ok(Json(VersionInferenceOverviewResponse {
        worker: VersionInferenceWorkerState {
            max_concurrency: worker_snapshot.max_concurrency,
            queued: worker_snapshot.queued,
            running: worker_snapshot.running,
            in_flight: worker_snapshot.in_flight,
        },
        gc: VersionInferenceGcState {
            retention_days: gc_snapshot.retention_days,
            interval_seconds: gc_snapshot.interval_seconds,
            last_run_at: gc_snapshot.last_run_at,
            last_deleted: gc_snapshot.last_deleted,
            last_duration_ms: gc_snapshot.last_duration_ms,
            last_error: gc_snapshot.last_error,
        },
        summary,
        tasks,
        rows,
        page,
        per_page,
        total,
    }))
}

pub(super) async fn version_inference_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let mut after_id = resolve_sse_after_id(&headers, q.after_id);
    if after_id <= 0 {
        after_id = state.snapshot_worker.latest_event_id().await;
    }
    let sse_state = state.clone();

    let stream = async_stream::stream! {
        loop {
            let batch = sse_state
                .snapshot_worker
                .events_since(after_id, 200)
                .await;

            if let Some(oldest_id) = batch.oldest_id
                && after_id > 0
                && after_id < oldest_id.saturating_sub(1)
            {
                let evt = sse_state
                    .snapshot_worker
                    .emit_resync_required(after_id, oldest_id, batch.latest_id)
                    .await;
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
                continue;
            }

            if batch.events.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            for evt in batch.events {
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
            }
        }
    };
    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((resp_headers, sse))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServiceResourceHistoryQuery {
    window: Option<String>,
}

pub(super) async fn get_service_resource_usage_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ServiceResourceHistoryQuery>,
) -> Result<Json<ServiceResourceHistoryResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    if !settings.enabled {
        return Err(
            ApiError::conflict("resource monitor disabled").with_details(json!({
                "reason": "resource_monitor_disabled",
            })),
        );
    }

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if stack_id.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let window = q.window.unwrap_or_else(|| "1h".to_string());
    let Some(window_seconds) = resource_usage::parse_window_to_seconds(&window) else {
        return Err(ApiError::invalid_argument(
            "window must be one of 15m/1h/6h",
        ));
    };

    let since = (time::OffsetDateTime::now_utc() - time::Duration::seconds(window_seconds as i64))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| map_internal(err.into()))?;
    let samples = state
        .db
        .list_service_resource_samples_since(&service_id, &since)
        .await
        .map_err(map_internal)?;

    Ok(Json(ServiceResourceHistoryResponse {
        service_id,
        window,
        samples,
    }))
}

pub(super) async fn service_resource_usage_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    if !settings.enabled {
        return Err(
            ApiError::conflict("resource monitor disabled").with_details(json!({
                "reason": "resource_monitor_disabled",
            })),
        );
    }

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if stack_id.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let mut subscription = state.resource_hub.subscribe(&service_id).await;
    let (initial, initial_error) = match state.resource_hub.sample_once(&service_id).await {
        Ok(sample) => (sample, None),
        Err(err) => {
            tracing::warn!(
                service_id = %service_id,
                error = %err,
                "resource monitor initial snapshot failed"
            );
            (None, Some(err.to_string()))
        }
    };
    let stream_service_id = service_id.clone();

    let stream = async_stream::stream! {
        let mut event_id: u64 = 0;
        if let Some(error) = initial_error {
            event_id = event_id.saturating_add(1);
            let data = json!({
                "serviceId": stream_service_id.clone(),
                "error": error,
            });
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(event_id.to_string())
                    .event("resource_usage_error")
                    .data(data.to_string()),
            );
        } else if let Some(sample) = initial {
                event_id = event_id.saturating_add(1);
                let data = json!({
                    "serviceId": stream_service_id.clone(),
                    "sample": sample,
                });
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(event_id.to_string())
                        .event("resource_usage_snapshot")
                        .data(data.to_string()),
                );
        } else {
            event_id = event_id.saturating_add(1);
            let data = json!({
                "serviceId": stream_service_id.clone(),
                "error": "runtime_stats_unavailable",
            });
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(event_id.to_string())
                    .event("resource_usage_error")
                    .data(data.to_string()),
            );
        }

        loop {
            match subscription.receiver.recv().await {
                Ok(resource_usage::RealtimeMessage::Tick(sample)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "sample": sample,
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_tick")
                            .data(data.to_string()),
                    );
                }
                Ok(resource_usage::RealtimeMessage::Error(error)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "error": error,
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_error")
                            .data(data.to_string()),
                    );
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "error": "resource_usage_lagged",
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_error")
                            .data(data.to_string()),
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((resp_headers, sse))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListServiceDigestTagsQuery {
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetServiceDigestTagsSnapshotQuery {
    digest: Option<String>,
}

pub(super) async fn get_service_digest_tags_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<GetServiceDigestTagsSnapshotQuery>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    if digest_trimmed.is_empty() {
        return Err(ApiError::invalid_argument("digest is required"));
    }

    let digest = snapshot_worker::normalize_digest(digest_trimmed)
        .ok_or_else(|| ApiError::invalid_argument("digest is required"))?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };

    let known_digest = snapshot_target
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .is_some_and(|d| d.eq_ignore_ascii_case(&digest))
        || snapshot_target
            .candidate_digest
            .as_deref()
            .and_then(snapshot_worker::normalize_digest)
            .is_some_and(|d| d.eq_ignore_ascii_case(&digest));
    if !known_digest {
        return Err(ApiError::not_found("digest snapshot not found"));
    }

    let image_repo = snapshot_worker::image_repo_from_image_ref(&snapshot_target.image_ref)
        .ok_or_else(|| ApiError::invalid_argument("invalid service image ref"))?;
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, _checked_at, _updated_at)) = snapshot else {
        state
            .snapshot_worker
            .enqueue(
                &image_repo,
                &digest,
                &host_platform,
                "api_snapshot_read_miss",
            )
            .await;
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    };

    let parsed: ServiceDigestTagsSnapshotResponse =
        serde_json::from_str(&snapshot_json).map_err(|e| {
            ApiError::internal("invalid digest tags snapshot").with_details(json!({
                "error": e.to_string(),
            }))
        })?;

    Ok(Json(parsed).into_response())
}

pub(super) async fn list_service_digest_tags(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ListServiceDigestTagsQuery>,
) -> Result<Json<ServiceDigestTagsResponse>, ApiError> {
    use std::time::Duration;

    use tokio::{
        task::JoinSet,
        time::{Instant, timeout, timeout_at},
    };

    let _user = require_user(&state, &headers).await?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    // This endpoint is primarily used for UI observability. When digest is missing, we still want
    // to return the full `repo_tags` list so the UI can show something actionable (and avoid
    // "empty bubbles").
    let (digest, wanted) = if digest_trimmed.is_empty() {
        (String::new(), None)
    } else if digest_trimmed.contains(':') {
        (digest_trimmed.to_string(), Some(digest_trimmed.to_string()))
    } else {
        let normalized = format!("sha256:{digest_trimmed}");
        (normalized.clone(), Some(normalized))
    };

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stack_id) = stack_id else {
        return Err(ApiError::not_found("service not found"));
    };

    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };

    let svc = stack
        .services
        .iter()
        .find(|s| s.id == service_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("service not found"))?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let img = registry::ImageRef::parse(&svc.image.reference)
        .map_err(|_| ApiError::invalid_argument("invalid image ref (expected repo/name:tag)"))?;

    // Digest tag listing is used for UI debugging / observability, not as part of the "update
    // candidates" hot path. Still, we bound latency to avoid hanging requests forever.
    const LIST_TAGS_TIMEOUT: Duration = Duration::from_secs(8);
    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(6);
    const MANIFEST_BUDGET: Duration = Duration::from_secs(40);
    const MANIFEST_CONCURRENCY: usize = 10;

    let repo_tags = match timeout(LIST_TAGS_TIMEOUT, state.registry.list_tags(&img)).await {
        Ok(Ok(tags)) => tags,
        Ok(Err(e)) => return Err(map_internal(e)),
        Err(_) => {
            return Err(ApiError::internal("registry timeout").with_details(json!({
                "op": "list_tags"
            })));
        }
    };

    let repo_tags_total = repo_tags.len();
    let Some(wanted) = wanted else {
        return Ok(Json(ServiceDigestTagsResponse {
            digest,
            tags: Vec::new(),
            repo_tags,
            scan: ServiceDigestTagsScanSummary {
                repo_tags_total,
                repo_tags_considered: 0,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        }));
    };

    let registry = state.registry.clone();
    let img = img.clone();
    let host_platform = host_platform.clone();

    let mut out: Vec<String> = Vec::new();
    let mut manifests_ok: usize = 0;
    let mut manifests_timeout: usize = 0;
    let mut manifests_error: usize = 0;

    enum ScanOutcome {
        OkMatch(String),
        OkNoMatch,
        Timeout,
        Error,
    }

    let mut join_set: JoinSet<ScanOutcome> = JoinSet::new();
    let mut queue = repo_tags.iter().cloned();

    let spawn_one = |join_set: &mut JoinSet<ScanOutcome>,
                     tag: String,
                     registry: Arc<dyn registry::RegistryClient>,
                     img: registry::ImageRef,
                     host_platform: String,
                     wanted: String| {
        join_set.spawn(async move {
            match timeout(
                MANIFEST_TIMEOUT,
                registry.get_manifest(&img, &tag, &host_platform),
            )
            .await
            {
                Ok(Ok(m)) => {
                    let ok = m
                        .digest
                        .as_deref()
                        .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted))
                        || m.platform_digest
                            .as_deref()
                            .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted));
                    if ok {
                        ScanOutcome::OkMatch(tag)
                    } else {
                        ScanOutcome::OkNoMatch
                    }
                }
                Ok(Err(_)) => ScanOutcome::Error,
                Err(_) => ScanOutcome::Timeout,
            }
        });
    };

    for _ in 0..MANIFEST_CONCURRENCY {
        let Some(tag) = queue.next() else { break };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    let deadline = Instant::now() + MANIFEST_BUDGET;
    while !join_set.is_empty() {
        let next = match timeout_at(deadline, join_set.join_next()).await {
            Ok(next) => next,
            Err(_) => {
                // Degrade gracefully: keep best-effort matches and surface incompleteness via the
                // scan summary instead of failing the whole request.
                join_set.abort_all();
                break;
            }
        };

        let Some(joined) = next else { break };
        match joined {
            Ok(ScanOutcome::OkMatch(tag)) => {
                manifests_ok += 1;
                out.push(tag);
            }
            Ok(ScanOutcome::OkNoMatch) => {
                manifests_ok += 1;
            }
            Ok(ScanOutcome::Timeout) => {
                manifests_timeout += 1;
            }
            Ok(ScanOutcome::Error) => {
                manifests_error += 1;
            }
            Err(_) => {
                manifests_error += 1;
            }
        };

        let Some(tag) = queue.next() else {
            continue;
        };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    // If the budget was exhausted (or tasks were aborted), treat the remaining tags as timeouts so
    // the UI can warn that the result may be incomplete.
    let processed = manifests_ok + manifests_timeout + manifests_error;
    if processed < repo_tags_total {
        manifests_timeout += repo_tags_total - processed;
    }

    let mut semver_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut other_tags: Vec<String> = Vec::new();
    for tag in out {
        if let Some(v) = ignore::parse_version(&tag) {
            semver_tags.push((v, tag));
        } else {
            other_tags.push(tag);
        }
    }

    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    other_tags.sort_by(|a, b| b.cmp(a));

    let mut sorted: Vec<String> = Vec::new();
    for (_, tag) in semver_tags {
        sorted.push(tag);
    }
    for tag in other_tags {
        sorted.push(tag);
    }

    Ok(Json(ServiceDigestTagsResponse {
        digest,
        tags: sorted,
        repo_tags,
        scan: ServiceDigestTagsScanSummary {
            repo_tags_total,
            repo_tags_considered: repo_tags_total,
            manifests_ok,
            manifests_timeout,
            manifests_error,
        },
    }))
}

pub(super) async fn put_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<ServiceSettingsRequest>,
) -> Result<Json<PutServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let settings = ServiceSettings {
        auto_rollback: req.auto_rollback,
        backup_targets: req.backup_targets,
    };

    let updated = state
        .db
        .put_service_settings(&service_id, &settings, &now)
        .await
        .map_err(map_internal)?;

    if !updated {
        return Err(ApiError::not_found("service not found"));
    }

    Ok(Json(PutServiceSettingsResponse { ok: true }))
}
