use super::*;

use crate::{
    compose_runner::{ComposeRunnerConfig, ComposeStack},
    runner::CommandRunner as _,
};

const LIFECYCLE_STATUS_TIMEOUT_SECONDS: u64 = 30;
const LIFECYCLE_ACTION_TIMEOUT_SECONDS: u64 = 300;

fn lifecycle_compose_stack(stack: &StackRecord) -> ComposeStack {
    ComposeStack {
        project_name: updater::sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    }
}

fn lifecycle_compose_config(
    state: &AppState,
) -> anyhow::Result<(ComposeRunnerConfig, Option<updater::DockerCliAuthBridge>)> {
    let auth_bridge = state
        .config
        .docker_config_path
        .as_deref()
        .map(updater::DockerCliAuthBridge::stage)
        .transpose()?;
    let env = auth_bridge
        .as_ref()
        .map(updater::DockerCliAuthBridge::env)
        .unwrap_or_default();
    Ok((
        ComposeRunnerConfig {
            compose_bin: state.config.compose_bin.clone(),
            env,
        },
        auth_bridge,
    ))
}

fn command_ids(output: &str) -> usize {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

pub(crate) fn lifecycle_state_from_counts(all: usize, running: usize) -> ServiceLifecycleState {
    if running > all {
        return ServiceLifecycleState::Unknown;
    }
    if all == 0 || running == 0 {
        ServiceLifecycleState::Stopped
    } else if running == all {
        ServiceLifecycleState::Running
    } else {
        ServiceLifecycleState::Partial
    }
}

fn lifecycle_state_for_compose(
    all: usize,
    running: usize,
    is_plugin: bool,
) -> (ServiceLifecycleState, Option<&'static str>) {
    if all == 0 && !is_plugin {
        return (
            ServiceLifecycleState::Unknown,
            Some("container_missing_for_compose_v1"),
        );
    }
    (lifecycle_state_from_counts(all, running), None)
}

async fn resolve_lifecycle_subject(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<(StackRecord, Service), ApiError> {
    let (stack_id, service) = resolve_service_for_transition(state, service_id).await?;
    let stack = state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("stack not found"))?;
    Ok((stack, service))
}

fn active_job_from_conflict(conflict: &PendingRollbackConflict) -> ServiceLifecycleActiveJob {
    let action = if matches!(conflict.job.r#type, JobType::ServiceLifecycle) {
        conflict
            .job
            .summary_json
            .get("action")
            .and_then(|value| value.as_str())
            .and_then(|value| match value {
                "start" => Some(ServiceLifecycleAction::Start),
                "stop" => Some(ServiceLifecycleAction::Stop),
                "restart" => Some(ServiceLifecycleAction::Restart),
                _ => None,
            })
    } else {
        None
    };
    ServiceLifecycleActiveJob {
        id: conflict.job.id.clone(),
        r#type: conflict.job.r#type.as_str().to_string(),
        status: conflict.job.status.clone(),
        action,
    }
}

async fn read_lifecycle_state(
    state: &Arc<AppState>,
    stack: &StackRecord,
    service: &Service,
) -> (ServiceLifecycleState, Option<String>) {
    let compose = lifecycle_compose_stack(stack);
    let Ok((config, _auth_bridge)) = lifecycle_compose_config(state.as_ref()) else {
        return (
            ServiceLifecycleState::Unknown,
            Some("lifecycle_status_unavailable".to_string()),
        );
    };
    let timeout = Duration::from_secs(LIFECYCLE_STATUS_TIMEOUT_SECONDS);
    let all = state
        .runner
        .run(compose.ps_all_q_service(&config, &service.name), timeout)
        .await;
    let running = state
        .runner
        .run(compose.ps_q_service(&config, &service.name), timeout)
        .await;
    match (all, running) {
        (Ok(all), Ok(running)) if all.status == 0 && running.status == 0 => {
            let all_count = command_ids(&all.stdout);
            let (lifecycle_state, unavailable_reason) = lifecycle_state_for_compose(
                all_count,
                command_ids(&running.stdout),
                crate::compose_runner::is_docker_plugin(&config.compose_bin),
            );
            (lifecycle_state, unavailable_reason.map(str::to_string))
        }
        _ => (
            ServiceLifecycleState::Unknown,
            Some("lifecycle_status_unavailable".to_string()),
        ),
    }
}

pub(crate) async fn get_service_lifecycle_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceLifecycleStatusResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let (stack, service) = resolve_lifecycle_subject(&state, &service_id).await?;
    let conflict = find_pending_service_operation_conflict(&state, &stack.id, &service_id).await?;
    let (state_value, mut unavailable_reason) =
        read_lifecycle_state(&state, &stack, &service).await;
    if updater::is_dockrev_image_ref(
        &service.image.reference,
        Some(state.config.dockrev_image_repo.as_str()),
    ) {
        unavailable_reason = Some("dockrev_service_managed_via_supervisor".to_string());
    } else if matches!(state_value, ServiceLifecycleState::Partial) {
        unavailable_reason = Some("partial_replicas_running".to_string());
    }
    if let Some(conflict) = conflict.as_ref() {
        unavailable_reason.get_or_insert_with(|| conflict.reason.clone());
    }
    Ok(Json(ServiceLifecycleStatusResponse {
        state: state_value,
        active_job: conflict.as_ref().map(active_job_from_conflict),
        unavailable_reason,
    }))
}

