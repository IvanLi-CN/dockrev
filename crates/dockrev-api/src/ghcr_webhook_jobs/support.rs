use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use serde_json::json;
use url::Url;

use crate::{
    api::types::{JobListItem, JobLogLine, JobProgress, JobType},
    state::AppState,
};

use super::GhcrWebhookOp;

const RETRY_MAX_ATTEMPTS: u32 = 3;

#[derive(Clone, Debug)]
pub(super) struct ParsedGhcrWebhookJob {
    pub(super) kind: ParsedGhcrWebhookJobKind,
    pub(super) repos: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ParsedGhcrWebhookJobKind {
    Legacy(GhcrWebhookOp),
    SyncAll,
    SyncRepo,
}

pub(super) fn parse_job_payload(job: &JobListItem) -> anyhow::Result<ParsedGhcrWebhookJob> {
    let mut repos: Vec<(String, String)> = Vec::new();
    if let Some(items) = job.summary_json.get("repos").and_then(|v| v.as_array()) {
        for item in items {
            let full_name = item.as_str().unwrap_or_default();
            let (owner, repo) = parse_full_name(full_name)?;
            repos.push((owner, repo));
        }
    }

    let kind = match &job.r#type {
        JobType::GitHubPackagesWebhook => {
            let op_raw = job
                .summary_json
                .get("op")
                .and_then(|v| v.as_str())
                .context("missing op")?;
            let op = GhcrWebhookOp::from_str(op_raw).context("invalid op")?;
            ParsedGhcrWebhookJobKind::Legacy(op)
        }
        JobType::GitHubPackagesWebhookSyncAll => ParsedGhcrWebhookJobKind::SyncAll,
        JobType::GitHubPackagesWebhookSyncRepo => ParsedGhcrWebhookJobKind::SyncRepo,
        _ => anyhow::bail!("unsupported ghcr webhook job type"),
    };

    if !matches!(
        kind,
        ParsedGhcrWebhookJobKind::Legacy(GhcrWebhookOp::AuditAll)
    ) && repos.is_empty()
    {
        anyhow::bail!("webhook job has no target repos");
    }

    Ok(ParsedGhcrWebhookJob { kind, repos })
}

pub(super) async fn github_call_with_retry<T, F, Fut>(
    state: &Arc<AppState>,
    job_id: &str,
    action: &str,
    target: &str,
    mut func: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    for attempt in 1..=RETRY_MAX_ATTEMPTS {
        match func().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let recoverable = github_error_is_recoverable(&err);
                if !recoverable || attempt >= RETRY_MAX_ATTEMPTS {
                    return Err(err);
                }

                let backoff_ms = retry_backoff_ms(attempt);
                let now = now_rfc3339();
                emit_job_event(
                    state,
                    job_id,
                    &json!({
                        "type": "ghcr_retry",
                        "jobId": job_id,
                        "action": action,
                        "target": target,
                        "attempt": attempt,
                        "maxAttempts": RETRY_MAX_ATTEMPTS,
                        "waitMs": backoff_ms,
                        "error": err.to_string(),
                        "ts": now,
                    }),
                )
                .await;

                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
        }
    }

    anyhow::bail!("retry exhausted")
}

fn github_error_is_recoverable(err: &anyhow::Error) -> bool {
    if github_error_is_timeout_or_connect(err) {
        return true;
    }

    if let Some(status) = github_http_status_from_error(err) {
        if status >= 500 || status == 429 {
            return true;
        }
        if matches!(status, 401 | 403 | 404 | 422) {
            return false;
        }
    }

    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("rate limit")
        || lower.contains("secondary rate")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("temporarily unavailable")
}

fn github_error_is_timeout_or_connect(err: &anyhow::Error) -> bool {
    if err
        .chain()
        .filter_map(|cause| cause.downcast_ref::<reqwest::Error>())
        .any(|req| req.is_timeout() || req.is_connect())
    {
        return true;
    }
    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
}

pub(super) fn github_http_status_from_error(err: &anyhow::Error) -> Option<u16> {
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

fn retry_backoff_ms(attempt: u32) -> u64 {
    let base_ms = match attempt {
        1 => 1_000,
        2 => 2_000,
        _ => 4_000,
    };
    let nanos = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let jitter = (nanos.unsigned_abs() % 250) as u64;
    base_ms + jitter
}

pub(super) async fn persist_progress(state: &Arc<AppState>, job_id: &str, progress: &JobProgress) {
    let progress_json = match serde_json::to_value(progress) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(job_id = %job_id, error = %err, "serialize ghcr progress failed");
            return;
        }
    };
    let _ = state.db.set_job_progress(job_id, &progress_json).await;

    emit_job_event(
        state,
        job_id,
        &json!({
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
        }),
    )
    .await;
}

pub(super) async fn emit_job_event(
    state: &Arc<AppState>,
    job_id: &str,
    payload: &serde_json::Value,
) {
    let now = now_rfc3339();
    let _ = state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: now,
                level: "event".to_string(),
                msg: payload.to_string(),
            },
        )
        .await;
}

pub(super) fn make_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    let percent = if total == 0 {
        0
    } else {
        ((current.saturating_mul(100)) / total).min(100)
    };
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent,
        planned_current: Some(current),
        planned_total: Some(total),
        planned_percent: Some(Some(percent)),
        current_target,
        updated_at,
    }
}

pub(super) fn parse_full_name(full_name: &str) -> anyhow::Result<(String, String)> {
    let mut parts = full_name.trim().split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        anyhow::bail!("invalid fullName");
    }
    Ok((owner.to_string(), repo.to_string()))
}

pub(super) fn urls_match(a: &str, b: &str) -> bool {
    let Ok(au) = Url::parse(a) else { return false };
    let Ok(bu) = Url::parse(b) else { return false };

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

pub(super) fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string())
}
