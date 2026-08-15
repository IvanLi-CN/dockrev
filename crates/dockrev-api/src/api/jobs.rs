use super::*;

const SLOW_JOBS_LIST_WARN_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(250);
const SLOW_JOBS_LIST_WARN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
static SLOW_JOBS_LIST_WARNED_AT: std::sync::OnceLock<std::sync::Mutex<Option<std::time::Instant>>> =
    std::sync::OnceLock::new();

pub(super) async fn list_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<ListJobsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::invalid_argument(
            "limit must be between 1 and 200",
        ));
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_jobs_cursor)
        .transpose()?;
    let types = query
        .r#type
        .as_deref()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let started_at = std::time::Instant::now();
    let page = state
        .db
        .list_jobs_page(crate::db::JobListFilters {
            types,
            status: query.status.clone().filter(|value| !value.is_empty()),
            stack_id: query.stack_id.clone().filter(|value| !value.is_empty()),
            service_id: query.service_id.clone().filter(|value| !value.is_empty()),
            cursor,
            limit,
        })
        .await
        .map_err(map_internal)?;
    let elapsed = started_at.elapsed();
    if should_emit_slow_jobs_list_warning(elapsed, std::time::Instant::now()) {
        tracing::warn!(
            returned = page.jobs.len(),
            limit,
            has_next = page.next_cursor.is_some(),
            has_type_filter = query.r#type.is_some(),
            has_status_filter = query.status.is_some(),
            has_stack_id_filter = query.stack_id.is_some(),
            has_service_id_filter = query.service_id.is_some(),
            duration_ms = elapsed.as_millis() as u64,
            "slow jobs list query"
        );
    }
    Ok(Json(ListJobsResponse {
        jobs: page.jobs.into_iter().map(|j| j.into_api()).collect(),
        next_cursor: page.next_cursor.map(encode_jobs_cursor),
    }))
}

fn should_emit_slow_jobs_list_warning(
    elapsed: std::time::Duration,
    now: std::time::Instant,
) -> bool {
    let warned_at = SLOW_JOBS_LIST_WARNED_AT.get_or_init(|| std::sync::Mutex::new(None));
    should_emit_slow_jobs_list_warning_with_state(warned_at, elapsed, now)
}

