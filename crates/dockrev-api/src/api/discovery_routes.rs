use super::*;

pub(super) async fn trigger_discovery_scan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TriggerDiscoveryScanJobResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let job_id = ids::new_discovery_id();
    let job = JobRecord::new_running(
        job_id.clone(),
        JobType::Discovery,
        JobScope::All,
        None,
        None,
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = user.principal;
    job_db.reason = "ui".to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        let outcome = discovery::run_scan_for_job(run_state.as_ref(), &run_job_id).await;
        let finished_at =
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        match outcome {
            Ok(resp) => {
                let summary = json!({ "scan": resp });
                let _ = run_state
                    .db
                    .finish_job(&run_job_id, "success", &finished_at, &summary)
                    .await;
            }
            Err(e) => {
                let _ = run_state
                    .db
                    .insert_job_log(
                        &run_job_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: format!("discovery scan failed: {e}"),
                        },
                    )
                    .await;
                let summary = json!({ "error": e.to_string() });
                let _ = run_state
                    .db
                    .finish_job(&run_job_id, "failed", &finished_at, &summary)
                    .await;
            }
        }
    });

    Ok(Json(TriggerDiscoveryScanJobResponse { job_id }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListDiscoveryProjectsQuery {
    archived: Option<String>,
}

pub(super) async fn list_discovery_projects(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListDiscoveryProjectsQuery>,
) -> Result<Json<ListDiscoveredProjectsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let projects = state
        .db
        .list_discovered_compose_projects(parse_archived_filter(q.archived.as_deref())?)
        .await
        .map_err(map_internal)?;
    Ok(Json(ListDiscoveredProjectsResponse { projects }))
}

pub(super) async fn archive_discovery_project(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_discovered_compose_project_archived(&project, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("project not found"));
    }
    state
        .management_events
        .publish_change(
            "discovery",
            "project",
            project,
            json!({ "operation": "archived" }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn restore_discovery_project(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_discovered_compose_project_archived(&project, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("project not found"));
    }
    state
        .management_events
        .publish_change(
            "discovery",
            "project",
            project,
            json!({ "operation": "restored" }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}
