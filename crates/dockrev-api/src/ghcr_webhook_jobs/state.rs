use std::sync::Arc;

use serde_json::json;

use crate::state::AppState;

use super::{
    GhcrWebhookOp,
    support::{emit_job_event, now_rfc3339},
};

#[derive(Clone, Debug)]
pub(super) struct GhcrWebhookSettings {
    pub(super) enabled: bool,
    pub(super) callback_url: Option<String>,
    pub(super) pat: Option<String>,
    pub(super) webhook_secret: Option<String>,
}

pub(super) async fn load_settings(state: &Arc<AppState>) -> anyhow::Result<GhcrWebhookSettings> {
    let settings = state.db.get_github_packages_settings().await?;
    Ok(GhcrWebhookSettings {
        enabled: settings.enabled,
        callback_url: if settings.callback_url.trim().is_empty() {
            None
        } else {
            Some(settings.callback_url)
        },
        pat: settings.pat,
        webhook_secret: settings.webhook_secret,
    })
}

pub(super) async fn mark_repo_error(
    state: &Arc<AppState>,
    owner: &str,
    repo: &str,
    job_id: &str,
    op: GhcrWebhookOp,
    message: &str,
) {
    mark_repo_state(
        state,
        owner,
        repo,
        job_id,
        op,
        "error",
        None,
        None,
        Some(message),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn mark_repo_state(
    state: &Arc<AppState>,
    owner: &str,
    repo: &str,
    job_id: &str,
    op: GhcrWebhookOp,
    webhook_state: &str,
    hook_id: Option<i64>,
    last_sync_at: Option<&str>,
    last_error: Option<&str>,
) {
    let existing = state
        .db
        .get_github_packages_repo(owner, repo)
        .await
        .ok()
        .flatten();
    let existing_sync_at = existing.as_ref().and_then(|row| row.last_sync_at.clone());
    let effective_sync_at = last_sync_at
        .map(ToString::to_string)
        .or(existing_sync_at)
        .filter(|v| !v.trim().is_empty());

    let now = now_rfc3339();
    let last_audit_at = if op == GhcrWebhookOp::AuditAll {
        Some(now.as_str())
    } else {
        existing
            .as_ref()
            .and_then(|row| row.last_audit_at.as_deref())
    };

    let effective_hook_id = match hook_id {
        Some(hook_id) => Some(hook_id),
        None if webhook_state == "missing" || webhook_state == "conflict" => None,
        None => existing.as_ref().and_then(|row| row.hook_id),
    };

    let _ = state
        .db
        .set_github_packages_repo_webhook_result(
            owner,
            repo,
            webhook_state,
            effective_hook_id,
            effective_sync_at.as_deref(),
            last_audit_at,
            last_error,
            Some(job_id),
            Some(op.as_str()),
            &now,
        )
        .await;

    emit_job_event(
        state,
        job_id,
        &json!({
            "type": "ghcr_repo_state",
            "jobId": job_id,
            "op": op.as_str(),
            "target": format!("{owner}/{repo}"),
            "webhookState": webhook_state,
            "hookId": effective_hook_id,
            "lastError": last_error,
            "ts": now,
        }),
    )
    .await;
}
