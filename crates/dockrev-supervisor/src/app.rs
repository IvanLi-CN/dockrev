use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{
    config::Config,
    docker_exec::{
        TargetRuntime, compose_up, docker_image_repo_digest, docker_image_semver_tag_ref_to_pull,
        docker_pull, resolve_target,
    },
    state_store::{
        LogLine, Progress, RequestParams, StateFile, load_or_idle, now_rfc3339, store_atomic,
    },
};

#[derive(Clone)]
pub struct App {
    pub cfg: Config,
    runtime: Arc<Mutex<Runtime>>,
}

const UI_FAVICON_PNG: &[u8] = include_bytes!("../../../web/public/favicon.png");

struct Runtime {
    state: StateFile,
    running_key: Option<StartKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StartKey {
    tag: String,
    digest: Option<String>,
    mode: String,
    rollback_on_failure: bool,
}

const MAX_LOG_OPERATION_GROUPS: usize = 30;

#[derive(Clone, Debug)]
struct OperationLogsGroup {
    op_id: String,
    started_at: String,
    updated_at: String,
    logs: Vec<LogLine>,
}

impl App {
    pub async fn new(cfg: Config) -> anyhow::Result<Self> {
        let loaded = load_or_idle(&cfg.state_path).await?;
        let mut state = loaded;
        let mut should_store = false;

        // If we crashed while running, surface it as failed but keep opId/logs for recovery.
        if state.state == "running" {
            let now = now_rfc3339()?;
            state.state = "failed".to_string();
            state.updated_at = now.clone();
            state.progress = Progress {
                step: "postcheck".to_string(),
                message: "supervisor restarted; previous operation interrupted".to_string(),
            };
            append_log_line(
                &mut state,
                &now,
                "ERROR",
                "supervisor restarted; previous operation interrupted",
            );
            should_store = true;
        }

        let log_count_before = state.logs.len();
        retain_recent_operation_logs(&mut state, MAX_LOG_OPERATION_GROUPS);
        if state.logs.len() != log_count_before {
            should_store = true;
        }

        if should_store {
            store_atomic(&cfg.state_path, &state).await?;
        }

        Ok(Self {
            cfg,
            runtime: Arc::new(Mutex::new(Runtime {
                state,
                running_key: None,
            })),
        })
    }

    pub fn router(self: Arc<Self>) -> Router {
        let base = self.cfg.base_path.clone();
        let api = Router::new()
            .route("/health", get(health))
            .route("/version", get(version))
            .route(
                "/self-upgrade",
                get(get_self_upgrade).post(post_self_upgrade),
            )
            .route("/self-upgrade/rollback", post(post_self_upgrade_rollback))
            .route("/favicon.png", get(ui_favicon))
            .route("/", get(ui_index))
            .with_state(self);
        Router::new().nest(&base, api)
    }

    async fn start_op(&self, req: StartSelfUpgradeRequest) -> Result<String, ApiError> {
        let key = StartKey {
            tag: req.target.tag.clone(),
            digest: req.target.digest.clone().map(normalize_digest),
            mode: req.mode.clone(),
            rollback_on_failure: req.rollback_on_failure,
        };

        let mut rt = self.runtime.lock().await;
        if rt.state.state == "running" {
            if rt.running_key.as_ref() == Some(&key) {
                return Ok(rt.state.op_id.clone());
            }
            return Err(ApiError::conflict(
                "已有运行中的 self-upgrade，请等待完成或先回滚/结束后再发起",
            ));
        }

        let now = now_rfc3339().map_err(ApiError::internal)?;
        let op_id = format!("sup_{}", ulid::Ulid::new());

        rt.state.schema_version = 1;
        rt.state.op_id = op_id.clone();
        rt.state.state = "running".to_string();
        rt.state.request = Some(RequestParams {
            mode: req.mode.clone(),
            rollback_on_failure: req.rollback_on_failure,
        });
        rt.state.target.tag = req.target.tag.clone();
        rt.state.target.digest = req.target.digest.clone().map(normalize_digest);
        rt.state.started_at = now.clone();
        rt.state.updated_at = now.clone();
        rt.state.progress = Progress {
            step: "precheck".to_string(),
            message: "starting".to_string(),
        };
        append_log_line(&mut rt.state, &now, "INFO", "self-upgrade requested");
        retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
        rt.running_key = Some(key.clone());

        store_atomic(&self.cfg.state_path, &rt.state)
            .await
            .map_err(ApiError::internal)?;

        let app = Arc::new(self.clone_for_task());
        tokio::spawn(async move {
            if let Err(err) = run_operation(app.clone(), key).await {
                tracing::error!(error = %err, "self-upgrade background task failed");
                mark_failed_if_running(app.as_ref(), err).await;
            }
        });

        Ok(op_id)
    }

