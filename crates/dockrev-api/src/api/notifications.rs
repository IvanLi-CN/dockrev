use super::*;

pub(super) async fn get_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<NotificationConfig>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_notification_settings()
        .await
        .map_err(map_internal)?;
    Ok(Json(NotificationConfig::from_db(settings)))
}

pub(super) async fn put_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<NotificationConfig>,
) -> Result<Json<PutNotificationsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let existing = state
        .db
        .get_notification_settings()
        .await
        .map_err(map_internal)?;
    let keep_existing_events = req.events.is_none();
    let existing_event_update_enabled = existing.event_update_enabled;
    let existing_event_new_version_enabled = existing.event_new_version_enabled;
    let existing_event_ghcr_webhook_anomaly_enabled = existing.event_ghcr_webhook_anomaly_enabled;
    let mut merged = req.into_db();

    merge_secret(&mut merged.email_smtp_url, existing.email_smtp_url);
    merge_secret(&mut merged.webhook_url, existing.webhook_url);
    merge_secret(&mut merged.telegram_bot_token, existing.telegram_bot_token);
    merge_telegram_chat_id(&mut merged.telegram_chat_id, existing.telegram_chat_id);
    merge_secret(
        &mut merged.webpush_vapid_private_key,
        existing.webpush_vapid_private_key,
    );
    if keep_existing_events {
        merged.event_update_enabled = existing_event_update_enabled;
        merged.event_new_version_enabled = existing_event_new_version_enabled;
        merged.event_ghcr_webhook_anomaly_enabled = existing_event_ghcr_webhook_anomaly_enabled;
    }

    state
        .db
        .put_notification_settings(&merged, &now)
        .await
        .map_err(map_internal)?;
    state
        .management_events
        .publish_change(
            "settings",
            "notifications",
            "default",
            serde_json::json!({ "operation": "notifications_updated" }),
        )
        .await;
    Ok(Json(PutNotificationsResponse { ok: true }))
}

pub(super) async fn test_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TestNotificationsRequest>,
) -> Result<Json<TestNotificationsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let message = req.message.unwrap_or_else(|| "dockrev test".to_string());
    let results = notify::send_test(state.as_ref(), &now, &message, req.channel)
        .await
        .map_err(map_internal)?;
    Ok(Json(TestNotificationsResponse { ok: true, results }))
}
