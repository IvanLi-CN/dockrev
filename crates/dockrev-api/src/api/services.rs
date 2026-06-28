use super::*;
use crate::updater::is_dockrev_image_ref;

mod github_releases;
mod repo_links;

pub(super) use repo_links::get_service_new_version_discovery_timeline;
use repo_links::normalize_repo_url_input;
#[allow(unused_imports)]
pub(crate) use repo_links::{
    RepoLinkInferenceContext, RepoLinkInferenceOutcomeKind, RepoLinkInferenceResult,
    build_repo_link_inference_context, infer_service_repo_link_for_snapshot_target,
};

#[allow(unused_imports)]
pub(crate) use github_releases::resolve_service_github_repo_ref;
#[cfg(test)]
use github_releases::{
    classify_github_releases_failure, github_release_tag_variants,
    list_service_github_releases_with_client, locate_service_github_release_with_client,
};
pub(super) use github_releases::{list_service_github_releases, locate_service_github_release};

pub(super) async fn get_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stored) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    Ok(Json(ServiceSettingsResponse {
        auto_rollback: stored.settings.auto_rollback,
        backup_targets: stored.settings.backup_targets,
        repo_url: stored.settings.repo_url,
        auto_update_policy: stored.auto_update_policy,
    }))
}

pub(super) async fn infer_service_repo_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceRepoLinkInferenceResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };
    let context = build_repo_link_inference_context(&state)
        .await
        .map_err(map_internal)?;
    Ok(Json(
        infer_service_repo_link_for_snapshot_target(&state, &snapshot_target, &context)
            .await
            .into_response(),
    ))
}

pub(super) async fn list_service_tag_suggestions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceTagSuggestionsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let stack = state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let service = stack
        .services
        .iter()
        .find(|svc| svc.id == service_id)
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let image_repo = service_tag_history_repo_key(&service.image.reference)
        .ok_or_else(|| ApiError::invalid_argument("service image is not tag-based"))?;
    let items = state
        .db
        .list_service_tag_suggestions(&service_id, &image_repo, 20)
        .await
        .map_err(map_internal)?
        .into_iter()
        .map(|item| ServiceTagSuggestionItem {
            tag: item.tag,
            last_used_at: item.last_used_at,
            source: item.source,
            use_count: item.use_count,
        })
        .collect();
    Ok(Json(ServiceTagSuggestionsResponse { items }))
}

pub(super) async fn put_service_compose_tag(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<PutServiceComposeTagRequest>,
) -> Result<Json<PutServiceComposeTagResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let tag = crate::compose::validate_docker_tag(&req.tag)
        .map_err(|e| ApiError::invalid_argument(e.to_string()))?;
    let now = now_rfc3339().map_err(map_internal)?;
    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let stack = state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let service = stack
        .services
        .iter()
        .find(|svc| svc.id == service_id)
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let image_repo = service_tag_history_repo_key(&service.image.reference)
        .ok_or_else(|| ApiError::invalid_argument("service image is not tag-based"))?;

    let compose_file = resolve_compose_file_for_service_image(&stack, &service.name)
        .await
        .map_err(|e| ApiError::invalid_argument(e.to_string()))?;
    let patch = crate::compose::patch_service_image_tag_in_file(
        std::path::Path::new(&compose_file),
        &service.name,
        &tag,
    )
    .map_err(|e| ApiError::invalid_argument(format!("{e:#}")))?;

    let service_specs = read_compose_service_specs(&stack.compose.compose_files)
        .await
        .map_err(|e| ApiError::invalid_argument(e.to_string()))?;
    state
        .db
        .sync_stack_from_compose(
            &stack.id,
            &stack.compose.compose_files,
            &service_specs,
            &now,
        )
        .await
        .map_err(map_internal)?;
    state
        .db
        .upsert_service_tag_history(&service_id, &image_repo, &tag, "manual", &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(PutServiceComposeTagResponse {
        ok: true,
        tag,
        image_ref: patch.image_ref,
        compose_file,
        updated_at: now,
    }))
}

