pub mod types;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Context as _;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::github;
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
            "/api/github-packages/settings",
            get(get_github_packages_settings).put(put_github_packages_settings),
        )
        .route(
            "/api/github-packages/resolve",
            post(resolve_github_packages_target),
        )
        .route(
            "/api/github-packages/sync",
            post(sync_github_packages_webhooks),
        )
        .route(
            "/api/web-push/subscriptions",
            post(create_web_push_subscription).delete(delete_web_push_subscription),
        )
        .route("/api/webhooks/trigger", post(webhook_trigger))
        .route(
            "/api/webhooks/github-packages",
            post(github_packages_webhook),
        )
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
    let mut manifest_digest_cache: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();

    for stack_id in &stack_ids {
        let compose_project = state
            .db
            .get_stack_compose_project(stack_id)
            .await
            .map_err(map_internal)?;

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

            let runtime_digest = if let Some(project) = compose_project.as_deref() {
                docker_compose_service_runtime_digest(
                    state.as_ref(),
                    project,
                    &svc.name,
                    &repo_candidates(&img),
                )
                .await
                .ok()
                .flatten()
            } else {
                None
            };

            let is_ignored = |tag: &str| matchers.iter().any(|(_, m)| m.matches(tag));
            let candidate_non_ignored =
                candidates::select_candidate_tag(&svc.image_tag, &tags, is_ignored);
            let candidate_any = candidates::select_candidate_tag(&svc.image_tag, &tags, |_| false);
            let mut candidate_tag = candidate_non_ignored.or(candidate_any);

            let current_digest_registry = state
                .registry
                .get_manifest(&img, &svc.image_tag, host_platform)
                .await
                .ok()
                .and_then(|m| m.digest);
            let effective_current_digest =
                runtime_digest.clone().or(current_digest_registry.clone());
            // Persist the best-known digest so that pinned tags and offline/missing compose projects
            // don't lose observability just because the runtime digest is unavailable.
            let current_digest = effective_current_digest.clone();

            let (
                candidate_digest_for_infer,
                candidate_arch_match_for_infer,
                candidate_arch_json_for_infer,
            ) = if let Some(tag) = candidate_tag.as_deref() {
                match state.registry.get_manifest(&img, tag, host_platform).await {
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

            let mut candidate_digest = candidate_digest_for_infer;
            let mut candidate_arch_match = candidate_arch_match_for_infer;
            let mut candidate_arch_json = candidate_arch_json_for_infer;

            // If the candidate resolves to the same digest as current, there's no actionable update.
            //
            // Note: for floating tags (e.g. `latest`) and missing runtime digest, comparing against the
            // registry digest could be misleading (the tag may have already moved), so we only do the
            // "no update" fast-path when runtime digest is known OR the current tag is semver/pinned.
            let can_compare_current =
                runtime_digest.is_some() || ignore::is_strict_semver(&svc.image_tag);
            if can_compare_current
                && let (Some(cur), Some(cand)) = (
                    effective_current_digest.as_deref(),
                    candidate_digest.as_deref(),
                )
                && cur == cand
            {
                candidate_tag = None;
                candidate_digest = None;
                candidate_arch_match = None;
                candidate_arch_json = None;
            }

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

            let (current_resolved_tag, current_resolved_tags_json) = if let Some(runtime_digest) =
                runtime_digest.as_deref()
                && !ignore::is_strict_semver(&svc.image_tag)
            {
                let mut semver_tags: Vec<(semver::Version, String)> = tags
                    .iter()
                    .filter_map(|t| ignore::parse_version(t).map(|v| (v, t.clone())))
                    .collect();
                semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

                let mut resolved_tags: Vec<String> = Vec::new();
                for (_v, tag) in semver_tags.into_iter().take(60) {
                    let digest = if candidate_tag.as_deref().is_some_and(|c| c == tag.as_str())
                        && candidate_digest.is_some()
                    {
                        candidate_digest.clone()
                    } else {
                        let cache_key = format!("{}/{}:{}", img.registry, img.name, tag);
                        if let Some(v) = manifest_digest_cache.get(&cache_key) {
                            v.clone()
                        } else {
                            let v = state
                                .registry
                                .get_manifest(&img, &tag, host_platform)
                                .await
                                .ok()
                                .and_then(|m| m.digest);
                            manifest_digest_cache.insert(cache_key, v.clone());
                            v
                        }
                    };

                    if digest.as_deref().is_some_and(|d| d == runtime_digest) {
                        resolved_tags.push(tag);
                    }
                }

                resolved_tags.retain(|t| t != &svc.image_tag);
                let resolved_tag = resolved_tags.first().cloned();
                let resolved_tags_json = if resolved_tags.len() > 1 {
                    serde_json::to_string(&resolved_tags).ok()
                } else {
                    None
                };

                (resolved_tag, resolved_tags_json)
            } else {
                (None, None)
            };

            state
                .db
                .update_service_check_result(
                    &svc.id,
                    current_digest,
                    current_resolved_tag,
                    current_resolved_tags_json,
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

fn repo_candidates(img: &registry::ImageRef) -> Vec<String> {
    let mut out = Vec::<String>::new();
    out.push(format!("{}/{}", img.registry, img.name));
    if img.registry == "docker.io" {
        out.push(img.name.clone());
        if let Some(short) = img.name.strip_prefix("library/") {
            out.push(short.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

async fn docker_compose_service_runtime_digest(
    state: &AppState,
    compose_project: &str,
    compose_service: &str,
    repo_candidates: &[String],
) -> anyhow::Result<Option<String>> {
    use crate::runner::CommandSpec;

    let ps = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.project={compose_project}"),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.service={compose_service}"),
                ],
                env: Vec::new(),
            },
            std::time::Duration::from_secs(8),
        )
        .await?;

    if ps.status != 0 {
        return Err(anyhow::anyhow!(
            "docker ps failed status={} stderr={}",
            ps.status,
            ps.stderr
        ));
    }

    let container_ids = ps
        .stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if container_ids.is_empty() {
        return Ok(None);
    }

    let mut digests = std::collections::BTreeSet::<String>::new();
    for id in container_ids {
        let img_id = state
            .runner
            .run(
                CommandSpec {
                    program: "docker".to_string(),
                    args: vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{.Image}}".to_string(),
                        id,
                    ],
                    env: Vec::new(),
                },
                std::time::Duration::from_secs(10),
            )
            .await?;
        if img_id.status != 0 {
            continue;
        }
        let img_id = img_id.stdout.trim().to_string();
        if img_id.is_empty() {
            continue;
        }

        let inspect = state
            .runner
            .run(
                CommandSpec {
                    program: "docker".to_string(),
                    args: vec![
                        "image".to_string(),
                        "inspect".to_string(),
                        img_id,
                        "--format".to_string(),
                        "{{json .RepoDigests}}".to_string(),
                    ],
                    env: Vec::new(),
                },
                std::time::Duration::from_secs(10),
            )
            .await?;
        if inspect.status != 0 {
            continue;
        }

        let parsed = serde_json::from_str::<Vec<String>>(inspect.stdout.trim()).unwrap_or_default();
        for d in parsed {
            for repo in repo_candidates {
                if let Some(rest) = d.strip_prefix(&format!("{repo}@"))
                    && !rest.trim().is_empty()
                {
                    digests.insert(rest.trim().to_string());
                }
            }
        }
    }

    if digests.len() == 1 {
        Ok(digests.iter().next().cloned())
    } else {
        Ok(None)
    }
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
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

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
                    .get_manifest(&img, reference, &host_platform)
                    .await
                    .map_err(map_internal)?;
                let arch_match =
                    registry::compute_arch_match(host_platform.as_str(), &manifest.arch);
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
                    .get_manifest(&img, reference, &host_platform)
                    .await
                    .map_err(map_internal)?;
                let arch_match =
                    registry::compute_arch_match(host_platform.as_str(), &manifest.arch);
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
        match state
            .registry
            .get_manifest(&img, &tag, &host_platform)
            .await
        {
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

fn mask_if_some(input: &Option<String>) -> Option<String> {
    input.as_ref().map(|_| "******".to_string())
}

fn gen_webhook_secret() -> anyhow::Result<String> {
    let rng = ring::rand::SystemRandom::new();
    let mut buf = [0u8; 32];
    ring::rand::SecureRandom::fill(&rng, &mut buf)
        .map_err(|_| anyhow::anyhow!("failed to generate webhook secret"))?;
    Ok(base64::engine::general_purpose::STANDARD_NO_PAD.encode(buf))
}

fn normalize_github_repo_selection(
    repos: Vec<GitHubPackagesRepoSelection>,
) -> anyhow::Result<Vec<(String, String, bool)>> {
    use std::collections::BTreeMap;

    let mut merged: BTreeMap<(String, String), bool> = BTreeMap::new();
    for r in repos {
        let full = r.full_name.trim();
        if full.is_empty() {
            continue;
        }
        let mut parts = full.split('/');
        let owner = parts.next().unwrap_or_default().trim();
        let repo = parts.next().unwrap_or_default().trim();
        if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
            return Err(anyhow::anyhow!("invalid repo fullName: {full}"));
        }
        merged
            .entry((owner.to_string(), repo.to_string()))
            .and_modify(|v| *v = *v || r.selected)
            .or_insert(r.selected);
    }
    Ok(merged
        .into_iter()
        .map(|((o, r), selected)| (o, r, selected))
        .collect())
}

async fn get_github_packages_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GitHubPackagesSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let targets = state
        .db
        .list_github_packages_targets()
        .await
        .map_err(map_internal)?;
    let repos = state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?;

    Ok(Json(GitHubPackagesSettingsResponse {
        enabled: settings.enabled,
        callback_url: settings.callback_url,
        targets: targets
            .into_iter()
            .map(|t| GitHubPackagesTarget {
                input: t.input,
                kind: t.kind,
                owner: t.owner,
                warnings: t.warnings,
            })
            .collect(),
        repos: repos
            .into_iter()
            .map(|r| GitHubPackagesRepo {
                full_name: format!("{}/{}", r.owner, r.repo),
                selected: r.selected,
                hook_id: r.hook_id,
                last_sync_at: r.last_sync_at,
                last_error: r.last_error,
            })
            .collect(),
        pat_masked: mask_if_some(&settings.pat),
        secret_masked: mask_if_some(&settings.webhook_secret),
    }))
}

async fn put_github_packages_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PutGitHubPackagesSettingsRequest>,
) -> Result<Json<PutGitHubPackagesSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let _ = Url::parse(&req.callback_url)
        .map_err(|_| ApiError::invalid_argument("invalid callbackUrl"))?;

    let existing = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;

    let mut pat = req.pat;
    merge_secret(&mut pat, existing.pat);

    let mut webhook_secret = existing.webhook_secret;
    if webhook_secret.as_deref().unwrap_or_default().is_empty() {
        webhook_secret = Some(gen_webhook_secret().map_err(map_internal)?);
    }

    if req.enabled && pat.as_deref().unwrap_or_default().is_empty() {
        return Err(ApiError::invalid_argument(
            "pat is required when enabled=true",
        ));
    }

    let settings = GitHubPackagesSettingsDb {
        enabled: req.enabled,
        callback_url: req.callback_url,
        pat,
        webhook_secret,
        updated_at: Some(now.clone()),
    };

    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .map_err(map_internal)?;

    let mut targets = Vec::new();
    for t in req.targets {
        let kind = github::parse_target_input(&t.input).map_err(|e| {
            ApiError::invalid_argument("invalid target input")
                .with_details(json!({"input": t.input, "error": e.to_string()}))
        })?;
        let (kind_str, owner) = match kind {
            github::TargetKind::Owner { owner } => ("owner".to_string(), owner),
            github::TargetKind::Repo { owner, .. } => ("repo".to_string(), owner),
        };
        targets.push(GitHubPackagesTargetDb {
            id: ulid::Ulid::new().to_string(),
            input: t.input,
            kind: kind_str,
            owner,
            warnings: Vec::new(),
            updated_at: Some(now.clone()),
        });
    }
    state
        .db
        .put_github_packages_targets(&targets, &now)
        .await
        .map_err(map_internal)?;

    let repos = normalize_github_repo_selection(req.repos).map_err(|e| {
        ApiError::invalid_argument("invalid repos").with_details(json!({"error": e.to_string()}))
    })?;
    state
        .db
        .put_github_packages_repos(&repos, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(PutGitHubPackagesSettingsResponse { ok: true }))
}

