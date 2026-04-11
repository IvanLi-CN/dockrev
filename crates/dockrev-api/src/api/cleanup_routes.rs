use super::*;

use crate::{cleanup, ids, models::JobRecord};

pub(super) async fn scan_cleanups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CleanupScanRequest>,
) -> Result<Json<CleanupScanResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    validate_cleanup_scan_request(&req)?;
    let scanned_at = now_rfc3339().map_err(map_internal)?;
    let plan = cleanup::build_execution_plan(state.as_ref(), &req, &scanned_at)
        .await
        .map_err(map_internal)?;
    Ok(Json(plan.to_response(req.reason)))
}

pub(super) async fn apply_cleanups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CleanupApplyRequest>,
) -> Result<Json<CleanupApplyResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    validate_cleanup_apply_request(&req)?;
    let scanned_at = now_rfc3339().map_err(map_internal)?;
    let scan_req = CleanupScanRequest {
        reason: CleanupScanReason::Confirm,
        preset: req.preset.clone(),
        scope: req.scope.clone(),
        stack_id: req.stack_id.clone(),
        service_id: req.service_id.clone(),
    };
    let plan = cleanup::build_execution_plan(state.as_ref(), &scan_req, &scanned_at)
        .await
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
