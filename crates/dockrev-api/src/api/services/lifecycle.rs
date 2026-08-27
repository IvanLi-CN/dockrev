use super::*;
use crate::db::ServiceLifecycleEventRow;
use serde::Deserialize;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceLifecycleQuery {
    pub since: Option<String>,
    pub until: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceLifecycleEventsQuery {
    pub after_id: Option<i64>,
}

pub(crate) async fn get_service_lifecycle_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceLifecycleQuery>,
) -> Result<Json<ServiceLifecycleSnapshotResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    ensure_service(&state, &service_id).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let retention_since =
        (time::OffsetDateTime::parse(&now, &time::format_description::well_known::Rfc3339)
            .map_err(|error| map_internal(error.into()))?
            - time::Duration::days(30))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|error| map_internal(error.into()))?;
    let since = query
        .since
        .filter(|value| value >= &retention_since)
        .unwrap_or_else(|| retention_since.clone());
    let until = query
        .until
        .filter(|value| value <= &now)
        .unwrap_or_else(|| now.clone());
    let projection = load_lifecycle_projection(&state, &service_id, &since, &until).await?;
    Ok(Json(ServiceLifecycleSnapshotResponse {
        service_id,
        since,
        until,
        next_cursor: projection.next_cursor,
        last_event_id: projection.last_event_id,
        availability_intervals: projection.availability_intervals,
        events: projection.events,
        retention_since,
    }))
}

pub(crate) async fn load_lifecycle_projection(
    state: &Arc<AppState>,
    service_id: &str,
    since: &str,
    until: &str,
) -> Result<ServiceLifecycleProjection, ApiError> {
    let rows = state
        .db
        .list_service_lifecycle_events_with_predecessor(service_id, since, until)
        .await
        .map_err(map_internal)?;
    let events = rows
        .iter()
        .filter(|row| row.observed_at.as_str() >= since && row.observed_at.as_str() <= until)
        .map(event_from_row)
        .collect::<Vec<_>>();
    let retention_since =
        time::OffsetDateTime::parse(until, &time::format_description::well_known::Rfc3339)
            .ok()
            .and_then(|value| {
                (value - time::Duration::days(30))
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .unwrap_or_else(|| since.to_string());
    Ok(ServiceLifecycleProjection {
        next_cursor: events.last().map(|event| event.id),
        last_event_id: events.last().map(|event| event.id),
        availability_intervals: derive_intervals(&rows),
        events,
        retention_since,
    })
}

pub(crate) async fn service_lifecycle_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceLifecycleEventsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    ensure_service(&state, &service_id).await?;
    let after_id = resolve_sse_after_id(&headers, query.after_id.unwrap_or(0)).max(0);
    let stream_service_id = service_id.clone();
    let stream = async_stream::stream! {
        let mut cursor = after_id;
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
        if let Ok((Some(first), Some(last))) = state.db.service_lifecycle_event_bounds(&stream_service_id).await
            && cursor > 0 && cursor < first.saturating_sub(1)
        {
            let envelope = ServiceLifecycleEventEnvelope::Reset { reason: "cursor_pruned".to_string(), cursor: last };
            if let Ok(data) = serde_json::to_string(&envelope) {
                yield Ok::<Event, Infallible>(Event::default().id(last.to_string()).event("lifecycle_event_reset").data(data));
            }
            cursor = last;
        }
        loop {
            match state.db.list_service_lifecycle_events_after(&stream_service_id, cursor).await {
                Ok(events) => {
                    for row in events {
                        cursor = cursor.max(row.id);
                        let envelope = ServiceLifecycleEventEnvelope::Event { event: event_from_row(&row) };
                        if let Ok(data) = serde_json::to_string(&envelope) {
                            yield Ok::<Event, Infallible>(Event::default().id(row.id.to_string()).event("lifecycle_event").data(data));
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, service_id = %stream_service_id, "lifecycle SSE database poll failed");
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok((
        response_headers,
        Sse::new(stream).keep_alive(edge_proxy_safe_keepalive()),
    ))
}

async fn ensure_service(state: &Arc<AppState>, service_id: &str) -> Result<(), ApiError> {
    if state
        .db
        .get_service_stack_id(service_id)
        .await
        .map_err(map_internal)?
        .is_none()
    {
        return Err(ApiError::not_found("service not found"));
    }
    Ok(())
}

fn event_from_row(row: &ServiceLifecycleEventRow) -> ServiceLifecycleEvent {
    ServiceLifecycleEvent {
        id: row.id,
        service_id: row.service_id.clone(),
        stack_id: row.stack_id.clone(),
        operation_group_id: row.operation_group_id.clone(),
        job_id: row.job_id.clone(),
        origin: row.origin.clone(),
        transition: row.transition.clone(),
        observed_at: row.observed_at.clone(),
        boundary_precision: row.boundary_precision.clone(),
        evidence: serde_json::from_str(&row.evidence_json).unwrap_or(Value::Null),
        details: serde_json::from_str(&row.details_json).unwrap_or(Value::Null),
        created_at: row.created_at.clone(),
    }
}

fn derive_intervals(rows: &[ServiceLifecycleEventRow]) -> Vec<LifecycleAvailabilityInterval> {
    let mut intervals = Vec::new();
    let mut stopped: Option<&ServiceLifecycleEventRow> = None;
    for row in rows {
        match row.transition.as_str() {
            "stopped" => stopped = Some(row),
            "started" => {
                if let Some(stop) = stopped.take()
                    && stop.boundary_precision == "exact"
                    && row.boundary_precision == "exact"
                {
                    intervals.push(LifecycleAvailabilityInterval {
                        operation_group_id: row.operation_group_id.clone(),
                        started_at: row.observed_at.clone(),
                        stopped_at: stop.observed_at.clone(),
                        start_event_id: row.id,
                        stop_event_id: stop.id,
                        complete: true,
                    });
                }
            }
            _ => {}
        }
    }
    intervals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, transition: &str, precision: &str, ts: &str) -> ServiceLifecycleEventRow {
        ServiceLifecycleEventRow {
            id,
            service_id: "svc".to_string(),
            stack_id: Some("stack".to_string()),
            operation_group_id: "op".to_string(),
            job_id: None,
            origin: "compose".to_string(),
            transition: transition.to_string(),
            observed_at: ts.to_string(),
            boundary_precision: precision.to_string(),
            evidence_json: "{}".to_string(),
            details_json: "{}".to_string(),
            created_at: ts.to_string(),
        }
    }

    #[test]
    fn only_exact_boundaries_form_availability_intervals() {
        let rows = vec![
            row(1, "stopped", "exact", "2026-08-01T00:00:00Z"),
            row(2, "started", "exact", "2026-08-01T00:05:00Z"),
            row(3, "stopped", "incomplete", "2026-08-01T01:00:00Z"),
            row(4, "started", "exact", "2026-08-01T01:05:00Z"),
        ];
        let intervals = derive_intervals(&rows);
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].start_event_id, 2);
        assert_eq!(intervals[0].stop_event_id, 1);
    }
}
