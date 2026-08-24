use super::*;

use crate::{cleanup, cleanup_snapshot_worker, ids, models::JobRecord};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CleanupScanRunEventsQuery {
    #[serde(default)]
    after_id: u64,
}

pub(super) async fn start_cleanup_scan_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CleanupScanRequest>,
) -> Result<(StatusCode, Json<CleanupScanRunStartResponse>), ApiError> {
    let _user = require_user(&state, &headers).await?;
    validate_cleanup_scan_request(&req)?;
    if req.reason != CleanupScanReason::Page
        || req.scope != CleanupScope::All
        || req.preset != CleanupPreset::Aggressive
    {
        return Err(ApiError::invalid_argument(
            "cleanup scan runs only support reason=page, preset=aggressive, and scope=all",
        ));
    }

    let previous_snapshot = cleanup_page_snapshot_response(&state, &req).await?;
    let (scan_id, should_start_scan) = state.cleanup_scan_runs.create_or_join_active().await;
    state
        .cleanup_scan_runs
        .append(
            &scan_id,
            CleanupScanRunEvent {
                scan_id: scan_id.clone(),
                phase: CleanupScanRunPhase::Started,
                response: previous_snapshot.clone(),
                message: None,
            },
        )
        .await;

    if should_start_scan {
        let run_state = state.clone();
        let run_req = req.clone();
        let run_scan_id = scan_id.clone();
        tokio::spawn(async move {
            run_cleanup_scan_stream(run_state, run_req, run_scan_id).await;
        });
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(CleanupScanRunStartResponse {
            scan_id,
            previous_snapshot,
            retry_after_ms: cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS,
        }),
    ))
}

