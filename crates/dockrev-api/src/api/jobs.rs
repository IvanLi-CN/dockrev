use super::*;

pub(super) async fn list_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ListJobsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let jobs = state.db.list_jobs().await.map_err(map_internal)?;
    Ok(Json(ListJobsResponse {
        jobs: jobs.into_iter().map(|j| j.into_api()).collect(),
    }))
}

pub(super) async fn get_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Result<Json<GetJobResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let job = state.db.get_job(&job_id).await.map_err(map_internal)?;
    let Some(job) = job else {
        return Err(ApiError::not_found("job not found"));
    };

    let logs = state
        .db
        .list_job_logs(&job_id)
        .await
        .map_err(map_internal)?;

    let logs_last_id = state
        .db
        .get_job_logs_last_id(&job_id)
        .await
        .map_err(map_internal)?;
    let progress = job
        .summary_json
        .as_object()
        .and_then(|o| o.get("progress"))
        .cloned()
        .and_then(|v| serde_json::from_value::<JobProgress>(v).ok());

    Ok(Json(GetJobResponse {
        job: JobDetail {
            id: job.id,
            r#type: job.r#type.as_str().to_string(),
            scope: job.scope.as_str().to_string(),
            stack_id: job.stack_id,
            service_id: job.service_id,
            status: job.status,
            created_by: job.created_by,
            reason: job.reason,
            created_at: job.created_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            allow_arch_mismatch: job.allow_arch_mismatch,
            backup_mode: job.backup_mode,
            summary: job.summary_json,
            progress,
            logs,
            logs_last_id,
        },
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct JobEventsQuery {
    #[serde(default)]
    after_id: i64,
}

pub(super) fn resolve_sse_after_id(headers: &HeaderMap, query_after_id: i64) -> i64 {
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let header_after_id: i64 = last_event_id.parse::<i64>().unwrap_or(0);
    std::cmp::max(header_after_id, query_after_id).max(0)
}

pub(super) async fn jobs_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<JobEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let mut after_id = resolve_sse_after_id(&headers, q.after_id);

    // Default to tail-following so the queue page subscribes to future updates without replay storms.
    if after_id <= 0 {
        after_id = state
            .db
            .get_job_logs_global_last_id()
            .await
            .map_err(map_internal)?;
    }

    let sse_state = state.clone();
    let stream = async_stream::stream! {
        loop {
            let rows = match sse_state.db.list_job_event_logs_since(after_id, 200).await {
                Ok(v) => v,
                Err(e) => {
                    let evt = json!({
                        "type": "job_events_error",
                        "error": e.to_string(),
                    });
                    yield Ok::<Event, Infallible>(Event::default().event("job_events_error").data(evt.to_string()));
                    break;
                }
            };

            if rows.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            for row in rows {
                after_id = row.id;
                let payload = match serde_json::from_str::<serde_json::Value>(&row.msg) {
                    Ok(mut parsed) => {
                        if let Some(obj) = parsed.as_object_mut() {
                            obj.entry("jobId".to_string())
                                .or_insert_with(|| json!(row.job_id.clone()));
                            obj.entry("ts".to_string())
                                .or_insert_with(|| json!(row.ts.clone()));
                            parsed
                        } else {
                            json!({
                                "type": "job_event",
                                "jobId": row.job_id,
                                "ts": row.ts,
                                "raw": row.msg,
                            })
                        }
                    }
                    Err(_) => json!({
                        "type": "job_event",
                        "jobId": row.job_id,
                        "ts": row.ts,
                        "raw": row.msg,
                    }),
                };

                let ev = Event::default()
                    .id(row.id.to_string())
                    .event("job_event")
                    .data(payload.to_string());
                yield Ok::<Event, Infallible>(ev);
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

pub(super) async fn job_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Query(q): Query<JobEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;

    // Fail fast on invalid job ids to avoid leaving open SSE connections forever.
    let job = state.db.get_job(&job_id).await.map_err(map_internal)?;
    if job.is_none() {
        return Err(ApiError::not_found("job not found"));
    }

    let mut after_id = resolve_sse_after_id(&headers, q.after_id);

    let sse_state = state.clone();
    let sse_job_id = job_id.clone();
    let stream = async_stream::stream! {
        // If the job is already finished and no new logs arrive for a while, close the stream.
        let mut finished_idle_ticks: u32 = 0;

        loop {
            let rows = match sse_state
                .db
                .list_job_logs_since(&sse_job_id, after_id, 200)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    let evt = json!({
                        "type": "job_events_error",
                        "jobId": sse_job_id,
                        "error": e.to_string(),
                    });
                    yield Ok::<Event, Infallible>(Event::default().event("job_events_error").data(evt.to_string()));
                    break;
                }
            };

            if rows.is_empty() {
                let finished = sse_state
                    .db
                    .get_job(&sse_job_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|j| j.finished_at)
                    .is_some();

                if finished {
                    finished_idle_ticks += 1;
                    if finished_idle_ticks >= 20 {
                        break;
                    }
                }

                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            finished_idle_ticks = 0;

            for row in rows {
                after_id = row.id;
                if row.level != "event" {
                    let evt = json!({
                        "type": "job_log",
                        "jobId": sse_job_id,
                        "ts": row.ts,
                        "level": row.level,
                        "msg": row.msg,
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(row.id.to_string())
                            .event("job_log")
                            .data(evt.to_string()),
                    );
                    continue;
                }

                let event_name = serde_json::from_str::<serde_json::Value>(&row.msg)
                    .ok()
                    .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "event".to_string());

                let ev = Event::default()
                    .id(row.id.to_string())
                    .event(event_name.clone())
                    .data(row.msg);
                let should_close = event_name.as_str() == "runtime_scan_finished";
                yield Ok::<Event, Infallible>(ev);

                if should_close {
                    break;
                }
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
