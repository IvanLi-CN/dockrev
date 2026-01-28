pub mod types;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    backup, candidates, discovery, error::ApiError, ids, ignore, notify, registry, state::AppState,
    ui, updater,
};
use types::*;

pub fn router(state: Arc<AppState>) -> Router {
    Router::<Arc<AppState>>::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route(
            "/api/stacks",
            get(list_stacks).post(register_stack_disabled),
        )
        .route("/api/stacks/{stack_id}", get(get_stack))
        .route("/api/stacks/{stack_id}/archive", post(archive_stack))
        .route("/api/stacks/{stack_id}/restore", post(restore_stack))
        .route("/api/services/{service_id}/archive", post(archive_service))
        .route("/api/services/{service_id}/restore", post(restore_service))
        .route(
            "/api/services/{service_id}/candidates",
            get(list_service_candidates),
        )
        .route("/api/discovery/scan", post(trigger_discovery_scan))
        .route("/api/discovery/projects", get(list_discovery_projects))
        .route(
            "/api/discovery/projects/{project}/archive",
            post(archive_discovery_project),
        )
        .route(
            "/api/discovery/projects/{project}/restore",
            post(restore_discovery_project),
        )
        .route("/api/checks", post(trigger_check))
        .route("/api/updates", post(trigger_update))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{job_id}", get(get_job))
        .route(
            "/api/ignores",
            get(list_ignores).post(create_ignore).delete(delete_ignore),
        )
        .route(
            "/api/services/{service_id}/settings",
            get(get_service_settings).put(put_service_settings),
        )
        .route(
            "/api/notifications",
            get(get_notifications).put(put_notifications),
        )
        .route("/api/notifications/test", post(test_notifications))
        .route(
            "/api/web-push/subscriptions",
            post(create_web_push_subscription).delete(delete_web_push_subscription),
        )
        .route("/api/webhooks/trigger", post(webhook_trigger))
        .route("/api/settings", get(get_settings).put(put_settings))
        .merge(ui::router())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct VersionResponse {
    version: String,
}

async fn version(State(state): State<Arc<AppState>>) -> Json<VersionResponse> {
    Json(VersionResponse {
        version: state.config.app_effective_version.clone(),
    })
}

async fn list_stacks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListStacksQuery>,
) -> Result<Json<ListStacksResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let stacks = state
        .db
        .list_stacks(parse_archived_filter(q.archived.as_deref())?)
        .await
        .map_err(map_internal)?;
    Ok(Json(ListStacksResponse { stacks }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListStacksQuery {
    archived: Option<String>,
}

fn parse_archived_filter(input: Option<&str>) -> Result<crate::db::ArchivedFilter, ApiError> {
    match input.unwrap_or("exclude") {
        "exclude" => Ok(crate::db::ArchivedFilter::Exclude),
        "include" => Ok(crate::db::ArchivedFilter::Include),
        "only" => Ok(crate::db::ArchivedFilter::Only),
        other => Err(ApiError::invalid_argument(format!(
            "invalid archived filter: {other}"
        ))),
    }
}

async fn get_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<GetStackResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };

    Ok(Json(GetStackResponse {
        stack: StackResponse {
            id: stack.id,
            name: stack.name,
            compose: stack.compose,
            services: stack.services,
            archived: Some(stack.archived),
        },
    }))
}

async fn register_stack_disabled(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let _user = require_user(&state, &headers)?;
    Ok((
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({
            "error": "manual stack registration is disabled; use auto-discovery instead"
        })),
    ))
}