fn should_emit_slow_jobs_list_warning_with_state(
    warned_at: &std::sync::Mutex<Option<std::time::Instant>>,
    elapsed: std::time::Duration,
    now: std::time::Instant,
) -> bool {
    if elapsed < SLOW_JOBS_LIST_WARN_THRESHOLD {
        return false;
    }

    let mut warned_at = warned_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if warned_at.is_some_and(|last_warned_at| {
        now.saturating_duration_since(last_warned_at) < SLOW_JOBS_LIST_WARN_INTERVAL
    }) {
        return false;
    }

    *warned_at = Some(now);
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListJobsQuery {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    stack_id: Option<String>,
    #[serde(default)]
    service_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobsCursor {
    created_at: String,
    id: String,
}

fn encode_jobs_cursor(cursor: (String, String)) -> String {
    let value = JobsCursor {
        created_at: cursor.0,
        id: cursor.1,
    };
    let bytes = serde_json::to_vec(&value).expect("jobs cursor serializes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_jobs_cursor(cursor: &str) -> Result<(String, String), ApiError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| ApiError::invalid_jobs_cursor())?;
    let value: JobsCursor =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid_jobs_cursor())?;
    if value.created_at.is_empty() || value.id.is_empty() {
        return Err(ApiError::invalid_jobs_cursor());
    }
    Ok((value.created_at, value.id))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn slow_jobs_list_warning_is_thresholded_and_rate_limited() {
        let now = Instant::now();
        let warned_at = std::sync::Mutex::new(None);

        assert!(!should_emit_slow_jobs_list_warning_with_state(
            &warned_at,
            SLOW_JOBS_LIST_WARN_THRESHOLD.saturating_sub(std::time::Duration::from_millis(1)),
            now,
        ));
        assert!(should_emit_slow_jobs_list_warning_with_state(
            &warned_at,
            SLOW_JOBS_LIST_WARN_THRESHOLD,
            now,
        ));
        assert!(!should_emit_slow_jobs_list_warning_with_state(
            &warned_at,
            SLOW_JOBS_LIST_WARN_THRESHOLD,
            now + std::time::Duration::from_secs(59),
        ));
        assert!(should_emit_slow_jobs_list_warning_with_state(
            &warned_at,
            SLOW_JOBS_LIST_WARN_THRESHOLD,
            now + SLOW_JOBS_LIST_WARN_INTERVAL,
        ));
    }
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
    let job_type = job.r#type.as_str().to_string();
    let result_reason = crate::api::types::result_reason_from_summary(
        &job_type,
        &job.status,
        &job.summary_json,
        progress.as_ref(),
    );

    Ok(Json(GetJobResponse {
        job: JobDetail {
            id: job.id,
            r#type: job_type,
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
            result_reason,
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

#[derive(Default)]
struct LiveCommandState {
    command_seq: u64,
    complete: bool,
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
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
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

    let sse = Sse::new(stream).keep_alive(edge_proxy_safe_keepalive());

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
    let live_job = job.as_ref().is_some_and(|job| {
        matches!(job.status.as_str(), "running" | "queued")
            && matches!(
                job.r#type.as_str(),
                "update" | "rollback" | "service_lifecycle" | "stack_lifecycle"
            )
    });
    // Capture the durable tail before subscribing. Rows at or below this id are
    // reconnect/history replay and must not consume transient live markers.
    let live_start_after_id = if live_job {
        state
            .db
            .get_job_logs_last_id(&job_id)
            .await
            .map_err(map_internal)?
    } else {
        0
    };
    let mut live_subscription = if live_job {
        Some(state.job_live_log_hub.subscribe(&job_id).await)
    } else {
        None
    };

    let sse_state = state.clone();
    let sse_job_id = job_id.clone();
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().comment("keep-alive"));
        // If the job is already finished and no new logs arrive for a while, close the stream.
        let mut finished_idle_ticks: u32 = 0;
        let mut live_commands = std::collections::VecDeque::<LiveCommandState>::new();

        loop {
            // Prefer transient output over durable backlog. This keeps the live stream flowing
            // while a reconnect or a busy job is also producing database rows.
            if let Some(live_subscription) = live_subscription.as_mut() {
                match live_subscription.try_recv() {
                    Ok(crate::job_live_logs::JobLiveEvent::Terminal(terminal)) => {
                        if live_commands
                            .back()
                            .is_none_or(|command| command.command_seq != terminal.command_seq)
                        {
                            live_commands.push_back(LiveCommandState {
                                command_seq: terminal.command_seq,
                                complete: false,
                            });
                        }
                        let evt = json!({
                            "type": "job_live_terminal",
                            "jobId": sse_job_id,
                            "ts": terminal.ts,
                            "commandSeq": terminal.command_seq,
                            "lines": terminal.lines,
                        });
                        yield Ok::<Event, Infallible>(
                            Event::default()
                                .event("job_live_terminal")
                                .data(evt.to_string()),
                        );
                        continue;
                    }
                    Ok(crate::job_live_logs::JobLiveEvent::CommandComplete(done)) => {
                        if let Some(command) = live_commands.back_mut()
                            && command.command_seq == done.command_seq
                            && !command.complete
                        {
                            if done.summary_persisted {
                                command.complete = true;
                            } else {
                                live_commands.pop_back();
                            }
                        }
                        let evt = json!({
                            "type": "job_live_command_complete",
                            "jobId": sse_job_id,
                            "commandSeq": done.command_seq,
                            "hadOutput": done.had_output,
                            "summaryPersisted": done.summary_persisted,
                        });
                        yield Ok::<Event, Infallible>(
                            Event::default()
                                .event("job_live_command_complete")
                                .data(evt.to_string()),
                        );
                        continue;
                    }
                    Ok(crate::job_live_logs::JobLiveEvent::Progress(progress)) => {
                        let evt = json!({
                            "type": "job_progress",
                            "jobId": sse_job_id,
                            "progress": progress,
                        });
                        yield Ok::<Event, Infallible>(
                            Event::default().event("job_progress").data(evt.to_string()),
                        );
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                        live_commands.clear();
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                    | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {}
                }
            }

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
                        sse_state.job_live_log_hub.close(&sse_job_id);
                        break;
                    }
                }

                if let Some(live_subscription) = live_subscription.as_mut() {
                    tokio::select! {
                        live = live_subscription.recv() => {
                            match live {
                                Ok(crate::job_live_logs::JobLiveEvent::Terminal(terminal)) => {
                                    if live_commands
                                        .back()
                                        .is_none_or(|command| command.command_seq != terminal.command_seq)
                                    {
                                        live_commands.push_back(LiveCommandState {
                                            command_seq: terminal.command_seq,
                                            complete: false,
                                        });
                                    }
                                    let evt = json!({
                                        "type": "job_live_terminal",
                                        "jobId": sse_job_id,
                                        "ts": terminal.ts,
                                        "commandSeq": terminal.command_seq,
                                        "lines": terminal.lines,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default()
                                            .event("job_live_terminal")
                                            .data(evt.to_string()),
                                    );
                                }
                                Ok(crate::job_live_logs::JobLiveEvent::CommandComplete(done)) => {
                                    if let Some(command) = live_commands.back_mut()
                                        && command.command_seq == done.command_seq
                                        && !command.complete
                                    {
                                        if done.summary_persisted {
                                            command.complete = true;
                                        } else {
                                            live_commands.pop_back();
                                        }
                                    }
                                    let evt = json!({
                                        "type": "job_live_command_complete",
                                        "jobId": sse_job_id,
                                        "commandSeq": done.command_seq,
                                        "hadOutput": done.had_output,
                                        "summaryPersisted": done.summary_persisted,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default()
                                            .event("job_live_command_complete")
                                            .data(evt.to_string()),
                                    );
                                }
                                Ok(crate::job_live_logs::JobLiveEvent::Progress(progress)) => {
                                    let evt = json!({
                                        "type": "job_progress",
                                        "jobId": sse_job_id,
                                        "progress": progress,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default().event("job_progress").data(evt.to_string()),
                                    );
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    // Raw lines are intentionally not replayable. Drop any
                                    // partially paired command so the next durable summary is
                                    // never suppressed using stale live output.
                                    live_commands.clear();
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    tokio::time::sleep(Duration::from_millis(250)).await;
                                }
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                continue;
            }

            finished_idle_ticks = 0;

            for row in rows {
                after_id = row.id;
                if row.level != "event" {
                    // A command publishes its completion marker after the persisted summary
                    // succeeds. Pair the next database summary with the next in-memory command
                    // marker before sending it, preserving event order for back-to-back commands.
                    // Reconnected streams have an empty queue and therefore restore history
                    // immediately without waiting for a marker that cannot be replayed.
                    if row.msg.starts_with("status=")
                        && row.id > live_start_after_id
                        && live_commands
                            .front()
                            .is_none_or(|command| !command.complete)
                        && let Some(live_subscription) = live_subscription.as_mut()
                    {
                        loop {
                            let live = match live_subscription.try_recv() {
                                Ok(live) => live,
                                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                                    live_commands.clear();
                                    break;
                                }
                                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                                    if live_commands.is_empty() {
                                        break;
                                    }
                                    match live_subscription.recv().await {
                                        Ok(live) => live,
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                            live_commands.clear();
                                            break;
                                        }
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                    }
                                }
                                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                            };
                            let command_complete = matches!(
                                &live,
                                crate::job_live_logs::JobLiveEvent::CommandComplete(_)
                            );
                            match live {
                                crate::job_live_logs::JobLiveEvent::Terminal(terminal) => {
                                    if live_commands
                                        .back()
                                        .is_none_or(|command| command.command_seq != terminal.command_seq)
                                    {
                                        live_commands.push_back(LiveCommandState {
                                            command_seq: terminal.command_seq,
                                            complete: false,
                                        });
                                    }
                                    let evt = json!({
                                        "type": "job_live_terminal",
                                        "jobId": sse_job_id,
                                        "ts": terminal.ts,
                                        "commandSeq": terminal.command_seq,
                                        "lines": terminal.lines,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default()
                                            .event("job_live_terminal")
                                            .data(evt.to_string()),
                                    );
                                }
                                crate::job_live_logs::JobLiveEvent::CommandComplete(done) => {
                                    if let Some(command) = live_commands.back_mut()
                                        && command.command_seq == done.command_seq
                                        && !command.complete
                                    {
                                        if done.summary_persisted {
                                            command.complete = true;
                                        } else {
                                            live_commands.pop_back();
                                        }
                                    }
                                    let evt = json!({
                                        "type": "job_live_command_complete",
                                        "jobId": sse_job_id,
                                        "commandSeq": done.command_seq,
                                        "hadOutput": done.had_output,
                                        "summaryPersisted": done.summary_persisted,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default()
                                            .event("job_live_command_complete")
                                            .data(evt.to_string()),
                                    );
                                }
                                crate::job_live_logs::JobLiveEvent::Progress(progress) => {
                                    let evt = json!({
                                        "type": "job_progress",
                                        "jobId": sse_job_id,
                                        "progress": progress,
                                    });
                                    yield Ok::<Event, Infallible>(
                                        Event::default().event("job_progress").data(evt.to_string()),
                                    );
                                }
                            }
                            if command_complete {
                                break;
                            }
                        }
                    }
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
                    if row.msg.starts_with("status=")
                        && live_commands.front().is_some_and(|command| command.complete)
                    {
                        live_commands.pop_front();
                    }
                    continue;
                }

                let event_name = serde_json::from_str::<serde_json::Value>(&row.msg)
                    .ok()
                    .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .unwrap_or_else(|| "event".to_string());

                // Preserve the named event for existing consumers and also expose the
                // durable row to task-detail log viewers, where level=event is user-filterable.
                let log_evt = json!({
                    "type": "job_log",
                    "jobId": sse_job_id,
                    "id": row.id,
                    "ts": row.ts,
                    "level": "event",
                    "msg": row.msg.clone(),
                });
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .event("job_log")
                        .data(log_evt.to_string()),
                );

                let ev = Event::default()
                    .id(row.id.to_string())
                    .event(event_name.clone())
                    .data(row.msg);
                let should_close = event_name.as_str() == "runtime_scan_finished";
                yield Ok::<Event, Infallible>(ev);

                if should_close {
                    sse_state.job_live_log_hub.close(&sse_job_id);
                    break;
                }
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
