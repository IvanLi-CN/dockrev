pub mod types;

#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, convert::Infallible, sync::Arc, time::Duration};

use anyhow::Context as _;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;
use url::Url;

use crate::github;
use crate::{
    backup, discovery, error::ApiError, ghcr_webhook_jobs, ids, ignore, notify, preflight,
    registry, runtime_scan, snapshot_worker, state::AppState, ui, updater,
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
            "/api/services/{service_id}/digest-tags",
            get(list_service_digest_tags),
        )
        .route(
            "/api/services/{service_id}/digest-tags-snapshot",
            get(get_service_digest_tags_snapshot),
        )
        .route(
            "/api/services/{service_id}/version-inference/refresh",
            post(trigger_service_version_inference_refresh),
        )
        .route(
            "/api/version-inference/overview",
            get(get_version_inference_overview),
        )
        .route(
            "/api/version-inference/events",
            get(version_inference_events),
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
        .route("/api/runtime-scans", post(trigger_runtime_scan))
        .route("/api/updates", post(trigger_update))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/events", get(jobs_events))
        .route("/api/jobs/{job_id}", get(get_job))
        .route("/api/jobs/{job_id}/events", get(job_events))
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
            "/api/github-packages/repos",
            get(list_github_packages_repos),
        )
        .route(
            "/api/github-packages/repos/selected",
            post(set_github_packages_repo_selected),
        )
        .route(
            "/api/github-packages/repos/delete",
            post(delete_github_packages_repo),
        )
        .route(
            "/api/github-packages/repos/bulk-selected",
            post(bulk_set_github_packages_repos_selected),
        )
        .route(
            "/api/github-packages/targets/add",
            post(add_github_packages_target),
        )
        .route(
            "/api/github-packages/targets/remove",
            post(remove_github_packages_target),
        )
        .route(
            "/api/github-packages/resolve",
            post(resolve_github_packages_target),
        )
        .route(
            "/api/github-packages/webhook/overview",
            get(get_github_packages_webhook_overview),
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
        .route("/api/deploy-check/report", get(get_deploy_check_report))
        .route(
            "/api/deploy-welcome",
            get(get_deploy_welcome).put(put_deploy_welcome),
        )
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

async fn enqueue_snapshot_for_image_ref(
    state: &Arc<AppState>,
    image_ref: &str,
    digest: &str,
    host_platform: &str,
    reason: &str,
) {
    let Some(repo) = snapshot_worker::image_repo_from_image_ref(image_ref) else {
        return;
    };
    let Some(normalized) = snapshot_worker::normalize_digest(digest) else {
        return;
    };
    state
        .snapshot_worker
        .enqueue(&repo, &normalized, host_platform, reason)
        .await;
}

fn needs_version_inference(service: &Service) -> bool {
    if !ignore::is_strict_semver(&service.image.tag) {
        return true;
    }
    service
        .candidate
        .as_ref()
        .is_some_and(|c| !ignore::is_strict_semver(&c.tag))
}

const VERSION_INFERENCE_REASON_CACHE_MISS: &str = "cache_miss";
const VERSION_INFERENCE_REASON_CACHE_STALE: &str = "cache_stale";
const VERSION_INFERENCE_REASON_ALL_FAILED: &str = "all_failed";
const VERSION_INFERENCE_REASON_NEW_VERSION: &str = "new_version";
const VERSION_INFERENCE_REASON_FORCE: &str = "force";
const VERSION_INFERENCE_REASON_RUNNING: &str = "running";
const VERSION_INFERENCE_REASON_NOT_REQUIRED: &str = "not_required";

fn checked_at_is_older_than(checked_at: &str, min_age: time::Duration) -> bool {
    let parsed =
        time::OffsetDateTime::parse(checked_at, &time::format_description::well_known::Rfc3339);
    let now = time::OffsetDateTime::now_utc();
    match parsed {
        Ok(ts) => now - ts > min_age,
        Err(_) => true,
    }
}

fn checked_at_is_stale(checked_at: &str) -> bool {
    checked_at_is_older_than(
        checked_at,
        time::Duration::days(snapshot_worker::SNAPSHOT_CACHE_TTL_DAYS),
    )
}

fn checked_at_is_retryable_all_failed(checked_at: Option<&str>) -> bool {
    checked_at.is_none_or(|ts| {
        checked_at_is_older_than(
            ts,
            time::Duration::minutes(snapshot_worker::SNAPSHOT_ALL_FAILED_RETRY_MINUTES),
        )
    })
}

fn needs_version_inference_for_tags(current_tag: &str, candidate_tag: Option<&str>) -> bool {
    if !ignore::is_strict_semver(current_tag) {
        return true;
    }
    candidate_tag.is_some_and(|tag| !ignore::is_strict_semver(tag))
}

#[derive(Clone)]
struct DigestSnapshotCacheValue {
    snapshot: ServiceDigestTagsSnapshotResponse,
    checked_at: String,
}

fn infer_semver_tags_from_snapshot(
    snapshot: &ServiceDigestTagsSnapshotResponse,
    raw_tag: &str,
) -> Vec<String> {
    let mut semver_tags = snapshot
        .tags
        .iter()
        .filter_map(|tag| ignore::parse_version(tag).map(|v| (v, tag.clone())))
        .collect::<Vec<_>>();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let raw_trim = raw_tag.trim();
    semver_tags
        .into_iter()
        .map(|(_, tag)| tag)
        .filter(|tag| tag != raw_trim)
        .collect()
}

fn checked_at_latest(mut existing: Option<String>, candidate: Option<&str>) -> Option<String> {
    let Some(candidate) = candidate else {
        return existing;
    };
    if existing.as_deref().is_none_or(|cur| candidate > cur) {
        existing = Some(candidate.to_string());
    }
    existing
}

fn parse_digest_snapshot_row(
    snapshot_json: &str,
    checked_at: &str,
) -> Option<DigestSnapshotCacheValue> {
    let mut parsed =
        serde_json::from_str::<ServiceDigestTagsSnapshotResponse>(snapshot_json).ok()?;
    if parsed.checked_at.trim().is_empty() {
        parsed.checked_at = checked_at.to_string();
    }
    Some(DigestSnapshotCacheValue {
        snapshot: parsed,
        checked_at: checked_at.to_string(),
    })
}

async fn enrich_stack_with_version_inference(
    state: &Arc<AppState>,
    stack: &mut StackRecord,
) -> Result<(), ApiError> {
    use std::collections::HashMap;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut snapshot_cache: HashMap<String, Option<DigestSnapshotCacheValue>> = HashMap::new();
    let mut inflight_cache: HashMap<String, Option<String>> = HashMap::new();

    for svc in stack.services.iter_mut() {
        if !needs_version_inference(svc) {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        }

        let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&svc.image.reference)
        else {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        };

        let mut digest_targets: Vec<(String, bool)> = Vec::new();
        if !ignore::is_strict_semver(&svc.image.tag)
            && let Some(current_digest) = svc
                .image
                .digest
                .as_deref()
                .and_then(snapshot_worker::normalize_digest)
        {
            digest_targets.push((current_digest, false));
        }
        if let Some(candidate) = svc.candidate.as_ref()
            && !ignore::is_strict_semver(&candidate.tag)
            && let Some(candidate_digest) = snapshot_worker::normalize_digest(&candidate.digest)
            && !digest_targets
                .iter()
                .any(|(digest, _)| digest == &candidate_digest)
        {
            digest_targets.push((candidate_digest, true));
        }

        if digest_targets.is_empty() {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        }

        let mut pending = false;
        let mut pending_reason: Option<String> = None;
        let mut ready_reason: Option<String> = None;
        let mut latest_checked_at: Option<String> = None;

        for (digest, for_candidate) in digest_targets {
            let digest_key = format!("{image_repo}@{digest}@{host_platform}");

            let snapshot_entry = if let Some(cached) = snapshot_cache.get(&digest_key) {
                cached.clone()
            } else {
                let row = state
                    .db
                    .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
                    .await
                    .map_err(map_internal)?;
                let parsed = row
                    .as_ref()
                    .and_then(|(snapshot_json, checked_at, _updated_at)| {
                        parse_digest_snapshot_row(snapshot_json, checked_at)
                    });
                snapshot_cache.insert(digest_key.clone(), parsed.clone());
                parsed
            };

            let mut enqueue_reason: Option<&str> = None;
            if let Some(snapshot_entry) = snapshot_entry.as_ref() {
                latest_checked_at =
                    checked_at_latest(latest_checked_at, Some(snapshot_entry.checked_at.as_str()));
                let snapshot_all_failed =
                    snapshot_worker::snapshot_is_all_failed(&snapshot_entry.snapshot);
                if checked_at_is_stale(&snapshot_entry.checked_at) {
                    enqueue_reason = Some(VERSION_INFERENCE_REASON_CACHE_STALE);
                } else if snapshot_all_failed {
                    ready_reason
                        .get_or_insert_with(|| VERSION_INFERENCE_REASON_ALL_FAILED.to_string());
                    if checked_at_is_retryable_all_failed(Some(snapshot_entry.checked_at.as_str()))
                    {
                        enqueue_reason = Some(VERSION_INFERENCE_REASON_ALL_FAILED);
                    }
                }
            } else {
                enqueue_reason = Some(VERSION_INFERENCE_REASON_CACHE_MISS);
            }

            if let Some(reason) = enqueue_reason {
                pending = true;
                let _ = state
                    .snapshot_worker
                    .enqueue(&image_repo, &digest, &host_platform, reason)
                    .await;
                pending_reason.get_or_insert_with(|| reason.to_string());
            }

            let in_flight_reason = if let Some(reason) = inflight_cache.get(&digest_key) {
                reason.clone()
            } else {
                let reason = state
                    .snapshot_worker
                    .in_flight_reason(&image_repo, &digest, &host_platform)
                    .await;
                inflight_cache.insert(digest_key.clone(), reason.clone());
                reason
            };
            if let Some(in_flight_reason) = in_flight_reason {
                pending = true;
                pending_reason.get_or_insert(in_flight_reason);
            }

            if let Some(snapshot_entry) = snapshot_entry.as_ref() {
                let tags = if for_candidate {
                    let raw = svc
                        .candidate
                        .as_ref()
                        .map(|candidate| candidate.tag.as_str())
                        .unwrap_or_default();
                    infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, raw)
                } else {
                    infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, &svc.image.tag)
                };
                let inferred_first = tags.first().cloned();
                if for_candidate {
                    if let Some(candidate) = svc.candidate.as_mut()
                        && let Some(inferred) = inferred_first
                    {
                        candidate.resolved_tag = Some(inferred);
                    }
                } else if let Some(inferred) = inferred_first {
                    svc.image.resolved_tag = Some(inferred);
                    svc.image.resolved_tags = if tags.len() > 1 { Some(tags) } else { None };
                }
            }
        }

        let status = if pending { "pending" } else { "ready" };
        let reason = if pending {
            Some(pending_reason.unwrap_or_else(|| VERSION_INFERENCE_REASON_RUNNING.to_string()))
        } else {
            ready_reason
        };

        svc.version_inference = Some(VersionInferenceState {
            status: status.to_string(),
            reason,
            checked_at: latest_checked_at,
        });
    }

    Ok(())
}