async fn resolve_compose_file_for_service_image(
    stack: &crate::api::types::StackRecord,
    service_name: &str,
) -> anyhow::Result<String> {
    let mut target: Option<String> = None;
    for path in &stack.compose.compose_files {
        let contents = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("read compose file {path}"))?;
        let services = crate::compose::parse_services(&contents)
            .with_context(|| format!("parse compose file {path}"))?;
        if services
            .iter()
            .any(|svc| svc.name == service_name && !svc.image_ref.trim().is_empty())
        {
            target = Some(path.clone());
        }
    }
    target.ok_or_else(|| anyhow::anyhow!("service image definition not found in compose files"))
}

fn service_tag_history_repo_key(image_ref: &str) -> Option<String> {
    let image_repo = crate::compose::image_repo_from_tagged_ref(image_ref)?;
    crate::snapshot_worker::image_repo_from_image_ref(&format!("{image_repo}:latest"))
}

async fn read_compose_service_specs(
    compose_files: &[String],
) -> anyhow::Result<Vec<crate::db::ComposeServiceSpec>> {
    let mut merged = std::collections::BTreeMap::new();
    for path in compose_files {
        let contents = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("read compose file {path}"))?;
        let parsed = crate::compose::parse_services(&contents)
            .with_context(|| format!("parse compose file {path}"))?;
        merged = crate::compose::merge_services(merged, parsed);
    }
    Ok(merged
        .into_values()
        .map(|svc| crate::db::ComposeServiceSpec {
            name: svc.name,
            image_ref: svc.image_ref,
            image_tag: svc.image_tag,
            homepage: svc.homepage,
            update_guard: svc.update_guard,
        })
        .collect())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TriggerVersionInferenceRefreshRequest {
    digest: Option<String>,
}

pub(super) async fn trigger_service_version_inference_refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let body: TriggerVersionInferenceRefreshRequest = if body.is_empty() {
        TriggerVersionInferenceRefreshRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::invalid_argument("invalid json"))?
    };
    let digest_input = body.digest.unwrap_or_default();
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
    let inserted = state
        .snapshot_worker
        .enqueue(
            &image_repo,
            &digest,
            &host_platform,
            VERSION_INFERENCE_REASON_FORCE,
        )
        .await;
    let reason = if inserted {
        VERSION_INFERENCE_REASON_FORCE
    } else {
        VERSION_INFERENCE_REASON_RUNNING
    };
    let resp = TriggerVersionInferenceRefreshResponse {
        status: "pending".to_string(),
        service_id,
        image_repo,
        digest,
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
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
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
    let sse = Sse::new(stream).keep_alive(edge_proxy_safe_keepalive());

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

pub(super) async fn get_service_resource_usage_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ServiceResourceHistoryQuery>,
) -> Result<Json<ServiceResourceOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;

    let window = q.window.unwrap_or_else(|| "1h".to_string());
    let Some(window_seconds) = resource_usage::parse_window_to_seconds(&window) else {
        return Err(ApiError::invalid_argument(
            "window must be one of 15m/1h/6h",
        ));
    };
    let generated_at = time::OffsetDateTime::now_utc();
    let generated_at_label = generated_at
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| map_internal(err.into()))?;
    let stale_after_seconds =
        resource_usage::normalize_sample_interval_seconds(settings.sample_interval_seconds)
            .saturating_mul(2)
            .max(60);

    if !settings.enabled {
        return Ok(Json(ServiceResourceOverviewResponse {
            enabled: false,
            window,
            generated_at: generated_at_label,
            stale_after_seconds,
            services: Vec::new(),
        }));
    }

    let since = (generated_at - time::Duration::seconds(window_seconds as i64))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| map_internal(err.into()))?;
    let rows = state
        .db
        .list_service_resource_latest_samples()
        .await
        .map_err(map_internal)?;
    let recent_counts = state
        .db
        .list_service_resource_recent_counts_since(&since)
        .await
        .map_err(map_internal)?;
    let recent_count_by_service = recent_counts
        .into_iter()
        .map(|row| (row.service_id, row.sample_count))
        .collect::<std::collections::HashMap<_, _>>();
    let services = rows
        .into_iter()
        .map(|row| {
            let recent_sample_count = recent_count_by_service
                .get(&row.service_id)
                .copied()
                .unwrap_or(0);
            to_resource_overview_item_from_latest(
                row,
                generated_at,
                stale_after_seconds,
                Some(recent_sample_count),
            )
        })
        .collect();

    Ok(Json(ServiceResourceOverviewResponse {
        enabled: true,
        window,
        generated_at: generated_at_label,
        stale_after_seconds,
        services,
    }))
}