    fn clone_for_task(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

fn normalize_digest(input: String) -> String {
    let t = input.trim().to_string();
    if t.starts_with("sha256:") {
        t
    } else {
        format!("sha256:{t}")
    }
}

fn log_boundary_starts_operation(msg: &str) -> bool {
    msg.contains("self-upgrade requested")
}

fn normalized_log_op_id(log: &LogLine) -> Option<String> {
    log.op_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn append_log_line(st: &mut StateFile, now: &str, level: &str, msg: impl Into<String>) {
    let op_id = st.op_id.trim();
    let op_id = if op_id.is_empty() {
        None
    } else {
        Some(op_id.to_string())
    };
    st.logs.push(LogLine {
        ts: now.to_string(),
        level: level.to_string(),
        msg: msg.into(),
        op_id,
    });
}

fn stable_legacy_group_id(log: &LogLine) -> String {
    let source = if !log.ts.trim().is_empty() {
        log.ts.as_str()
    } else if !log.msg.trim().is_empty() {
        log.msg.as_str()
    } else {
        "legacy"
    };
    let mut out = String::with_capacity(source.len());
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    format!("legacy-{out}")
}

fn escape_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_html_attr(input: &str) -> String {
    escape_html_text(input)
}

fn build_operation_groups(logs: &[LogLine]) -> Vec<OperationLogsGroup> {
    let mut groups: Vec<OperationLogsGroup> = Vec::new();
    let mut legacy_active: Option<String> = None;

    for log in logs {
        let group_id = if let Some(op_id) = normalized_log_op_id(log) {
            legacy_active = None;
            op_id
        } else if log_boundary_starts_operation(&log.msg) {
            let id = stable_legacy_group_id(log);
            legacy_active = Some(id.clone());
            id
        } else if let Some(id) = legacy_active.clone() {
            id
        } else {
            let id = stable_legacy_group_id(log);
            legacy_active = Some(id.clone());
            id
        };

        let should_append = groups.last().map(|g| g.op_id.as_str()) == Some(group_id.as_str());
        if should_append {
            if let Some(last) = groups.last_mut() {
                last.updated_at = log.ts.clone();
                last.logs.push(log.clone());
            }
            continue;
        }

        groups.push(OperationLogsGroup {
            op_id: group_id,
            started_at: log.ts.clone(),
            updated_at: log.ts.clone(),
            logs: vec![log.clone()],
        });
    }

    groups
}

fn infer_operation_state(group: &OperationLogsGroup, st: &StateFile) -> String {
    if group.op_id == st.op_id
        && matches!(
            st.state.as_str(),
            "running" | "succeeded" | "failed" | "rolled_back"
        )
    {
        return st.state.clone();
    }

    // For historical groups, prefer the latest terminal marker in log order.
    for line in group.logs.iter().rev() {
        if line.msg.contains("rolled back") {
            return "rolled_back".to_string();
        }
        if line.level.eq_ignore_ascii_case("ERROR") {
            return "failed".to_string();
        }
        if line.msg.contains("dry-run done") || line.msg.contains("succeeded") {
            return "succeeded".to_string();
        }
    }

    "unknown".to_string()
}

fn retain_recent_operation_logs(st: &mut StateFile, max_groups: usize) {
    if st.logs.is_empty() {
        return;
    }
    let groups = build_operation_groups(&st.logs);
    if groups.len() <= max_groups {
        return;
    }
    let keep_from = groups.len().saturating_sub(max_groups);
    let mut kept: Vec<LogLine> = Vec::new();
    for group in groups.into_iter().skip(keep_from) {
        kept.extend(group.logs.into_iter());
    }
    st.logs = kept;
}

async fn mark_failed_if_running(app: &App, err: anyhow::Error) {
    let now = now_rfc3339()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().unix_timestamp().to_string());
    let mut rt = app.runtime.lock().await;
    if rt.state.state == "running" {
        let step = rt.state.progress.step.clone();
        rt.state.state = "failed".to_string();
        rt.state.progress = Progress {
            step,
            message: format!("failed: {err}"),
        };
        rt.state.updated_at = now.clone();
        append_log_line(&mut rt.state, &now, "ERROR", err.to_string());
    }
    retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
    rt.running_key = None;

    if let Err(e) = store_atomic(&app.cfg.state_path, &rt.state).await {
        tracing::error!(error = %e, "failed to persist supervisor state after background failure");
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

const DEFAULT_REPOSITORY_URL: &str = "https://github.com/IvanLi-CN/dockrev";
const DEFAULT_DEVELOPER_NAME: &str = "Ivan Li";
const DEFAULT_DEVELOPER_URL: &str = "https://github.com/IvanLi-CN";

#[derive(Clone, Debug)]
struct SupervisorMeta {
    version: String,
    repository: String,
    developer_name: String,
    developer_url: String,
    release_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionResponse {
    version: String,
    repository: String,
    developer_name: String,
    developer_url: String,
}

async fn version(State(_app): State<Arc<App>>) -> impl IntoResponse {
    let meta = supervisor_meta();
    Json(VersionResponse {
        version: meta.version,
        repository: meta.repository,
        developer_name: meta.developer_name,
        developer_url: meta.developer_url,
    })
}

fn supervisor_meta() -> SupervisorMeta {
    let app_effective_version = std::env::var("APP_EFFECTIVE_VERSION").ok();
    build_supervisor_meta(
        app_effective_version.as_deref(),
        env!("CARGO_PKG_VERSION"),
        option_env!("CARGO_PKG_REPOSITORY"),
        option_env!("CARGO_PKG_AUTHORS"),
        option_env!("CARGO_PKG_HOMEPAGE"),
    )
}

fn build_supervisor_meta(
    app_effective_version: Option<&str>,
    package_version: &str,
    package_repository: Option<&str>,
    package_authors: Option<&str>,
    package_homepage: Option<&str>,
) -> SupervisorMeta {
    let version = trimmed_non_empty(app_effective_version)
        .unwrap_or(package_version)
        .to_string();

    let package_repository = trimmed_non_empty(package_repository);
    let repository = package_repository
        .unwrap_or(DEFAULT_REPOSITORY_URL)
        .to_string();

    let developer_name = parse_first_author(package_authors)
        .or_else(|| {
            if package_repository.is_some() {
                github_owner_from_repo(&repository)
            } else {
                None
            }
        })
        .unwrap_or(DEFAULT_DEVELOPER_NAME.to_string());

    let developer_url = trimmed_non_empty(package_homepage)
        .map(ToString::to_string)
        .or_else(|| {
            if package_repository.is_some() {
                github_owner_profile_url(&repository)
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_DEVELOPER_URL.to_string());

    let release_url = github_release_url(&repository, &version);

    SupervisorMeta {
        version,
        repository,
        developer_name,
        developer_url,
        release_url,
    }
}

fn trimmed_non_empty(input: Option<&str>) -> Option<&str> {
    input.map(str::trim).filter(|s| !s.is_empty())
}

fn parse_first_author(authors: Option<&str>) -> Option<String> {
    let raw = trimmed_non_empty(authors)?;
    for item in raw.split(':') {
        let candidate = item.trim();
        if candidate.is_empty() {
            continue;
        }
        let normalized = candidate
            .split_once('<')
            .map(|(name, _)| name.trim())
            .unwrap_or(candidate);
        if !normalized.is_empty() {
            return Some(normalized.to_string());
        }
    }
    None
}

fn github_owner_from_repo(repo: &str) -> Option<String> {
    let normalized = normalize_github_repo_url(repo)?;
    let without_host = normalized.strip_prefix("https://github.com/")?;
    let owner = without_host.split('/').next()?.trim();
    if owner.is_empty() {
        None
    } else {
        Some(owner.to_string())
    }
}

fn github_owner_profile_url(repo: &str) -> Option<String> {
    let owner = github_owner_from_repo(repo)?;
    Some(format!("https://github.com/{owner}"))
}

fn github_release_url(repo: &str, version: &str) -> Option<String> {
    let normalized_repo = normalize_github_repo_url(repo)?;
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(format!("{normalized_repo}/releases/tag/{version}"))
}

fn normalize_github_repo_url(repo: &str) -> Option<String> {
    let trimmed = repo.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let without_host = without_scheme.strip_prefix("github.com/")?;
    let without_git = without_host.strip_suffix(".git").unwrap_or(without_host);
    let mut parts = without_git.split('/').filter(|s| !s.trim().is_empty());
    let owner = parts.next()?.trim();
    let name = parts.next()?.trim();
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{name}"))
}

async fn ui_favicon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], UI_FAVICON_PNG)
}

fn require_user(app: &App, headers: &HeaderMap) -> Result<String, ApiError> {
    let name = app.cfg.auth_forward_header_name.as_str();
    let Some(v) = headers.get(name) else {
        return Err(ApiError::auth_required());
    };
    let user = v.to_str().unwrap_or("").trim();
    if user.is_empty() {
        return Err(ApiError::auth_required());
    }
    Ok(user.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelfUpgradeResponse {
    state: String,
    op_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<HttpRequestParams>,
    target: HttpTarget,
    previous: HttpPrevious,
    started_at: String,
    updated_at: String,
    progress: Progress,
    logs: Vec<LogLine>,
    operations: Vec<SelfUpgradeOperation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelfUpgradeOperation {
    op_id: String,
    state: String,
    started_at: String,
    updated_at: String,
    logs: Vec<LogLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpRequestParams {
    mode: String,
    rollback_on_failure: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpTarget {
    image: String,
    tag: String,
    digest: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpPrevious {
    tag: String,
    digest: Option<String>,
}

fn build_response_operations(st: &StateFile) -> Vec<SelfUpgradeOperation> {
    let mut operations = Vec::new();
    for group in build_operation_groups(&st.logs).into_iter().rev() {
        operations.push(SelfUpgradeOperation {
            op_id: group.op_id.clone(),
            state: infer_operation_state(&group, st),
            started_at: group.started_at,
            updated_at: group.updated_at,
            logs: group.logs,
        });
    }
    operations
}

async fn get_self_upgrade(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Json<SelfUpgradeResponse>, ApiError> {
    let _user = require_user(&app, &headers)?;
    let rt = app.runtime.lock().await;
    let st = &rt.state;
    let request = if st.state == "running" {
        st.request.as_ref().map(|req| HttpRequestParams {
            mode: req.mode.clone(),
            rollback_on_failure: req.rollback_on_failure,
        })
    } else {
        None
    };
    Ok(Json(SelfUpgradeResponse {
        state: st.state.clone(),
        op_id: st.op_id.clone(),
        request,
        target: HttpTarget {
            image: app.cfg.target_image_repo.clone(),
            tag: st.target.tag.clone(),
            digest: st.target.digest.clone(),
        },
        previous: HttpPrevious {
            tag: st.previous.tag.clone(),
            digest: st.previous.digest.clone(),
        },
        started_at: st.started_at.clone(),
        updated_at: st.updated_at.clone(),
        progress: st.progress.clone(),
        logs: st.logs.clone(),
        operations: build_response_operations(st),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartSelfUpgradeRequest {
    target: StartTarget,
    mode: String,
    rollback_on_failure: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartTarget {
    tag: String,
    #[serde(default)]
    digest: Option<String>,
}

async fn post_self_upgrade(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(req): Json<StartSelfUpgradeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _user = require_user(&app, &headers)?;
    if req.target.tag.trim().is_empty() {
        return Err(ApiError::invalid_argument("target.tag is required"));
    }
    if req.mode != "apply" && req.mode != "dry-run" {
        return Err(ApiError::invalid_argument("mode must be apply|dry-run"));
    }

    let op_id = app.start_op(req).await?;
    Ok(Json(json!({ "opId": op_id })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackRequest {
    op_id: String,
}

async fn post_self_upgrade_rollback(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _user = require_user(&app, &headers)?;

    let mut rt = app.runtime.lock().await;
    if rt.state.state == "running" {
        return Err(ApiError::conflict("self-upgrade is running"));
    }
    if rt.state.op_id != req.op_id {
        return Err(ApiError::invalid_argument("opId not found"));
    }
    if rt.state.previous.digest.is_none() && rt.state.previous.tag == "unknown" {
        return Err(ApiError::conflict("no rollback target available"));
    }

    // Spawn rollback-only path by reusing current target runtime discovery.
    let now = now_rfc3339().map_err(ApiError::internal)?;
    rt.state.state = "running".to_string();
    rt.state.request = None;
    rt.state.progress = Progress {
        step: "rollback".to_string(),
        message: "manual rollback".to_string(),
    };
    rt.state.updated_at = now.clone();
    append_log_line(&mut rt.state, &now, "WARN", "manual rollback requested");
    retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
    store_atomic(&app.cfg.state_path, &rt.state)
        .await
        .map_err(ApiError::internal)?;

    let app2 = Arc::new(app.as_ref().clone_for_task());
    let prev = rt.state.previous.clone();
    tokio::spawn(async move {
        let _ = run_rollback_only(app2, prev).await;
    });

    Ok(Json(json!({ "ok": true })))
}

async fn ui_index(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Html<String>, ApiError> {
    let _user = require_user(&app, &headers)?;
    let meta = supervisor_meta();
    Ok(Html(render_ui(&app.cfg.base_path, &meta)))
}

fn render_ui(base_path: &str, meta: &SupervisorMeta) -> String {
    let version_html = if let Some(release_url) = trimmed_non_empty(meta.release_url.as_deref()) {
        format!(
            r#"<a href="{url}" target="_blank" rel="noopener noreferrer"><code>{value}</code></a>"#,
            url = escape_html_attr(release_url),
            value = escape_html_text(&meta.version)
        )
    } else {
        format!("<code>{}</code>", escape_html_text(&meta.version))
    };
    let repository_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.repository),
        value = escape_html_text(&meta.repository)
    );
    let developer_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.developer_url),
        value = escape_html_text(&meta.developer_name)
    );

    // Minimal, dependency-free console. Uses same-origin fetch under base_path.
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev Supervisor</title>
    <link rel="icon" type="image/png" href="{base_path}/favicon.png" />
    <style>
      body {{ font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial; padding: 18px; max-width: 960px; margin: 0 auto; }}
      .row {{ display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }}
      .card {{ border: 1px solid rgba(0,0,0,0.12); border-radius: 12px; padding: 14px; margin-top: 12px; }}
      .muted {{ color: rgba(0,0,0,0.62); font-size: 12px; }}
      button {{ padding: 8px 12px; border-radius: 10px; border: 1px solid rgba(0,0,0,0.18); background: white; cursor: pointer; }}
      button[disabled] {{ opacity: 0.5; cursor: not-allowed; }}
      button.btnRunning {{ display: inline-flex; align-items: center; gap: 8px; }}
      button.btnRunning::before {{ content: ''; width: 12px; height: 12px; border-radius: 999px; border: 2px solid rgba(0,0,0,0.22); border-top-color: rgba(0,0,0,0.72); animation: supervisorButtonSpin 0.8s linear infinite; }}
      input {{ padding: 8px 10px; border-radius: 10px; border: 1px solid rgba(0,0,0,0.18); }}
      pre {{ background: rgba(0,0,0,0.06); padding: 10px; border-radius: 10px; overflow: auto; }}
      .tabsPanel {{ margin-top: 10px; }}
      .opTabs {{ display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; max-height: 40px; overflow: hidden; }}
      .opTabs.expanded {{ max-height: none; overflow: visible; }}
      .opTab {{ display: inline-flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: 10px; border: 1px solid rgba(0,0,0,0.14); background: rgba(255,255,255,0.7); font-size: 12px; }}
      .opTab.active {{ border-color: rgba(37,99,235,0.45); background: rgba(37,99,235,0.12); }}
      .opDot {{ width: 8px; height: 8px; border-radius: 999px; flex: 0 0 auto; }}
      .opDot-running {{ background: #2563eb; }}
      .opDot-succeeded {{ background: #16a34a; }}
      .opDot-failed {{ background: #dc2626; }}
      .opDot-rolled_back {{ background: #dc2626; }}
      .opDot-unknown {{ background: #6b7280; }}
      .newBadge {{ color: #b45309; border: 1px solid rgba(180,83,9,0.3); background: rgba(251,191,36,0.18); border-radius: 999px; padding: 1px 6px; font-size: 11px; }}
      #tabsToggle {{ padding: 4px 10px; font-size: 12px; }}
      .popWrap {{ position: relative; display: inline-flex; align-items: center; }}
      .popCard {{ position: absolute; top: calc(100% + 8px); left: 0; width: min(320px, calc(100vw - 36px)); border: 1px solid rgba(0,0,0,0.18); border-radius: 12px; background: #fff; box-shadow: 0 10px 28px rgba(0,0,0,0.16); padding: 10px; z-index: 20; }}
      .popTitle {{ margin: 0 0 6px; font-size: 13px; font-weight: 700; }}
      .popActions {{ display: flex; gap: 8px; justify-content: flex-end; margin-top: 10px; }}
      .danger {{ border-color: #dc2626; background: #dc2626; color: #fff; }}
      .ok {{ color: #16a34a; }}
      .bad {{ color: #dc2626; }}
      .metaLine {{ margin-top: 6px; display: flex; flex-wrap: wrap; gap: 8px 16px; }}
      .metaItem {{ color: rgba(0,0,0,0.68); font-size: 12px; }}
      .metaItem code {{ font-size: 12px; }}
      @keyframes supervisorButtonSpin {{ from {{ transform: rotate(0deg); }} to {{ transform: rotate(360deg); }} }}
      @media (prefers-reduced-motion: reduce) {{
        button.btnRunning::before {{ animation: none; }}
      }}
    </style>
  </head>
  <body>
    <div class="row" style="gap:12px;">
      <img src="{base_path}/favicon.png" alt="" aria-hidden="true" width="24" height="24" style="display:block" />
      <h1 style="margin:0;">Dockrev 自我升级（Supervisor）</h1>
    </div>
    <div class="muted">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用。</div>
    <div class="metaLine">
      <div class="metaItem">Supervisor 版本：{version_html}</div>
      <div class="metaItem">开源仓库：{repository_html}</div>
      <div class="metaItem">开发者：{developer_html}</div>
    </div>

    <div class="card">
      <div class="row">
        <div>Target tag:</div>
        <input id="tag" value="latest" />
        <button id="dry">预览（dry-run）</button>
        <button id="apply">开始升级（apply）</button>
        <div id="rollbackWrap" class="popWrap">
          <button id="rollback" aria-haspopup="dialog" aria-expanded="false">回滚</button>
          <div id="rollbackPop" class="popCard" role="dialog" aria-modal="false" hidden>
            <div class="popTitle">确认手动回滚？</div>
            <div class="muted">将尝试回滚到 previous digest，并可能触发容器重启。</div>
            <div class="muted">opId: <code id="rollbackOpId">-</code></div>
            <div class="popActions">
              <button id="rollbackCancel">取消</button>
              <button id="rollbackConfirm" class="danger">确认回滚</button>
            </div>
          </div>
        </div>
        <button id="refresh">刷新</button>
        <a href="/" style="margin-left:auto">返回 Dockrev</a>
      </div>
      <div class="muted">提示：失败将尝试回滚到 previous digest（如可用）。</div>
    </div>

    <div class="card">
      <div id="status" class="muted">loading…</div>
      <div class="tabsPanel">
        <div class="row" style="justify-content: space-between; gap: 8px;">
          <div id="tabHint" class="muted">loading…</div>
          <button id="tabsToggle" hidden>展开</button>
        </div>
        <div id="opTabs" class="opTabs"></div>
      </div>
      <pre id="logs"></pre>
    </div>

    <script>
      const base = {base_path_json};
      let activeOpId = null;
      let latestOpId = null;
      let tabsExpanded = false;
      let tabsCanExpand = false;
      let latestHasNewer = false;
      let lastKnownSelfUpgradeState = null;
      const toUrl = (p) => base.replace(/\/$/, '') + '/' + p.replace(/^\//, '');

	      async function fetchJson(path, init) {{
	        const resp = await fetch(toUrl(path), {{ ...init, headers: {{ 'Content-Type': 'application/json' }} }});
	        const text = await resp.text();
	        if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{text}}`);
	        return text ? JSON.parse(text) : null;
	      }}

	      const rollbackWrap = document.getElementById('rollbackWrap');
	      const dryBtn = document.getElementById('dry');
	      const applyBtn = document.getElementById('apply');
	      const rollbackBtn = document.getElementById('rollback');
	      const rollbackPop = document.getElementById('rollbackPop');
	      const rollbackOpId = document.getElementById('rollbackOpId');
	      const rollbackCancelBtn = document.getElementById('rollbackCancel');
	      const rollbackConfirmBtn = document.getElementById('rollbackConfirm');
	      let rollbackPopOpen = false;
	      let rollbackPendingOpId = null;

	      function canRollback(st) {{
	        return !!st.opId && (st.state === 'failed' || st.state === 'rolled_back' || st.state === 'succeeded');
	      }}

	      function setRollbackPopOpen(open) {{
	        rollbackPopOpen = open;
	        rollbackPop.hidden = !open;
	        rollbackBtn.setAttribute('aria-expanded', open ? 'true' : 'false');
	        if (!open) rollbackPendingOpId = null;
	      }}

	      function syncRollbackState(st) {{
	        const allowed = canRollback(st);
	        rollbackBtn.disabled = !allowed;
	        if (!allowed) {{
	          setRollbackPopOpen(false);
	          rollbackOpId.textContent = '-';
	          return;
	        }}
	        if (rollbackPopOpen) rollbackOpId.textContent = st.opId || '-';
	      }}
      function setRunningButton(button, running) {{
        button.classList.toggle('btnRunning', running);
        button.setAttribute('aria-busy', running ? 'true' : 'false');
      }}

      function syncUpgradeActionState(st) {{
        const running = !!st && st.state === 'running';
        const runningUpgrade = running && st?.progress?.step !== 'rollback';
        const mode = st?.request?.mode;
        dryBtn.disabled = running;
        applyBtn.disabled = running;
        setRunningButton(dryBtn, runningUpgrade && mode === 'dry-run');
        setRunningButton(applyBtn, runningUpgrade && mode === 'apply');
      }}
      function statusClass(st) {{
        const s = st && st.state;
        return s === 'succeeded' ? 'ok' : (s === 'failed' || s === 'rolled_back') ? 'bad' : '';
      }}

      function renderStatusText(st) {{
        const target = `${{st.target?.image}}:${{st.target?.tag}}${{st.target?.digest ? '@'+st.target.digest : ''}}`;
        const prev = `${{st.previous?.tag}}${{st.previous?.digest ? '@'+st.previous.digest : ''}}`;
        return `${{st.state}} · opId=${{st.opId||'-'}} · step=${{st.progress?.step}} · target=${{target}} · previous=${{prev}}`;
      }}

      function formatLogs(logs) {{
        return (logs || []).map(l => `[${{l.ts}}] ${{l.level}} ${{l.msg}}`).join('\n');
      }}

      function pad2(v) {{
        return String(v).padStart(2, '0');
      }}

      function formatTabTime(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-- --:--';
        return `${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}`;
      }}

      function formatTabLabel(opId, startedAt) {{
        const suffix = String(opId || '-').slice(-6);
        return `${{formatTabTime(startedAt)}} · ${{suffix}}`;
      }}

      function measureTabsOverflow(tabsEl) {{
        const wasExpanded = tabsEl.classList.contains('expanded');
        if (wasExpanded) tabsEl.classList.remove('expanded');
        const overflow = tabsEl.scrollHeight > tabsEl.clientHeight + 1;
        if (wasExpanded) tabsEl.classList.add('expanded');
        return overflow;
      }}

      function syncTabsToggle() {{
        const tabsEl = document.getElementById('opTabs');
        const toggleEl = document.getElementById('tabsToggle');
        if (!tabsEl || !toggleEl) return;
        tabsEl.classList.toggle('expanded', tabsExpanded);
        tabsCanExpand = measureTabsOverflow(tabsEl);
        if (!tabsCanExpand) {{
          tabsExpanded = false;
          tabsEl.classList.remove('expanded');
        }}
        toggleEl.hidden = !tabsCanExpand;
        toggleEl.textContent = tabsExpanded ? '收起' : '展开';
      }}

      function renderOperations(st) {{
        const operations = Array.isArray(st.operations) ? st.operations : [];
        const tabsEl = document.getElementById('opTabs');
        const hintEl = document.getElementById('tabHint');
        const logsEl = document.getElementById('logs');
        tabsEl.textContent = '';

        if (!operations.length) {{
          activeOpId = null;
          latestOpId = null;
          latestHasNewer = false;
          hintEl.textContent = '暂无分组日志';
          logsEl.textContent = formatLogs(st.logs || []);
          requestAnimationFrame(syncTabsToggle);
          return;
        }}

        const previousLatest = latestOpId;
        const nextLatest = operations[0]?.opId || null;
        const wasViewingLatest = !activeOpId || (previousLatest && activeOpId === previousLatest);
        if (nextLatest && wasViewingLatest) {{
          activeOpId = nextLatest;
        }} else if (!operations.some((op) => op.opId === activeOpId)) {{
          activeOpId = nextLatest;
        }}
        if (!wasViewingLatest && previousLatest && nextLatest && previousLatest !== nextLatest) {{
          latestHasNewer = true;
        }}
        latestOpId = nextLatest;
        if (activeOpId && activeOpId === latestOpId) {{
          latestHasNewer = false;
        }}

        for (let i = 0; i < operations.length; i += 1) {{
          const op = operations[i];
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = 'opTab';
          if (op.opId === activeOpId) {{
            btn.classList.add('active');
          }}
          btn.onclick = () => {{
            activeOpId = op.opId;
            if (activeOpId === latestOpId) {{
              latestHasNewer = false;
            }}
            renderOperations(st);
          }};

          const dot = document.createElement('span');
          dot.className = `opDot opDot-${{op.state || 'unknown'}}`;
          btn.appendChild(dot);

          const text = document.createElement('span');
          text.textContent = formatTabLabel(op.opId, op.startedAt);
          btn.appendChild(text);

          if (i === 0 && latestHasNewer && activeOpId !== op.opId) {{
            const badge = document.createElement('span');
            badge.className = 'newBadge';
            badge.textContent = '新';
            btn.appendChild(badge);
          }}

          tabsEl.appendChild(btn);
        }}

        const active = operations.find((op) => op.opId === activeOpId) || operations[0];
        logsEl.textContent = formatLogs(active.logs || []);
        hintEl.textContent = `operations: ${{operations.length}}（当前 ${{active.opId}}）`;
        requestAnimationFrame(syncTabsToggle);
      }}

      async function refresh() {{
        const statusEl = document.getElementById('status');
        try {{
          const st = await fetchJson('self-upgrade');
          lastKnownSelfUpgradeState = st;
          statusEl.className = `muted ${{statusClass(st)}}`.trim();
          statusEl.textContent = renderStatusText(st);
          syncUpgradeActionState(st);
          renderOperations(st);
          syncRollbackState(st);
        }} catch (e) {{
          statusEl.className = 'muted bad';
          statusEl.textContent = `offline ${{String(e.message||e)}}`;
          if (lastKnownSelfUpgradeState) syncUpgradeActionState(lastKnownSelfUpgradeState);
          setRollbackPopOpen(false);
        }}
      }}

      document.getElementById('refresh').onclick = () => refresh();
      dryBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'dry-run', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      applyBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'apply', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      document.getElementById('rollback').onclick = async (evt) => {{
        evt.preventDefault();
        if (rollbackBtn.disabled) return;
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
	        if (!canRollback(st)) {{
	          await refresh();
	          return;
	        }}
	        rollbackPendingOpId = st.opId || null;
	        rollbackOpId.textContent = rollbackPendingOpId || '-';
	        setRollbackPopOpen(true);
	      }};
      document.getElementById('tabsToggle').onclick = () => {{
        if (!tabsCanExpand) return;
        tabsExpanded = !tabsExpanded;
        syncTabsToggle();
      }};
      window.addEventListener('resize', () => {{
        requestAnimationFrame(syncTabsToggle);
      }});
      rollbackCancelBtn.onclick = () => {{
        setRollbackPopOpen(false);
      }};
      document.getElementById('rollbackConfirm').onclick = async () => {{
        if (!rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
        if (!canRollback(st)) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        if (!st.opId || st.opId !== rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        rollbackConfirmBtn.disabled = true;
        rollbackCancelBtn.disabled = true;
        try {{
          await fetchJson('self-upgrade/rollback', {{ method: 'POST', body: JSON.stringify({{ opId: rollbackPendingOpId }}) }});
          setRollbackPopOpen(false);
          await refresh();
        }} finally {{
          rollbackConfirmBtn.disabled = false;
          rollbackCancelBtn.disabled = false;
        }}
      }};
      document.addEventListener('click', (evt) => {{
        if (!rollbackPopOpen) return;
        const target = evt.target;
        if (!rollbackWrap.contains(target)) setRollbackPopOpen(false);
      }});
      document.addEventListener('keydown', (evt) => {{
        if (evt.key === 'Escape' && rollbackPopOpen) {{
          evt.preventDefault();
          setRollbackPopOpen(false);
          rollbackBtn.focus();
        }}
      }});

      refresh();
      setInterval(refresh, 1500);
    </script>
  </body>
</html>"#,
        base_path = base_path,
        version_html = version_html,
        repository_html = repository_html,
        developer_html = developer_html,
        base_path_json =
            serde_json::to_string(base_path).unwrap_or_else(|_| "\"/supervisor\"".to_string())
    )
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn auth_required() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "auth_required",
            message: "auth required".to_string(),
        }
    }
    fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_argument",
            message: msg.into(),
        }
    }
    fn conflict(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            message: msg.into(),
        }
    }
    fn internal(e: impl Into<anyhow::Error>) -> Self {
        let err = e.into();
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body =
            Json(json!({ "error": { "code": self.code, "message": self.message, "details": {} } }));
        (self.status, body).into_response()
    }
}

async fn run_operation(app: Arc<App>, key: StartKey) -> anyhow::Result<()> {
    let target = resolve_target(&app.cfg).await?;

    let image_ref = if let Some(d) = key.digest.as_deref() {
        format!("{}@{}", app.cfg.target_image_repo, d)
    } else {
        format!("{}:{}", app.cfg.target_image_repo, key.tag)
    };

    let current_digest = docker_image_repo_digest(
        &app.cfg,
        &target.current_image_id,
        &app.cfg.target_image_repo,
    )
    .await?;
    let previous_tag = if target.current_image_ref.trim().is_empty() {
        "unknown".to_string()
    } else {
        target.current_image_ref.clone()
    };

    update_state(&app, |st, now| {
        st.previous.tag = previous_tag;
        st.previous.digest = current_digest.clone();
        st.progress = Progress {
            step: "pull".to_string(),
            message: "pulling image".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", format!("pull {image_ref}"));
    })
    .await?;

    docker_pull(&app.cfg, &image_ref, Duration::from_secs(300)).await?;

    match docker_image_semver_tag_ref_to_pull(&app.cfg, &image_ref, &app.cfg.target_image_repo)
        .await
    {
        Ok(Some(tag_ref)) => {
            update_state(&app, |st, now| {
                append_log_line(
                    st,
                    now,
                    "INFO",
                    format!("best-effort pull semver tag {tag_ref}"),
                );
            })
            .await?;

            if let Err(e) = docker_pull(&app.cfg, &tag_ref, Duration::from_secs(300)).await {
                update_state(&app, |st, now| {
                    append_log_line(
                        st,
                        now,
                        "WARN",
                        format!("semver tag pull failed: {tag_ref}: {e}"),
                    );
                })
                .await?;
            }
        }
        Ok(None) => {}
        Err(e) => {
            update_state(&app, |st, now| {
                append_log_line(st, now, "WARN", format!("semver tag pull skipped: {e}"));
            })
            .await?;
        }
    }

    if key.mode == "dry-run" {
        update_state(&app, |st, now| {
            st.state = "succeeded".to_string();
            st.progress = Progress {
                step: "done".to_string(),
                message: "dry-run completed".to_string(),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "INFO", "dry-run done");
        })
        .await?;
        clear_running(&app).await;
        return Ok(());
    }

    let override_path = override_file_path(&app.cfg.state_path)?;
    write_override(&override_path, &target.compose_service, &image_ref).await?;

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "apply".to_string(),
            message: "docker compose up".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", "compose up");
    })
    .await?;

    let apply_result =
        compose_up(&app.cfg, &target, &override_path, Duration::from_secs(600)).await;
    if let Err(e) = apply_result {
        return fail_and_maybe_rollback(app, target, key, current_digest, e).await;
    }

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "wait_healthy".to_string(),
            message: "waiting /api/health".to_string(),
        };
        st.updated_at = now.to_string();
    })
    .await?;

    let post_target = match wait_dockrev_health(&app.cfg, Duration::from_secs(180)).await {
        Ok(v) => v,
        Err(e) => return fail_and_maybe_rollback(app, target, key, current_digest, e).await,
    };

    update_state(&app, |st, now| {
        st.progress = Progress {
            step: "postcheck".to_string(),
            message: "fetching /api/version".to_string(),
        };
        st.updated_at = now.to_string();
    })
    .await?;

    let _ = fetch_dockrev_version(&post_target).await;

    update_state(&app, |st, now| {
        st.state = "succeeded".to_string();
        st.progress = Progress {
            step: "done".to_string(),
            message: "ok".to_string(),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "INFO", "succeeded");
    })
    .await?;

    clear_running(&app).await;
    Ok(())
}

fn rollback_image_ref(
    target_image_repo: &str,
    previous: &crate::state_store::PreviousRef,
) -> anyhow::Result<String> {
    if let Some(d) = previous.digest.as_deref() {
        return Ok(format!("{target_image_repo}@{d}"));
    }

    let t = previous.tag.trim();
    if t.is_empty() || t == "unknown" {
        return Err(anyhow::anyhow!("no rollback target available"));
    }

    if t == target_image_repo
        || t.starts_with(&format!("{target_image_repo}:"))
        || t.starts_with(&format!("{target_image_repo}@"))
        || t.contains(['/', ':', '@'])
    {
        return Ok(t.to_string());
    }

    Ok(format!("{target_image_repo}:{t}"))
}

async fn run_rollback_only(
    app: Arc<App>,
    previous: crate::state_store::PreviousRef,
) -> anyhow::Result<()> {
    let result: anyhow::Result<()> = async {
        let target = resolve_target(&app.cfg).await?;
        let image_ref = rollback_image_ref(&app.cfg.target_image_repo, &previous)?;
        let override_path = override_file_path(&app.cfg.state_path)?;
        write_override(&override_path, &target.compose_service, &image_ref).await?;

        compose_up(&app.cfg, &target, &override_path, Duration::from_secs(600)).await?;
        let _ = wait_dockrev_health(&app.cfg, Duration::from_secs(180)).await?;

        update_state(&app, |st, now| {
            st.state = "rolled_back".to_string();
            st.progress = Progress {
                step: "done".to_string(),
                message: "rolled back".to_string(),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "WARN", "rolled back");
        })
        .await?;

        Ok(())
    }
    .await;

    if let Err(err) = result {
        let _ = update_state(&app, |st, now| {
            st.state = "failed".to_string();
            st.progress = Progress {
                step: "rollback".to_string(),
                message: format!("rollback failed: {err}"),
            };
            st.updated_at = now.to_string();
            append_log_line(st, now, "ERROR", format!("rollback failed: {err}"));
        })
        .await;
    }

    clear_running(&app).await;
    Ok(())
}

async fn fail_and_maybe_rollback(
    app: Arc<App>,
    _target: TargetRuntime,
    key: StartKey,
    previous_digest: Option<String>,
    err: anyhow::Error,
) -> anyhow::Result<()> {
    update_state(&app, |st, now| {
        st.state = "failed".to_string();
        st.progress = Progress {
            step: "rollback".to_string(),
            message: format!("failed: {err}"),
        };
        st.updated_at = now.to_string();
        append_log_line(st, now, "ERROR", err.to_string());
    })
    .await?;

    if !key.rollback_on_failure {
        clear_running(&app).await;
        return Ok(());
    }

    let prev_tag = {
        let rt = app.runtime.lock().await;
        rt.state.previous.tag.clone()
    };
    let prev = crate::state_store::PreviousRef {
        tag: prev_tag,
        digest: previous_digest,
    };
    let _ = run_rollback_only(app.clone(), prev).await;
    Ok(())
}

async fn update_state(app: &App, f: impl FnOnce(&mut StateFile, &str)) -> anyhow::Result<()> {
    let now = now_rfc3339()?;
    let mut rt = app.runtime.lock().await;
    f(&mut rt.state, &now);
    retain_recent_operation_logs(&mut rt.state, MAX_LOG_OPERATION_GROUPS);
    store_atomic(&app.cfg.state_path, &rt.state).await?;
    Ok(())
}

async fn clear_running(app: &App) {
    let mut rt = app.runtime.lock().await;
    rt.running_key = None;
}

fn override_file_path(state_path: &Path) -> anyhow::Result<PathBuf> {
    let dir = state_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid state path"))?;
    Ok(dir.join("self-upgrade.override.yml"))
}

async fn write_override(path: &Path, service: &str, image: &str) -> anyhow::Result<()> {
    let body = format!(
        "services:\n  {service}:\n    image: {image}\n",
        service = service,
        image = image
    );
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, body).await?;
    Ok(())
}

async fn wait_dockrev_health(cfg: &Config, timeout: Duration) -> anyhow::Result<TargetRuntime> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()?;
    let started = std::time::Instant::now();
    let mut last_error: Option<String> = None;

    while started.elapsed() < timeout {
        match resolve_target(cfg).await {
            Ok(target) => {
                let url = format!(
                    "http://{}:{}/api/health",
                    target.container_ip, target.dockrev_http_port
                );
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => return Ok(target),
                    Ok(resp) => {
                        last_error = Some(format!("HTTP {} {}", resp.status().as_u16(), url))
                    }
                    Err(e) => last_error = Some(format!("{e} {url}")),
                }
            }
            Err(e) => last_error = Some(e.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }

    Err(anyhow::anyhow!(
        "timeout waiting for dockrev health; last_error={}",
        last_error.unwrap_or_else(|| "none".to_string())
    ))
}

async fn fetch_dockrev_version(target: &TargetRuntime) -> Option<String> {
    let url = format!(
        "http://{}:{}/api/version",
        target.container_ip, target.dockrev_http_port
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<serde_json::Value>().await.ok()?;
    parsed
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_image_ref_handles_full_refs_and_plain_tags() {
        let repo = "dockrev";

        let p = crate::state_store::PreviousRef {
            tag: "local".to_string(),
            digest: None,
        };
        assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev:local");

        let p = crate::state_store::PreviousRef {
            tag: "dockrev:local".to_string(),
            digest: None,
        };
        assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev:local");

        let p = crate::state_store::PreviousRef {
            tag: "dockrev".to_string(),
            digest: None,
        };
        assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev");

        let p = crate::state_store::PreviousRef {
            tag: "v0.1.0".to_string(),
            digest: Some("sha256:abc".to_string()),
        };
        assert_eq!(rollback_image_ref(repo, &p).unwrap(), "dockrev@sha256:abc");

        let p = crate::state_store::PreviousRef {
            tag: "unknown".to_string(),
            digest: None,
        };
        assert!(rollback_image_ref(repo, &p).is_err());
    }

    #[test]
    fn render_ui_joins_logs_with_real_newlines() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains(r".join('\n')"));
        assert!(!html.contains(r".join('\\n')"));
    }

    fn test_meta() -> SupervisorMeta {
        build_supervisor_meta(
            Some("0.9.0"),
            "0.1.0",
            Some("https://github.com/IvanLi-CN/dockrev"),
            Some("Ivan Li"),
            Some("https://github.com/IvanLi-CN"),
        )
    }

    fn test_log(ts: &str, level: &str, msg: &str, op_id: Option<&str>) -> LogLine {
        LogLine {
            ts: ts.to_string(),
            level: level.to_string(),
            msg: msg.to_string(),
            op_id: op_id.map(|v| v.to_string()),
        }
    }

    #[test]
    fn build_operation_groups_handles_legacy_and_opid_logs() {
        let logs = vec![
            test_log(
                "2026-02-01T00:00:01Z",
                "INFO",
                "self-upgrade requested",
                None,
            ),
            test_log("2026-02-01T00:00:02Z", "INFO", "pull image", None),
            test_log("2026-02-01T00:00:03Z", "INFO", "dry-run done", None),
            test_log(
                "2026-02-01T00:10:01Z",
                "INFO",
                "self-upgrade requested",
                Some("sup_a"),
            ),
            test_log(
                "2026-02-01T00:10:02Z",
                "ERROR",
                "compose failed",
                Some("sup_a"),
            ),
            test_log(
                "2026-02-01T00:20:01Z",
                "INFO",
                "self-upgrade requested",
                None,
            ),
            test_log("2026-02-01T00:20:02Z", "INFO", "pull image", None),
        ];

        let groups = build_operation_groups(&logs);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].op_id, "legacy-2026_02_01t00_00_01z");
        assert_eq!(groups[0].logs.len(), 3);
        assert_eq!(groups[1].op_id, "sup_a");
        assert_eq!(groups[1].logs.len(), 2);
        assert_eq!(groups[2].op_id, "legacy-2026_02_01t00_20_01z");
        assert_eq!(groups[2].logs.len(), 2);
    }

    #[test]
    fn retain_recent_operation_logs_caps_groups_to_thirty() {
        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        for i in 0..35 {
            st.logs.push(test_log(
                &format!("2026-02-01T01:{i:02}:00Z"),
                "INFO",
                &format!("self-upgrade requested op{i}"),
                None,
            ));
            st.logs.push(test_log(
                &format!("2026-02-01T01:{i:02}:10Z"),
                "INFO",
                &format!("succeeded op{i}"),
                None,
            ));
        }

        retain_recent_operation_logs(&mut st, MAX_LOG_OPERATION_GROUPS);
        let groups = build_operation_groups(&st.logs);
        assert_eq!(groups.len(), 30);
        assert!(groups[0].logs[0].msg.contains("op5"));
        assert!(groups[29].logs[0].msg.contains("op34"));
    }

    #[test]
    fn build_response_operations_returns_newest_first_with_state() {
        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        st.state = "running".to_string();
        st.op_id = "sup_live".to_string();
        st.logs = vec![
            test_log(
                "2026-02-01T02:00:00Z",
                "INFO",
                "self-upgrade requested",
                None,
            ),
            test_log("2026-02-01T02:00:01Z", "ERROR", "compose failed", None),
            test_log(
                "2026-02-01T02:05:00Z",
                "INFO",
                "self-upgrade requested",
                Some("sup_live"),
            ),
            test_log(
                "2026-02-01T02:05:01Z",
                "INFO",
                "pull image",
                Some("sup_live"),
            ),
        ];

        let ops = build_response_operations(&st);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].op_id, "sup_live");
        assert_eq!(ops[0].state, "running");
        assert_eq!(ops[1].state, "failed");
    }

    #[test]
    fn infer_operation_state_prefers_current_state_for_active_op() {
        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        st.state = "failed".to_string();
        st.op_id = "sup_same".to_string();
        st.logs = vec![
            test_log(
                "2026-02-01T02:00:00Z",
                "INFO",
                "self-upgrade requested",
                Some("sup_same"),
            ),
            test_log(
                "2026-02-01T02:00:01Z",
                "INFO",
                "succeeded",
                Some("sup_same"),
            ),
            test_log(
                "2026-02-01T02:00:02Z",
                "ERROR",
                "rollback failed: boom",
                Some("sup_same"),
            ),
        ];

        let ops = build_response_operations(&st);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].state, "failed");
    }

    #[test]
    fn infer_operation_state_prefers_latest_terminal_marker_for_history() {
        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        st.logs = vec![
            test_log(
                "2026-02-01T02:00:00Z",
                "INFO",
                "self-upgrade requested",
                Some("sup_old"),
            ),
            test_log(
                "2026-02-01T02:00:01Z",
                "ERROR",
                "pull failed once",
                Some("sup_old"),
            ),
            test_log("2026-02-01T02:00:02Z", "INFO", "succeeded", Some("sup_old")),
            test_log(
                "2026-02-01T02:01:00Z",
                "INFO",
                "self-upgrade requested",
                Some("sup_new"),
            ),
            test_log("2026-02-01T02:01:01Z", "INFO", "succeeded", Some("sup_new")),
            test_log(
                "2026-02-01T02:01:02Z",
                "ERROR",
                "rollback failed: boom",
                Some("sup_new"),
            ),
        ];

        let ops = build_response_operations(&st);
        assert_eq!(
            ops.iter().find(|o| o.op_id == "sup_old").unwrap().state,
            "succeeded"
        );
        assert_eq!(
            ops.iter().find(|o| o.op_id == "sup_new").unwrap().state,
            "failed"
        );
    }

    #[test]
    fn legacy_group_ids_stay_stable_after_retention() {
        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        for i in 0..31 {
            st.logs.push(test_log(
                &format!("2026-02-01T03:{i:02}:00Z"),
                "INFO",
                "self-upgrade requested",
                None,
            ));
            st.logs.push(test_log(
                &format!("2026-02-01T03:{i:02}:10Z"),
                "INFO",
                "dry-run done",
                None,
            ));
        }

        let before = build_operation_groups(&st.logs);
        let expected_first_id = before[1].op_id.clone();
        retain_recent_operation_logs(&mut st, MAX_LOG_OPERATION_GROUPS);
        let after = build_operation_groups(&st.logs);
        assert_eq!(after.len(), 30);
        assert_eq!(after[0].op_id, expected_first_id);
    }

    #[test]
    fn render_ui_contains_operation_tabs_markup() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains("id=\"opTabs\""));
        assert!(html.contains("id=\"tabsToggle\""));
        assert!(html.contains("latestHasNewer"));
    }

    #[test]
    fn render_ui_disables_dry_and_apply_during_running_operation() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains("function syncUpgradeActionState(st)"));
        assert!(html.contains("dryBtn.disabled = running;"));
        assert!(html.contains("applyBtn.disabled = running;"));
    }

    #[test]
    fn render_ui_shows_mode_specific_spinner_for_running_operation() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains("button.btnRunning::before"));
        assert!(html.contains("mode === 'dry-run'"));
        assert!(html.contains("mode === 'apply'"));
        assert!(html.contains("st?.progress?.step !== 'rollback'"));
        assert!(html.contains(
            "if (lastKnownSelfUpgradeState) syncUpgradeActionState(lastKnownSelfUpgradeState);"
        ));
        assert!(html.contains("setRunningButton(dryBtn"));
        assert!(html.contains("setRunningButton(applyBtn"));
    }

    #[test]
    fn self_upgrade_response_serializes_request_mode_when_present() {
        let now = "2026-03-03T13:23:00Z";
        let resp = SelfUpgradeResponse {
            state: "running".to_string(),
            op_id: "sup_test".to_string(),
            request: Some(HttpRequestParams {
                mode: "apply".to_string(),
                rollback_on_failure: true,
            }),
            target: HttpTarget {
                image: "ghcr.io/ivanli-cn/dockrev".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            previous: HttpPrevious {
                tag: "prev".to_string(),
                digest: None,
            },
            started_at: now.to_string(),
            updated_at: now.to_string(),
            progress: Progress {
                step: "apply".to_string(),
                message: "docker compose up".to_string(),
            },
            logs: Vec::new(),
            operations: Vec::new(),
        };

        let value = serde_json::to_value(resp).unwrap();
        assert_eq!(value["request"]["mode"], "apply");
        assert_eq!(value["request"]["rollbackOnFailure"], true);
    }

    #[test]
    fn self_upgrade_response_omits_request_when_absent() {
        let now = "2026-03-03T13:23:00Z";
        let resp = SelfUpgradeResponse {
            state: "idle".to_string(),
            op_id: String::new(),
            request: None,
            target: HttpTarget {
                image: "ghcr.io/ivanli-cn/dockrev".to_string(),
                tag: "latest".to_string(),
                digest: None,
            },
            previous: HttpPrevious {
                tag: "unknown".to_string(),
                digest: None,
            },
            started_at: now.to_string(),
            updated_at: now.to_string(),
            progress: Progress {
                step: "done".to_string(),
                message: "idle".to_string(),
            },
            logs: Vec::new(),
            operations: Vec::new(),
        };

        let value = serde_json::to_value(resp).unwrap();
        assert!(value.get("request").is_none());
    }

    #[tokio::test]
    async fn get_self_upgrade_returns_request_only_while_running() {
        let dir = std::env::temp_dir().join(format!(
            "dockrev-supervisor-test-{}-request-visibility",
            std::process::id()
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let app = Arc::new(
            App::new(Config {
                http_addr: "127.0.0.1:0".to_string(),
                base_path: "/supervisor".to_string(),
                auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
                target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
                target_container_id: Some("ctr".to_string()),
                target_compose_project: Some("p".to_string()),
                target_compose_service: Some("dockrev".to_string()),
                target_compose_files: vec!["/abs/compose.yml".to_string()],
                docker_bin: "docker".to_string(),
                docker_host: None,
                compose_bin: "docker-compose".to_string(),
                state_path: dir.join("state.json"),
            })
            .await
            .unwrap(),
        );

        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-User", "ops".parse().unwrap());

        {
            let mut rt = app.runtime.lock().await;
            rt.state.state = "succeeded".to_string();
            rt.state.request = Some(RequestParams {
                mode: "apply".to_string(),
                rollback_on_failure: true,
            });
        }
        let Json(done_resp) = get_self_upgrade(State(app.clone()), headers.clone())
            .await
            .unwrap();
        assert!(done_resp.request.is_none());

        {
            let mut rt = app.runtime.lock().await;
            rt.state.state = "running".to_string();
            rt.state.request = Some(RequestParams {
                mode: "dry-run".to_string(),
                rollback_on_failure: false,
            });
        }
        let Json(running_resp) = get_self_upgrade(State(app.clone()), headers).await.unwrap();
        let req = running_resp
            .request
            .expect("running response should expose request");
        assert_eq!(req.mode, "dry-run");
        assert!(!req.rollback_on_failure);
    }

    #[test]
    fn render_ui_contains_rollback_popconfirm_elements() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains(r#"id="rollbackPop""#));
        assert!(html.contains(r#"id="rollbackConfirm""#));
        assert!(html.contains(r#"id="rollbackCancel""#));
        assert!(html.contains("let rollbackPendingOpId = null;"));
        assert!(html.contains(r"document.addEventListener('keydown'"));
    }

    #[test]
    fn render_ui_requires_confirm_before_rollback_post() {
        let html = render_ui("/supervisor", &test_meta());

        let rollback_click_idx = html
            .find("document.getElementById('rollback').onclick = async (evt) =>")
            .unwrap();
        let open_pop_idx = html.find("setRollbackPopOpen(true);").unwrap();
        let confirm_click_idx = html
            .find("document.getElementById('rollbackConfirm').onclick = async () =>")
            .unwrap();
        let rollback_post_idx = html.find("fetchJson('self-upgrade/rollback'").unwrap();

        assert!(rollback_click_idx < open_pop_idx);
        assert!(open_pop_idx < confirm_click_idx);
        assert!(confirm_click_idx < rollback_post_idx);
        assert!(html.contains("st.opId !== rollbackPendingOpId"));
        assert!(html.contains("JSON.stringify({ opId: rollbackPendingOpId })"));
    }

    #[test]
    fn render_ui_contains_supervisor_meta_links() {
        let html = render_ui("/supervisor", &test_meta());
        assert!(html.contains("Supervisor 版本"));
        assert!(html.contains("开源仓库"));
        assert!(html.contains("开发者"));
        assert!(html.contains("releases/tag/0.9.0"));
        assert!(html.contains("https://github.com/IvanLi-CN/dockrev"));
        assert!(html.contains("https://github.com/IvanLi-CN"));
    }

    #[test]
    fn build_supervisor_meta_uses_fallbacks_when_metadata_missing() {
        let meta = build_supervisor_meta(None, "0.3.0", None, None, None);
        assert_eq!(meta.version, "0.3.0");
        assert_eq!(meta.repository, DEFAULT_REPOSITORY_URL);
        assert_eq!(meta.developer_name, DEFAULT_DEVELOPER_NAME);
        assert_eq!(meta.developer_url, DEFAULT_DEVELOPER_URL);
        assert_eq!(
            meta.release_url.as_deref(),
            Some("https://github.com/IvanLi-CN/dockrev/releases/tag/0.3.0")
        );
    }

    #[test]
    fn build_supervisor_meta_prefers_provided_runtime_and_package_metadata() {
        let meta = build_supervisor_meta(
            Some("9.9.9"),
            "0.3.0",
            Some("https://github.com/acme/dockrev-fork"),
            Some("Alice <alice@example.com>:Bob"),
            Some("https://acme.example/dev"),
        );
        assert_eq!(meta.version, "9.9.9");
        assert_eq!(meta.repository, "https://github.com/acme/dockrev-fork");
        assert_eq!(meta.developer_name, "Alice");
        assert_eq!(meta.developer_url, "https://acme.example/dev");
        assert_eq!(
            meta.release_url.as_deref(),
            Some("https://github.com/acme/dockrev-fork/releases/tag/9.9.9")
        );
    }

    #[test]
    fn build_supervisor_meta_normalizes_github_repo_release_link() {
        let with_dot_git = build_supervisor_meta(
            Some("1.2.3"),
            "0.3.0",
            Some("https://github.com/acme/dockrev-fork.git"),
            Some("Alice"),
            None,
        );
        assert_eq!(
            with_dot_git.release_url.as_deref(),
            Some("https://github.com/acme/dockrev-fork/releases/tag/1.2.3")
        );

        let with_trailing_slash = build_supervisor_meta(
            Some("1.2.3"),
            "0.3.0",
            Some("https://github.com/acme/dockrev-fork/"),
            Some("Alice"),
            None,
        );
        assert_eq!(
            with_trailing_slash.release_url.as_deref(),
            Some("https://github.com/acme/dockrev-fork/releases/tag/1.2.3")
        );
    }

    #[tokio::test]
    async fn start_is_idempotent_while_running() {
        let dir = std::env::temp_dir().join(format!(
            "dockrev-supervisor-test-{}-idem",
            std::process::id()
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let cfg = Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        };

        let app = App::new(cfg).await.unwrap();

        // Force state to running and set running key.
        {
            let mut rt = app.runtime.lock().await;
            rt.state.state = "running".to_string();
            rt.state.op_id = "sup_1".to_string();
            rt.running_key = Some(StartKey {
                tag: "latest".to_string(),
                digest: None,
                mode: "apply".to_string(),
                rollback_on_failure: true,
            });
        }

        let op1 = app
            .start_op(StartSelfUpgradeRequest {
                target: StartTarget {
                    tag: "latest".to_string(),
                    digest: None,
                },
                mode: "apply".to_string(),
                rollback_on_failure: true,
            })
            .await
            .unwrap();
        assert_eq!(op1, "sup_1");

        let err = app
            .start_op(StartSelfUpgradeRequest {
                target: StartTarget {
                    tag: "v1.2.3".to_string(),
                    digest: None,
                },
                mode: "apply".to_string(),
                rollback_on_failure: true,
            })
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn mark_failed_if_running_transitions_state_and_clears_running_key() {
        let dir = std::env::temp_dir().join(format!(
            "dockrev-supervisor-test-{}-fail",
            std::process::id()
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let cfg = Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        };

        let app = App::new(cfg).await.unwrap();
        {
            let mut rt = app.runtime.lock().await;
            rt.state.state = "running".to_string();
            rt.state.progress = Progress {
                step: "precheck".to_string(),
                message: "starting".to_string(),
            };
            rt.running_key = Some(StartKey {
                tag: "latest".to_string(),
                digest: None,
                mode: "apply".to_string(),
                rollback_on_failure: true,
            });
        }

        mark_failed_if_running(&app, anyhow::anyhow!("boom")).await;

        let rt = app.runtime.lock().await;
        assert_eq!(rt.state.state, "failed");
        assert!(rt.running_key.is_none());
    }

    #[tokio::test]
    async fn app_new_trims_legacy_large_logs_on_boot() {
        let dir = std::env::temp_dir().join(format!(
            "dockrev-supervisor-test-{}-boottrim",
            std::process::id()
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let cfg = Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: Some("ctr".to_string()),
            target_compose_project: Some("p".to_string()),
            target_compose_service: Some("dockrev".to_string()),
            target_compose_files: vec!["/abs/compose.yml".to_string()],
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: dir.join("state.json"),
        };

        let now = crate::state_store::now_rfc3339().unwrap();
        let mut st = StateFile::idle(&now);
        for i in 0..31 {
            let op_id = format!("sup_boot_{i}");
            st.logs.push(test_log(
                &format!("2026-02-01T05:{i:02}:00Z"),
                "INFO",
                "self-upgrade requested",
                Some(&op_id),
            ));
            st.logs.push(test_log(
                &format!("2026-02-01T05:{i:02}:10Z"),
                "INFO",
                "dry-run done",
                Some(&op_id),
            ));
        }
        store_atomic(&cfg.state_path, &st).await.unwrap();

        let app = App::new(cfg.clone()).await.unwrap();
        let rt = app.runtime.lock().await;
        assert_eq!(build_operation_groups(&rt.state.logs).len(), 30);
        drop(rt);

        let persisted = load_or_idle(&cfg.state_path).await.unwrap();
        assert_eq!(build_operation_groups(&persisted.logs).len(), 30);
    }

    #[tokio::test]
    async fn post_self_upgrade_rollback_clears_request_for_running_rollback() {
        let dir = std::env::temp_dir().join(format!(
            "dockrev-supervisor-test-{}-rollback-request",
            std::process::id()
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let app = Arc::new(
            App::new(Config {
                http_addr: "127.0.0.1:0".to_string(),
                base_path: "/supervisor".to_string(),
                auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
                target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
                target_container_id: Some("ctr".to_string()),
                target_compose_project: Some("p".to_string()),
                target_compose_service: Some("dockrev".to_string()),
                target_compose_files: vec!["/abs/compose.yml".to_string()],
                docker_bin: "docker".to_string(),
                docker_host: None,
                compose_bin: "docker-compose".to_string(),
                state_path: dir.join("state.json"),
            })
            .await
            .unwrap(),
        );

        {
            let mut rt = app.runtime.lock().await;
            rt.state.state = "failed".to_string();
            rt.state.op_id = "sup_test".to_string();
            rt.state.request = Some(RequestParams {
                mode: "apply".to_string(),
                rollback_on_failure: true,
            });
            rt.state.previous.tag = "prev".to_string();
            rt.state.previous.digest = Some("sha256:abc".to_string());
        }

        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-User", "ops".parse().unwrap());

        let result = post_self_upgrade_rollback(
            State(app.clone()),
            headers,
            Json(RollbackRequest {
                op_id: "sup_test".to_string(),
            }),
        )
        .await;
        assert!(result.is_ok());

        let rt = app.runtime.lock().await;
        assert!(rt.state.request.is_none());
    }
}