async fn get_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<GetStackResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(mut stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };
    enrich_stack_with_version_inference(&state, &mut stack).await?;

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

const CHECK_PROGRESS_LOG_INTERVAL: Duration = Duration::from_millis(500);
const CHECK_PARALLELISM: usize = crate::config::FIXED_CHECK_PARALLELISM;
const CHECK_SPAWN_STAGGER: Duration = Duration::from_secs(1);
const UPDATE_STACK_BASE_PROGRESS: f64 = 0.15;
const UPDATE_STACK_APPLY_SPAN: f64 = 0.80;

fn progress_percent(current: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / total).min(100)
}

fn make_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    make_job_progress_with_percent(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        progress_percent(current, total),
    )
}

fn make_job_progress_with_percent(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
    percent: u32,
) -> JobProgress {
    make_job_progress_with_percent_and_plan(
        phase,
        message,
        current,
        total,
        current_target,
        updated_at,
        percent,
        current,
        total,
        percent,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_job_progress_with_percent_and_plan(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
    percent: u32,
    planned_current: u32,
    planned_total: u32,
    planned_percent: u32,
) -> JobProgress {
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent: percent.min(100),
        planned_current: Some(planned_current),
        planned_total: Some(planned_total),
        planned_percent: Some(planned_percent.min(100)),
        current_target,
        updated_at,
    }
}

fn make_check_job_progress(
    message: String,
    completed: u32,
    planned: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    make_job_progress_with_percent_and_plan(
        "scanning",
        message,
        completed,
        total,
        current_target,
        updated_at,
        progress_percent(completed, total),
        planned,
        total,
        progress_percent(planned, total),
    )
}

fn update_progress_percent(processed_stacks: u32, total_stacks: u32, stack_fraction: f64) -> u32 {
    if total_stacks == 0 {
        return 0;
    }
    let stack_fraction = stack_fraction.clamp(0.0, 1.0);
    let overall = ((processed_stacks as f64) + stack_fraction) / (total_stacks as f64);
    (overall.clamp(0.0, 1.0) * 100.0).floor() as u32
}

fn update_apply_fraction(evt: &updater::UpdateProgressEvent) -> f64 {
    use updater::UpdateProgressStep as S;

    let service_total = evt.service_total.max(1);
    let service_index = evt.service_index.min(service_total.saturating_sub(1));
    let unit = 1.0 / service_total as f64;

    let step_fraction = match evt.step {
        S::ServiceStart => 0.02,
        S::PullStart => 0.08,
        S::PullProgress => {
            let f = evt.pull_fraction.unwrap_or(0.0).clamp(0.0, 1.0);
            0.08 + 0.42 * f
        }
        S::PullDone => 0.52,
        S::UpStart => 0.60,
        S::UpDone => 0.82,
        S::HealthStart => 0.86,
        S::HealthDone => 0.95,
        S::ServiceDone => 1.0,
    };

    ((service_index as f64) + step_fraction) * unit
}

async fn persist_job_progress(
    state: &Arc<AppState>,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let evt = json!({
        "type": "job_progress",
        "jobId": job_id,
        "ts": progress.updated_at,
        "phase": progress.phase,
        "message": progress.message,
        "current": progress.current,
        "total": progress.total,
        "percent": progress.percent,
        "plannedCurrent": progress.planned_current,
        "plannedTotal": progress.planned_total,
        "plannedPercent": progress.planned_percent,
        "currentTarget": progress.current_target,
        "updatedAt": progress.updated_at,
    });

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: progress.updated_at.clone(),
                level: "event".to_string(),
                msg: evt.to_string(),
            },
        )
        .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_check_worker_result(
    state: &Arc<AppState>,
    job_id: &str,
    now: &str,
    host_platform: &str,
    joined: Result<CheckWorkerResult, tokio::task::JoinError>,
    total_services: u32,
    planned_services: u32,
    services_checked: &mut u32,
    services_with_candidate: &mut u32,
    latest_target: &mut Option<String>,
    last_progress_logged_at: &mut Option<std::time::Instant>,
    latest_progress: &mut JobProgress,
) -> Result<(), ApiError> {
    let CheckWorkerResult {
        stack_id,
        service_name,
        service_image_ref,
        service_image_tag,
        outcome,
    } = joined.map_err(|e| map_internal(anyhow::anyhow!("check worker join failed: {e}")))?;

    *services_checked = (*services_checked).saturating_add(1);
    *latest_target = Some(format!("{stack_id}/{service_name}"));

    let outcome = outcome.map_err(map_internal)?;
    if outcome.candidate_present {
        *services_with_candidate = (*services_with_candidate).saturating_add(1);
    }
    if outcome.candidate_digest_changed
        && outcome.candidate_digest.is_some()
        && needs_version_inference_for_tags(&service_image_tag, outcome.candidate_tag.as_deref())
        && let Some(image_repo) =
            crate::snapshot_worker::image_repo_from_image_ref(&service_image_ref)
        && let Some(candidate_digest) = outcome
            .candidate_digest
            .as_deref()
            .and_then(snapshot_worker::normalize_digest)
    {
        let _ = state
            .snapshot_worker
            .enqueue(
                &image_repo,
                &candidate_digest,
                host_platform,
                VERSION_INFERENCE_REASON_NEW_VERSION,
            )
            .await;
    }

    let now_instant = std::time::Instant::now();
    let should_emit = *services_checked == 1
        || *services_checked == total_services
        || last_progress_logged_at
            .map(|ts| now_instant.duration_since(ts) >= CHECK_PROGRESS_LOG_INTERVAL)
            .unwrap_or(true);
    if should_emit {
        *last_progress_logged_at = Some(now_instant);
        let updated_at = now_rfc3339().unwrap_or_else(|_| now.to_string());
        *latest_progress = make_check_job_progress(
            format!("checking services ({}/{total_services})", *services_checked),
            *services_checked,
            planned_services,
            total_services,
            (*latest_target).clone(),
            updated_at.clone(),
        );
        if let Err(e) = persist_job_progress(state, job_id, latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist check progress");
        }
        let _ = state
            .db
            .insert_job_log(
                job_id,
                &JobLogLine {
                    ts: updated_at,
                    level: "info".to_string(),
                    msg: format!(
                        "check progress: {}/{} ({}%) current={}",
                        latest_progress.current,
                        latest_progress.total,
                        latest_progress.percent,
                        latest_progress.current_target.as_deref().unwrap_or("-"),
                    ),
                },
            )
            .await;
    }

    Ok(())
}

#[derive(Debug)]
struct CheckWorkerResult {
    stack_id: String,
    service_name: String,
    service_image_ref: String,
    service_image_tag: String,
    outcome: anyhow::Result<crate::service_check::ServiceCheckOutcome>,
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

    // Prevent accidental parallel checks from UI double-clicks / multiple tabs.
    // If we detect a stale running check (likely orphaned by a restart), we terminate it and proceed.
    let stale_threshold = time::Duration::hours(2);
    if let Ok(Some(existing)) = state
        .db
        .find_latest_running_check_job(
            &req.scope,
            req.stack_id.as_deref(),
            req.service_id.as_deref(),
        )
        .await
    {
        let started_at = existing
            .started_at
            .as_deref()
            .unwrap_or(existing.created_at.as_str());
        let existing_stale =
            time::OffsetDateTime::parse(started_at, &time::format_description::well_known::Rfc3339)
                .ok()
                .and_then(|started| {
                    time::OffsetDateTime::parse(
                        &now,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .ok()
                    .map(|cur| cur - started)
                })
                .is_some_and(|age| age > stale_threshold);

        if existing_stale {
            let _ = state
                .db
                .terminate_job_as_failed(&existing.id, &now, "stale_check")
                .await;
        } else {
            return Err(
                ApiError::conflict("check already running").with_details(json!({
                    "existingJobId": existing.id,
                })),
            );
        }
    }

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

    // Run the check job in the background so it is not tied to the HTTP request lifecycle.
    // This avoids orphaned `running` jobs when the client disconnects or the gateway times out.
    let run_state = state.clone();
    let run_check_id = check_id.clone();
    let run_scope = req.scope.clone();
    let run_stack_id = req.stack_id.clone();
    let run_service_id = req.service_id.clone();
    let run_host_platform = host_platform.clone();
    let run_started_at = now.clone();
    tokio::spawn(async move {
        if let Err(e) = run_state
            .db
            .insert_job_log(
                &run_check_id,
                &JobLogLine {
                    ts: run_started_at.clone(),
                    level: "info".to_string(),
                    msg: "check started".to_string(),
                },
            )
            .await
        {
            tracing::warn!(job_id = %run_check_id, error = %e, "failed to insert check started log");
        }

        let outcome = run_check_for_job(
            &run_state,
            &run_check_id,
            &run_scope,
            run_stack_id.as_deref(),
            run_service_id.as_deref(),
            &run_host_platform,
            &run_started_at,
        )
        .await;

        let finished_at = match now_rfc3339() {
            Ok(ts) => ts,
            Err(err) => {
                tracing::warn!(
                    job_id = %run_check_id,
                    error = %err,
                    "failed to format finished_at as RFC3339; falling back to started_at"
                );
                run_started_at.clone()
            }
        };
        match outcome {
            Ok(summary) => {
                if let Err(e) = run_state
                    .db
                    .finish_job(&run_check_id, "success", &finished_at, &summary)
                    .await
                {
                    tracing::error!(job_id = %run_check_id, error = %e, "failed to finish check job");
                }
            }
            Err(e) => {
                if let Err(err) = run_state
                    .db
                    .insert_job_log(
                        &run_check_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: format!("check failed: {e:?}"),
                        },
                    )
                    .await
                {
                    tracing::warn!(job_id = %run_check_id, error = %err, "failed to insert check failure log");
                }
                let summary = json!({"error": format!("{e:?}")});
                if let Err(err) = run_state
                    .db
                    .finish_job(&run_check_id, "failed", &finished_at, &summary)
                    .await
                {
                    tracing::error!(job_id = %run_check_id, error = %err, "failed to finish failed check job");
                }
            }
        }
    });