pub(super) async fn get_homepage_nav(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<HomepageNavResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    let generated_at = time::OffsetDateTime::now_utc();
    let generated_at_label = generated_at
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| map_internal(err.into()))?;
    let stale_after_seconds =
        resource_usage::normalize_sample_interval_seconds(settings.sample_interval_seconds)
            .saturating_mul(2)
            .max(60);
    let latest_samples = state
        .db
        .list_service_resource_latest_samples()
        .await
        .map_err(map_internal)?;
    let mut overview_services = latest_samples
        .iter()
        .cloned()
        .map(|row| {
            to_resource_overview_item_from_latest(row, generated_at, stale_after_seconds, None)
        })
        .collect::<Vec<_>>();
    overview_services.sort_by(|left, right| left.service_id.cmp(&right.service_id));

    let mut rows = state
        .db
        .list_homepage_nav_services()
        .await
        .map_err(map_internal)?;
    let last_check_at = rows
        .iter()
        .map(|row| row.stack_last_check_at.as_str())
        .max()
        .map(ToString::to_string);
    let metrics_by_service = overview_services
        .iter()
        .cloned()
        .map(|item| (item.service_id.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();

    let mut services = rows
        .iter_mut()
        .map(|row| row.service.clone())
        .collect::<Vec<_>>();
    crate::api::stacks::enrich_services_with_version_inference(&state, &mut services).await?;
    crate::api::stacks::enrich_services_with_new_version_discovery_counts(&state, &mut services)
        .await?;
    for (index, service) in services.into_iter().enumerate() {
        rows[index].service = service;
    }

    let mut items = rows
        .into_iter()
        .filter_map(|row| {
            let homepage = row.service.homepage.clone()?;
            let href = homepage
                .href
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(HomepageNavItem {
                stack_id: row.stack_id,
                stack_name: row.stack_name,
                service_id: row.service.id.clone(),
                service_name: row.service.name.clone(),
                image_ref: row.service.image.reference.clone(),
                image_tag: row.service.image.tag.clone(),
                image_digest: row.service.image.digest.clone(),
                image_resolved_tag: row.service.image.resolved_tag.clone(),
                image_resolved_tags: row.service.image.resolved_tags.clone(),
                is_dockrev: is_dockrev_image_ref(
                    &row.service.image.reference,
                    Some(state.config.dockrev_image_repo.as_str()),
                ),
                homepage: ServiceHomepage {
                    href: Some(href.to_string()),
                    ..homepage
                },
                candidate: row.service.candidate.clone(),
                ignore: row.service.ignore.clone(),
                version_inference: row.service.version_inference.clone(),
                new_version_discovery_count: row.service.new_version_discovery_count,
                settings: row.service.settings.clone(),
                archived: row.service.archived,
                resource: metrics_by_service.get(&row.service.id).cloned().unwrap_or(
                    ServiceResourceOverviewItem {
                        service_id: row.service.id,
                        sampled_at: None,
                        cpu_percent: None,
                        mem_used_bytes: None,
                        mem_limit_bytes: None,
                        net_rx_rate_bps: None,
                        net_tx_rate_bps: None,
                        stale: true,
                        sample_count: 0,
                    },
                ),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.stack_name
            .cmp(&right.stack_name)
            .then_with(|| left.service_name.cmp(&right.service_name))
    });

    Ok(Json(HomepageNavResponse {
        generated_at: generated_at_label,
        last_check_at,
        resource_summary: ServiceResourceOverviewResponse {
            enabled: settings.enabled,
            window: "1h".to_string(),
            generated_at: generated_at
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|err| map_internal(err.into()))?,
            stale_after_seconds,
            services: if settings.enabled {
                overview_services
            } else {
                Vec::new()
            },
        },
        items,
    }))
}

fn to_resource_overview_item_from_latest(
    row: crate::db::ServiceResourceLatestSampleRow,
    generated_at: time::OffsetDateTime,
    stale_after_seconds: u64,
    sample_count_override: Option<u32>,
) -> ServiceResourceOverviewItem {
    let has_sample = row.sampled_at.is_some();
    let has_prev_sample = row.prev_sampled_at.is_some();
    let stale = row
        .sampled_at
        .as_deref()
        .and_then(|sampled_at| {
            time::OffsetDateTime::parse(sampled_at, &time::format_description::well_known::Rfc3339)
                .ok()
        })
        .is_none_or(|sampled_at| {
            (generated_at - sampled_at).whole_seconds() > stale_after_seconds as i64
        });
    let (net_rx_rate_bps, net_tx_rate_bps) = compute_resource_rates_from_latest(
        &row.prev_sampled_at,
        row.prev_net_rx_bytes,
        row.prev_net_tx_bytes,
        &row.sampled_at,
        row.net_rx_bytes,
        row.net_tx_bytes,
    );
    ServiceResourceOverviewItem {
        service_id: row.service_id,
        sampled_at: row.sampled_at,
        cpu_percent: row.cpu_percent,
        mem_used_bytes: row.mem_used_bytes,
        mem_limit_bytes: row.mem_limit_bytes,
        net_rx_rate_bps,
        net_tx_rate_bps,
        stale,
        sample_count: sample_count_override
            .unwrap_or(u32::from(has_sample) + u32::from(has_prev_sample)),
    }
}

fn compute_resource_rates_from_latest(
    prev_sampled_at: &Option<String>,
    prev_net_rx_bytes: Option<u64>,
    prev_net_tx_bytes: Option<u64>,
    next_sampled_at: &Option<String>,
    next_net_rx_bytes: Option<u64>,
    next_net_tx_bytes: Option<u64>,
) -> (Option<f64>, Option<f64>) {
    let prev_ts = prev_sampled_at.as_deref().and_then(|value| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
    });
    let next_ts = next_sampled_at.as_deref().and_then(|value| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
    });
    let Some((prev_ts, next_ts)) = prev_ts.zip(next_ts) else {
        return (None, None);
    };
    let seconds = (next_ts - prev_ts).as_seconds_f64();
    if seconds <= 0.0 {
        return (None, None);
    }
    (
        compute_counter_rate(prev_net_rx_bytes, next_net_rx_bytes, seconds),
        compute_counter_rate(prev_net_tx_bytes, next_net_tx_bytes, seconds),
    )
}