async fn archive_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_stack_archived(&stack_id, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("stack not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_stack_archived(&stack_id, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("stack not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn archive_service(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_service_archived(&service_id, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("service not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_service(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_service_archived(&service_id, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("service not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn trigger_discovery_scan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TriggerDiscoveryScanJobResponse>, ApiError> {
    let user = require_user(&state, &headers)?;
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
    job_db.created_by = user;
    job_db.reason = "ui".to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        let outcome = discovery::run_scan(run_state.as_ref()).await;
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
struct ListDiscoveryProjectsQuery {
    archived: Option<String>,
}

async fn list_discovery_projects(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListDiscoveryProjectsQuery>,
) -> Result<Json<ListDiscoveredProjectsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let projects = state
        .db
        .list_discovered_compose_projects(parse_archived_filter(q.archived.as_deref())?)
        .await
        .map_err(map_internal)?;
    Ok(Json(ListDiscoveredProjectsResponse { projects }))
}

async fn archive_discovery_project(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_discovered_compose_project_archived(&project, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("project not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_discovery_project(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_discovered_compose_project_archived(&project, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("project not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn trigger_check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerCheckRequest>,
) -> Result<Json<TriggerCheckResponse>, ApiError> {
    let user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    let check_id = ids::new_check_id();
    let job = JobRecord::new_running(
        check_id.clone(),
        JobType::Check,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = user.clone();
    job_db.reason = req.reason.as_str().to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let outcome = run_check_for_job(
        &state,
        &check_id,
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
        &host_platform,
        &now,
    )
    .await;

    let finished_at = now_rfc3339().map_err(map_internal)?;
    match outcome {
        Ok(summary) => {
            state
                .db
                .finish_job(&check_id, "success", &finished_at, &summary)
                .await
                .map_err(map_internal)?;
        }
        Err(e) => {
            let summary = json!({"error": format!("{e:?}")});
            let _ = state
                .db
                .finish_job(&check_id, "failed", &finished_at, &summary)
                .await;
            return Err(e);
        }
    }

    Ok(Json(TriggerCheckResponse { check_id }))
}

async fn run_check_for_job(
    state: &Arc<AppState>,
    job_id: &str,
    scope: &JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    host_platform: &str,
    now: &str,
) -> Result<serde_json::Value, ApiError> {
    let stack_ids = match scope {
        JobScope::All => state.db.list_stack_ids().await.map_err(map_internal)?,
        JobScope::Stack => stack_id.map(|s| vec![s.to_string()]).unwrap_or_default(),
        JobScope::Service => {
            let service_id = service_id.unwrap_or_default().to_string();
            state
                .db
                .get_service_stack_id(&service_id)
                .await
                .map_err(map_internal)?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };

    let mut services_checked = 0u32;
    let mut services_with_candidate = 0u32;

    for stack_id in &stack_ids {
        let services = state
            .db
            .list_services_for_check(stack_id)
            .await
            .map_err(map_internal)?;

        for svc in services {
            services_checked += 1;
            let img = match registry::ImageRef::parse(&svc.image_ref) {
                Ok(img) => img,
                Err(_) => {
                    state
                        .db
                        .insert_job_log(
                            job_id,
                            &JobLogLine {
                                ts: now.to_string(),
                                level: "warn".to_string(),
                                msg: format!("skip service {}: invalid image ref", svc.id),
                            },
                        )
                        .await
                        .map_err(map_internal)?;
                    continue;
                }
            };

            let ignore_rules = state
                .db
                .list_ignore_rules_for_service(&svc.id)
                .await
                .map_err(map_internal)?;
            let matchers = ignore_rules
                .iter()
                .map(|r| {
                    let kind = ignore::IgnoreKind::parse(&r.matcher.kind);
                    (
                        r.id.clone(),
                        ignore::IgnoreRuleMatcher {
                            kind,
                            value: r.matcher.value.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>();

            let tags = match state.registry.list_tags(&img).await {
                Ok(t) => t,
                Err(e) => {
                    state
                        .db
                        .insert_job_log(
                            job_id,
                            &JobLogLine {
                                ts: now.to_string(),
                                level: "warn".to_string(),
                                msg: format!("list tags failed for {}: {}", img.name, e),
                            },
                        )
                        .await
                        .map_err(map_internal)?;
                    continue;
                }
            };

            let is_ignored = |tag: &str| matchers.iter().any(|(_, m)| m.matches(tag));
            let candidate_non_ignored =
                candidates::select_candidate_tag(&svc.image_tag, &tags, is_ignored);
            let candidate_any = candidates::select_candidate_tag(&svc.image_tag, &tags, |_| false);
            let candidate_tag = candidate_non_ignored.or(candidate_any);
            if candidate_tag.is_some() {
                services_with_candidate += 1;
            }

            let mut ignore_match: Option<(String, String)> = None;
            if let Some(ref tag) = candidate_tag
                && let Some((rule_id, _)) = matchers.iter().find(|(_, m)| m.matches(tag))
            {
                ignore_match = Some((
                    rule_id.clone(),
                    format!("matched ignore rule for tag {tag}"),
                ));
            }

            let current_digest = state
                .registry
                .get_manifest(&img, &svc.image_tag)
                .await
                .ok()
                .and_then(|m| m.digest);

            let (candidate_digest, candidate_arch_match, candidate_arch_json) =
                if let Some(tag) = candidate_tag.as_deref() {
                    match state.registry.get_manifest(&img, tag).await {
                        Ok(m) => {
                            let arch_match = registry::compute_arch_match(host_platform, &m.arch);
                            (
                                m.digest,
                                Some(arch_match.as_str().to_string()),
                                Some(serde_json::to_string(&m.arch).unwrap_or_default()),
                            )
                        }
                        Err(_) => (None, None, None),
                    }
                } else {
                    (None, None, None)
                };

            state
                .db
                .update_service_check_result(
                    &svc.id,
                    current_digest,
                    candidate_tag.clone(),
                    candidate_digest,
                    candidate_arch_match,
                    candidate_arch_json,
                    ignore_match.as_ref().map(|(id, _)| id.clone()),
                    ignore_match.as_ref().map(|(_, r)| r.clone()),
                    now,
                    now,
                )
                .await
                .map_err(map_internal)?;
        }

        state
            .db
            .update_stack_last_check_at(stack_id, now)
            .await
            .map_err(map_internal)?;
    }

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: now.to_string(),
                level: "info".to_string(),
                msg: "check finished".to_string(),
            },
        )
        .await
        .map_err(map_internal)?;

    Ok(json!({
        "hostPlatform": host_platform,
        "scope": scope.as_str(),
        "stackIds": stack_ids,
        "servicesChecked": services_checked,
        "servicesWithCandidate": services_with_candidate,
    }))
}

async fn trigger_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<Json<TriggerUpdateResponse>, ApiError> {
    let user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    if (req.target_tag.is_some() || req.target_digest.is_some()) && req.scope != JobScope::Service {
        return Err(ApiError::invalid_argument(
            "targetTag/targetDigest is only supported for scope=service",
        ));
    }

    let job_id = enqueue_update_job(state, user, req.reason.as_str().to_string(), req, now).await?;

    Ok(Json(TriggerUpdateResponse { job_id }))
}

async fn enqueue_update_job(
    state: Arc<AppState>,
    created_by: String,
    reason: String,
    req: TriggerUpdateRequest,
    now: String,
) -> Result<String, ApiError> {
    let stack_ids = resolve_stack_ids_for_update(&state, &req)
        .await
        .map_err(map_internal)?;
    validate_arch_mismatch_for_update(&state, &req, &stack_ids).await?;

    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::Update,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );
    job.allow_arch_mismatch = req.allow_arch_mismatch;
    job.backup_mode = req.backup_mode.as_str().to_string();
    job.summary_json = json!({ "mode": req.mode.as_str() });

    let mut job_db = job.to_db();
    job_db.created_by = created_by;
    job_db.reason = reason;
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    state
        .db
        .insert_job_log(
            &job_id,
            &JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: "update started".to_string(),
            },
        )
        .await
        .map_err(map_internal)?;

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_req = req.clone();
    tokio::spawn(async move {
        let _ = run_update_job(run_state, run_job_id, run_req).await;
    });

    Ok(job_id)
}

async fn resolve_stack_ids_for_update(
    state: &AppState,
    req: &TriggerUpdateRequest,
) -> anyhow::Result<Vec<String>> {
    let stack_ids = match req.scope {
        JobScope::All => state.db.list_stack_ids().await?,
        JobScope::Stack => req.stack_id.clone().into_iter().collect(),
        JobScope::Service => {
            let service_id = req.service_id.clone().unwrap_or_default();
            state
                .db
                .get_service_stack_id(&service_id)
                .await?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };
    Ok(stack_ids)
}

async fn validate_arch_mismatch_for_update(
    state: &AppState,
    req: &TriggerUpdateRequest,
    stack_ids: &[String],
) -> Result<(), ApiError> {
    if req.allow_arch_mismatch {
        return Ok(());
    }

    // For stack/all updates we intentionally skip arch-mismatch services (UI shows them as non-actionable),
    // so only enforce mismatch blocking for service-scoped updates.
    if req.scope != JobScope::Service {
        return Ok(());
    }

    for stack_id in stack_ids {
        let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
            continue;
        };

        for svc in &stack.services {
            if req.service_id.as_deref().is_some_and(|id| id != svc.id) {
                continue;
            }

            if let Some(tag) = req.target_tag.as_deref() {
                let img = registry::ImageRef::parse(&svc.image.reference).map_err(|_| {
                    ApiError::invalid_argument("invalid image ref (expected repo/name:tag)")
                })?;

                let reference = req.target_digest.as_deref().unwrap_or(tag);
                let manifest = state
                    .registry
                    .get_manifest(&img, reference)
                    .await
                    .map_err(map_internal)?;
                let arch_match = registry::compute_arch_match(
                    registry::host_platform_override(state.config.host_platform.as_deref())
                        .unwrap_or_else(|| "linux/amd64".to_string())
                        .as_str(),
                    &manifest.arch,
                );
                if matches!(arch_match, ArchMatch::Mismatch) {
                    return Err(ApiError::invalid_argument(
                        "candidate arch mismatch (set allowArchMismatch=true to override)",
                    ));
                }
                continue;
            }

            if req.target_digest.is_some() {
                let img = registry::ImageRef::parse(&svc.image.reference).map_err(|_| {
                    ApiError::invalid_argument("invalid image ref (expected repo/name:tag)")
                })?;
                let reference = req.target_digest.as_deref().unwrap();
                let manifest = state
                    .registry
                    .get_manifest(&img, reference)
                    .await
                    .map_err(map_internal)?;
                let arch_match = registry::compute_arch_match(
                    registry::host_platform_override(state.config.host_platform.as_deref())
                        .unwrap_or_else(|| "linux/amd64".to_string())
                        .as_str(),
                    &manifest.arch,
                );
                if matches!(arch_match, ArchMatch::Mismatch) {
                    return Err(ApiError::invalid_argument(
                        "candidate arch mismatch (set allowArchMismatch=true to override)",
                    ));
                }
                continue;
            }

            if let Some(candidate) = svc.candidate.as_ref()
                && matches!(candidate.arch_match, ArchMatch::Mismatch)
            {
                return Err(ApiError::invalid_argument(
                    "candidate arch mismatch (set allowArchMismatch=true to override)",
                ));
            }
        }
    }

    Ok(())
}

type UpdateStackSummaries = Vec<serde_json::Value>;
type UpdateBackupsToCleanup = Vec<(String, u32)>;
type UpdateJobOutcome = (String, UpdateStackSummaries, UpdateBackupsToCleanup);

async fn run_update_job(
    state: Arc<AppState>,
    job_id: String,
    req: TriggerUpdateRequest,
) -> anyhow::Result<()> {
    fn extract_changed_service_ids(update: &serde_json::Value) -> Option<Vec<String>> {
        let ids = update
            .get("newDigests")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect::<Vec<_>>())?;
        if ids.is_empty() { None } else { Some(ids) }
    }

    let outcome: anyhow::Result<UpdateJobOutcome> = async {
        let backup_settings = state.db.get_backup_settings().await?;
        let stack_ids = resolve_stack_ids_for_update(state.as_ref(), &req).await?;

        let mut final_status = "success".to_string();
        let mut stack_summaries = Vec::new();
        let mut backups_to_cleanup: Vec<(String, u32)> = Vec::new();

        for stack_id in &stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await? else {
                continue;
            };

            let logging_runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
            };

            let mut stack_summary = serde_json::Map::new();
            stack_summary.insert("stackId".to_string(), json!(stack_id));

            let mut backup_id_for_cleanup: Option<(String, u32)> = None;
            if req.mode.as_str() == "apply"
                && backup::should_run_backup(&backup_settings, req.backup_mode.as_str())
            {
                let backup_id = ids::new_backup_id();
                let now = now_rfc3339()?;
                state
                    .db
                    .insert_backup(&backup_id, stack_id, &job_id, &now)
                    .await?;
                state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: now.clone(),
                            level: "info".to_string(),
                            msg: format!("backup started: {backup_id}"),
                        },
                    )
                    .await?;

                match backup::run_pre_update_backup(
                    &logging_runner,
                    &backup_settings,
                    &stack,
                    &req.scope,
                    req.service_id.as_deref(),
                    &now,
                )
                .await
                {
                    Ok(res) => {
                        for msg in &res.log_lines {
                            let _ = state
                                .db
                                .insert_job_log(
                                    &job_id,
                                    &JobLogLine {
                                        ts: now.clone(),
                                        level: "info".to_string(),
                                        msg: msg.clone(),
                                    },
                                )
                                .await;
                        }

                        let _ = state
                            .db
                            .finish_backup(
                                &backup_id,
                                &res.status,
                                &now,
                                res.artifact_path.as_deref(),
                                res.size_bytes,
                                None,
                            )
                            .await;

                        stack_summary.insert("backup".to_string(), res.summary_json);

                        if res.status == "success" {
                            backup_id_for_cleanup = Some((
                                backup_id,
                                stack.backup.retention.delete_after_stable_seconds,
                            ));
                        }
                    }
                    Err(e) => {
                        let err = e.to_string();
                        let _ = state
                            .db
                            .finish_backup(&backup_id, "failed", &now, None, None, Some(&err))
                            .await;
                        let _ = state
                            .db
                            .insert_job_log(
                                &job_id,
                                &JobLogLine {
                                    ts: now.clone(),
                                    level: "warn".to_string(),
                                    msg: format!("backup failed: {err}"),
                                },
                            )
                            .await;

                        stack_summary
                            .insert("backup".to_string(), json!({"status":"failed","error":err}));

                        if backup_settings.require_success {
                            final_status = "failed".to_string();
                            stack_summaries.push(serde_json::Value::Object(stack_summary));
                            break;
                        }
                    }
                }
            } else {
                stack_summary.insert(
                    "backup".to_string(),
                    if req.mode.as_str() != "apply" {
                        json!({"status":"skipped","reason":"dry_run"})
                    } else {
                        json!({"status":"skipped","reason":"disabled"})
                    },
                );
            }

            let update_outcome = updater::run_update_job(
                &logging_runner,
                &state.config.compose_bin,
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.mode.as_str(),
                req.target_tag.as_deref(),
                req.target_digest.as_deref(),
                req.allow_arch_mismatch,
            )
            .await;
            match update_outcome {
                Ok(outcome) => {
                    final_status = outcome.status.clone();
                    stack_summary.insert("update".to_string(), outcome.summary_json);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));

                    if final_status != "success" {
                        break;
                    }

                    if let Some(b) = backup_id_for_cleanup.take() {
                        backups_to_cleanup.push(b);
                    }
                }
                Err(e) => {
                    final_status = "failed".to_string();
                    stack_summary.insert("update".to_string(), json!({"error": e.to_string()}));
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    break;
                }
            }
        }

        Ok((final_status, stack_summaries, backups_to_cleanup))
    }
    .await;

    let (final_status, stack_summaries, backups_to_cleanup, final_summary, finished_at) =
        match outcome {
            Ok((final_status, stack_summaries, backups_to_cleanup)) => {
                let final_summary = json!({
                    "mode": req.mode.as_str(),
                    "stacks": stack_summaries.clone(),
                });
                let finished_at = now_rfc3339()?;
                (
                    final_status,
                    stack_summaries,
                    backups_to_cleanup,
                    final_summary,
                    finished_at,
                )
            }
            Err(err) => {
                let finished_at = now_rfc3339()?;
                let _ = state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: format!("update failed: {err}"),
                        },
                    )
                    .await;
                let final_summary = json!({
                    "mode": req.mode.as_str(),
                    "error": err.to_string(),
                });
                (
                    "failed".to_string(),
                    Vec::new(),
                    Vec::new(),
                    final_summary,
                    finished_at,
                )
            }
        };

    let force_notify = final_status != "success";
    let mut should_notify = true;
    let mut notify_summary = final_summary.clone();
    let mut notify_skip_reason: Option<String> = None;
    if !force_notify {
        match req.scope {
            JobScope::Service => {
                if let Some(service_id) = req.service_id.as_deref()
                    && let Some(true) = state.db.is_service_archived(service_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived service".to_string());
                }
                if should_notify
                    && let Some(service_id) = req.service_id.as_deref()
                    && let Some(stack_id) = state.db.get_service_stack_id(service_id).await?
                    && let Some(true) = state.db.is_stack_archived(&stack_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived stack".to_string());
                }
            }
            JobScope::Stack | JobScope::All => {
                let mut filtered = Vec::<serde_json::Value>::new();
                for s in &stack_summaries {
                    let Some(stack_id) = s.get("stackId").and_then(|v| v.as_str()) else {
                        continue;
                    };

                    if let Some(true) = state.db.is_stack_archived(stack_id).await? {
                        continue;
                    }

                    let include = if let Some(update) = s.get("update")
                        && let Some(changed_ids) = extract_changed_service_ids(update)
                    {
                        state.db.has_unarchived_services(&changed_ids).await?
                    } else {
                        state.db.has_unarchived_services_in_stack(stack_id).await?
                    };

                    if include {
                        filtered.push(s.clone());
                    }
                }

                if filtered.is_empty() {
                    should_notify = false;
                    notify_skip_reason =
                        Some("all stacks archived or only archived services touched".to_string());
                } else {
                    notify_summary = json!({
                        "mode": req.mode.as_str(),
                        "stacks": filtered,
                    });
                }
            }
        }
    }

    if !should_notify {
        let _ = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: finished_at.clone(),
                    level: "info".to_string(),
                    msg: format!(
                        "notify skipped ({})",
                        notify_skip_reason.as_deref().unwrap_or("filtered")
                    ),
                },
            )
            .await;
    }

    state
        .db
        .finish_job(&job_id, &final_status, &finished_at, &final_summary)
        .await?;

    if final_status == "success"
        && let Ok(now_dt) = time::OffsetDateTime::parse(
            &finished_at,
            &time::format_description::well_known::Rfc3339,
        )
    {
        for (backup_id, after_seconds) in backups_to_cleanup {
            let cleanup_after = now_dt + time::Duration::seconds(after_seconds as i64);
            if let Ok(cleanup_after) =
                cleanup_after.format(&time::format_description::well_known::Rfc3339)
            {
                let _ = state
                    .db
                    .schedule_backup_cleanup(&backup_id, &cleanup_after)
                    .await;
            }
        }
    }

    if should_notify {
        let notify_state = state.clone();
        let notify_job_id = job_id.clone();
        let notify_status = final_status.clone();
        let notify_now = finished_at.clone();
        let notify_summary = notify_summary.clone();
        tokio::spawn(async move {
            let _ = notify::notify_job_updated(
                notify_state.as_ref(),
                &notify_job_id,
                &notify_status,
                &notify_now,
                &notify_summary,
            )
            .await;
        });
    }

    Ok(())
}