    Ok(Json(TriggerCheckResponse { check_id }))
}

async fn trigger_runtime_scan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerRuntimeScanRequest>,
) -> Result<Json<TriggerRuntimeScanResponse>, ApiError> {
    let user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    // Prevent accidental parallel scans from UI double-clicks / multiple tabs.
    if let Ok(Some(existing)) = state
        .db
        .find_latest_running_runtime_scan_job(
            &req.scope,
            req.stack_id.as_deref(),
            req.service_id.as_deref(),
        )
        .await
    {
        return Err(
            ApiError::conflict("runtime scan already running").with_details(json!({
                "existingJobId": existing.id,
            })),
        );
    }

    let job_id = ids::new_job_id();
    let job = JobRecord::new_running(
        job_id.clone(),
        JobType::RuntimeScan,
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

    // Run the scan job in the background so it is not tied to the HTTP request lifecycle.
    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_scope = req.scope.clone();
    let run_stack_id = req.stack_id.clone();
    let run_service_id = req.service_id.clone();
    let run_host_platform = host_platform.clone();
    let run_started_at = now.clone();
    let run_reason = req.reason.as_str().to_string();
    tokio::spawn(async move {
        runtime_scan::run_job(
            run_state,
            runtime_scan::RuntimeScanJobArgs {
                job_id: run_job_id,
                scope: run_scope,
                stack_id: run_stack_id,
                service_id: run_service_id,
                host_platform: run_host_platform,
                started_at: run_started_at,
                reason: run_reason,
            },
        )
        .await;
    });

    Ok(Json(TriggerRuntimeScanResponse { job_id }))
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
    #[derive(Debug)]
    struct CheckUnit {
        stack_id: String,
        compose_project: Option<String>,
        service: crate::db::ServiceForCheck,
    }

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

    let mut units: std::collections::VecDeque<CheckUnit> = std::collections::VecDeque::new();

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
            units.push_back(CheckUnit {
                stack_id: stack_id.clone(),
                compose_project: compose_project.clone(),
                service: svc,
            });
        }
    }

    let total_services = units.len() as u32;
    let started_ts = now_rfc3339().unwrap_or_else(|_| now.to_string());
    let mut latest_progress = make_job_progress(
        "prepare",
        format!("preparing check targets ({total_services} services)"),
        0,
        total_services,
        None,
        started_ts,
    );
    if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial check progress");
    }

    let mut join_set: JoinSet<CheckWorkerResult> = JoinSet::new();

    let mut planned_services = 0u32;
    let mut services_checked = 0u32;
    let mut services_with_candidate = 0u32;
    let mut last_progress_logged_at: Option<std::time::Instant> = None;
    let mut latest_target: Option<String> = None;
    let mut next_spawn_not_before: Option<std::time::Instant> = None;
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    while services_checked < total_services {
        // Drain any ready workers first so completed progress stays responsive.
        while let Some(joined) = join_set.try_join_next() {
            handle_check_worker_result(
                state,
                job_id,
                now,
                host_platform,
                joined,
                total_services,
                planned_services,
                &mut services_checked,
                &mut services_with_candidate,
                &mut latest_target,
                &mut last_progress_logged_at,
                &mut latest_progress,
            )
            .await?;
        }

        if join_set.len() >= CHECK_PARALLELISM {
            if let Some(joined) = join_set.join_next().await {
                handle_check_worker_result(
                    state,
                    job_id,
                    now,
                    host_platform,
                    joined,
                    total_services,
                    planned_services,
                    &mut services_checked,
                    &mut services_with_candidate,
                    &mut latest_target,
                    &mut last_progress_logged_at,
                    &mut latest_progress,
                )
                .await?;
            }
            continue;
        }

        let Some(unit) = units.pop_front() else {
            if let Some(joined) = join_set.join_next().await {
                handle_check_worker_result(
                    state,
                    job_id,
                    now,
                    host_platform,
                    joined,
                    total_services,
                    planned_services,
                    &mut services_checked,
                    &mut services_with_candidate,
                    &mut latest_target,
                    &mut last_progress_logged_at,
                    &mut latest_progress,
                )
                .await?;
            }
            continue;
        };

        if let Some(not_before) = next_spawn_not_before
            && let Some(wait) = not_before.checked_duration_since(std::time::Instant::now())
        {
            if join_set.is_empty() {
                tokio::time::sleep(wait).await;
            } else {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    Some(joined) = join_set.join_next() => {
                        handle_check_worker_result(
                            state,
                            job_id,
                            now,
                            host_platform,
                            joined,
                            total_services,
                            planned_services,
                            &mut services_checked,
                            &mut services_with_candidate,
                            &mut latest_target,
                            &mut last_progress_logged_at,
                            &mut latest_progress,
                        ).await?;
                        units.push_front(unit);
                        continue;
                    }
                }
            }
        }

        let spawn_state = state.clone();
        let spawn_job_id = job_id.to_string();
        let spawn_host_platform = host_platform.to_string();
        let spawn_now = now.to_string();
        let spawn_manifest_digest_cache = manifest_digest_cache.clone();
        let spawn_repo_tags_cache = repo_tags_cache.clone();
        join_set.spawn(async move {
            let stack_id = unit.stack_id.clone();
            let service_name = unit.service.name.clone();
            let service_image_ref = unit.service.image_ref.clone();
            let service_image_tag = unit.service.image_tag.clone();
            let runtime_digest = match (
                unit.compose_project.as_deref(),
                registry::ImageRef::parse(&unit.service.image_ref),
            ) {
                (Some(project), Ok(img)) => docker_compose_service_runtime_digest(
                    spawn_state.as_ref(),
                    project,
                    &unit.service.name,
                    &repo_candidates(&img),
                )
                .await
                .ok()
                .flatten(),
                _ => None,
            };
            let outcome = crate::service_check::check_service_and_persist(
                &spawn_state,
                &spawn_job_id,
                &unit.service,
                runtime_digest,
                &spawn_host_platform,
                &spawn_now,
                &spawn_manifest_digest_cache,
                &spawn_repo_tags_cache,
            )
            .await;
            CheckWorkerResult {
                stack_id,
                service_name,
                service_image_ref,
                service_image_tag,
                outcome,
            }
        });

        planned_services = planned_services.saturating_add(1);
        next_spawn_not_before = Some(std::time::Instant::now() + CHECK_SPAWN_STAGGER);
        let updated_at = now_rfc3339().unwrap_or_else(|_| now.to_string());
        latest_progress = make_check_job_progress(
            format!("scheduled checks ({planned_services}/{total_services})"),
            services_checked,
            planned_services,
            total_services,
            latest_target.clone(),
            updated_at,
        );
        if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist check scheduling progress");
        }
    }

    for stack_id in &stack_ids {
        state
            .db
            .update_stack_last_check_at(stack_id, now)
            .await
            .map_err(map_internal)?;
    }

    let finished_ts = now_rfc3339().unwrap_or_else(|_| now.to_string());
    latest_progress = make_job_progress_with_percent_and_plan(
        "done",
        "check finished".to_string(),
        services_checked,
        total_services,
        latest_target,
        finished_ts.clone(),
        progress_percent(services_checked, total_services),
        planned_services,
        total_services,
        progress_percent(planned_services, total_services),
    );
    if let Err(e) = persist_job_progress(state, job_id, &latest_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist final check progress");
    }

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: finished_ts,
                level: "info".to_string(),
                msg: format!(
                    "check finished: servicesChecked={services_checked} servicesWithCandidate={services_with_candidate}"
                ),
            },
        )
        .await
        .map_err(map_internal)?;

    let progress_json = serde_json::to_value(&latest_progress)
        .map_err(anyhow::Error::from)
        .map_err(map_internal)?;
    Ok(json!({
        "hostPlatform": host_platform,
        "scope": scope.as_str(),
        "stackIds": stack_ids,
        "servicesChecked": services_checked,
        "servicesWithCandidate": services_with_candidate,
        "progress": progress_json,
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

    if req.scope == JobScope::Service
        && req
            .target_digest
            .as_deref()
            .is_none_or(|d| d.trim().is_empty())
    {
        return Err(ApiError::invalid_argument(
            "targetDigest is required for scope=service",
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
    let init_progress = make_job_progress(
        "prepare",
        "preparing update job".to_string(),
        0,
        0,
        None,
        now.clone(),
    );
    if let Err(e) = persist_job_progress(&state, &job_id, &init_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial update progress");
    }

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
    fn normalize_digest_for_compare(input: &str) -> Option<String> {
        let t = input.trim();
        if t.is_empty() {
            return None;
        }
        if t.contains(':') {
            return Some(t.to_string());
        }
        Some(format!("sha256:{t}"))
    }

    // For stack/all updates we intentionally skip arch-mismatch services (UI shows them as
    // non-actionable), so only enforce mismatch blocking and target locking for service updates.
    if req.scope != JobScope::Service {
        return Ok(());
    }

    let got_digest = normalize_digest_for_compare(req.target_digest.as_deref().unwrap_or_default());

    for stack_id in stack_ids {
        let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
            continue;
        };

        for svc in &stack.services {
            if req.service_id.as_deref().is_some_and(|id| id != svc.id) {
                continue;
            }

            // Cross-tag updates are not supported. If the client sends targetTag, it must match
            // the service's configured tag.
            if let Some(tag) = req.target_tag.as_deref()
                && tag.trim() != svc.image.tag.trim()
            {
                return Err(ApiError::invalid_argument(
                    "cross-tag updates are not supported (targetTag must match service image tag)",
                ));
            }

            // Enforce "update locks to scan result": targetDigest must match the latest persisted
            // candidate digest for this service.
            let expected_opt = svc
                .candidate
                .as_ref()
                .and_then(|c| normalize_digest_for_compare(&c.digest));
            let got_opt = got_digest.clone();
            let (Some(expected), Some(got)) = (expected_opt.clone(), got_opt.clone()) else {
                return Err(ApiError::conflict(
                    "target digest no longer matches latest scan (rescan required)",
                )
                .with_details(json!({
                    "serviceId": svc.id,
                    "expectedDigest": expected_opt,
                    "gotDigest": got_opt,
                })));
            };
            if expected != got {
                return Err(ApiError::conflict(
                    "target digest no longer matches latest scan (rescan required)",
                )
                .with_details(json!({
                    "serviceId": svc.id,
                    "expectedDigest": expected,
                    "gotDigest": got,
                })));
            }

            if !req.allow_arch_mismatch
                && svc
                    .candidate
                    .as_ref()
                    .is_some_and(|c| matches!(c.arch_match, ArchMatch::Mismatch))
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
type UpdateJobOutcome = (
    String,
    UpdateStackSummaries,
    UpdateBackupsToCleanup,
    JobProgress,
);

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
        let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
        let backup_settings = state.db.get_backup_settings().await?;
        let stack_ids = resolve_stack_ids_for_update(state.as_ref(), &req).await?;
        let total_stacks = stack_ids.len() as u32;

        let mut final_status = "success".to_string();
        let mut stack_summaries = Vec::new();
        let mut backups_to_cleanup: Vec<(String, u32)> = Vec::new();
        let mut processed_stacks = 0u32;
        let mut latest_progress = make_job_progress(
            "prepare",
            format!("preparing update targets ({total_stacks} stacks)"),
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        for stack_id in &stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await? else {
                processed_stacks = processed_stacks.saturating_add(1);
                latest_progress = make_job_progress(
                    "apply",
                    format!("skipped missing stack {stack_id}"),
                    processed_stacks,
                    total_stacks,
                    Some(stack_id.clone()),
                    now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                );
                if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "failed to persist update progress"
                    );
                }
                continue;
            };
            latest_progress = make_job_progress_with_percent(
                "backup",
                format!("processing stack {stack_id}"),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, 0.08),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let logging_runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
            };

            let mut stack_summary = serde_json::Map::new();
            stack_summary.insert("stackId".to_string(), json!(stack_id));
            let planned_selection = updater::select_update_services(
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
            );
            let skipped_version_anomaly = planned_selection.skipped_version_anomaly.clone();
            let no_actionable_services_after_anomaly_skip = req.mode.as_str() == "apply"
                && !req.reason.as_str().eq_ignore_ascii_case("ui")
                && planned_selection.services.is_empty()
                && !skipped_version_anomaly.is_empty();

            let mut backup_id_for_cleanup: Option<(String, u32)> = None;
            if req.mode.as_str() == "apply"
                && !no_actionable_services_after_anomaly_skip
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
                            processed_stacks = processed_stacks.saturating_add(1);
                            latest_progress = make_job_progress(
                                "apply",
                                format!("processed stacks ({processed_stacks}/{total_stacks})"),
                                processed_stacks,
                                total_stacks,
                                Some(stack_id.clone()),
                                now_rfc3339().unwrap_or_else(|_| {
                                    time::OffsetDateTime::now_utc().to_string()
                                }),
                            );
                            if let Err(err) =
                                persist_job_progress(&state, &job_id, &latest_progress).await
                            {
                                tracing::warn!(
                                    job_id = %job_id,
                                    error = %err,
                                    "failed to persist update progress"
                                );
                            }
                            break;
                        }
                    }
                }
            } else {
                stack_summary.insert(
                    "backup".to_string(),
                    if no_actionable_services_after_anomaly_skip {
                        json!({"status":"skipped","reason":"no_actionable_services_after_anomaly_skip"})
                    } else if req.mode.as_str() != "apply" {
                        json!({"status":"skipped","reason":"dry_run"})
                    } else {
                        json!({"status":"skipped","reason":"disabled"})
                    },
                );
            }

            latest_progress = make_job_progress_with_percent(
                "apply",
                format!("applying updates for stack {stack_id}"),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, UPDATE_STACK_BASE_PROGRESS),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::unbounded_channel::<updater::UpdateProgressEvent>();
            let progress_state = state.clone();
            let progress_job_id = job_id.clone();
            let progress_stack_id = stack_id.clone();
            let processed_stacks_for_progress = processed_stacks;
            let total_stacks_for_progress = total_stacks;
            let progress_task = tokio::spawn(async move {
                let mut last_percent = update_progress_percent(
                    processed_stacks_for_progress,
                    total_stacks_for_progress,
                    UPDATE_STACK_BASE_PROGRESS,
                );
                let mut last_emit = std::time::Instant::now()
                    .checked_sub(Duration::from_secs(5))
                    .unwrap_or_else(std::time::Instant::now);

                while let Some(evt) = progress_rx.recv().await {
                    let apply_fraction = update_apply_fraction(&evt);
                    let stack_fraction =
                        UPDATE_STACK_BASE_PROGRESS + UPDATE_STACK_APPLY_SPAN * apply_fraction;
                    let next_percent = update_progress_percent(
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        stack_fraction,
                    )
                    .max(last_percent);

                    let force_emit = matches!(
                        evt.step,
                        updater::UpdateProgressStep::PullDone
                            | updater::UpdateProgressStep::UpDone
                            | updater::UpdateProgressStep::HealthDone
                            | updater::UpdateProgressStep::ServiceDone
                    );
                    let should_emit = force_emit
                        || next_percent > last_percent
                        || last_emit.elapsed() >= Duration::from_millis(600);
                    if !should_emit {
                        continue;
                    }

                    last_percent = next_percent;
                    last_emit = std::time::Instant::now();
                    let updated_at = now_rfc3339()
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                    let progress_message = if evt.message.contains(&evt.service_name) {
                        evt.message
                    } else {
                        format!("{} · {}", evt.service_name, evt.message)
                    };
                    let progress = make_job_progress_with_percent(
                        "apply",
                        progress_message,
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        Some(progress_stack_id.clone()),
                        updated_at,
                        next_percent,
                    );
                    if let Err(e) =
                        persist_job_progress(&progress_state, &progress_job_id, &progress).await
                    {
                        tracing::warn!(
                            job_id = %progress_job_id,
                            error = %e,
                            "failed to persist streamed update progress"
                        );
                    }
                }
            });

            let update_outcome = updater::run_update_job(
                &logging_runner,
                &state.config.compose_bin,
                updater::IdempotentRetryPolicy {
                    max_attempts: state.config.update_idempotent_retry_max_attempts,
                    base_ms: state.config.update_idempotent_retry_base_ms,
                    max_ms: state.config.update_idempotent_retry_max_ms,
                },
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.mode.as_str(),
                req.target_tag.as_deref(),
                req.target_digest.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
                Some(progress_tx),
            )
            .await;
            let _ = progress_task.await;
            match update_outcome {
                Ok(outcome) => {
                    if let Some(changed_service_ids) =
                        extract_changed_service_ids(&outcome.summary_json)
                        && let Some(project) = state.db.get_stack_compose_project(stack_id).await?
                    {
                        for changed_service_id in changed_service_ids {
                            let Some(svc) = stack
                                .services
                                .iter()
                                .find(|svc| svc.id == changed_service_id)
                            else {
                                continue;
                            };
                            let Ok(img) = registry::ImageRef::parse(&svc.image.reference) else {
                                continue;
                            };
                            let runtime_digest = docker_compose_service_runtime_digest(
                                state.as_ref(),
                                &project,
                                &svc.name,
                                &repo_candidates(&img),
                            )
                            .await
                            .ok()
                            .flatten();
                            if let Some(runtime_digest) = runtime_digest {
                                enqueue_snapshot_for_image_ref(
                                    &state,
                                    &svc.image.reference,
                                    &runtime_digest,
                                    &host_platform,
                                    "update_digest_changed",
                                )
                                .await;
                            }
                        }
                    }
                    final_status = outcome.status.clone();
                    stack_summary.insert("update".to_string(), outcome.summary_json);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        format!("processed stacks ({processed_stacks}/{total_stacks})"),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to persist update progress"
                        );
                    }

                    if final_status != "success" {
                        break;
                    }

                    if let Some(b) = backup_id_for_cleanup.take() {
                        backups_to_cleanup.push(b);
                    }
                }
                Err(e) => {
                    final_status = "failed".to_string();
                    let mut update_summary = json!({"error": e.to_string()});
                    if let Some(step_failure) = e.downcast_ref::<updater::UpdateStepFailure>()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        if let Some(partial) = step_failure.partial_summary.as_ref()
                            && let Some(partial_obj) = partial.as_object()
                        {
                            for (key, value) in partial_obj {
                                obj.entry(key.clone()).or_insert_with(|| value.clone());
                            }
                        }
                        obj.insert(
                            "failureStep".to_string(),
                            json!(step_failure.step.clone()),
                        );
                        obj.insert("retry".to_string(), json!(step_failure.retry.clone()));
                        obj.insert(
                            "lastError".to_string(),
                            json!(step_failure.last_error.clone()),
                        );
                    }
                    if !skipped_version_anomaly.is_empty()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        obj.insert(
                            "skippedVersionAnomaly".to_string(),
                            serde_json::Value::Array(skipped_version_anomaly.clone()),
                        );
                    }
                    stack_summary.insert("update".to_string(), update_summary);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        format!("processed stacks ({processed_stacks}/{total_stacks})"),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(err) = persist_job_progress(&state, &job_id, &latest_progress).await
                    {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %err,
                            "failed to persist update progress"
                        );
                    }
                    break;
                }
            }
        }

        latest_progress = make_job_progress(
            "done",
            if final_status == "success" {
                "update finished".to_string()
            } else {
                "update finished with failures".to_string()
            },
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        Ok((
            final_status,
            stack_summaries,
            backups_to_cleanup,
            latest_progress,
        ))
    }
    .await;

    let (final_status, stack_summaries, backups_to_cleanup, final_summary, finished_at) =
        match outcome {
            Ok((final_status, stack_summaries, backups_to_cleanup, progress)) => {
                let progress_json = serde_json::to_value(&progress)?;
                let final_summary = json!({
                    "mode": req.mode.as_str(),
                    "stacks": stack_summaries.clone(),
                    "progress": progress_json,
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
                let progress = make_job_progress(
                    "done",
                    "update failed".to_string(),
                    0,
                    0,
                    None,
                    finished_at.clone(),
                );
                let progress_json = serde_json::to_value(&progress)?;
                let _ = persist_job_progress(&state, &job_id, &progress).await;
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
                    "progress": progress_json,
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

    async fn run_stream(
        &self,
        spec: crate::runner::CommandSpec,
        timeout: std::time::Duration,
        on_stdout: &mut (dyn FnMut(String) + Send),
        on_stderr: &mut (dyn FnMut(String) + Send),
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

        let mut captured_stdout = String::new();
        let mut captured_stderr = String::new();
        let mut tap_stdout = |chunk: String| {
            captured_stdout.push_str(&chunk);
            on_stdout(chunk);
        };
        let mut tap_stderr = |chunk: String| {
            captured_stderr.push_str(&chunk);
            on_stderr(chunk);
        };

        let out = self
            .inner
            .run_stream(spec, timeout, &mut tap_stdout, &mut tap_stderr)
            .await?;
        if captured_stdout.is_empty() {
            captured_stdout = out.stdout.clone();
        }
        if captured_stderr.is_empty() {
            captured_stderr = out.stderr.clone();
        }

        let ts = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        let msg = format!(
            "status={} stdout={} stderr={}",
            out.status,
            truncate(&captured_stdout, 2000),
            truncate(&captured_stderr, 2000)
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

        Ok(crate::runner::CommandOutput {
            status: out.status,
            stdout: captured_stdout,
            stderr: captured_stderr,
        })
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
struct JobEventsQuery {
    #[serde(default)]
    after_id: i64,
}

fn resolve_sse_after_id(headers: &HeaderMap, query_after_id: i64) -> i64 {
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let header_after_id: i64 = last_event_id.parse::<i64>().unwrap_or(0);
    std::cmp::max(header_after_id, query_after_id).max(0)
}

async fn jobs_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<JobEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers)?;
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

async fn job_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Query(q): Query<JobEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers)?;

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

async fn trigger_service_version_inference_refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Response, ApiError> {
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
    let Some(service) = stack.services.iter().find(|svc| svc.id == service_id) else {
        return Err(ApiError::not_found("service not found"));
    };
    let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&service.image.reference)
    else {
        return Err(ApiError::invalid_argument("invalid service image ref"));
    };
    let mut digests: Vec<String> = Vec::new();
    if let Some(current_digest) = service
        .image
        .digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
    {
        digests.push(current_digest);
    }
    if let Some(candidate_digest) = service
        .candidate
        .as_ref()
        .and_then(|candidate| snapshot_worker::normalize_digest(&candidate.digest))
        && !digests.iter().any(|digest| digest == &candidate_digest)
    {
        digests.push(candidate_digest);
    }
    if digests.is_empty() {
        return Err(ApiError::invalid_argument("service digest is missing"));
    }

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut inserted = false;
    for digest in digests {
        let enqueued = state
            .snapshot_worker
            .enqueue(
                &image_repo,
                &digest,
                &host_platform,
                VERSION_INFERENCE_REASON_FORCE,
            )
            .await;
        inserted = inserted || enqueued;
    }
    let reason = if inserted {
        VERSION_INFERENCE_REASON_FORCE
    } else {
        VERSION_INFERENCE_REASON_RUNNING
    };
    let resp = TriggerVersionInferenceRefreshResponse {
        status: "pending".to_string(),
        service_id,
        image_repo,
        reason: reason.to_string(),
    };
    Ok((StatusCode::ACCEPTED, Json(resp)).into_response())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionInferenceOverviewQuery {
    q: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionInferenceEventsQuery {
    #[serde(default)]
    after_id: i64,
}

#[derive(Debug, Clone)]
struct VersionInferenceOverviewRowAccum {
    image_repo: String,
    host_platform: String,
    service_count: u32,
    has_snapshot: bool,
    has_stale: bool,
    all_failed_only: bool,
    checked_at: Option<String>,
    updated_at: Option<String>,
    task: Option<snapshot_worker::SnapshotTaskSnapshot>,
}

fn normalize_version_inference_status_filter(
    input: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if raw == "all" {
        return Ok(None);
    }
    match raw {
        "queued" | "running" | "ready" | "stale" | "all_failed" => Ok(Some(raw.to_string())),
        other => Err(ApiError::invalid_argument(format!(
            "invalid status filter: {other}"
        ))),
    }
}

fn map_task_progress_state(
    progress: Option<snapshot_worker::SnapshotTaskProgress>,
) -> Option<VersionInferenceTaskProgressState> {
    progress.map(|p| VersionInferenceTaskProgressState {
        phase: p.phase,
        message: p.message,
        current: p.current,
        total: p.total,
        percent: p.percent,
        assigned_current: p.assigned_current,
        assigned_total: p.assigned_total,
        assigned_percent: p.assigned_percent,
        result_current: p.result_current,
        result_total: p.result_total,
        result_percent: p.result_percent,
        updated_at: p.updated_at,
    })
}

fn derive_overview_row_status(
    row: &VersionInferenceOverviewRowAccum,
) -> (
    String,
    Option<String>,
    Option<VersionInferenceTaskProgressState>,
) {
    if let Some(task) = row.task.as_ref() {
        if task.status == "running" {
            return (
                "running".to_string(),
                Some(task.reason.clone()),
                map_task_progress_state(task.progress.clone()),
            );
        }
        if task.status == "queued" {
            return ("queued".to_string(), Some(task.reason.clone()), None);
        }
    }

    if row.has_snapshot {
        if row.has_stale {
            return (
                "stale".to_string(),
                Some(VERSION_INFERENCE_REASON_CACHE_STALE.to_string()),
                None,
            );
        }
        if row.all_failed_only {
            return (
                "all_failed".to_string(),
                Some(VERSION_INFERENCE_REASON_ALL_FAILED.to_string()),
                None,
            );
        }
        return ("ready".to_string(), None, None);
    }

    // Rows are constructed from cached snapshots and in-flight tasks only.
    (
        "queued".to_string(),
        Some(VERSION_INFERENCE_REASON_CACHE_MISS.to_string()),
        None,
    )
}

fn version_inference_status_rank(status: &str) -> u8 {
    match status {
        "running" => 0,
        "queued" => 1,
        "stale" => 2,
        "all_failed" => 3,
        "ready" => 4,
        _ => 9,
    }
}

async fn get_version_inference_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceOverviewQuery>,
) -> Result<Json<VersionInferenceOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let status_filter = normalize_version_inference_status_filter(q.status.as_deref())?;
    let search =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());

    let worker_snapshot = state.snapshot_worker.worker_stats().await;
    let gc_snapshot = state.snapshot_worker.gc_status().await;
    let task_snapshots = state.snapshot_worker.snapshot_tasks().await;
    let snapshot_rows = state
        .db
        .list_image_digest_tags_snapshots()
        .await
        .map_err(map_internal)?;
    let service_targets = state
        .db
        .list_version_inference_service_targets()
        .await
        .map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut service_count_by_key = BTreeMap::<String, u32>::new();
    for service in service_targets {
        if !needs_version_inference_for_tags(&service.image_tag, service.candidate_tag.as_deref()) {
            continue;
        }
        let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&service.image_ref)
        else {
            continue;
        };
        let key = format!("{image_repo}@{host_platform}");
        let count = service_count_by_key.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    }

    let mut rows_by_key = BTreeMap::<String, VersionInferenceOverviewRowAccum>::new();

    for snapshot in snapshot_rows {
        let key = format!("{}@{}", snapshot.image_repo, snapshot.host_platform);
        let entry =
            rows_by_key
                .entry(key.clone())
                .or_insert_with(|| VersionInferenceOverviewRowAccum {
                    image_repo: snapshot.image_repo.clone(),
                    host_platform: snapshot.host_platform.clone(),
                    service_count: *service_count_by_key.get(&key).unwrap_or(&0),
                    has_snapshot: false,
                    has_stale: false,
                    all_failed_only: true,
                    checked_at: None,
                    updated_at: None,
                    task: None,
                });
        entry.has_snapshot = true;
        entry.checked_at =
            checked_at_latest(entry.checked_at.clone(), Some(snapshot.checked_at.as_str()));
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(snapshot.updated_at.as_str()));
        if checked_at_is_stale(&snapshot.checked_at) {
            entry.has_stale = true;
        }
        let all_failed = parse_digest_snapshot_row(&snapshot.snapshot_json, &snapshot.checked_at)
            .is_some_and(|parsed| snapshot_worker::snapshot_is_all_failed(&parsed.snapshot));
        entry.all_failed_only = entry.all_failed_only && all_failed;
    }

    for task in task_snapshots.iter() {
        let image_key = format!("{}@{}", task.image_repo, task.host_platform);
        let entry = rows_by_key.entry(image_key.clone()).or_insert_with(|| {
            VersionInferenceOverviewRowAccum {
                image_repo: task.image_repo.clone(),
                host_platform: task.host_platform.clone(),
                service_count: *service_count_by_key.get(&image_key).unwrap_or(&0),
                has_snapshot: false,
                has_stale: false,
                all_failed_only: false,
                checked_at: None,
                updated_at: Some(task.updated_at.clone()),
                task: None,
            }
        });
        let replace = entry.task.as_ref().is_none_or(|existing| {
            version_inference_status_rank(&task.status)
                < version_inference_status_rank(&existing.status)
                || (task.status == existing.status && task.updated_at > existing.updated_at)
        });
        if replace {
            entry.task = Some(task.clone());
        }
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(task.updated_at.as_str()));
    }

    let mut all_rows = rows_by_key
        .into_values()
        .map(|row| {
            let (status, reason, progress) = derive_overview_row_status(&row);
            VersionInferenceOverviewRow {
                key: format!("{}@{}", row.image_repo, row.host_platform),
                image_repo: row.image_repo,
                host_platform: row.host_platform,
                status,
                service_count: row.service_count,
                reason,
                checked_at: row.checked_at,
                updated_at: row.updated_at,
                progress,
            }
        })
        .collect::<Vec<_>>();

    all_rows.sort_by(|a, b| {
        version_inference_status_rank(&a.status)
            .cmp(&version_inference_status_rank(&b.status))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.image_repo.cmp(&b.image_repo))
            .then_with(|| a.host_platform.cmp(&b.host_platform))
    });

    let snapshots_total = all_rows
        .iter()
        .filter(|row| row.checked_at.is_some())
        .count() as u32;
    let mut summary = VersionInferenceOverviewSummary {
        snapshots_total,
        queued: 0,
        running: 0,
        ready: 0,
        stale: 0,
        all_failed: 0,
    };
    for row in &all_rows {
        match row.status.as_str() {
            "queued" => summary.queued = summary.queued.saturating_add(1),
            "running" => summary.running = summary.running.saturating_add(1),
            "ready" => summary.ready = summary.ready.saturating_add(1),
            "stale" => summary.stale = summary.stale.saturating_add(1),
            "all_failed" => summary.all_failed = summary.all_failed.saturating_add(1),
            _ => {}
        }
    }

    let mut filtered_rows = all_rows;
    if let Some(status_filter) = status_filter.as_deref() {
        filtered_rows.retain(|row| row.status == status_filter);
    }
    if let Some(search) = search.as_deref() {
        filtered_rows.retain(|row| {
            row.image_repo.to_ascii_lowercase().contains(search)
                || row.key.to_ascii_lowercase().contains(search)
        });
    }

    let total = filtered_rows.len() as u32;
    let start = page.saturating_sub(1).saturating_mul(per_page) as usize;
    let rows = filtered_rows
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect::<Vec<_>>();

    let tasks = task_snapshots
        .into_iter()
        .map(|task| VersionInferenceTaskState {
            key: task.key,
            image_repo: task.image_repo,
            host_platform: task.host_platform,
            status: task.status,
            reason: task.reason,
            enqueued_at: task.enqueued_at,
            started_at: task.started_at,
            updated_at: task.updated_at,
            progress: map_task_progress_state(task.progress),
        })
        .collect::<Vec<_>>();

    Ok(Json(VersionInferenceOverviewResponse {
        worker: VersionInferenceWorkerState {
            max_concurrency: worker_snapshot.max_concurrency,
            queued: worker_snapshot.queued,
            running: worker_snapshot.running,
            in_flight: worker_snapshot.in_flight,
        },
        gc: VersionInferenceGcState {
            retention_days: gc_snapshot.retention_days,
            interval_seconds: gc_snapshot.interval_seconds,
            last_run_at: gc_snapshot.last_run_at,
            last_deleted: gc_snapshot.last_deleted,
            last_duration_ms: gc_snapshot.last_duration_ms,
            last_error: gc_snapshot.last_error,
        },
        summary,
        tasks,
        rows,
        page,
        per_page,
        total,
    }))
}

