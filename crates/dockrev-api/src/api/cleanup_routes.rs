use super::*;

use crate::{cleanup, cleanup_snapshot_worker, ids, models::JobRecord};

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
                let plan = cleanup::build_execution_plan_from_snapshot(
                    &snapshot,
                    &req,
                    &snapshot.scanned_at,
                )
                .map_err(map_internal)?;
                let mut response = plan.to_response(req.reason);
                let should_refresh = req.refresh || !is_fresh || is_running;
                response.refreshing = should_refresh;
                response.retry_after_ms = should_refresh
                    .then_some(cleanup_snapshot_worker::CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS);
                if req.refresh {
                    let _ = state.cleanup_snapshot_worker.enqueue().await;
                }
                return Ok(Json(response));
            }

            if req.refresh {
                let _ = state.cleanup_snapshot_worker.enqueue().await;
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
            } else if req.refresh {
                let _ = state.cleanup_snapshot_worker.enqueue().await;
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
