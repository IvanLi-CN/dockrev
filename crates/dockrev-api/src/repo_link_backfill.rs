use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use serde_json::json;

use crate::{
    api::{
        services::{
            RepoLinkInferenceOutcomeKind, build_repo_link_inference_context,
            infer_service_repo_link_for_snapshot_target,
        },
        types::{JobListItem, JobLogLine, JobProgress, JobScope, JobType},
    },
    ids, now_rfc3339,
    state::AppState,
};

const WORKER_IDLE_POLL_MS: u64 = 750;

#[derive(Clone, Copy, Debug, Default)]
struct RepoLinkBackfillCounters {
    total: u32,
    updated: u32,
    skipped_disabled: u32,
    no_match: u32,
    error: u32,
}

impl RepoLinkBackfillCounters {
    fn into_json(self) -> serde_json::Value {
        json!({
            "total": self.total,
            "updated": self.updated,
            "skippedDisabled": self.skipped_disabled,
            "noMatch": self.no_match,
            "error": self.error,
        })
    }
}

pub fn spawn_tasks(state: Arc<AppState>) {
    tokio::spawn(async move {
        run_worker_loop(state).await;
    });
}

pub async fn enqueue_startup_backfill_if_needed(
    state: &AppState,
) -> anyhow::Result<Option<String>> {
    let eligible = state.db.count_repo_link_backfill_candidates(None).await?;
    if eligible == 0 {
        return Ok(None);
    }
    if let Some(existing) = pending_backfill_job_to_reuse(state, None).await? {
        return Ok(Some(existing));
    }
    enqueue_backfill_job(state, JobScope::All, None, eligible, "system", "startup").await
}

pub async fn enqueue_stack_backfill_if_needed(
    state: &AppState,
    stack_id: &str,
    reason: &str,
) -> anyhow::Result<Option<String>> {
    let eligible = state
        .db
        .count_repo_link_backfill_candidates(Some(stack_id))
        .await?;
    if eligible == 0 {
        return Ok(None);
    }
    if let Some(existing) = pending_backfill_job_to_reuse(state, Some(stack_id)).await? {
        return Ok(Some(existing));
    }
    enqueue_backfill_job(
        state,
        JobScope::Stack,
        Some(stack_id.to_string()),
        eligible,
        "system",
        reason,
    )
    .await
}

async fn pending_backfill_job_to_reuse(
    state: &AppState,
    stack_id: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let pending = state
        .db
        .list_jobs_by_type_and_statuses(JobType::RepoLinkBackfill, &["queued", "running"], 200)
        .await?;
    if let Some(existing) = pending.iter().find(|job| job.scope == JobScope::All) {
        return Ok(Some(existing.id.clone()));
    }
    let Some(stack_id) = stack_id else {
        return Ok(None);
    };
    Ok(pending
        .iter()
        .find(|job| job.scope == JobScope::Stack && job.stack_id.as_deref() == Some(stack_id))
        .map(|job| job.id.clone()))
}

async fn enqueue_backfill_job(
    state: &AppState,
    scope: JobScope,
    stack_id: Option<String>,
    total: u32,
    created_by: &str,
    reason: &str,
) -> anyhow::Result<Option<String>> {
    let now = now_rfc3339()?;
    let job_id = ids::new_job_id();
    let progress = make_job_progress(
        "queued",
        format!("waiting to backfill repo links ({total} eligible)"),
        0,
        total,
        stack_id.clone(),
        now.clone(),
    );
    let summary_json = json!({
        "counters": RepoLinkBackfillCounters {
            total,
            ..RepoLinkBackfillCounters::default()
        }.into_json(),
        "progress": serde_json::to_value(&progress)?,
    });
    state
        .db
        .insert_job(JobListItem {
            id: job_id.clone(),
            r#type: JobType::RepoLinkBackfill,
            scope,
            stack_id,
            service_id: None,
            status: "queued".to_string(),
            created_at: now,
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json,
        })
        .await?;
    Ok(Some(job_id))
}