struct DbLoggingRunner {
    db: crate::db::Db,
    inner: Arc<dyn crate::runner::CommandRunner>,
    job_id: String,
}

#[async_trait::async_trait]
impl crate::runner::CommandRunner for DbLoggingRunner {
    async fn run(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let start = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!("$ {} {}", spec.program, spec.args.join(" "));
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts: start,
                    level: "info".to_string(),
                    msg,
                },
            )
            .await;

        let out = self.inner.run(spec, timeout).await?;
        let ts = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status,
            truncate(&out.stdout, 2000),
            truncate(&out.stderr, 2000)
        );
        let _ = self
            .db
            .insert_job_log(
                &self.job_id,
                &JobLogLine {
                    ts,
                    level: if out.status == 0 {
                        "info".to_string()
                    } else {
                        "warn".to_string()
                    },
                    msg,
                },
            )
            .await;
        Ok(out)
    }
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_string();
    }
    format!("{}...(truncated)", &input[..max])
}

async fn list_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ListJobsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let jobs = state.db.list_jobs().await.map_err(map_internal)?;
    Ok(Json(ListJobsResponse {
        jobs: jobs.into_iter().map(|j| j.into_api()).collect(),
    }))
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Result<Json<GetJobResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let job = state.db.get_job(&job_id).await.map_err(map_internal)?;
    let Some(job) = job else {
        return Err(ApiError::not_found("job not found"));
    };

    let logs = state
        .db
        .list_job_logs(&job_id)
        .await
        .map_err(map_internal)?;

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
            logs,
        },
    }))
}

