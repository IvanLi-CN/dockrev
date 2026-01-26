use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{
    config::Config,
    docker_exec::{
        TargetRuntime, compose_up, docker_image_repo_digest, docker_pull, resolve_target,
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

impl App {
    pub async fn new(cfg: Config) -> anyhow::Result<Self> {
        let loaded = load_or_idle(&cfg.state_path).await?;
        let mut state = loaded;

        // If we crashed while running, surface it as failed but keep opId/logs for recovery.
        if state.state == "running" {
            let now = now_rfc3339()?;
            state.state = "failed".to_string();
            state.updated_at = now.clone();
            state.progress = Progress {
                step: "postcheck".to_string(),
                message: "supervisor restarted; previous operation interrupted".to_string(),
            };
            state.logs.push(LogLine {
                ts: now,
                level: "ERROR".to_string(),
                msg: "supervisor restarted; previous operation interrupted".to_string(),
            });
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
        rt.state.logs.push(LogLine {
            ts: now,
            level: "INFO".to_string(),
            msg: "self-upgrade requested".to_string(),
        });
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
        rt.state.logs.push(LogLine {
            ts: now,
            level: "ERROR".to_string(),
            msg: err.to_string(),
        });
    }
    rt.running_key = None;

    if let Err(e) = store_atomic(&app.cfg.state_path, &rt.state).await {
        tracing::error!(error = %e, "failed to persist supervisor state after background failure");
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

#[derive(Serialize)]
struct VersionResponse<'a> {
    version: &'a str,
}

async fn version(State(_app): State<Arc<App>>) -> impl IntoResponse {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
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
    target: HttpTarget,
    previous: HttpPrevious,
    started_at: String,
    updated_at: String,
    progress: Progress,
    logs: Vec<LogLine>,
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

async fn get_self_upgrade(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Json<SelfUpgradeResponse>, ApiError> {
    let _user = require_user(&app, &headers)?;
    let rt = app.runtime.lock().await;
    let st = &rt.state;
    Ok(Json(SelfUpgradeResponse {
        state: st.state.clone(),
        op_id: st.op_id.clone(),
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
    rt.state.progress = Progress {
        step: "rollback".to_string(),
        message: "manual rollback".to_string(),
    };
    rt.state.updated_at = now.clone();
    rt.state.logs.push(LogLine {
        ts: now,
        level: "WARN".to_string(),
        msg: "manual rollback requested".to_string(),
    });
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
    Ok(Html(render_ui(&app.cfg.base_path)))
}

fn render_ui(base_path: &str) -> String {
    // Minimal, dependency-free console. Uses same-origin fetch under base_path.
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev Supervisor</title>
    <style>
      body {{ font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial; padding: 18px; max-width: 960px; margin: 0 auto; }}
      .row {{ display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }}
      .card {{ border: 1px solid rgba(0,0,0,0.12); border-radius: 12px; padding: 14px; margin-top: 12px; }}
      .muted {{ color: rgba(0,0,0,0.62); font-size: 12px; }}
      button {{ padding: 8px 12px; border-radius: 10px; border: 1px solid rgba(0,0,0,0.18); background: white; cursor: pointer; }}
      button[disabled] {{ opacity: 0.5; cursor: not-allowed; }}
      input {{ padding: 8px 10px; border-radius: 10px; border: 1px solid rgba(0,0,0,0.18); }}
      pre {{ background: rgba(0,0,0,0.06); padding: 10px; border-radius: 10px; overflow: auto; }}
      .ok {{ color: #16a34a; }}
      .bad {{ color: #dc2626; }}
    </style>
  </head>
  <body>
    <h1>Dockrev 自我升级（Supervisor）</h1>
    <div class="muted">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用。</div>

    <div class="card">
      <div class="row">
        <div>Target tag:</div>
        <input id="tag" value="latest" />
        <button id="dry">预览（dry-run）</button>
        <button id="apply">开始升级（apply）</button>
        <button id="rollback">回滚</button>
        <button id="refresh">刷新</button>
        <a href="/" style="margin-left:auto">返回 Dockrev</a>
      </div>
      <div class="muted">提示：失败将尝试回滚到 previous digest（如可用）。</div>
    </div>

    <div class="card">
      <div id="status" class="muted">loading…</div>
      <pre id="logs"></pre>
    </div>

    <script>
      const base = {base_path};
      const toUrl = (p) => base.replace(/\/$/, '') + '/' + p.replace(/^\//, '');

      async function fetchJson(path, init) {{
        const resp = await fetch(toUrl(path), {{ ...init, headers: {{ 'Content-Type': 'application/json' }} }});
        const text = await resp.text();
        if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{text}}`);
        return text ? JSON.parse(text) : null;
      }}

	      async function refresh() {{
	        const statusEl = document.getElementById('status');
	        try {{
	          const st = await fetchJson('self-upgrade');
	          statusEl.className = `muted ${{statusClass(st)}}`.trim();
	          statusEl.textContent = renderStatusText(st);
	          document.getElementById('logs').textContent = (st.logs||[]).map(l => `[${{l.ts}}] ${{l.level}} ${{l.msg}}`).join('\\n');
	          document.getElementById('rollback').disabled = !st.opId || (st.state !== 'failed' && st.state !== 'rolled_back' && st.state !== 'succeeded');
	        }} catch (e) {{
	          statusEl.className = 'muted bad';
	          statusEl.textContent = `offline ${{String(e.message||e)}}`;
	        }}
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

      document.getElementById('refresh').onclick = () => refresh();
      document.getElementById('dry').onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'dry-run', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      document.getElementById('apply').onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'apply', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      document.getElementById('rollback').onclick = async () => {{
        const st = await fetchJson('self-upgrade');
        await fetchJson('self-upgrade/rollback', {{ method: 'POST', body: JSON.stringify({{ opId: st.opId }}) }});
        await refresh();
      }};

      refresh();
      setInterval(refresh, 1500);
    </script>
  </body>
</html>"#,
        base_path =
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
        st.logs.push(LogLine {
            ts: now.to_string(),
            level: "INFO".to_string(),
            msg: format!("pull {image_ref}"),
        });
    })
    .await?;

    docker_pull(&app.cfg, &image_ref, Duration::from_secs(300)).await?;

    if key.mode == "dry-run" {
        update_state(&app, |st, now| {
            st.state = "succeeded".to_string();
            st.progress = Progress {
                step: "done".to_string(),
                message: "dry-run completed".to_string(),
            };
            st.updated_at = now.to_string();
            st.logs.push(LogLine {
                ts: now.to_string(),
                level: "INFO".to_string(),
                msg: "dry-run done".to_string(),
            });
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
        st.logs.push(LogLine {
            ts: now.to_string(),
            level: "INFO".to_string(),
            msg: "compose up".to_string(),
        });
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
        st.logs.push(LogLine {
            ts: now.to_string(),
            level: "INFO".to_string(),
            msg: "succeeded".to_string(),
        });
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
            st.logs.push(LogLine {
                ts: now.to_string(),
                level: "WARN".to_string(),
                msg: "rolled back".to_string(),
            });
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
            st.logs.push(LogLine {
                ts: now.to_string(),
                level: "ERROR".to_string(),
                msg: format!("rollback failed: {err}"),
            });
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
        st.logs.push(LogLine {
            ts: now.to_string(),
            level: "ERROR".to_string(),
            msg: err.to_string(),
        });
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
}
