use super::*;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NotificationInboxQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

pub(super) async fn get_notification_inbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<NotificationInboxQuery>,
) -> Result<Json<NotificationInboxResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let page = state
        .db
        .list_notification_items(query.limit.unwrap_or(50), query.cursor.as_deref(), &now)
        .await
        .map_err(|error| {
            if error.to_string().contains("invalid notification cursor") {
                ApiError::invalid_argument("invalid notification cursor")
            } else {
                map_internal(error)
            }
        })?;
    Ok(Json(notification_inbox_response(page)))
}

pub(super) async fn get_notification_unread_count(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<NotificationUnreadCountResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let unread_count = state
        .db
        .count_notification_items(&now)
        .await
        .map_err(map_internal)?;
    Ok(Json(NotificationUnreadCountResponse { unread_count }))
}

pub(super) async fn mark_notification_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(notification_id): Path<String>,
) -> Result<Json<NotificationReadResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let result = state
        .db
        .mark_notification_item_read(&notification_id, &now)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("notification item not found"))?;
    Ok(Json(NotificationReadResponse {
        notification_id: result.notification_id,
        read_at: result.read_at,
        unread_count: result.unread_count,
    }))
}

pub(super) async fn mark_all_notifications_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<NotificationReadAllResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let result = state
        .db
        .mark_all_notification_items_read(&now)
        .await
        .map_err(map_internal)?;
    Ok(Json(NotificationReadAllResponse {
        read_at: result.read_at,
        unread_count: result.unread_count,
    }))
}

fn notification_inbox_response(
    page: crate::db::NotificationInboxPage,
) -> NotificationInboxResponse {
    NotificationInboxResponse {
        items: page.items.into_iter().map(notification_item).collect(),
        next_cursor: page.next_cursor,
        unread_count: page.unread_count,
    }
}

fn notification_item(item: crate::db::NotificationItemRow) -> NotificationItem {
    NotificationItem {
        id: item.id,
        kind: item.kind,
        title: item.title,
        body: item.body,
        url: item.target_url,
        source_job_id: item.source_job_id,
        created_at: item.created_at,
        read_at: item.read_at,
    }
}

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