async fn resolve_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ResolveGitHubPackagesTargetRequest>,
) -> Result<Json<ResolveGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let parsed = github::parse_target_input(&req.input).map_err(|e| {
        ApiError::invalid_argument("invalid input")
            .with_details(json!({"input": req.input, "error": e.to_string()}))
    })?;

    match parsed {
        github::TargetKind::Repo { owner, repo } => Ok(Json(ResolveGitHubPackagesTargetResponse {
            kind: "repo".to_string(),
            owner: owner.clone(),
            repos: vec![GitHubPackagesRepoSelection {
                full_name: format!("{owner}/{repo}"),
                selected: true,
            }],
            warnings: Vec::new(),
        })),
        github::TargetKind::Owner { owner } => {
            let settings = state
                .db
                .get_github_packages_settings()
                .await
                .map_err(map_internal)?;
            let Some(pat) = settings.pat else {
                return Err(ApiError::invalid_argument(
                    "pat is required before resolving owner",
                ));
            };
            let client = github::GitHubClient::new(&pat).map_err(map_internal)?;
            let repos = client
                .list_owner_repos(&owner)
                .await
                .map_err(map_internal)?;
            Ok(Json(ResolveGitHubPackagesTargetResponse {
                kind: "owner".to_string(),
                owner,
                repos: repos
                    .into_iter()
                    .map(|r| GitHubPackagesRepoSelection {
                        full_name: r.full_name,
                        selected: true,
                    })
                    .collect(),
                warnings: Vec::new(),
            }))
        }
    }
}

