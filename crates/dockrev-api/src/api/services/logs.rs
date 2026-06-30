use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetServiceLogsSnapshotQuery {
    #[serde(default = "default_logs_tail")]
    tail: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceLogsEventsQuery {
    #[serde(default)]
    after_id: i64,
}

fn default_logs_tail() -> usize {
    crate::service_logs::DEFAULT_SERVICE_LOG_TAIL
}

pub(crate) async fn get_service_logs_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<GetServiceLogsSnapshotQuery>,
) -> Result<Json<ServiceLogSnapshotResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let service_exists = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if service_exists.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let snapshot = state
        .service_log_hub
        .snapshot(&service_id, q.tail)
        .await
        .map_err(map_internal)?;
    let snapshot = snapshot.unwrap_or(ServiceLogSnapshotResponse {
        service_id,
        lines: Vec::new(),
        last_event_id: 0,
        buffer_limit: crate::service_logs::SERVICE_LOG_RING_BUFFER_LIMIT,
    });
    Ok(Json(snapshot))
}

pub(crate) async fn service_logs_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ServiceLogsEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let service_exists = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if service_exists.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let after_id = resolve_sse_after_id(&headers, q.after_id).max(0) as u64;
    let mut subscription = state.service_log_hub.subscribe(&service_id).await;
    let replay = state
        .service_log_hub
        .events_since(&service_id, after_id)
        .await
        .map_err(map_internal)?
        .unwrap_or_default();
    let stream_service_id = service_id.clone();

    let stream = async_stream::stream! {
        let mut last_sent_id = after_id;
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));

        if replay.reset_required {
            let payload = serde_json::to_string(&ServiceLogEventEnvelope::Reset {
                id: after_id.saturating_add(1),
                service_id: stream_service_id.clone(),
                reason: "buffer_gap_reset".to_string(),
            })
            .unwrap_or_else(|_| "{\"type\":\"service_log_reset\"}".to_string());
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(after_id.saturating_add(1).to_string())
                    .event("service_log_reset")
                    .data(payload),
            );
            last_sent_id = after_id.saturating_add(1);
        }

        for event in replay.events {
            let (event_name, event_id) = match &event {
                ServiceLogEventEnvelope::Line { id, .. } => ("service_log_line", *id),
                ServiceLogEventEnvelope::Reset { id, .. } => ("service_log_reset", *id),
            };
            last_sent_id = last_sent_id.max(event_id);
            let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(event_id.to_string())
                    .event(event_name)
                    .data(payload),
            );
        }

        loop {
            match subscription.recv().await {
                Ok(crate::service_logs::ServiceLogRealtimeMessage::Event(event)) => {
                    let (event_name, event_id) = match &event {
                        ServiceLogEventEnvelope::Line { id, .. } => ("service_log_line", *id),
                        ServiceLogEventEnvelope::Reset { id, .. } => ("service_log_reset", *id),
                    };
                    if event_id <= last_sent_id {
                        continue;
                    }
                    last_sent_id = event_id;
                    let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event(event_name)
                            .data(payload),
                    );
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let payload = serde_json::to_string(&ServiceLogEventEnvelope::Reset {
                        id: 0,
                        service_id: stream_service_id.clone(),
                        reason: "subscriber_lagged".to_string(),
                    })
                    .unwrap_or_else(|_| "{\"type\":\"service_log_reset\"}".to_string());
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .event("service_log_reset")
                            .data(payload),
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
