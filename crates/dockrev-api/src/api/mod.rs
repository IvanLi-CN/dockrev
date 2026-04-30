pub mod types;

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    convert::Infallible,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

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
use cron::Schedule;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use url::Url;

use crate::github;
use crate::{
    authz::{self, AuthzFailure, AuthzMatchKind, RequestAuth},
    backup,
    db::GitHubPackagesWebhookDeliveryRecordInput,
    discovery,
    error::ApiError,
    ghcr_webhook_jobs, ids, ignore, notify, preflight, registry, resource_usage, runtime_scan,
    snapshot_worker,
    state::AppState,
    ui, updater,
};
use types::*;

mod cleanup_routes;
mod discovery_routes;
mod github_packages;
mod homepage_icons;
mod ignore_rules;
mod jobs;
mod notifications;
mod operations;
pub(crate) mod services;
mod stacks;
mod webhooks;

use cleanup_routes::*;
use discovery_routes::*;
use github_packages::*;
use homepage_icons::*;
use ignore_rules::*;
use jobs::*;
use notifications::*;
use operations::*;
use services::*;
pub(crate) use stacks::needs_version_inference_for_tags;
use stacks::*;
use webhooks::*;
pub fn router(state: Arc<AppState>) -> Router {
    Router::<Arc<AppState>>::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route(
            "/api/homepage-icons/{provider}/{*path}",
            get(proxy_homepage_icon),
        )
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
            "/api/services/{service_id}/resource-usage/history",
            get(get_service_resource_usage_history),
        )
        .route(
            "/api/services/resource-usage/overview",
            get(get_service_resource_usage_overview),
        )
        .route(
            "/api/services/{service_id}/resource-usage/events",
            get(service_resource_usage_events),
        )
        .route(
            "/api/services/{service_id}/version-inference/refresh",
            post(trigger_service_version_inference_refresh),
        )
        .route(
            "/api/services/{service_id}/repo-link/infer",
            post(infer_service_repo_link),
        )
        .route(
            "/api/services/{service_id}/new-version-discovery-timeline",
            get(get_service_new_version_discovery_timeline),
        )
        .route(
            "/api/services/{service_id}/github-releases",
            get(list_service_github_releases),
        )
        .route(
            "/api/services/{service_id}/github-releases/locate",
            get(locate_service_github_release),
        )
        .route(
            "/api/services/{service_id}/rollback-target",
            get(get_service_rollback_target),
        )
        .route(
            "/api/services/{service_id}/rollback",
            post(trigger_service_rollback),
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
        .route("/api/cleanups/scan", post(scan_cleanups))
        .route("/api/cleanups/apply", post(apply_cleanups))
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
            "/api/github-packages/webhook/sync-all",
            post(trigger_github_packages_webhook_sync_all),
        )
        .route(
            "/api/github-packages/webhook/sync-repo",
            post(trigger_github_packages_webhook_sync_repo),
        )
        .route(
            "/api/github-packages/webhook/deliveries",
            get(list_github_packages_webhook_deliveries),
        )
        .route(
            "/api/github-packages/webhook/deliveries/events",
            get(github_packages_webhook_delivery_events),
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

pub(crate) async fn run_check_for_job(
    state: &Arc<AppState>,
    job_id: &str,
    scope: &JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    host_platform: &str,
    now: &str,
) -> Result<serde_json::Value, ApiError> {
    operations::run_check_for_job(
        state,
        job_id,
        scope,
        stack_id,
        service_id,
        host_platform,
        now,
    )
    .await
}
fn mask_if_some(input: &Option<String>) -> Option<String> {
    input.as_ref().map(|_| "******".to_string())
}

fn normalize_public_base_url(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("instance.publicBaseUrl must not be empty"));
    }

    let mut url = Url::parse(trimmed).context("instance.publicBaseUrl is not a valid URL")?;
    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(anyhow::anyhow!(
                "instance.publicBaseUrl must start with http:// or https://"
            ));
        }
    }
    if url.host_str().is_none() {
        return Err(anyhow::anyhow!("instance.publicBaseUrl must be absolute"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(anyhow::anyhow!(
            "instance.publicBaseUrl must not include query or fragment"
        ));
    }

    // Ensure the base behaves like a directory for URL join.
    let mut path = url.path().to_string();
    if !path.ends_with('/') {
        path.push('/');
        url.set_path(&path);
    }

    Ok(url.to_string())
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
async fn get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SettingsResponse>, ApiError> {
    let auth = require_user(&state, &headers).await?;

    let backup = state.db.get_backup_settings().await.map_err(map_internal)?;
    let resource_monitor = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    let schedules = state
        .db
        .get_schedule_settings()
        .await
        .map_err(map_internal)?;
    let public_base_url = state
        .db
        .get_instance_public_base_url()
        .await
        .map_err(map_internal)?;
    let auth_view = authz::config_view(&state.config);
    Ok(Json(SettingsResponse {
        backup,
        resource_monitor,
        schedules,
        auth: AuthSettings {
            forward_header_name: auth_view.forward_header_name,
            group_header_name: auth_view.group_header_name,
            allow_anonymous_in_dev: auth_view.allow_anonymous_in_dev,
            authorization_mode: auth_view.authorization_mode.to_string(),
            allowed_user_masked: auth_view.allowed_user_masked,
            allowed_group_masked: auth_view.allowed_group_masked,
            current_user: auth.user.clone(),
            current_groups: authz::mask_list(&auth.groups),
            avatar_url: auth.avatar_url.clone(),
            matched_by: authz_match_label(&auth.matched_by).to_string(),
        },
        instance: InstanceSettings { public_base_url },
    }))
}

async fn put_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<PutSettingsRequest>,
) -> Result<Json<PutSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    let existing_resource_monitor = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    let mut merged_resource_monitor =
        req.resource_monitor
            .map_or(existing_resource_monitor, |rm| ResourceMonitorSettings {
                enabled: rm.enabled,
                sample_interval_seconds: rm.sample_interval_seconds,
                retention_days: resource_usage::RESOURCE_MONITOR_RETENTION_DAYS,
            });
    if !resource_usage::is_valid_sample_interval_seconds(
        merged_resource_monitor.sample_interval_seconds,
    ) {
        return Err(ApiError::invalid_argument(
            "resourceMonitor.sampleIntervalSeconds must be one of 10/30/60/300",
        ));
    }
    merged_resource_monitor.retention_days = resource_usage::RESOURCE_MONITOR_RETENTION_DAYS;

    let existing_schedules = state
        .db
        .get_schedule_settings()
        .await
        .map_err(map_internal)?;
    let mut merged_schedules = existing_schedules;
    if let Some(put) = req.schedules {
        if let Some(v) = put.update_check {
            merged_schedules.update_check = v;
        }
        if let Some(v) = put.ghcr_webhook_audit {
            merged_schedules.ghcr_webhook_audit = v;
        }
    }
    merged_schedules.update_check.cron =
        crate::cron_expr::canonicalize_for_store(&merged_schedules.update_check.cron);
    merged_schedules.ghcr_webhook_audit.cron =
        crate::cron_expr::canonicalize_for_store(&merged_schedules.ghcr_webhook_audit.cron);

    let validate_cron = |expr: &str, field: &str| -> Result<(), ApiError> {
        let normalized = crate::cron_expr::normalize_cron(expr).map_err(|e| {
            ApiError::invalid_argument("cron expression is invalid").with_details(json!({
                "reason": "cron_invalid",
                "field": field,
                "error": e.to_string(),
            }))
        })?;
        Schedule::from_str(&normalized).map_err(|e| {
            ApiError::invalid_argument("cron expression is invalid").with_details(json!({
                "reason": "cron_invalid",
                "field": field,
                "error": e.to_string(),
            }))
        })?;
        Ok(())
    };

    if merged_schedules.update_check.enabled {
        validate_cron(
            &merged_schedules.update_check.cron,
            "schedules.updateCheck.cron",
        )?;
    }
    if merged_schedules.ghcr_webhook_audit.enabled {
        validate_cron(
            &merged_schedules.ghcr_webhook_audit.cron,
            "schedules.ghcrWebhookAudit.cron",
        )?;
    }

    let existing_public_base_url = state
        .db
        .get_instance_public_base_url()
        .await
        .map_err(map_internal)?;
    let mut merged_public_base_url = existing_public_base_url;
    if let Some(instance) = req.instance
        && let Some(value) = instance.public_base_url
    {
        merged_public_base_url = value;
    }
    merged_public_base_url = merged_public_base_url
        .map(|v| v.trim().to_string())
        .and_then(|v| if v.is_empty() { None } else { Some(v) });
    if let Some(raw) = merged_public_base_url {
        let normalized = normalize_public_base_url(&raw).map_err(|e| {
            ApiError::invalid_argument(e.to_string()).with_details(serde_json::json!({
                "reason": "instance_public_base_url_invalid",
                "field": "instance.publicBaseUrl",
            }))
        })?;
        merged_public_base_url = Some(normalized);
    }

    state
        .db
        .put_settings(
            &req.backup,
            &merged_resource_monitor,
            &merged_schedules,
            merged_public_base_url,
            &now,
        )
        .await
        .map_err(map_internal)?;
    Ok(Json(PutSettingsResponse { ok: true }))
}

