use super::*;

pub(super) async fn create_web_push_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<WebPushSubscriptionRequest>,
) -> Result<Json<WebPushSubscriptionResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    state
        .db
        .upsert_web_push_subscription(&req.endpoint, &req.keys.p256dh, &req.keys.auth, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(WebPushSubscriptionResponse { ok: true }))
}

pub(super) async fn delete_web_push_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteWebPushSubscriptionRequest>,
) -> Result<Json<WebPushSubscriptionResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let deleted = state
        .db
        .delete_web_push_subscription(&req.endpoint)
        .await
        .map_err(map_internal)?;
    Ok(Json(WebPushSubscriptionResponse { ok: deleted }))
}

pub(super) async fn webhook_trigger(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<WebhookTriggerRequest>,
) -> Result<Json<WebhookTriggerResponse>, ApiError> {
    let secret = state.config.webhook_secret.as_deref().ok_or_else(|| {
        ApiError::unauthorized().with_details(json!({"reason":"webhook_secret_not_configured"}))
    })?;

    let provided = headers
        .get("X-Dockrev-Webhook-Secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if provided != secret {
        return Err(ApiError::unauthorized());
    }

    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    let WebhookTriggerRequest {
        action,
        scope,
        stack_id,
        service_id,
        targets,
        allow_arch_mismatch,
        backup_mode,
    } = req;

    match action {
        WebhookAction::Check => {
            let job_id = ids::new_job_id();
            let mut job = JobRecord::new_running(
                job_id.clone(),
                JobType::Check,
                scope.clone(),
                stack_id.clone(),
                service_id.clone(),
                &now,
            );
            job.allow_arch_mismatch = allow_arch_mismatch;
            job.backup_mode = backup_mode.as_str().to_string();

            let mut job_db = job.to_db();
            job_db.created_by = "webhook".to_string();
            job_db.reason = "webhook".to_string();
            state.db.insert_job(job_db).await.map_err(map_internal)?;

            let host_platform =
                registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());

            spawn_check_job_task(
                state.clone(),
                job_id.clone(),
                scope,
                stack_id,
                service_id,
                host_platform,
                now.clone(),
                "webhook".to_string(),
                "webhook check started".to_string(),
                "webhook check failed".to_string(),
                None,
            );

            Ok(Json(WebhookTriggerResponse { job_id }))
        }
        WebhookAction::Update => {
            if targets.is_none() {
                return Err(ApiError::invalid_argument(
                    "targets is required for webhook update",
                ));
            }

            let update_req = TriggerUpdateRequest {
                scope,
                stack_id,
                service_id,
                target_tag: None,
                target_digest: None,
                pull_tags: None,
                targets,
                mode: UpdateMode::Apply,
                allow_arch_mismatch,
                backup_mode,
                reason: UpdateReason::Webhook,
            };

            let job_id = enqueue_update_job(
                state,
                "webhook".to_string(),
                "webhook".to_string(),
                update_req,
                now,
            )
            .await?;
            Ok(Json(WebhookTriggerResponse { job_id }))
        }
    }
}