pub(crate) async fn trigger_service_lifecycle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<TriggerServiceLifecycleRequest>,
) -> Result<Json<TriggerServiceLifecycleResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let (stack, service) = resolve_lifecycle_subject(&state, &service_id).await?;
    if updater::is_dockrev_image_ref(
        &service.image.reference,
        Some(state.config.dockrev_image_repo.as_str()),
    ) {
        return Err(ApiError::conflict(
            "dockrev service is managed via supervisor",
        ));
    }
    if let Some(conflict) =
        find_pending_service_operation_conflict(&state, &stack.id, &service_id).await?
    {
        return Err(service_operation_conflict_error(&conflict.job));
    }
    let (lifecycle_state, unavailable_reason) =
        read_lifecycle_state(&state, &stack, &service).await;
    if matches!(
        lifecycle_state,
        ServiceLifecycleState::Partial | ServiceLifecycleState::Unknown
    ) {
        return Err(ApiError::conflict("service lifecycle is unavailable").with_details(json!({
            "reason": unavailable_reason.unwrap_or_else(|| lifecycle_state.as_str().to_string()),
        })));
    }
    let action_allowed = matches!(
        (&req.action, &lifecycle_state),
        (
            ServiceLifecycleAction::Start,
            ServiceLifecycleState::Stopped
        ) | (ServiceLifecycleAction::Stop, ServiceLifecycleState::Running)
            | (
                ServiceLifecycleAction::Restart,
                ServiceLifecycleState::Running
            )
    );
    if !action_allowed {
        return Err(
            ApiError::conflict("lifecycle action is incompatible with service state").with_details(
                json!({
                    "reason": "lifecycle_action_incompatible",
                    "state": lifecycle_state.as_str(),
                }),
            ),
        );
    }

    let now = now_rfc3339().map_err(map_internal)?;
    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::ServiceLifecycle,
        JobScope::Service,
        Some(stack.id.clone()),
        Some(service.id.clone()),
        &now,
    );
    job.summary_json = json!({
        "action": req.action.as_str(),
        "serviceName": service.name,
        "initialState": lifecycle_state.as_str(),
    });
    let mut job_db = job.to_db();
    job_db.created_by = user.principal;
    job_db.reason = "ui".to_string();
    if let Some(conflict) = state
        .db
        .insert_service_operation_job_if_unblocked(
            job_db,
            vec![crate::db::ServiceOperationTarget {
                service_id: service.id.clone(),
                stack_id: stack.id.clone(),
            }],
            Some(JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: format!("service lifecycle {} started", req.action.as_str()),
            }),
        )
        .await
        .map_err(map_internal)?
    {
        return Err(service_operation_conflict_error(&conflict));
    }
    let run_state = state.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        run_service_lifecycle_job(run_state, run_job_id, stack, service, req.action).await;
    });
    Ok(Json(TriggerServiceLifecycleResponse { job_id }))
}

async fn run_service_lifecycle_job(
    state: Arc<AppState>,
    job_id: String,
    stack: StackRecord,
    service: Service,
    action: ServiceLifecycleAction,
) {
    let compose = lifecycle_compose_stack(&stack);
    let outcome = match lifecycle_compose_config(state.as_ref()) {
        Ok((config, _auth_bridge)) => {
            let command = match action {
                ServiceLifecycleAction::Start => {
                    compose.start_service_without_pull(&config, &service.name)
                }
                ServiceLifecycleAction::Stop => {
                    compose.stop_services(&config, std::slice::from_ref(&service.name))
                }
                ServiceLifecycleAction::Restart => compose.restart_service(&config, &service.name),
            };
            let runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
            };
            runner
                .run(
                    command,
                    Duration::from_secs(LIFECYCLE_ACTION_TIMEOUT_SECONDS),
                )
                .await
                .and_then(|output| {
                    if output.status == 0 {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "compose lifecycle command exited with {}",
                            output.status
                        ))
                    }
                })
        }
        Err(error) => Err(error),
    };
    let finished_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    let (status, message, error) = match outcome {
        Ok(()) => (
            "success",
            format!("service lifecycle {} finished", action.as_str()),
            None,
        ),
        Err(error) => (
            "failed",
            format!("service lifecycle {} failed", action.as_str()),
            Some(error.to_string()),
        ),
    };
    let _ = state
        .db
        .insert_job_log(
            &job_id,
            &JobLogLine {
                ts: finished_at.clone(),
                level: if status == "success" {
                    "info".to_string()
                } else {
                    "error".to_string()
                },
                msg: message,
            },
        )
        .await;
    let _ = state
        .db
        .finish_job(
            &job_id,
            status,
            &finished_at,
            &json!({
                "action": action.as_str(),
                "serviceName": service.name,
                "error": error,
            }),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_state_distinguishes_stopped_running_and_partial() {
        assert_eq!(
            lifecycle_state_from_counts(0, 0),
            ServiceLifecycleState::Stopped
        );
        assert_eq!(
            lifecycle_state_from_counts(2, 0),
            ServiceLifecycleState::Stopped
        );
        assert_eq!(
            lifecycle_state_from_counts(2, 2),
            ServiceLifecycleState::Running
        );
        assert_eq!(
            lifecycle_state_from_counts(3, 1),
            ServiceLifecycleState::Partial
        );
        assert_eq!(
            lifecycle_state_from_counts(1, 2),
            ServiceLifecycleState::Unknown
        );
        assert_eq!(
            lifecycle_state_for_compose(0, 0, false),
            (
                ServiceLifecycleState::Unknown,
                Some("container_missing_for_compose_v1")
            )
        );
        assert_eq!(
            lifecycle_state_for_compose(0, 0, true),
            (ServiceLifecycleState::Stopped, None)
        );
    }
}