async fn list_ignores(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ListIgnoresResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let rules = state.db.list_ignore_rules().await.map_err(map_internal)?;
    Ok(Json(ListIgnoresResponse { rules }))
}

async fn create_ignore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateIgnoreRequest>,
) -> Result<(StatusCode, Json<CreateIgnoreResponse>), ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    if req.scope.kind != "service" {
        return Err(ApiError::invalid_argument("scope.type must be 'service'"));
    }
    if req.scope.service_id.is_empty() {
        return Err(ApiError::invalid_argument(
            "scope.serviceId must not be empty",
        ));
    }

    let rule_id = ids::new_ignore_id();
    let rule = IgnoreRule {
        id: rule_id.clone(),
        enabled: req.enabled,
        scope: IgnoreRuleScope {
            kind: req.scope.kind,
            service_id: req.scope.service_id,
        },
        matcher: IgnoreRuleMatch {
            kind: req.matcher.kind,
            value: req.matcher.value,
        },
        note: req.note,
    };
    state
        .db
        .insert_ignore_rule(&rule, &now)
        .await
        .map_err(map_internal)?;

    Ok((StatusCode::CREATED, Json(CreateIgnoreResponse { rule_id })))
}

async fn delete_ignore(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteIgnoreRequest>,
) -> Result<Json<DeleteIgnoreResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let deleted = state
        .db
        .delete_ignore_rule(&req.rule_id)
        .await
        .map_err(map_internal)?;

    Ok(Json(DeleteIgnoreResponse { deleted }))
}