async fn get_deploy_check_report(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DeployCheckReportResponse>, ApiError> {
    let auth = require_user(&state, &headers).await?;

    let report = preflight::build_report(state.as_ref())
        .await
        .map_err(map_internal)?;
    Ok(Json(attach_authz_checks(
        state.as_ref(),
        &headers,
        report,
        Ok(auth),
    )))
}

async fn get_deploy_welcome(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<DeployWelcomeResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
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
    let _user = require_user(&state, &headers).await?;
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

async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<RequestAuth, ApiError> {
    authorize_request(state, headers).await
}

async fn authorize_request(state: &AppState, headers: &HeaderMap) -> Result<RequestAuth, ApiError> {
    match authz::authorize_request(&state.config, headers) {
        Ok(auth) => Ok(auth),
        Err(failure) => Err(authz_error(state, failure).await),
    }
}

async fn authz_error(state: &AppState, failure: AuthzFailure) -> ApiError {
    let auth_view = authz::config_view(&state.config);

    ApiError::auth_required().with_details(json!({
        "reason": failure.reason,
        "message": failure.message,
        "forwardHeaderName": auth_view.forward_header_name,
        "groupHeaderName": auth_view.group_header_name,
        "allowedUserMasked": auth_view.allowed_user_masked,
        "allowedGroupMasked": auth_view.allowed_group_masked,
        "currentUser": failure.current_user,
        "currentGroups": authz::mask_list(&failure.current_groups),
        "avatarUrl": failure.avatar_url,
        "authorizationMode": auth_view.authorization_mode,
    }))
}

fn attach_authz_checks(
    state: &AppState,
    headers: &HeaderMap,
    mut report: DeployCheckReportResponse,
    auth: Result<RequestAuth, ApiError>,
) -> DeployCheckReportResponse {
    let mut checks = build_authz_checks(state, headers, &auth);
    checks.extend(report.checks);
    let blocking_check_ids = checks
        .iter()
        .filter(|item| item.required && item.status == DeployCheckStatus::Fail)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    report.overall = if blocking_check_ids.is_empty() {
        DeployCheckOverall {
            result: DeployCheckResult::Pass,
            blocking_check_ids,
            summary: "All required capabilities are available".to_string(),
        }
    } else {
        DeployCheckOverall {
            result: DeployCheckResult::Fail,
            blocking_check_ids,
            summary: "At least one required capability is unavailable".to_string(),
        }
    };
    report.checks = checks;
    report
}

fn authz_match_label(kind: &AuthzMatchKind) -> &'static str {
    match kind {
        AuthzMatchKind::User => "user",
        AuthzMatchKind::Group => "group",
        AuthzMatchKind::AnonymousDev => "anonymous_dev",
    }
}

fn deploy_check_pass(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: impl Into<String>,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Core,
        required: true,
        status: DeployCheckStatus::Pass,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence: evidence.into(),
        recommendation: String::new(),
    }
}

