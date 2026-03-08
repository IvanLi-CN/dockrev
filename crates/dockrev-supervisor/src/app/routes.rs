use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state_store::{LogLine, Progress, StateFile, now_rfc3339, store_atomic};

use super::{
    App, UI_FAVICON_PNG,
    auth::require_user,
    errors::ApiError,
    meta::supervisor_meta,
    orchestration::run_rollback_only,
    state_helpers::{
        MAX_LOG_OPERATION_GROUPS, append_log_line, build_operation_groups, infer_operation_state,
        retain_recent_operation_logs,
    },
    ui::render_ui,
};

pub(crate) async fn health(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let _user = require_user(&app, &headers)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionResponse {
    version: String,
    repository: String,
    developer_name: String,
    developer_url: String,
}

pub(crate) async fn version(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let _user = require_user(&app, &headers)?;
    let meta = supervisor_meta();
    Ok(Json(VersionResponse {
        version: meta.version,
        repository: meta.repository,
        developer_name: meta.developer_name,
        developer_url: meta.developer_url,
    }))
}

pub(crate) async fn ui_favicon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], UI_FAVICON_PNG)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfUpgradeResponse {
    pub(crate) state: String,
    pub(crate) op_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request: Option<HttpRequestParams>,
    pub(crate) target: HttpTarget,
    pub(crate) previous: HttpPrevious,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) progress: Progress,
    pub(crate) logs: Vec<LogLine>,
    pub(crate) operations: Vec<SelfUpgradeOperation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfUpgradeOperation {
    pub(crate) op_id: String,
    pub(crate) state: String,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) logs: Vec<LogLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HttpRequestParams {
    pub(crate) mode: String,
    pub(crate) rollback_on_failure: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HttpTarget {
    pub(crate) image: String,
    pub(crate) tag: String,
    pub(crate) digest: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HttpPrevious {
    pub(crate) tag: String,
    pub(crate) digest: Option<String>,
}

pub(crate) fn build_response_operations(st: &StateFile) -> Vec<SelfUpgradeOperation> {
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

pub(crate) async fn get_self_upgrade(
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
pub(crate) struct StartSelfUpgradeRequest {
    pub(crate) target: StartTarget,
    pub(crate) mode: String,
    pub(crate) rollback_on_failure: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartTarget {
    pub(crate) tag: String,
    #[serde(default)]
    pub(crate) digest: Option<String>,
}

pub(crate) async fn post_self_upgrade(
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
pub(crate) struct RollbackRequest {
    pub(crate) op_id: String,
}

pub(crate) async fn post_self_upgrade_rollback(
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

pub(crate) async fn ui_index(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Html<String>, ApiError> {
    let _user = require_user(&app, &headers)?;
    let meta = supervisor_meta();
    Ok(Html(render_ui(&app.cfg.base_path, &meta)))
}