pub(super) async fn cleanup_scan_run_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(scan_id): Path<String>,
    Query(q): Query<CleanupScanRunEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let mut after_id = std::cmp::max(last_event_id, q.after_id);
    let stream_state = state.clone();
    let stream_scan_id = scan_id.clone();

    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
        loop {
            let Some((events, finished, notify)) = stream_state
                .cleanup_scan_runs
                .snapshot_after(&stream_scan_id, after_id)
                .await
            else {
                let payload = json!({
                    "scanId": stream_scan_id,
                    "phase": "scan_failed",
                    "message": "cleanup scan run not found",
                });
                yield Ok::<Event, Infallible>(Event::default().event("scan_failed").data(payload.to_string()));
                break;
            };

            if events.is_empty() {
                if finished {
                    break;
                }
                let notified = notify.notified();
                let Some((refreshed_events, refreshed_finished, _)) = stream_state
                    .cleanup_scan_runs
                    .snapshot_after(&stream_scan_id, after_id)
                    .await
                else {
                    let payload = json!({
                        "scanId": stream_scan_id,
                        "phase": "scan_failed",
                        "message": "cleanup scan run not found",
                    });
                    yield Ok::<Event, Infallible>(Event::default().event("scan_failed").data(payload.to_string()));
                    break;
                };
                if refreshed_finished && refreshed_events.is_empty() {
                    break;
                }
                if !refreshed_events.is_empty() {
                    for event in refreshed_events {
                        after_id = event.id;
                        yield Ok::<Event, Infallible>(cleanup_scan_run_sse_event(event, &stream_scan_id));
                    }
                    continue;
                }
                notified.await;
                continue;
            }

            for event in events {
                after_id = event.id;
                yield Ok::<Event, Infallible>(cleanup_scan_run_sse_event(event, &stream_scan_id));
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

fn cleanup_scan_run_sse_event(
    event: crate::cleanup_scan_runs::CleanupScanRunStoredEvent,
    scan_id: &str,
) -> Event {
    let event_id = event.id.to_string();
    let event_name = event.event_name;
    let payload = match serde_json::to_string(&event.payload) {
        Ok(value) => value,
        Err(err) => json!({
            "scanId": scan_id,
            "phase": "scan_failed",
            "message": err.to_string(),
        })
        .to_string(),
    };
    Event::default()
        .id(event_id)
        .event(event_name)
        .data(payload)
}

pub(super) async fn scan_cleanups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CleanupScanRequest>,
) -> Result<Json<CleanupScanResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    validate_cleanup_scan_request(&req)?;
    let now = time::OffsetDateTime::now_utc();
    let snapshot_row = state
        .db
        .get_cleanup_inventory_snapshot(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY)
        .await
        .map_err(map_internal)?;
    let is_running = state.cleanup_snapshot_worker.is_running();

    match req.reason {
        CleanupScanReason::Page => {
            if let Some(row) = snapshot_row {
                let snapshot = serde_json::from_str::<CleanupInventorySnapshot>(&row.snapshot_json)
                    .map_err(|err| map_internal(err.into()))?;
                let is_fresh =
                    cleanup_snapshot_worker::cleanup_snapshot_is_fresh(&row.checked_at, now);
                let mut refreshing = is_running;
                let last_error = state.cleanup_snapshot_worker.last_error().await;
                if (!is_fresh || req.refresh) && !refreshing {
                    if !req.refresh
                        && !is_fresh
                        && let Some(last_error) = last_error.clone()
                    {
                        return Err(ApiError::internal(format!(
                            "cleanup snapshot refresh failed: {last_error}"
                        )));
                    }
                    refreshing = state.cleanup_snapshot_worker.enqueue().await
                        || state.cleanup_snapshot_worker.is_running();
                    if !refreshing
                        && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
                    {
                        return Err(ApiError::internal(format!(
                            "cleanup snapshot refresh failed: {last_error}"
                        )));
                    }
                }
                let plan = cleanup::build_execution_plan_from_snapshot(
                    &snapshot,
                    &req,
                    &snapshot.scanned_at,
                )
                .map_err(map_internal)?;
                let mut response = plan.to_response(req.reason);
                response.refreshing = refreshing;
                response.retry_after_ms = refreshing
                    .then_some(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS);
                return Ok(Json(response));
            }

            if req.refresh {
                let started = state.cleanup_snapshot_worker.enqueue().await;
                let refreshing = started || state.cleanup_snapshot_worker.is_running();
                if !refreshing
                    && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
                {
                    return Err(ApiError::internal(format!(
                        "cleanup snapshot refresh failed: {last_error}"
                    )));
                }
            } else if let Some(last_error) = state.cleanup_snapshot_worker.last_error().await {
                return Err(ApiError::internal(format!(
                    "cleanup snapshot refresh failed: {last_error}"
                )));
            }
            Ok(Json(CleanupScanResponse {
                status: CleanupScanStatus::Pending,
                reason: req.reason,
                preset: req.preset,
                scope: req.scope,
                scanned_at: None,
                refreshing: true,
                retry_after_ms: Some(
                    cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS,
                ),
                estimated_reclaimable_bytes: None,
                has_unknown_size: false,
                server_disk_usage: None,
                stack_groups: Vec::new(),
                unowned_group: None,
                confirmation_fingerprint: None,
            }))
        }
        CleanupScanReason::Confirm => {
            if let Some(row) = snapshot_row {
                let snapshot = serde_json::from_str::<CleanupInventorySnapshot>(&row.snapshot_json)
                    .map_err(|err| map_internal(err.into()))?;
                let is_fresh =
                    cleanup_snapshot_worker::cleanup_snapshot_is_fresh(&row.checked_at, now);
                if is_fresh && !is_running {
                    let plan = cleanup::build_execution_plan_from_snapshot(
                        &snapshot,
                        &req,
                        &snapshot.scanned_at,
                    )
                    .map_err(map_internal)?;
                    return Ok(Json(plan.to_response(req.reason)));
                }
                if req.refresh {
                    let _ = state.cleanup_snapshot_worker.enqueue().await;
                }
                if !state.cleanup_snapshot_worker.is_running()
                    && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
                {
                    return Err(ApiError::internal(format!(
                        "cleanup snapshot refresh failed: {last_error}"
                    )));
                }
            } else if req.refresh {
                let _ = state.cleanup_snapshot_worker.enqueue().await;
                if !state.cleanup_snapshot_worker.is_running()
                    && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
                {
                    return Err(ApiError::internal(format!(
                        "cleanup snapshot refresh failed: {last_error}"
                    )));
                }
            } else if let Some(last_error) = state.cleanup_snapshot_worker.last_error().await {
                return Err(ApiError::internal(format!(
                    "cleanup snapshot refresh failed: {last_error}"
                )));
            }

            Ok(Json(CleanupScanResponse {
                status: CleanupScanStatus::Pending,
                reason: req.reason,
                preset: req.preset,
                scope: req.scope,
                scanned_at: None,
                refreshing: true,
                retry_after_ms: Some(
                    cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS,
                ),
                estimated_reclaimable_bytes: None,
                has_unknown_size: false,
                server_disk_usage: None,
                stack_groups: Vec::new(),
                unowned_group: None,
                confirmation_fingerprint: None,
            }))
        }
    }
}