async fn version_inference_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers)?;
    let mut after_id = resolve_sse_after_id(&headers, q.after_id);
    if after_id <= 0 {
        after_id = state.snapshot_worker.latest_event_id().await;
    }
    let sse_state = state.clone();

    let stream = async_stream::stream! {
        loop {
            let batch = sse_state
                .snapshot_worker
                .events_since(after_id, 200)
                .await;

            if let Some(oldest_id) = batch.oldest_id
                && after_id > 0
                && after_id < oldest_id.saturating_sub(1)
            {
                let evt = sse_state
                    .snapshot_worker
                    .emit_resync_required(after_id, oldest_id, batch.latest_id)
                    .await;
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
                continue;
            }

            if batch.events.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            for evt in batch.events {
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListServiceDigestTagsQuery {
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetServiceDigestTagsSnapshotQuery {
    digest: Option<String>,
}

async fn get_service_digest_tags_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<GetServiceDigestTagsSnapshotQuery>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers)?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    if digest_trimmed.is_empty() {
        return Err(ApiError::invalid_argument("digest is required"));
    }

    let digest = snapshot_worker::normalize_digest(digest_trimmed)
        .ok_or_else(|| ApiError::invalid_argument("digest is required"))?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };

    let known_digest = snapshot_target
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .is_some_and(|d| d.eq_ignore_ascii_case(&digest))
        || snapshot_target
            .candidate_digest
            .as_deref()
            .and_then(snapshot_worker::normalize_digest)
            .is_some_and(|d| d.eq_ignore_ascii_case(&digest));
    if !known_digest {
        return Err(ApiError::not_found("digest snapshot not found"));
    }

    let image_repo = snapshot_worker::image_repo_from_image_ref(&snapshot_target.image_ref)
        .ok_or_else(|| ApiError::invalid_argument("invalid service image ref"))?;
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, _checked_at, _updated_at)) = snapshot else {
        state
            .snapshot_worker
            .enqueue(
                &image_repo,
                &digest,
                &host_platform,
                "api_snapshot_read_miss",
            )
            .await;
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    };

    let parsed: ServiceDigestTagsSnapshotResponse =
        serde_json::from_str(&snapshot_json).map_err(|e| {
            ApiError::internal("invalid digest tags snapshot").with_details(json!({
                "error": e.to_string(),
            }))
        })?;

    Ok(Json(parsed).into_response())
}