async fn get_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    Ok(Json(ServiceSettingsResponse {
        auto_rollback: settings.auto_rollback,
        backup_targets: settings.backup_targets,
    }))
}

async fn list_service_candidates(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceCandidatesResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stack_id) = stack_id else {
        return Err(ApiError::not_found("service not found"));
    };

    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };

    let svc = stack
        .services
        .iter()
        .find(|s| s.id == service_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("service not found"))?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let img = registry::ImageRef::parse(&svc.image.reference)
        .map_err(|_| ApiError::invalid_argument("invalid image ref (expected repo/name:tag)"))?;

    let ignore_rules = state
        .db
        .list_ignore_rules_for_service(&svc.id)
        .await
        .map_err(map_internal)?;
    let matchers = ignore_rules
        .iter()
        .map(|r| {
            let kind = ignore::IgnoreKind::parse(&r.matcher.kind);
            (
                r.id.clone(),
                ignore::IgnoreRuleMatcher {
                    kind,
                    value: r.matcher.value.clone(),
                },
            )
        })
        .collect::<Vec<_>>();

    let tags = state.registry.list_tags(&img).await.map_err(map_internal)?;

    let current_tag = svc.image.tag.clone();
    let current_semver = ignore::parse_version(&current_tag);

    let mut semver_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut other_tags: Vec<String> = Vec::new();

    for tag in tags {
        if tag == current_tag {
            continue;
        }
        if let Some(current) = current_semver.as_ref() {
            if let Some(v) = ignore::parse_version(&tag)
                && v > *current
            {
                semver_tags.push((v, tag));
            }
            continue;
        }
        other_tags.push(tag);
    }

    semver_tags.sort_by(|a, b| b.0.cmp(&a.0));
    other_tags.sort_by(|a, b| b.cmp(a));

    let mut picked: Vec<String> = Vec::new();
    for (_, tag) in semver_tags {
        picked.push(tag);
    }
    for tag in other_tags {
        picked.push(tag);
    }

    // Avoid expensive manifest fan-out.
    if picked.len() > 30 {
        picked.truncate(30);
    }

    let is_ignored = |tag: &str| matchers.iter().any(|(_, m)| m.matches(tag));

    let mut out: Vec<ServiceCandidateOption> = Vec::new();
    for tag in picked {
        let ignored = is_ignored(&tag);
        match state.registry.get_manifest(&img, &tag).await {
            Ok(m) => {
                let arch_match = registry::compute_arch_match(&host_platform, &m.arch);
                out.push(ServiceCandidateOption {
                    tag,
                    digest: m.digest,
                    arch_match,
                    arch: m.arch,
                    ignored,
                });
            }
            Err(_) => {
                out.push(ServiceCandidateOption {
                    tag,
                    digest: None,
                    arch_match: ArchMatch::Unknown,
                    arch: Vec::new(),
                    ignored,
                });
            }
        }
    }

    Ok(Json(ServiceCandidatesResponse { candidates: out }))
}