async fn cleanup_page_snapshot_response(
    state: &Arc<AppState>,
    req: &CleanupScanRequest,
) -> Result<Option<CleanupScanResponse>, ApiError> {
    let Some(row) = state
        .db
        .get_cleanup_inventory_snapshot(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY)
        .await
        .map_err(map_internal)?
    else {
        return Ok(None);
    };
    let snapshot = serde_json::from_str::<CleanupInventorySnapshot>(&row.snapshot_json)
        .map_err(|err| map_internal(err.into()))?;
    let plan = cleanup::build_execution_plan_from_snapshot(&snapshot, req, &snapshot.scanned_at)
        .map_err(map_internal)?;
    let mut response = plan.to_response(CleanupScanReason::Page);
    response.refreshing = true;
    response.retry_after_ms =
        Some(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS);
    Ok(Some(response))
}

async fn run_cleanup_scan_stream(state: Arc<AppState>, req: CleanupScanRequest, scan_id: String) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<CleanupInventorySnapshot>();
    let partial_state = state.clone();
    let partial_req = req.clone();
    let partial_scan_id = scan_id.clone();
    let partial_forwarder = tokio::spawn(async move {
        while let Some(snapshot) = rx.recv().await {
            let Ok(plan) = cleanup::build_execution_plan_from_snapshot(
                &snapshot,
                &partial_req,
                &snapshot.scanned_at,
            ) else {
                continue;
            };
            let mut response = plan.to_response(CleanupScanReason::Page);
            response.status = CleanupScanStatus::Pending;
            response.refreshing = true;
            response.retry_after_ms =
                Some(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS);
            response.confirmation_fingerprint = None;
            partial_state
                .cleanup_scan_runs
                .append_to_active(
                    &partial_scan_id,
                    CleanupScanRunPhase::Partial,
                    Some(response),
                    None,
                )
                .await;
        }
    });

    let result = cleanup::build_inventory_snapshot_with_progress(
        state.db.clone(),
        state.runner.clone(),
        move |snapshot| {
            let _ = tx.send(snapshot);
        },
    )
    .await;

    let _ = partial_forwarder.await;

    match result {
        Ok(snapshot) => {
            let snapshot_json = match serde_json::to_string(&snapshot) {
                Ok(value) => value,
                Err(err) => {
                    finish_cleanup_scan_failed(&state, &scan_id, err.to_string()).await;
                    return;
                }
            };
            let checked_at = snapshot.scanned_at.clone();
            let updated_at = time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
            if let Err(err) = state
                .db
                .upsert_cleanup_inventory_snapshot(
                    cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY,
                    &snapshot_json,
                    &checked_at,
                    &updated_at,
                )
                .await
            {
                finish_cleanup_scan_failed(&state, &scan_id, err.to_string()).await;
                return;
            }
            let response = match cleanup::build_execution_plan_from_snapshot(
                &snapshot,
                &req,
                &snapshot.scanned_at,
            ) {
                Ok(plan) => plan.to_response(CleanupScanReason::Page),
                Err(err) => {
                    finish_cleanup_scan_failed(&state, &scan_id, err.to_string()).await;
                    return;
                }
            };
            state
                .cleanup_scan_runs
                .append_to_active(&scan_id, CleanupScanRunPhase::Ready, Some(response), None)
                .await;
            state
                .management_events
                .publish_immediate(
                    "cleanup",
                    vec![
                        crate::management_events::ManagementEventEntity {
                            entity_type: "scan".to_string(),
                            id: scan_id.clone(),
                        },
                        crate::management_events::ManagementEventEntity {
                            entity_type: "scan".to_string(),
                            id: "active".to_string(),
                        },
                    ],
                    json!({ "scanId": scan_id, "phase": "ready" }),
                )
                .await;
        }
        Err(err) => {
            finish_cleanup_scan_failed(&state, &scan_id, err.to_string()).await;
        }
    }
}