async fn list_service_digest_tags(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ListServiceDigestTagsQuery>,
) -> Result<Json<ServiceDigestTagsResponse>, ApiError> {
    use std::time::Duration;

    use tokio::{
        task::JoinSet,
        time::{Instant, timeout, timeout_at},
    };

    let _user = require_user(&state, &headers)?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    // This endpoint is primarily used for UI observability. When digest is missing, we still want
    // to return the full `repo_tags` list so the UI can show something actionable (and avoid
    // "empty bubbles").
    let (digest, wanted) = if digest_trimmed.is_empty() {
        (String::new(), None)
    } else if digest_trimmed.contains(':') {
        (digest_trimmed.to_string(), Some(digest_trimmed.to_string()))
    } else {
        let normalized = format!("sha256:{digest_trimmed}");
        (normalized.clone(), Some(normalized))
    };

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

    // Digest tag listing is used for UI debugging / observability, not as part of the "update
    // candidates" hot path. Still, we bound latency to avoid hanging requests forever.
    const LIST_TAGS_TIMEOUT: Duration = Duration::from_secs(8);
    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(6);
    const MANIFEST_BUDGET: Duration = Duration::from_secs(40);
    const MANIFEST_CONCURRENCY: usize = 10;

    let repo_tags = match timeout(LIST_TAGS_TIMEOUT, state.registry.list_tags(&img)).await {
        Ok(Ok(tags)) => tags,
        Ok(Err(e)) => return Err(map_internal(e)),
        Err(_) => {
            return Err(ApiError::internal("registry timeout").with_details(json!({
                "op": "list_tags"
            })));
        }
    };

    let repo_tags_total = repo_tags.len();
    let Some(wanted) = wanted else {
        return Ok(Json(ServiceDigestTagsResponse {
            digest,
            tags: Vec::new(),
            repo_tags,
            scan: ServiceDigestTagsScanSummary {
                repo_tags_total,
                repo_tags_considered: 0,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        }));
    };

    let registry = state.registry.clone();
    let img = img.clone();
    let host_platform = host_platform.clone();

    let mut out: Vec<String> = Vec::new();
    let mut manifests_ok: usize = 0;
    let mut manifests_timeout: usize = 0;
    let mut manifests_error: usize = 0;

    enum ScanOutcome {
        OkMatch(String),
        OkNoMatch,
        Timeout,
        Error,
    }

    let mut join_set: JoinSet<ScanOutcome> = JoinSet::new();
    let mut queue = repo_tags.iter().cloned();

    let spawn_one = |join_set: &mut JoinSet<ScanOutcome>,
                     tag: String,
                     registry: Arc<dyn registry::RegistryClient>,
                     img: registry::ImageRef,
                     host_platform: String,
                     wanted: String| {
        join_set.spawn(async move {
            match timeout(
                MANIFEST_TIMEOUT,
                registry.get_manifest(&img, &tag, &host_platform),
            )
            .await
            {
                Ok(Ok(m)) => {
                    let ok = m
                        .digest
                        .as_deref()
                        .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted))
                        || m.platform_digest
                            .as_deref()
                            .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted));
                    if ok {
                        ScanOutcome::OkMatch(tag)
                    } else {
                        ScanOutcome::OkNoMatch
                    }
                }
                Ok(Err(_)) => ScanOutcome::Error,
                Err(_) => ScanOutcome::Timeout,
            }
        });
    };

    for _ in 0..MANIFEST_CONCURRENCY {
        let Some(tag) = queue.next() else { break };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    let deadline = Instant::now() + MANIFEST_BUDGET;
    while !join_set.is_empty() {
        let next = match timeout_at(deadline, join_set.join_next()).await {
            Ok(next) => next,
            Err(_) => {
                // Degrade gracefully: keep best-effort matches and surface incompleteness via the
                // scan summary instead of failing the whole request.
                join_set.abort_all();
                break;
            }
        };

        let Some(joined) = next else { break };
        match joined {
            Ok(ScanOutcome::OkMatch(tag)) => {
                manifests_ok += 1;
                out.push(tag);
            }
            Ok(ScanOutcome::OkNoMatch) => {
                manifests_ok += 1;
            }
            Ok(ScanOutcome::Timeout) => {
                manifests_timeout += 1;
            }
            Ok(ScanOutcome::Error) => {
                manifests_error += 1;
            }
            Err(_) => {
                manifests_error += 1;
            }
        };

        let Some(tag) = queue.next() else {
            continue;
        };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    // If the budget was exhausted (or tasks were aborted), treat the remaining tags as timeouts so
    // the UI can warn that the result may be incomplete.
    let processed = manifests_ok + manifests_timeout + manifests_error;
    if processed < repo_tags_total {
        manifests_timeout += repo_tags_total - processed;
    }

    let mut semver_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut other_tags: Vec<String> = Vec::new();
    for tag in out {
        if let Some(v) = ignore::parse_version(&tag) {
            semver_tags.push((v, tag));
        } else {
            other_tags.push(tag);
        }
    }

    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    other_tags.sort_by(|a, b| b.cmp(a));

    let mut sorted: Vec<String> = Vec::new();
    for (_, tag) in semver_tags {
        sorted.push(tag);
    }
    for tag in other_tags {
        sorted.push(tag);
    }

    Ok(Json(ServiceDigestTagsResponse {
        digest,
        tags: sorted,
        repo_tags,
        scan: ServiceDigestTagsScanSummary {
            repo_tags_total,
            repo_tags_considered: repo_tags_total,
            manifests_ok,
            manifests_timeout,
            manifests_error,
        },
    }))
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
    let repos_total = state
        .db
        .count_github_packages_repos_total()
        .await
        .map_err(map_internal)?;
    let repos_selected_total = state
        .db
        .count_github_packages_repos_selected_total()
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
        repos_total,
        repos_selected_total,
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

    if let Some(req_targets) = req.targets {
        let mut targets = Vec::new();
        for t in req_targets {
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
    }

    if let Some(req_repos) = req.repos {
        let repos = normalize_github_repo_selection(req_repos).map_err(|e| {
            ApiError::invalid_argument("invalid repos")
                .with_details(json!({"error": e.to_string()}))
        })?;
        state
            .db
            .put_github_packages_repos(&repos, &now)
            .await
            .map_err(map_internal)?;
    }

    Ok(Json(PutGitHubPackagesSettingsResponse { ok: true }))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListGitHubPackagesReposQuery {
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    per_page: Option<u32>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    selected_filter: Option<String>, // all|selected|unselected
}

fn parse_selected_filter(v: Option<&str>) -> Result<Option<bool>, ApiError> {
    let Some(v) = v else { return Ok(None) };
    match v.trim() {
        "" | "all" => Ok(None),
        "selected" => Ok(Some(true)),
        "unselected" => Ok(Some(false)),
        _ => Err(ApiError::invalid_argument("invalid selectedFilter")),
    }
}

async fn list_github_packages_repos(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListGitHubPackagesReposQuery>,
) -> Result<Json<ListGitHubPackagesReposResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let selected_filter = parse_selected_filter(q.selected_filter.as_deref())?;

    let total = state
        .db
        .count_github_packages_repos_total()
        .await
        .map_err(map_internal)?;
    let selected_total = state
        .db
        .count_github_packages_repos_selected_total()
        .await
        .map_err(map_internal)?;
    let filtered_total = state
        .db
        .count_github_packages_repos_filtered(q.q.as_deref(), selected_filter)
        .await
        .map_err(map_internal)?;

    let offset = (page - 1).saturating_mul(per_page);
    let repos = state
        .db
        .list_github_packages_repos_page(q.q.as_deref(), selected_filter, per_page, offset)
        .await
        .map_err(map_internal)?;

    Ok(Json(ListGitHubPackagesReposResponse {
        page,
        per_page,
        total,
        filtered_total,
        selected_total,
        repos: repos
            .into_iter()
            .map(|r| GitHubPackagesRepo {
                full_name: format!("{}/{}", r.owner, r.repo),
                selected: r.selected,
                webhook_state: Some(r.webhook_state),
                webhook_job_id: r.webhook_job_id,
                hook_id: r.hook_id,
                last_sync_at: r.last_sync_at,
                last_audit_at: r.last_audit_at,
                last_op: r.last_op,
                last_error: r.last_error,
            })
            .collect(),
    }))
}

async fn get_github_packages_webhook_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GitHubPackagesWebhookOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let overview = ghcr_webhook_jobs::get_overview(&state)
        .await
        .map_err(map_internal)?;
    Ok(Json(overview))
}

async fn set_github_packages_repo_selected(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetGitHubPackagesRepoSelectedRequest>,
) -> Result<Json<SetGitHubPackagesRepoSelectedResponse>, ApiError> {
    let user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let mut parts = req.full_name.split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(ApiError::invalid_argument("invalid fullName"));
    }

    state
        .db
        .upsert_github_packages_repo_selected(owner, repo, req.selected, &now)
        .await
        .map_err(map_internal)?;

    let mut job_id: Option<String> = None;
    if req.selected {
        let settings = state
            .db
            .get_github_packages_settings()
            .await
            .map_err(map_internal)?;
        let callback_ready =
            !settings.callback_url.trim().is_empty() && Url::parse(&settings.callback_url).is_ok();
        if settings.enabled && callback_ready {
            let full_name = format!("{owner}/{repo}");
            let queued = ghcr_webhook_jobs::enqueue_repo_job(
                &state,
                &full_name,
                ghcr_webhook_jobs::GhcrWebhookOp::Register,
                &user,
                "ui",
            )
            .await
            .map_err(map_internal)?;
            job_id = Some(queued);
        }
    }

    Ok(Json(SetGitHubPackagesRepoSelectedResponse {
        ok: true,
        job_id,
    }))
}