async fn run_worker_loop(state: Arc<AppState>) {
    loop {
        let started_at =
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        match state
            .db
            .claim_next_queued_job_by_type(JobType::RepoLinkBackfill, &started_at)
            .await
        {
            Ok(Some(job)) => {
                if let Err(err) = run_claimed_job(state.clone(), job).await {
                    tracing::error!(error = %err, "repo link backfill job run failed");
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_millis(WORKER_IDLE_POLL_MS)).await,
            Err(err) => {
                tracing::error!(error = %err, "repo link backfill worker claim failed");
                tokio::time::sleep(Duration::from_millis(WORKER_IDLE_POLL_MS)).await;
            }
        }
    }
}

pub async fn run_claimed_job(state: Arc<AppState>, job: JobListItem) -> anyhow::Result<()> {
    let job_id = job.id.clone();
    let finished_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    match run_claimed_job_inner(&state, &job, &job_id).await {
        Ok(summary) => {
            state
                .db
                .finish_job(&job_id, "success", &finished_at, &summary)
                .await?;
        }
        Err(err) => {
            let _ = state
                .db
                .insert_job_log(
                    &job_id,
                    &JobLogLine {
                        ts: finished_at.clone(),
                        level: "error".to_string(),
                        msg: format!("repo link backfill failed: {err}"),
                    },
                )
                .await;
            let failed_progress = make_job_progress(
                "done",
                format!("repo link backfill failed: {err}"),
                0,
                0,
                job.stack_id.clone(),
                finished_at.clone(),
            );
            let summary = json!({
                "error": err.to_string(),
                "progress": serde_json::to_value(&failed_progress).unwrap_or_else(|_| json!({})),
            });
            state
                .db
                .finish_job(&job_id, "failed", &finished_at, &summary)
                .await?;
        }
    }
    Ok(())
}

async fn run_claimed_job_inner(
    state: &Arc<AppState>,
    job: &JobListItem,
    job_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let stack_filter = match job.scope {
        JobScope::All => None,
        JobScope::Stack => job.stack_id.as_deref(),
        JobScope::Service => anyhow::bail!("service scope is not supported for repo link backfill"),
    };
    let targets = state
        .db
        .list_repo_link_backfill_targets(stack_filter)
        .await?;
    let total = targets.len() as u32;
    let context = build_repo_link_inference_context(state)
        .await
        .context("build repo link inference context")?;
    let mut counters = RepoLinkBackfillCounters {
        total,
        ..RepoLinkBackfillCounters::default()
    };

    let initial_progress = make_job_progress(
        "backfill",
        format!("backfilling repo links (0/{total})"),
        0,
        total,
        job.stack_id.clone(),
        now_rfc3339()?,
    );
    persist_progress(state, job_id, &initial_progress).await;

    let mut completed = 0u32;
    for target in targets {
        let current_target = Some(format!("{}/{}", target.stack_name, target.service_name));
        let progress = make_job_progress(
            "backfill",
            format!("backfilling repo links ({completed}/{total})"),
            completed,
            total,
            current_target.clone(),
            now_rfc3339()?,
        );
        persist_progress(state, job_id, &progress).await;

        if target.repo_url_auto_disabled {
            counters.skipped_disabled = counters.skipped_disabled.saturating_add(1);
            completed = completed.saturating_add(1);
            continue;
        }

        let inference =
            infer_service_repo_link_for_snapshot_target(state, &target.snapshot_target, &context)
                .await;
        if let Some(repo_url) = inference.repo_url.as_deref() {
            let changed = state
                .db
                .set_service_repo_url_if_empty(&target.service_id, repo_url, &now_rfc3339()?)
                .await?;
            if changed {
                counters.updated = counters.updated.saturating_add(1);
            } else {
                counters.skipped_disabled = counters.skipped_disabled.saturating_add(1);
            }
        } else {
            match inference.outcome {
                RepoLinkInferenceOutcomeKind::Match => {
                    counters.error = counters.error.saturating_add(1);
                }
                RepoLinkInferenceOutcomeKind::NoMatch => {
                    counters.no_match = counters.no_match.saturating_add(1);
                }
                RepoLinkInferenceOutcomeKind::Error => {
                    counters.error = counters.error.saturating_add(1);
                }
            }
        }

        completed = completed.saturating_add(1);
        let progress = make_job_progress(
            "backfill",
            format!("backfilling repo links ({completed}/{total})"),
            completed,
            total,
            current_target,
            now_rfc3339()?,
        );
        persist_progress(state, job_id, &progress).await;
    }

    let finished_at = now_rfc3339()?;
    let final_progress = make_job_progress(
        "done",
        format!(
            "repo link backfill finished (updated={}, skipped_disabled={}, no_match={}, error={})",
            counters.updated, counters.skipped_disabled, counters.no_match, counters.error
        ),
        total,
        total,
        job.stack_id.clone(),
        finished_at,
    );
    Ok(json!({
        "counters": counters.into_json(),
        "progress": serde_json::to_value(&final_progress)?,
    }))
}

fn progress_percent(current: u32, total: u32) -> u32 {
    if total == 0 {
        0
    } else {
        ((current.saturating_mul(100)) / total).min(100)
    }
}

fn make_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    let percent = progress_percent(current, total);
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

async fn persist_progress(state: &Arc<AppState>, job_id: &str, progress: &JobProgress) {
    let progress_json = match serde_json::to_value(progress) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(job_id = %job_id, error = %err, "serialize repo link backfill progress failed");
            return;
        }
    };
    let _ = state.db.set_job_progress(job_id, &progress_json).await;
}