async fn put_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<ServiceSettingsRequest>,
) -> Result<Json<PutServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let settings = ServiceSettings {
        auto_rollback: req.auto_rollback,
        backup_targets: req.backup_targets,
    };

    let updated = state
        .db
        .put_service_settings(&service_id, &settings, &now)
        .await
        .map_err(map_internal)?;

    if !updated {
        return Err(ApiError::not_found("service not found"));
    }

    Ok(Json(PutServiceSettingsResponse { ok: true }))
}

async fn get_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<NotificationConfig>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let settings = state
        .db
        .get_notification_settings()
        .await
        .map_err(map_internal)?;
    Ok(Json(NotificationConfig::from_db(settings)))
}

async fn put_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<NotificationConfig>,
) -> Result<Json<PutNotificationsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let existing = state
        .db
        .get_notification_settings()
        .await
        .map_err(map_internal)?;
    let mut merged = req.into_db();

    merge_secret(&mut merged.email_smtp_url, existing.email_smtp_url);
    merge_secret(&mut merged.webhook_url, existing.webhook_url);
    merge_secret(&mut merged.telegram_bot_token, existing.telegram_bot_token);
    merge_secret(&mut merged.telegram_chat_id, existing.telegram_chat_id);
    merge_secret(
        &mut merged.webpush_vapid_private_key,
        existing.webpush_vapid_private_key,
    );

    state
        .db
        .put_notification_settings(&merged, &now)
        .await
        .map_err(map_internal)?;
    Ok(Json(PutNotificationsResponse { ok: true }))
}