async fn delete_github_packages_repo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteGitHubPackagesRepoRequest>,
) -> Result<Json<DeleteGitHubPackagesRepoResponse>, ApiError> {
    let user = require_user(&state, &headers)?;

    let mut parts = req.full_name.split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return Err(ApiError::invalid_argument("invalid fullName"));
    }

    let existing = state
        .db
        .get_github_packages_repo(owner, repo)
        .await
        .map_err(map_internal)?;
    if existing.is_none() {
        return Err(ApiError::not_found("repo is not tracked"));
    }

    let job_id = ghcr_webhook_jobs::enqueue_repo_job(
        &state,
        &format!("{owner}/{repo}"),
        ghcr_webhook_jobs::GhcrWebhookOp::Unregister,
        &user,
        "ui",
    )
    .await
    .map_err(map_internal)?;

    Ok(Json(DeleteGitHubPackagesRepoResponse { ok: true, job_id }))
}

async fn bulk_set_github_packages_repos_selected(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<BulkSetGitHubPackagesReposSelectedRequest>,
) -> Result<Json<BulkSetGitHubPackagesReposSelectedResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let selected_filter = parse_selected_filter(req.selected_filter.as_deref())?;
    let affected = state
        .db
        .bulk_set_github_packages_repos_selected(
            req.q.as_deref(),
            selected_filter,
            req.selected,
            &now,
        )
        .await
        .map_err(map_internal)?;

    Ok(Json(BulkSetGitHubPackagesReposSelectedResponse {
        ok: true,
        affected,
    }))
}