fn urls_match(a: &str, b: &str) -> bool {
    let Ok(au) = Url::parse(a) else { return false };
    let Ok(bu) = Url::parse(b) else { return false };
    au == bu
}

async fn sync_github_packages_webhooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SyncGitHubPackagesWebhooksRequest>,
) -> Result<Json<SyncGitHubPackagesWebhooksResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;

    if !settings.enabled {
        return Err(ApiError::invalid_argument(
            "github packages webhook is disabled",
        ));
    }
    let Some(pat) = settings.pat.clone() else {
        return Err(ApiError::invalid_argument("pat is required"));
    };
    let Some(secret) = settings.webhook_secret.clone() else {
        return Err(ApiError::internal("webhook secret missing"));
    };
    if settings.callback_url.trim().is_empty() {
        return Err(ApiError::invalid_argument("callbackUrl is required"));
    }
    let _ = Url::parse(&settings.callback_url)
        .map_err(|_| ApiError::invalid_argument("invalid callbackUrl"))?;

    let selected_repos: Vec<(String, String)> = state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
        .map(|r| (r.owner, r.repo))
        .collect();

    let client = github::GitHubClient::new(&pat).map_err(map_internal)?;
    let mut results = Vec::new();

    let mut conflict_instructions =
        std::collections::BTreeMap::<String, ResolveGitHubPackagesConflicts>::new();
    if let Some(items) = req.resolve_conflicts {
        for i in items {
            conflict_instructions.insert(i.repo.clone(), i);
        }
    }

    let dry_run = req.dry_run.unwrap_or(false);

    for (owner, repo) in selected_repos {
        let full = format!("{owner}/{repo}");

        if let Some(instr) = conflict_instructions.get(&full) {
            if !dry_run {
                for hid in &instr.delete_hook_ids {
                    let _ = client.delete_repo_hook(&owner, &repo, *hid).await;
                }
            }
        }

        let hooks = match client.list_repo_hooks(&owner, &repo).await {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string();
                let _ = state
                    .db
                    .set_github_packages_repo_sync_result(
                        &owner,
                        &repo,
                        None,
                        None,
                        Some(&msg),
                        &now,
                    )
                    .await;
                results.push(SyncGitHubPackagesWebhookResult {
                    repo: full,
                    action: "error".to_string(),
                    hook_id: None,
                    conflict_hooks: None,
                    message: Some(msg),
                });
                continue;
            }
        };

        let mut matches = Vec::new();
        for h in &hooks {
            let Some(url) = h.config.url.as_deref() else {
                continue;
            };
            if urls_match(url, &settings.callback_url) && h.events.iter().any(|e| e == "package") {
                matches.push(h);
            }
        }

        if matches.len() > 1 {
            let conflict_hooks = matches
                .into_iter()
                .map(|h| GitHubPackagesConflictHook {
                    id: h.id,
                    url: h.config.url.clone().unwrap_or_default(),
                    events: h.events.clone(),
                    active: h.active,
                })
                .collect::<Vec<_>>();
            let msg = "multiple matching webhooks found".to_string();
            let _ = state
                .db
                .set_github_packages_repo_sync_result(&owner, &repo, None, None, Some(&msg), &now)
                .await;
            results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "conflict".to_string(),
                hook_id: None,
                conflict_hooks: Some(conflict_hooks),
                message: Some(msg),
            });
            continue;
        }

        if matches.is_empty() {
            if dry_run {
                results.push(SyncGitHubPackagesWebhookResult {
                    repo: full,
                    action: "created".to_string(),
                    hook_id: None,
                    conflict_hooks: None,
                    message: Some("dryRun: would create".to_string()),
                });
                continue;
            }

            let created = client
                .create_repo_hook(
                    &owner,
                    &repo,
                    &github::CreateWebhookRequest {
                        name: "web",
                        active: true,
                        events: vec!["package"],
                        config: github::CreateWebhookConfig {
                            url: &settings.callback_url,
                            content_type: "json",
                            secret: &secret,
                            insecure_ssl: "0",
                        },
                    },
                )
                .await;
            match created {
                Ok(h) => {
                    let _ = state
                        .db
                        .set_github_packages_repo_sync_result(
                            &owner,
                            &repo,
                            Some(h.id),
                            Some(&now),
                            None,
                            &now,
                        )
                        .await;
                    results.push(SyncGitHubPackagesWebhookResult {
                        repo: full,
                        action: "created".to_string(),
                        hook_id: Some(h.id),
                        conflict_hooks: None,
                        message: None,
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = state
                        .db
                        .set_github_packages_repo_sync_result(
                            &owner,
                            &repo,
                            None,
                            None,
                            Some(&msg),
                            &now,
                        )
                        .await;
                    results.push(SyncGitHubPackagesWebhookResult {
                        repo: full,
                        action: "error".to_string(),
                        hook_id: None,
                        conflict_hooks: None,
                        message: Some(msg),
                    });
                }
            }
            continue;
        }

        let existing = matches[0];
        let needs_update = !existing.active || !existing.events.iter().any(|e| e == "package");
        if !needs_update {
            let _ = state
                .db
                .set_github_packages_repo_sync_result(
                    &owner,
                    &repo,
                    Some(existing.id),
                    Some(&now),
                    None,
                    &now,
                )
                .await;
            results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "noop".to_string(),
                hook_id: Some(existing.id),
                conflict_hooks: None,
                message: None,
            });
            continue;
        }

        if dry_run {
            results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "updated".to_string(),
                hook_id: Some(existing.id),
                conflict_hooks: None,
                message: Some("dryRun: would update".to_string()),
            });
            continue;
        }

        let updated = client
            .update_repo_hook(
                &owner,
                &repo,
                existing.id,
                &github::UpdateWebhookRequest {
                    active: true,
                    events: vec!["package"],
                    config: github::UpdateWebhookConfig {
                        url: &settings.callback_url,
                        content_type: "json",
                        secret: &secret,
                        insecure_ssl: "0",
                    },
                },
            )
            .await;
        match updated {
            Ok(h) => {
                let _ = state
                    .db
                    .set_github_packages_repo_sync_result(
                        &owner,
                        &repo,
                        Some(h.id),
                        Some(&now),
                        None,
                        &now,
                    )
                    .await;
                results.push(SyncGitHubPackagesWebhookResult {
                    repo: full,
                    action: "updated".to_string(),
                    hook_id: Some(h.id),
                    conflict_hooks: None,
                    message: None,
                });
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = state
                    .db
                    .set_github_packages_repo_sync_result(
                        &owner,
                        &repo,
                        None,
                        None,
                        Some(&msg),
                        &now,
                    )
                    .await;
                results.push(SyncGitHubPackagesWebhookResult {
                    repo: full,
                    action: "error".to_string(),
                    hook_id: None,
                    conflict_hooks: None,
                    message: Some(msg),
                });
            }
        }
    }

    Ok(Json(SyncGitHubPackagesWebhooksResponse {
        ok: results
            .iter()
            .all(|r| r.action != "error" && r.action != "conflict"),
        results,
    }))
}

