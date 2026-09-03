use super::*;

pub(super) async fn trigger_managed_override_reconcile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<TriggerManagedOverrideReconcileResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let stack = state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("stack not found"))?;
    let project = state
        .db
        .list_discovered_compose_projects(crate::db::ArchivedFilter::Include)
        .await
        .map_err(map_internal)?
        .into_iter()
        .find(|project| project.stack_id.as_deref() == Some(stack_id.as_str()))
        .ok_or_else(|| ApiError::not_found("discovery project not found"))?;
    let eligible = project
        .last_error
        .as_deref()
        .is_some_and(|error| error.starts_with(crate::managed_override::STALE_TEMP_WARNING));
    if !eligible {
        return Err(ApiError::conflict(
            "stack does not have a stale Dockrev temporary override warning",
        ));
    }

    for service in &stack.services {
        if let Some(job) = state
            .db
            .find_latest_pending_update_blocking_service(&stack_id, &service.id)
            .await
            .map_err(map_internal)?
        {
            return Err(ApiError::conflict("stack update is already running")
                .with_details(json!({"existingJobId": job.id})));
        }
        if let Some(job) = state
            .db
            .find_latest_pending_stack_lifecycle_blocking_service(&stack_id, &service.id)
            .await
            .map_err(map_internal)?
        {
            return Err(
                ApiError::conflict("stack lifecycle operation is already running")
                    .with_details(json!({"existingJobId": job.id})),
            );
        }
    }

    let Some(reconcile_guard) = discovery::try_managed_reconcile_lock() else {
        return Err(ApiError::conflict(
            "managed override reconciliation is already running",
        ));
    };
    let running = state
        .db
        .list_jobs_page(crate::db::JobListFilters {
            types: vec!["managed_override_reconcile".to_string()],
            status: Some("running".to_string()),
            stack_id: Some(stack_id.clone()),
            service_id: None,
            cursor: None,
            limit: 1,
        })
        .await
        .map_err(map_internal)?;
    if let Some(job) = running.jobs.first() {
        return Err(
            ApiError::conflict("managed override reconciliation is already running")
                .with_details(json!({"existingJobId": job.id})),
        );
    }

    let now = now_rfc3339().map_err(map_internal)?;
    let job_id = ids::new_job_id();
    let job = JobRecord::new_running(
        job_id.clone(),
        JobType::ManagedOverrideReconcile,
        JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &now,
    );
    let mut job_db = job.to_db();
    job_db.created_by = user.principal;
    job_db.reason = "ui".to_string();
    let targets = stack
        .services
        .iter()
        .map(|service| crate::db::ServiceOperationTarget {
            service_id: service.id.clone(),
            stack_id: stack_id.clone(),
        })
        .collect::<Vec<_>>();
    if let Some(conflict) = state
        .db
        .insert_service_operation_job_if_unblocked(
            job_db,
            targets,
            Some(JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: "managed override reconciliation started".to_string(),
            }),
        )
        .await
        .map_err(map_internal)?
    {
        return Err(ApiError::conflict("service operation is already running")
            .with_details(json!({"existingJobId": conflict.id})));
    }

    let run_state = state.clone();
    let run_stack_id = stack_id.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        discovery::run_managed_override_reconcile(
            &run_state,
            &run_job_id,
            &run_stack_id,
            reconcile_guard,
        )
        .await;
    });

    Ok(Json(TriggerManagedOverrideReconcileResponse { job_id }))
}

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