async fn test_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TestNotificationsRequest>,
) -> Result<Json<TestNotificationsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let message = req.message.unwrap_or_else(|| "dockrev test".to_string());
    let results = notify::send_test(state.as_ref(), &now, &message)
        .await
        .map_err(map_internal)?;
    Ok(Json(TestNotificationsResponse { ok: true, results }))
}

async fn create_web_push_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<WebPushSubscriptionRequest>,
) -> Result<Json<WebPushSubscriptionResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    state
        .db
        .upsert_web_push_subscription(&req.endpoint, &req.keys.p256dh, &req.keys.auth, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(WebPushSubscriptionResponse { ok: true }))
}

async fn delete_web_push_subscription(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteWebPushSubscriptionRequest>,
) -> Result<Json<WebPushSubscriptionResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let deleted = state
        .db
        .delete_web_push_subscription(&req.endpoint)
        .await
        .map_err(map_internal)?;
    Ok(Json(WebPushSubscriptionResponse { ok: deleted }))
}

async fn webhook_trigger(
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

            state
                .db
                .insert_job_log(
                    &job_id,
                    &JobLogLine {
                        ts: now.clone(),
                        level: "info".to_string(),
                        msg: "webhook check started".to_string(),
                    },
                )
                .await
                .map_err(map_internal)?;

            let host_platform =
                registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());
            let outcome = run_check_for_job(
                &state,
                &job_id,
                &scope,
                stack_id.as_deref(),
                service_id.as_deref(),
                &host_platform,
                &now,
            )
            .await;

            let finished_at = now_rfc3339().map_err(map_internal)?;
            match outcome {
                Ok(summary) => {
                    state
                        .db
                        .finish_job(&job_id, "success", &finished_at, &summary)
                        .await
                        .map_err(map_internal)?;
                    Ok(Json(WebhookTriggerResponse { job_id }))
                }
                Err(e) => {
                    let summary = json!({"error": format!("{e:?}")});
                    let _ = state
                        .db
                        .finish_job(&job_id, "failed", &finished_at, &summary)
                        .await;
                    Err(e)
                }
            }
        }
        WebhookAction::Update => {
            let update_req = TriggerUpdateRequest {
                scope,
                stack_id,
                service_id,
                target_tag: None,
                target_digest: None,
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

async fn get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let backup = state.db.get_backup_settings().await.map_err(map_internal)?;
    Ok(Json(SettingsResponse {
        backup,
        auth: AuthSettings {
            forward_header_name: state.config.auth_forward_header_name.to_string(),
            allow_anonymous_in_dev: state.config.auth_allow_anonymous_in_dev,
        },
    }))
}

async fn put_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PutSettingsRequest>,
) -> Result<Json<PutSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    state
        .db
        .put_backup_settings(&req.backup, &now)
        .await
        .map_err(map_internal)?;
    Ok(Json(PutSettingsResponse { ok: true }))
}