async fn add_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<AddGitHubPackagesTargetRequest>,
) -> Result<Json<AddGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;

    let settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let Some(pat) = settings.pat else {
        return Err(ApiError::invalid_argument("pat is required"));
    };

    let parsed = github::parse_target_input(&req.input).map_err(|e| {
        ApiError::invalid_argument("invalid target input")
            .with_details(json!({"input": req.input, "error": e.to_string()}))
    })?;

    let client = github::GitHubClient::new(&pat).map_err(map_internal)?;

    let (kind, owner, repos): (String, String, Vec<(String, String)>) = match parsed {
        github::TargetKind::Repo { owner, repo } => {
            ("repo".to_string(), owner.clone(), vec![(owner, repo)])
        }
        github::TargetKind::Owner { owner } => {
            let repos = client
                .list_owner_repos(&owner)
                .await
                .map_err(map_internal)?;
            let mut out = Vec::new();
            for r in repos {
                let mut parts = r.full_name.split('/');
                let ro = parts.next().unwrap_or_default().trim();
                let rr = parts.next().unwrap_or_default().trim();
                if ro.is_empty() || rr.is_empty() || parts.next().is_some() {
                    continue;
                }
                out.push((ro.to_string(), rr.to_string()));
            }
            ("owner".to_string(), owner, out)
        }
    };

    state
        .db
        .upsert_github_packages_target_by_input(&req.input, &kind, &owner, &[], &now)
        .await
        .map_err(map_internal)?;

    let repos_added = state
        .db
        .upsert_github_packages_repos_default_selected(&repos, &now)
        .await
        .map_err(map_internal)?;

    Ok(Json(AddGitHubPackagesTargetResponse {
        ok: true,
        kind,
        owner,
        repos_added,
    }))
}

async fn remove_github_packages_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RemoveGitHubPackagesTargetRequest>,
) -> Result<Json<RemoveGitHubPackagesTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;

    let _ = state
        .db
        .delete_github_packages_target_by_input(&req.input)
        .await
        .map_err(map_internal)?;

    Ok(Json(RemoveGitHubPackagesTargetResponse { ok: true }))
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
        github::TargetKind::Repo { owner, repo } => {
            let selected = state
                .db
                .get_github_packages_repo_selected(owner.as_str(), repo.as_str())
                .await
                .map_err(map_internal)?
                .unwrap_or(true);
            Ok(Json(ResolveGitHubPackagesTargetResponse {
                kind: "repo".to_string(),
                owner: owner.clone(),
                repos: vec![GitHubPackagesRepoSelection {
                    full_name: format!("{owner}/{repo}"),
                    selected,
                    visibility: Some("unknown".to_string()),
                    last_activity_at: None,
                }],
                warnings: Vec::new(),
            }))
        }
        github::TargetKind::Owner { owner } => {
            let settings = state
                .db
                .get_github_packages_settings()
                .await
                .map_err(map_internal)?;
            let Some(pat) = settings.pat else {
                return Err(
                    ApiError::invalid_argument("pat is required before resolving owner")
                        .with_details(json!({"reason":"ghcr_pat_missing","owner":owner})),
                );
            };
            let client = github::GitHubClient::new(&pat).map_err(map_internal)?;
            let repos = client
                .list_owner_repos(&owner)
                .await
                .map_err(|e| map_github_owner_resolve_error(&owner, e))?;
            // Default to "not selected", but keep existing tracked repos selected.
            let existing = state
                .db
                .list_github_packages_repos_selected_by_owner(owner.as_str())
                .await
                .map_err(map_internal)?;
            let mut existing_selected = std::collections::HashSet::<String>::new();
            for (repo, selected) in existing {
                if selected {
                    existing_selected.insert(repo.to_lowercase());
                }
            }
            Ok(Json(ResolveGitHubPackagesTargetResponse {
                kind: "owner".to_string(),
                owner: owner.clone(),
                repos: repos
                    .into_iter()
                    .filter_map(|r| {
                        // Avoid borrowing `full_name` across moving it into the response.
                        let full_name = r.full_name;
                        let visibility = if r.is_private { "private" } else { "public" };
                        let last_activity_at = r.pushed_at.or(r.updated_at);
                        let selected = {
                            let mut parts = full_name.split('/');
                            let ro = parts.next().unwrap_or_default().trim();
                            let rr = parts.next().unwrap_or_default().trim();
                            if ro.is_empty() || rr.is_empty() || parts.next().is_some() {
                                return None;
                            }
                            existing_selected.contains(&rr.to_lowercase())
                        };
                        Some(GitHubPackagesRepoSelection {
                            full_name,
                            selected,
                            visibility: Some(visibility.to_string()),
                            last_activity_at,
                        })
                    })
                    .collect(),
                warnings: Vec::new(),
            }))
        }
    }
}