async fn finish_cleanup_scan_failed(state: &Arc<AppState>, scan_id: &str, message: String) {
    state
        .cleanup_scan_runs
        .append_to_active(
            scan_id,
            CleanupScanRunPhase::Failed,
            None,
            Some(message.clone()),
        )
        .await;
    state
        .management_events
        .publish_immediate(
            "cleanup",
            vec![crate::management_events::ManagementEventEntity {
                entity_type: "scan".to_string(),
                id: "active".to_string(),
            }],
            json!({ "scanId": scan_id, "phase": "failed", "message": message }),
        )
        .await;
}

pub(super) async fn apply_cleanups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CleanupApplyRequest>,
) -> Result<Json<CleanupApplyResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    validate_cleanup_apply_request(&req)?;
    let scan_req = CleanupScanRequest {
        reason: CleanupScanReason::Confirm,
        preset: req.preset.clone(),
        refresh: false,
        scope: req.scope.clone(),
        stack_id: req.stack_id.clone(),
        service_id: req.service_id.clone(),
    };
    let Some(snapshot_row) = state
        .db
        .get_cleanup_inventory_snapshot(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_KEY)
        .await
        .map_err(map_internal)?
    else {
        let _ = state.cleanup_snapshot_worker.enqueue().await;
        if !state.cleanup_snapshot_worker.is_running()
            && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
        {
            return Err(ApiError::internal(format!(
                "cleanup snapshot refresh failed: {last_error}"
            )));
        }
        return Err(ApiError::cleanup_snapshot_stale(cleanup_pending_response(
            &scan_req,
        )));
    };
    let snapshot = serde_json::from_str::<CleanupInventorySnapshot>(&snapshot_row.snapshot_json)
        .map_err(|err| map_internal(err.into()))?;
    let now = time::OffsetDateTime::now_utc();
    let is_fresh =
        cleanup_snapshot_worker::cleanup_snapshot_is_fresh(&snapshot_row.checked_at, now);
    let is_running = state.cleanup_snapshot_worker.is_running();
    if !is_fresh || is_running {
        if !is_running {
            let _ = state.cleanup_snapshot_worker.enqueue().await;
            if !state.cleanup_snapshot_worker.is_running()
                && let Some(last_error) = state.cleanup_snapshot_worker.last_error().await
            {
                return Err(ApiError::internal(format!(
                    "cleanup snapshot refresh failed: {last_error}"
                )));
            }
        }
        return Err(ApiError::cleanup_snapshot_stale(cleanup_pending_response(
            &scan_req,
        )));
    }
    let plan =
        cleanup::build_execution_plan_from_snapshot(&snapshot, &scan_req, &snapshot.scanned_at)
            .map_err(map_internal)?;
    let submitted_fingerprint = req.confirmation_fingerprint.trim();
    if plan.confirmation_fingerprint() != submitted_fingerprint {
        tracing::warn!(
            principal = %user.principal,
            request_reason = %req.reason.as_str(),
            preset = %req.preset.as_str(),
            scope = %req.scope.as_str(),
            stack_id = req.stack_id.as_deref().unwrap_or(""),
            service_id = req.service_id.as_deref().unwrap_or(""),
            submitted_fingerprint = %submitted_fingerprint,
            latest_fingerprint = %plan.confirmation_fingerprint(),
            target_count = plan.target_count(),
            estimated_reclaimable_bytes = plan.estimated_reclaimable_bytes(),
            has_unknown_size = plan.has_unknown_size(),
            "cleanup apply rejected because confirmation snapshot is stale"
        );
        return Err(ApiError::cleanup_snapshot_stale(
            plan.to_response(CleanupScanReason::Confirm),
        ));
    }

    let now = now_rfc3339().map_err(map_internal)?;
    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::CleanupApply,
        cleanup_scope_to_job_scope(&req.scope),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );
    job.summary_json = plan.initial_job_summary();

    let mut job_db = job.to_db();
    job_db.created_by = user.principal;
    job_db.reason = req.reason.as_str().to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;
    state
        .db
        .insert_job_log(
            &job_id,
            &JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: "cleanup started".to_string(),
            },
        )
        .await
        .map_err(map_internal)?;

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        let _ = cleanup::run_cleanup_job(run_state, &run_job_id, plan).await;
    });

    Ok(Json(CleanupApplyResponse { job_id }))
}