fn verify_github_signature(secret: &str, sig_header: &str, body: &[u8]) -> anyhow::Result<()> {
    let header = sig_header.trim();
    let hex = header
        .strip_prefix("sha256=")
        .context("signature must start with sha256=")?;
    let tag = hex::decode(hex).context("invalid signature hex")?;
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    ring::hmac::verify(&key, body, &tag).map_err(|_| anyhow::anyhow!("signature mismatch"))?;
    Ok(())
}

fn extract_repo_full_name(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("repository")
        .and_then(|v| v.get("full_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            payload
                .get("package")
                .and_then(|p| p.get("repository"))
                .and_then(|v| v.get("full_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

fn extract_owner_login(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("organization")
        .and_then(|v| v.get("login"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            payload
                .get("repository")
                .and_then(|v| v.get("owner"))
                .and_then(|v| v.get("login"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            payload
                .get("sender")
                .and_then(|v| v.get("login"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
}

async fn github_packages_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if event != "package" {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "not_package_event"}),
        ));
    }

    let delivery_id = headers
        .get("X-GitHub-Delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if delivery_id.is_empty() {
        return Err(ApiError::invalid_argument("missing X-GitHub-Delivery"));
    }

    let sig = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let Some(secret) = settings.webhook_secret else {
        return Err(ApiError::unauthorized()
            .with_details(json!({"reason":"webhook_secret_not_configured"})));
    };
    if verify_github_signature(&secret, &sig, &body).is_err() {
        return Err(ApiError::unauthorized().with_details(json!({"reason":"invalid_signature"})));
    }

    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| ApiError::invalid_argument("invalid json"))?;
    let action = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if action != "published" {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "not_published"}),
        ));
    }

    let repo_full_name = extract_repo_full_name(&payload);
    let owner = repo_full_name
        .as_deref()
        .and_then(|s| s.split('/').next().map(|v| v.to_string()))
        .or_else(|| extract_owner_login(&payload));

    let is_new = state
        .db
        .insert_github_packages_delivery_if_new(
            &delivery_id,
            &now_rfc3339().map_err(map_internal)?,
            owner.as_deref(),
            repo_full_name.as_deref().and_then(|s| s.split('/').nth(1)),
        )
        .await
        .map_err(map_internal)?;
    if !is_new {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "duplicate_delivery"}),
        ));
    }

    let selected = state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
        .map(|r| format!("{}/{}", r.owner, r.repo))
        .collect::<std::collections::BTreeSet<_>>();

    let should_trigger = if let Some(full) = &repo_full_name {
        selected.contains(full)
    } else if let Some(owner) = &owner {
        selected.iter().any(|r| r.starts_with(&format!("{owner}/")))
    } else {
        false
    };

    if !should_trigger {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "repo_not_selected"}),
        ));
    }

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
    job_db.created_by = "github".to_string();
    job_db.reason = "github_webhook".to_string();
    state.db.insert_job(job_db).await.map_err(map_internal)?;

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_repo_full_name = repo_full_name.clone();
    tokio::spawn(async move {
        let outcome = discovery::run_scan(run_state.as_ref()).await;
        let finished_at =
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        match outcome {
            Ok(resp) => {
                let summary =
                    json!({ "scan": resp, "source": "github_webhook", "repo": run_repo_full_name });
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
                let summary = json!({ "error": e.to_string(), "source": "github_webhook" });
                let _ = run_state
                    .db
                    .finish_job(&run_job_id, "failed", &finished_at, &summary)
                    .await;
            }
        }
    });

    Ok(Json(json!({"ok": true, "jobId": job_id})))
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