#[allow(dead_code)]
fn urls_match(a: &str, b: &str) -> bool {
    let Ok(au) = Url::parse(a) else { return false };
    let Ok(bu) = Url::parse(b) else { return false };

    // GitHub webhook config URLs are effectively compared by the request destination we will
    // receive, not by exact `Url` string equality. Be tolerant of benign differences to avoid
    // re-creating equivalent hooks (e.g. trailing slashes, default port normalization).
    //
    // We intentionally ignore fragments because they are not sent to the server.
    let (Some(ah), Some(bh)) = (au.host_str(), bu.host_str()) else {
        return false;
    };

    if !au.scheme().eq_ignore_ascii_case(bu.scheme()) {
        return false;
    }

    if !ah.eq_ignore_ascii_case(bh) {
        return false;
    }

    if au.port_or_known_default() != bu.port_or_known_default() {
        return false;
    }

    fn normalize_path(path: &str) -> &str {
        if path.len() <= 1 {
            return path;
        }
        path.trim_end_matches('/')
    }
    if normalize_path(au.path()) != normalize_path(bu.path()) {
        return false;
    }

    au.query() == bu.query()
}

async fn sync_github_packages_webhooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SyncGitHubPackagesWebhooksRequest>,
) -> Result<Json<SyncGitHubPackagesWebhooksResponse>, ApiError> {
    let user = require_user(&state, &headers)?;

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
    if settings.callback_url.trim().is_empty() {
        return Err(ApiError::invalid_argument("callbackUrl is required"));
    }
    let _ = Url::parse(&settings.callback_url)
        .map_err(|_| ApiError::invalid_argument("invalid callbackUrl"))?;

    let mut selected_repos: Vec<(String, String)> = state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
        .map(|r| (r.owner, r.repo))
        .collect();
    if let Some(req_repos) = &req.repos {
        let allow = req_repos
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect::<std::collections::HashSet<_>>();
        selected_repos.retain(|(o, r)| allow.contains(&format!("{}/{}", o, r).to_lowercase()));
    }

    let mut results = Vec::new();

    let dry_run = req.dry_run.unwrap_or(false);

    for (owner, repo) in selected_repos {
        let full = format!("{owner}/{repo}");
        if dry_run {
            results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "queued".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some("dryRun: would enqueue register job".to_string()),
            });
            continue;
        }

        let queued = ghcr_webhook_jobs::enqueue_repo_job(
            &state,
            &full,
            ghcr_webhook_jobs::GhcrWebhookOp::Register,
            &user,
            "ui",
        )
        .await;
        match queued {
            Ok(job_id) => results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "queued".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some(format!("jobId={job_id}")),
            }),
            Err(err) => results.push(SyncGitHubPackagesWebhookResult {
                repo: full,
                action: "error".to_string(),
                hook_id: None,
                conflict_hooks: None,
                message: Some(err.to_string()),
            }),
        }
    }

    Ok(Json(SyncGitHubPackagesWebhooksResponse {
        ok: results.iter().all(|r| r.action != "error"),
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

    if !settings.enabled {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "disabled"}),
        ));
    }

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

    // NOTE: GitHub repo names are case-insensitive but case-preserving; compare in lower-case so we
    // don't mistakenly drop events due to casing differences between stored data and payloads.
    let mut selected_repos_lower = std::collections::HashSet::<String>::new();
    let mut selected_owners_lower = std::collections::HashSet::<String>::new();
    for r in state
        .db
        .list_github_packages_repos()
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter(|r| r.selected)
    {
        selected_owners_lower.insert(r.owner.to_ascii_lowercase());
        selected_repos_lower.insert(format!("{}/{}", r.owner, r.repo).to_ascii_lowercase());
    }

    let should_trigger = if let Some(full) = &repo_full_name {
        selected_repos_lower.contains(&full.to_ascii_lowercase())
    } else if let Some(owner) = &owner {
        selected_owners_lower.contains(&owner.to_ascii_lowercase())
    } else {
        false
    };

    if !should_trigger {
        return Ok(Json(
            json!({"ok": true, "ignored": true, "reason": "repo_not_selected"}),
        ));
    }

    // Only persist delivery IDs for events that are eligible to trigger a scan. This prevents
    // unbounded growth in the deliveries table when the webhook exists but repos are deselected.
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
        let outcome = discovery::run_scan_for_job(run_state.as_ref(), &run_job_id).await;
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

            let host_platform =
                registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());

            let run_state = state.clone();
            let run_job_id = job_id.clone();
            let run_scope = scope.clone();
            let run_stack_id = stack_id.clone();
            let run_service_id = service_id.clone();
            let run_host_platform = host_platform.clone();
            let run_started_at = now.clone();
            tokio::spawn(async move {
                if let Err(e) = run_state
                    .db
                    .insert_job_log(
                        &run_job_id,
                        &JobLogLine {
                            ts: run_started_at.clone(),
                            level: "info".to_string(),
                            msg: "webhook check started".to_string(),
                        },
                    )
                    .await
                {
                    tracing::warn!(
                        job_id = %run_job_id,
                        error = %e,
                        "failed to insert webhook check started log"
                    );
                }

                let outcome = run_check_for_job(
                    &run_state,
                    &run_job_id,
                    &run_scope,
                    run_stack_id.as_deref(),
                    run_service_id.as_deref(),
                    &run_host_platform,
                    &run_started_at,
                )
                .await;

                let finished_at = match now_rfc3339() {
                    Ok(ts) => ts,
                    Err(err) => {
                        tracing::warn!(
                            job_id = %run_job_id,
                            error = %err,
                            "failed to format finished_at as RFC3339; falling back to started_at"
                        );
                        run_started_at.clone()
                    }
                };
                match outcome {
                    Ok(summary) => {
                        if let Err(e) = run_state
                            .db
                            .finish_job(&run_job_id, "success", &finished_at, &summary)
                            .await
                        {
                            tracing::error!(
                                job_id = %run_job_id,
                                error = %e,
                                "failed to finish webhook check job"
                            );
                        }
                    }
                    Err(e) => {
                        if let Err(err) = run_state
                            .db
                            .insert_job_log(
                                &run_job_id,
                                &JobLogLine {
                                    ts: finished_at.clone(),
                                    level: "error".to_string(),
                                    msg: format!("webhook check failed: {e:?}"),
                                },
                            )
                            .await
                        {
                            tracing::warn!(
                                job_id = %run_job_id,
                                error = %err,
                                "failed to insert webhook check failure log"
                            );
                        }
                        let summary = json!({"error": format!("{e:?}")});
                        if let Err(err) = run_state
                            .db
                            .finish_job(&run_job_id, "failed", &finished_at, &summary)
                            .await
                        {
                            tracing::error!(
                                job_id = %run_job_id,
                                error = %err,
                                "failed to finish failed webhook check job"
                            );
                        }
                    }
                }
            });

            Ok(Json(WebhookTriggerResponse { job_id }))
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

async fn get_deploy_check_report(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DeployCheckReportResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let report = preflight::build_report(state.as_ref())
        .await
        .map_err(map_internal)?;
    Ok(Json(report))
}

async fn get_deploy_welcome(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DeployWelcomeResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let settings = state
        .db
        .get_deploy_welcome_settings()
        .await
        .map_err(map_internal)?;
    Ok(Json(DeployWelcomeResponse::from(settings)))
}

async fn put_deploy_welcome(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PutDeployWelcomeRequest>,
) -> Result<Json<PutDeployWelcomeResponse>, ApiError> {
    let _user = require_user(&state, &headers)?;
    let now = now_rfc3339().map_err(map_internal)?;
    state
        .db
        .put_deploy_welcome_settings(req.never_auto_open, &now)
        .await
        .map_err(map_internal)?;
    Ok(Json(PutDeployWelcomeResponse {
        ok: true,
        never_auto_open: req.never_auto_open,
        updated_at: Some(now),
    }))
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

fn map_github_owner_resolve_error(owner: &str, err: anyhow::Error) -> ApiError {
    if let Some(status) = github_http_status_from_error(&err)
        && (status == 401 || status == 403)
    {
        return ApiError::invalid_argument("github pat is invalid or lacks required scopes")
            .with_details(json!({
                "reason":"ghcr_pat_invalid_or_scope_insufficient",
                "owner": owner,
                "status": status,
                "cause": err.to_string(),
            }));
    }

    if github_error_is_timeout(&err) {
        return ApiError::internal("github upstream timeout").with_details(json!({
            "reason":"github_upstream_timeout",
            "owner": owner,
            "cause": err.to_string(),
        }));
    }

    ApiError::internal("github upstream unavailable").with_details(json!({
        "reason":"github_upstream_unavailable",
        "owner": owner,
        "status": github_http_status_from_error(&err),
        "cause": err.to_string(),
    }))
}

fn github_http_status_from_error(err: &anyhow::Error) -> Option<u16> {
    for cause in err.chain() {
        let text = cause.to_string();
        if let Some(rest) = text.strip_prefix("github http ") {
            let head = rest.split(':').next()?.trim();
            let status_token = head.split_whitespace().next()?;
            if let Ok(status) = status_token.parse::<u16>() {
                return Some(status);
            }
        }
    }
    None
}

fn github_error_is_timeout(err: &anyhow::Error) -> bool {
    if err
        .chain()
        .filter_map(|cause| cause.downcast_ref::<reqwest::Error>())
        .any(reqwest::Error::is_timeout)
    {
        return true;
    }
    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
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