fn compute_counter_rate(prev: Option<u64>, next: Option<u64>, seconds: f64) -> Option<f64> {
    let (Some(prev), Some(next)) = (prev, next) else {
        return None;
    };
    if next < prev {
        return None;
    }
    Some((next - prev) as f64 / seconds)
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
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
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
            match subscription.recv().await {
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

    let sse = Sse::new(stream).keep_alive(edge_proxy_safe_keepalive());

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
    digest_tags_snapshot_response(&state, &service_id, q.digest.as_deref()).await
}

pub(super) async fn list_service_digest_tags(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ListServiceDigestTagsQuery>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;
    if q.digest
        .as_deref()
        .is_none_or(|digest| digest.trim().is_empty())
    {
        return list_service_digest_repo_tags(&state, &service_id).await;
    }
    digest_tags_snapshot_response(&state, &service_id, q.digest.as_deref()).await
}

async fn list_service_digest_repo_tags(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<Response, ApiError> {
    let snapshot_target = state
        .db
        .get_service_snapshot_target(service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };

    let image = registry::ImageRef::parse(&snapshot_target.image_ref).map_err(|_| {
        ApiError::invalid_argument("invalid image ref (expected repo/name[:tag][@sha256:digest])")
    })?;
    let repo_tags = state
        .registry
        .list_tags(&image)
        .await
        .map_err(map_internal)?;
    Ok(Json(ServiceDigestTagsResponse {
        digest: snapshot_target.current_digest.unwrap_or_default(),
        tags: Vec::new(),
        repo_tags: repo_tags.clone(),
        scan: ServiceDigestTagsScanSummary {
            repo_tags_total: repo_tags.len(),
            repo_tags_considered: 0,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    })
    .into_response())
}

async fn digest_tags_snapshot_response(
    state: &Arc<AppState>,
    service_id: &str,
    digest_input: Option<&str>,
) -> Result<Response, ApiError> {
    let digest_trimmed = digest_input.unwrap_or_default().trim();
    if digest_trimmed.is_empty() {
        return Err(ApiError::invalid_argument("digest is required"));
    }

    let digest = snapshot_worker::normalize_digest(digest_trimmed)
        .ok_or_else(|| ApiError::invalid_argument("digest is required"))?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(service_id)
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

    let in_flight_reason = state
        .snapshot_worker
        .in_flight_reason(&image_repo, &digest, &host_platform)
        .await;

    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, _checked_at, _updated_at)) = snapshot else {
        if in_flight_reason.is_none() {
            state
                .snapshot_worker
                .enqueue(
                    &image_repo,
                    &digest,
                    &host_platform,
                    "api_snapshot_read_miss",
                )
                .await;
        }
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    };

    if in_flight_reason.as_deref() == Some(VERSION_INFERENCE_REASON_FORCE) {
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    }

    let parsed: ServiceDigestTagsSnapshotResponse =
        serde_json::from_str(&snapshot_json).map_err(|e| {
            ApiError::internal("invalid digest tags snapshot").with_details(json!({
                "error": e.to_string(),
            }))
        })?;

    Ok(Json(parsed).into_response())
}

pub(super) async fn put_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<ServiceSettingsRequest>,
) -> Result<Json<PutServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let current_settings = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let (repo_url, repo_url_auto_disabled) = match req.repo_url {
        Some(repo_url) => {
            let repo_url = normalize_repo_url_input(repo_url.as_deref())?;
            let repo_url_auto_disabled = repo_url.is_none();
            (repo_url, repo_url_auto_disabled)
        }
        None => (
            current_settings.settings.repo_url.clone(),
            current_settings.repo_url_auto_disabled,
        ),
    };

    let settings = ServiceSettings {
        auto_rollback: req.auto_rollback,
        backup_targets: req.backup_targets,
        repo_url,
    };
    let auto_update_policy = req
        .auto_update_policy
        .clone()
        .unwrap_or(current_settings.auto_update_policy.clone());
    crate::auto_update::validate_policy_for_scope(&auto_update_policy, "service")?;

    let updated = state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &settings,
            repo_url_auto_disabled,
            &now,
        )
        .await
        .map_err(map_internal)?;

    if !updated {
        return Err(ApiError::not_found("service not found"));
    }
    state
        .db
        .put_auto_update_policy("service", &service_id, &auto_update_policy, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(PutServiceSettingsResponse { ok: true }))
}

#[cfg(test)]
mod tests;