fn deploy_check_fail(
    id: &str,
    title: &str,
    summary: &str,
    impact: &str,
    evidence: impl Into<String>,
    recommendation: &str,
) -> DeployCheckItem {
    DeployCheckItem {
        id: id.to_string(),
        title: title.to_string(),
        group: DeployCheckGroup::Core,
        required: true,
        status: DeployCheckStatus::Fail,
        na_reason: None,
        summary: summary.to_string(),
        impact: impact.to_string(),
        evidence: evidence.into(),
        recommendation: recommendation.to_string(),
    }
}

fn build_authz_checks(
    state: &AppState,
    headers: &HeaderMap,
    auth: &Result<RequestAuth, ApiError>,
) -> Vec<DeployCheckItem> {
    let configured =
        state.config.auth_allowed_user.is_some() || state.config.auth_allowed_group.is_some();
    let config_evidence = format!(
        "user={}; group={}; forwardHeader={}; groupHeader={}",
        authz::mask_value(state.config.auth_allowed_user.as_deref())
            .unwrap_or_else(|| "unset".to_string()),
        authz::mask_value(state.config.auth_allowed_group.as_deref())
            .unwrap_or_else(|| "unset".to_string()),
        state.config.auth_forward_header_name,
        state.config.auth_group_header_name,
    );
    let config_check = if configured {
        deploy_check_pass(
            "core.forward_auth_authorization_config",
            "项目鉴权目标已配置",
            "authorization target is configured",
            "未配置允许用户/组时，生产环境无法判断谁可访问 Dockrev",
            config_evidence,
        )
    } else {
        deploy_check_fail(
            "core.forward_auth_authorization_config",
            "项目鉴权目标已配置",
            "authorization target is missing",
            "未配置允许用户/组时，生产环境无法判断谁可访问 Dockrev",
            config_evidence,
            "设置 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`（二选一或同时设置）；开发模式仅建议本地临时使用。",
        )
    };

    let user = headers
        .get(&state.config.auth_forward_header_name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let groups = headers
        .get(&state.config.auth_group_header_name)
        .and_then(|value| value.to_str().ok())
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let request_evidence = format!(
        "currentUser={}; currentGroups={}",
        authz::mask_value(user.as_deref()).unwrap_or_else(|| "missing".to_string()),
        if groups.is_empty() {
            "missing".to_string()
        } else {
            authz::mask_list(&groups).join(",")
        },
    );
    let request_check = match auth {
        Ok(request_auth) => deploy_check_pass(
            "core.forward_auth_request_authorization",
            "当前请求满足项目鉴权",
            "request is authorized for Dockrev",
            "不满足时，当前访问者会被重定向到自检页或 401 提示页",
            format!(
                "{request_evidence}; matchedBy={}",
                authz_match_label(&request_auth.matched_by)
            ),
        ),
        Err(_) => deploy_check_fail(
            "core.forward_auth_request_authorization",
            "当前请求满足项目鉴权",
            "request is not authorized for Dockrev",
            "不满足时，当前访问者会被重定向到自检页或 401 提示页",
            request_evidence,
            "确认 Traefik/Authelia 已注入正确的 Forward Auth 用户/组头，并让当前身份命中 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`。",
        ),
    };

    vec![config_check, request_check]
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

fn map_ghcr_sync_enqueue_error(err: anyhow::Error) -> ApiError {
    for cause in err.chain() {
        match cause.to_string().as_str() {
            "invalid fullName" => return ApiError::invalid_argument("invalid fullName"),
            "no tracked repos selected" => {
                return ApiError::invalid_argument("no tracked repos selected");
            }
            "repo is not selected" => return ApiError::invalid_argument("repo is not selected"),
            "repo is not tracked" => return ApiError::not_found("repo is not tracked"),
            "repo unregister in progress" => {
                return ApiError::conflict("repo unregister in progress");
            }
            _ => {}
        }
    }

    map_internal(err)
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
        Some(v) => {
            let trimmed = v.trim();
            is_mask_literal(trimmed) || trimmed.is_empty()
        }
    };
    if keep {
        *target = existing;
    }
}

fn merge_telegram_chat_id(target: &mut Option<String>, existing: Option<String>) {
    match target.take() {
        None => *target = existing,
        Some(value) => {
            let trimmed = value.trim();
            if is_mask_literal(trimmed) {
                *target = existing;
            } else if trimmed.is_empty() {
                *target = None;
            } else {
                *target = Some(trimmed.to_string());
            }
        }
    }
}

fn is_mask_literal(value: &str) -> bool {
    value == "******" || value == "••••••••••••••••"
}