fn require_user(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    if let Some(value) = headers.get(&state.config.auth_forward_header_name) {
        let user = value.to_str().unwrap_or_default().trim().to_string();
        if !user.is_empty() {
            return Ok(user);
        }
    }

    if state.config.auth_allow_anonymous_in_dev {
        return Ok("anonymous".to_string());
    }

    Err(ApiError::auth_required())
}

fn validate_scope(
    scope: &JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
) -> Result<(), ApiError> {
    match scope {
        JobScope::All => Ok(()),
        JobScope::Stack => {
            if stack_id.unwrap_or_default().is_empty() {
                return Err(ApiError::invalid_argument(
                    "stackId is required for scope=stack",
                ));
            }
            Ok(())
        }
        JobScope::Service => {
            if service_id.unwrap_or_default().is_empty() {
                return Err(ApiError::invalid_argument(
                    "serviceId is required for scope=service",
                ));
            }
            Ok(())
        }
    }
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

fn map_internal(err: anyhow::Error) -> ApiError {
    tracing::error!(error = %err, "internal error");
    ApiError::internal("internal error").with_details(json!({"cause": err.to_string()}))
}

fn merge_secret(target: &mut Option<String>, existing: Option<String>) {
    let keep = match target.as_deref() {
        None => true,
        Some(v) => v == "******" || v.trim().is_empty(),
    };
    if keep {
        *target = existing;
    }
}