fn cleanup_pending_response(req: &CleanupScanRequest) -> CleanupScanResponse {
    CleanupScanResponse {
        status: CleanupScanStatus::Pending,
        reason: req.reason.clone(),
        preset: req.preset.clone(),
        scope: req.scope.clone(),
        scanned_at: None,
        refreshing: true,
        retry_after_ms: Some(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS),
        estimated_reclaimable_bytes: None,
        has_unknown_size: false,
        server_disk_usage: None,
        stack_groups: Vec::new(),
        unowned_group: None,
        confirmation_fingerprint: None,
    }
}

fn validate_cleanup_scan_request(req: &CleanupScanRequest) -> Result<(), ApiError> {
    match req.reason {
        CleanupScanReason::Page => {
            if req.scope != CleanupScope::All {
                return Err(ApiError::invalid_argument(
                    "scope must be all for reason=page",
                ));
            }
            if req.stack_id.is_some() || req.service_id.is_some() {
                return Err(ApiError::invalid_argument(
                    "stackId/serviceId is not supported for reason=page",
                ));
            }
        }
        CleanupScanReason::Confirm => match req.scope {
            CleanupScope::All => {
                if req.stack_id.is_some() || req.service_id.is_some() {
                    return Err(ApiError::invalid_argument(
                        "stackId/serviceId is not supported for scope=all",
                    ));
                }
            }
            CleanupScope::Stack => {
                if req
                    .stack_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err(ApiError::invalid_argument(
                        "stackId is required for scope=stack",
                    ));
                }
                if req.service_id.is_some() {
                    return Err(ApiError::invalid_argument(
                        "serviceId is not supported for scope=stack",
                    ));
                }
            }
            CleanupScope::Service => {
                if req
                    .stack_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err(ApiError::invalid_argument(
                        "stackId is required for scope=service",
                    ));
                }
                if req
                    .service_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err(ApiError::invalid_argument(
                        "serviceId is required for scope=service",
                    ));
                }
            }
        },
    }
    Ok(())
}

fn validate_cleanup_apply_request(req: &CleanupApplyRequest) -> Result<(), ApiError> {
    if req.reason != CleanupApplyReason::Ui {
        return Err(ApiError::invalid_argument("reason must be ui"));
    }
    if req.confirmation_fingerprint.trim().is_empty() {
        return Err(ApiError::invalid_argument(
            "confirmationFingerprint is required",
        ));
    }
    match req.scope {
        CleanupScope::All => {
            if req.stack_id.is_some() || req.service_id.is_some() {
                return Err(ApiError::invalid_argument(
                    "stackId/serviceId is not supported for scope=all",
                ));
            }
        }
        CleanupScope::Stack => {
            if req
                .stack_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(ApiError::invalid_argument(
                    "stackId is required for scope=stack",
                ));
            }
            if req.service_id.is_some() {
                return Err(ApiError::invalid_argument(
                    "serviceId is not supported for scope=stack",
                ));
            }
        }
        CleanupScope::Service => {
            if req
                .stack_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(ApiError::invalid_argument(
                    "stackId is required for scope=service",
                ));
            }
            if req
                .service_id
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(ApiError::invalid_argument(
                    "serviceId is required for scope=service",
                ));
            }
        }
    }
    Ok(())
}

fn cleanup_scope_to_job_scope(scope: &CleanupScope) -> JobScope {
    match scope {
        CleanupScope::Service => JobScope::Service,
        CleanupScope::Stack => JobScope::Stack,
        CleanupScope::All => JobScope::All,
    }
}
