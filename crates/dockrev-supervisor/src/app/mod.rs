use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use tokio::sync::Mutex;

use crate::{
    config::Config,
    state_store::{Progress, RequestParams, StateFile, load_or_idle, now_rfc3339, store_atomic},
};

mod auth;
mod errors;
mod meta;
mod orchestration;
mod routes;
mod state_helpers;
mod ui;

#[cfg(test)]
mod tests;

use errors::ApiError;
use orchestration::run_operation;
use routes::{
    StartSelfUpgradeRequest, get_self_upgrade, health, post_self_upgrade,
    post_self_upgrade_rollback, ui_favicon, ui_index, version,
};
use state_helpers::{
    MAX_LOG_OPERATION_GROUPS, append_log_line, mark_failed_if_running, normalize_digest,
    retain_recent_operation_logs,
};

pub(super) const UI_FAVICON_PNG: &[u8] = include_bytes!("../../../../web/public/favicon.png");

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
pub(super) struct StartKey {
    tag: String,
    digest: Option<String>,
    mode: String,
    rollback_on_failure: bool,
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
        let base_with_slash = format!("{base}/");
        let api = Router::new()
            .route("/health", get(health))
            .route("/version", get(version))
            .route(
                "/self-upgrade",
                get(get_self_upgrade).post(post_self_upgrade),
            )
            .route("/self-upgrade/rollback", post(post_self_upgrade_rollback))
            .route("/favicon.png", get(ui_favicon))
            .with_state(self.clone());
        Router::new()
            .route(&base, get(ui_index))
            .route(&base_with_slash, get(ui_index))
            .nest(&base, api)
            .with_state(self)
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
