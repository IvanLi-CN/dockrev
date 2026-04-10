use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use serde_json::json;
use tokio::sync::Semaphore;
use url::Url;

use crate::{
    api::types::{GitHubPackagesWebhookOverviewResponse, JobListItem, JobScope, JobType},
    github, ids, notify,
    state::AppState,
};

mod overview;
mod state;
mod support;
mod sync;

use state::{load_settings, mark_repo_error, mark_repo_state};
use support::{
    ParsedGhcrWebhookJobKind, emit_job_event, github_call_with_retry,
    github_http_status_from_error, make_progress, now_rfc3339, parse_full_name, parse_job_payload,
    persist_progress, urls_match,
};
use sync::{
    is_legacy_register_job, is_pending_status, lock_repo_sync, repo_registration_in_progress,
    repo_unregistration_in_progress, sync_enqueue_lock,
};

const WORKER_IDLE_POLL_MS: u64 = 400;
const GHCR_SYNC_ALL_MAX_CONCURRENCY: usize = 5;
const GHCR_SYNC_REPO_WORKERS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhcrWebhookOp {
    Register,
    Unregister,
    AuditAll,
}

impl GhcrWebhookOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Unregister => "unregister",
            Self::AuditAll => "audit_all",
        }
    }

    pub fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "register" => Some(Self::Register),
            "unregister" => Some(Self::Unregister),
            "audit_all" => Some(Self::AuditAll),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct GhcrJobCounters {
    total: u32,
    ok: u32,
    missing: u32,
    conflict: u32,
    error: u32,
    deleted: u32,
    hard_failures: u32,
}

#[derive(Clone, Debug)]
pub struct GhcrSyncEnqueueResult {
    pub job_id: String,
    pub status: String,
    pub reused: bool,
}

pub fn spawn_tasks(state: Arc<AppState>) {
    spawn_worker_loops(&state, JobType::GitHubPackagesWebhook, 1);
    spawn_worker_loops(&state, JobType::GitHubPackagesWebhookSyncAll, 1);
    spawn_worker_loops(
        &state,
        JobType::GitHubPackagesWebhookSyncRepo,
        GHCR_SYNC_REPO_WORKERS,
    );
}

fn spawn_worker_loops(state: &Arc<AppState>, job_type: JobType, workers: usize) {
    for _ in 0..workers.max(1) {
        let worker_state = state.clone();
        let worker_job_type = job_type.clone();
        tokio::spawn(async move {
            run_worker_loop(worker_state, worker_job_type).await;
        });
    }
}

pub async fn enqueue_repo_job(
    state: &Arc<AppState>,
    full_name: &str,
    op: GhcrWebhookOp,
    created_by: &str,
    reason: &str,
) -> anyhow::Result<String> {
    let _guard = sync_enqueue_lock().lock().await;

    if op == GhcrWebhookOp::AuditAll {
        anyhow::bail!("audit_all must be enqueued via enqueue_audit_job");
    }

    let (owner, repo) = parse_full_name(full_name)?;
    if state
        .db
        .get_github_packages_repo(&owner, &repo)
        .await?
        .is_none()
    {
        anyhow::bail!("repo is not tracked");
    }

    let now = now_rfc3339();
    let job_id = ids::new_job_id();
    let progress = make_progress(
        "queued",
        format!("waiting to {} webhook", op.as_str()),
        0,
        1,
        Some(format!("{owner}/{repo}")),
        now.clone(),
    );

    let summary_json = json!({
        "op": op.as_str(),
        "repos": [format!("{owner}/{repo}")],
        "progress": serde_json::to_value(&progress)?,
    });

    state
        .db
        .insert_job(JobListItem {
            id: job_id.clone(),
            r#type: JobType::GitHubPackagesWebhook,
            scope: JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json,
        })
        .await?;

    state
        .db
        .set_github_packages_repo_webhook_job_state(
            &owner,
            &repo,
            "queued",
            Some(&job_id),
            Some(op.as_str()),
            &now,
        )
        .await?;

    emit_job_event(
        state,
        &job_id,
        &json!({
            "type": "job_enqueued",
            "jobType": "github_packages_webhook",
            "op": op.as_str(),
            "target": format!("{owner}/{repo}"),
            "jobId": job_id,
            "ts": now,
        }),
    )
    .await;

    Ok(job_id)
}

pub async fn enqueue_sync_all_job(
    state: &Arc<AppState>,
    created_by: &str,
    reason: &str,
) -> anyhow::Result<GhcrSyncEnqueueResult> {
    let _guard = sync_enqueue_lock().lock().await;

    if let Some(existing) = state
        .db
        .find_latest_pending_job_by_type(JobType::GitHubPackagesWebhookSyncAll)
        .await?
    {
        return Ok(GhcrSyncEnqueueResult {
            job_id: existing.id,
            status: existing.status,
            reused: true,
        });
    }

    let repos = state
        .db
        .list_github_packages_repos()
        .await?
        .into_iter()
        .filter(|row| row.selected)
        .filter(|row| !repo_unregistration_in_progress(&row.webhook_state, row.last_op.as_deref()))
        .map(|row| format!("{}/{}", row.owner, row.repo))
        .collect::<Vec<_>>();
    if repos.is_empty() {
        anyhow::bail!("no tracked repos selected");
    }

    let now = now_rfc3339();
    let job_id = ids::new_job_id();
    let progress = make_progress(
        "queued",
        "waiting to sync tracked repos".to_string(),
        0,
        repos.len() as u32,
        None,
        now.clone(),
    );
    let summary_json = json!({
        "op": "sync_all",
        "repos": repos,
        "progress": serde_json::to_value(&progress)?,
    });

    state
        .db
        .insert_job(JobListItem {
            id: job_id.clone(),
            r#type: JobType::GitHubPackagesWebhookSyncAll,
            scope: JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json,
        })
        .await?;

    emit_job_event(
        state,
        &job_id,
        &json!({
            "type": "job_enqueued",
            "jobType": JobType::GitHubPackagesWebhookSyncAll.as_str(),
            "op": "sync_all",
            "jobId": job_id,
            "ts": now,
        }),
    )
    .await;

    Ok(GhcrSyncEnqueueResult {
        job_id,
        status: "queued".to_string(),
        reused: false,
    })
}

pub async fn enqueue_sync_repo_job(
    state: &Arc<AppState>,
    full_name: &str,
    created_by: &str,
    reason: &str,
) -> anyhow::Result<GhcrSyncEnqueueResult> {
    let (owner, repo) = parse_full_name(full_name)?;
    let repo_key = format!("{owner}/{repo}").to_ascii_lowercase();

    let _guard = sync_enqueue_lock().lock().await;

    if let Some(existing) = state
        .db
        .find_latest_pending_job_by_type_and_service_id(
            JobType::GitHubPackagesWebhookSyncRepo,
            &repo_key,
        )
        .await?
    {
        return Ok(GhcrSyncEnqueueResult {
            job_id: existing.id,
            status: existing.status,
            reused: true,
        });
    }

    let tracked = state.db.get_github_packages_repo(&owner, &repo).await?;
    let Some(tracked) = tracked else {
        anyhow::bail!("repo is not tracked");
    };
    if !tracked.selected {
        anyhow::bail!("repo is not selected");
    }
    if repo_unregistration_in_progress(&tracked.webhook_state, tracked.last_op.as_deref()) {
        anyhow::bail!("repo unregister in progress");
    }
    if repo_registration_in_progress(&tracked.webhook_state, tracked.last_op.as_deref())
        && let Some(existing_job_id) = tracked.webhook_job_id.as_ref()
        && let Some(existing_job) = state.db.get_job(existing_job_id).await?
        && is_pending_status(&existing_job.status)
        && is_legacy_register_job(&existing_job)
    {
        return Ok(GhcrSyncEnqueueResult {
            job_id: existing_job.id,
            status: existing_job.status,
            reused: true,
        });
    }

    let full_name = format!("{owner}/{repo}");
    let now = now_rfc3339();
    let job_id = ids::new_job_id();
    let progress = make_progress(
        "queued",
        "waiting to sync webhook".to_string(),
        0,
        1,
        Some(full_name.clone()),
        now.clone(),
    );
    let summary_json = json!({
        "op": "sync_repo",
        "repos": [full_name],
        "progress": serde_json::to_value(&progress)?,
    });

    state
        .db
        .insert_job(JobListItem {
            id: job_id.clone(),
            r#type: JobType::GitHubPackagesWebhookSyncRepo,
            scope: JobScope::All,
            stack_id: None,
            service_id: Some(repo_key),
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json,
        })
        .await?;

    state
        .db
        .set_github_packages_repo_webhook_job_state(
            &owner,
            &repo,
            "queued",
            Some(&job_id),
            Some(GhcrWebhookOp::Register.as_str()),
            &now,
        )
        .await?;

    emit_job_event(
        state,
        &job_id,
        &json!({
            "type": "job_enqueued",
            "jobType": JobType::GitHubPackagesWebhookSyncRepo.as_str(),
            "op": "sync_repo",
            "target": format!("{owner}/{repo}"),
            "jobId": job_id,
            "ts": now,
        }),
    )
    .await;

    Ok(GhcrSyncEnqueueResult {
        job_id,
        status: "queued".to_string(),
        reused: false,
    })
}

pub async fn enqueue_audit_job(
    state: &Arc<AppState>,
    created_by: &str,
    reason: &str,
) -> anyhow::Result<String> {
    let now = now_rfc3339();
    let job_id = ids::new_job_id();
    let progress = make_progress(
        "queued",
        "waiting to audit webhook drift".to_string(),
        0,
        0,
        None,
        now.clone(),
    );

    let summary_json = json!({
        "op": GhcrWebhookOp::AuditAll.as_str(),
        "repos": [],
        "progress": serde_json::to_value(&progress)?,
    });

    state
        .db
        .insert_job(JobListItem {
            id: job_id.clone(),
            r#type: JobType::GitHubPackagesWebhook,
            scope: JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: created_by.to_string(),
            reason: reason.to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json,
        })
        .await?;

    emit_job_event(
        state,
        &job_id,
        &json!({
            "type": "job_enqueued",
            "jobType": "github_packages_webhook",
            "op": GhcrWebhookOp::AuditAll.as_str(),
            "jobId": job_id,
            "ts": now,
        }),
    )
    .await;

    Ok(job_id)
}

pub async fn get_overview(
    state: &Arc<AppState>,
) -> anyhow::Result<GitHubPackagesWebhookOverviewResponse> {
    overview::get_overview(state).await
}

async fn run_worker_loop(state: Arc<AppState>, job_type: JobType) {
    loop {
        let started_at = now_rfc3339();
        match state
            .db
            .claim_next_queued_job_by_type(job_type.clone(), &started_at)
            .await
        {
            Ok(Some(job)) => {
                if let Err(err) = run_claimed_job(state.clone(), job).await {
                    tracing::error!(
                        error = %err,
                        job_type = %job_type.as_str(),
                        "ghcr webhook job run failed"
                    );
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_millis(WORKER_IDLE_POLL_MS)).await;
            }
            Err(err) => {
                tracing::error!(
                    error = %err,
                    job_type = %job_type.as_str(),
                    "ghcr webhook worker claim failed"
                );
                tokio::time::sleep(Duration::from_millis(WORKER_IDLE_POLL_MS)).await;
            }
        }
    }
}

async fn run_claimed_job(state: Arc<AppState>, job: JobListItem) -> anyhow::Result<()> {
    let job_id = job.id.clone();
    let started_at = job.started_at.clone().unwrap_or_else(now_rfc3339);
    let parsed = parse_job_payload(&job).context("parse ghcr webhook job payload")?;
    let op_label = match parsed.kind {
        ParsedGhcrWebhookJobKind::Legacy(op) => op.as_str(),
        ParsedGhcrWebhookJobKind::SyncAll => "sync_all",
        ParsedGhcrWebhookJobKind::SyncRepo => "sync_repo",
    };

    emit_job_event(
        &state,
        &job_id,
        &json!({
            "type": "ghcr_webhook_job_started",
            "jobId": job_id,
            "op": op_label,
            "repos": parsed.repos.iter().map(|(o,r)| format!("{o}/{r}")).collect::<Vec<_>>(),
            "ts": started_at,
        }),
    )
    .await;

    let counters = match parsed.kind {
        ParsedGhcrWebhookJobKind::Legacy(GhcrWebhookOp::Register) => {
            run_register_job(&state, &job_id, &parsed.repos, "register", 1).await
        }
        ParsedGhcrWebhookJobKind::Legacy(GhcrWebhookOp::Unregister) => {
            run_unregister_job(&state, &job_id, &parsed.repos).await
        }
        ParsedGhcrWebhookJobKind::Legacy(GhcrWebhookOp::AuditAll) => {
            run_audit_job(&state, &job_id).await
        }
        ParsedGhcrWebhookJobKind::SyncAll => {
            run_register_job(
                &state,
                &job_id,
                &parsed.repos,
                "sync_all",
                GHCR_SYNC_ALL_MAX_CONCURRENCY,
            )
            .await
        }
        ParsedGhcrWebhookJobKind::SyncRepo => {
            run_register_job(&state, &job_id, &parsed.repos, "sync_repo", 1).await
        }
    };

    let finished_at = now_rfc3339();
    let final_progress = make_progress(
        "done",
        format!(
            "{} finished (ok={}, missing={}, conflict={}, error={}, deleted={})",
            op_label,
            counters.ok,
            counters.missing,
            counters.conflict,
            counters.error,
            counters.deleted,
        ),
        counters.total,
        counters.total,
        None,
        finished_at.clone(),
    );
    persist_progress(&state, &job_id, &final_progress).await;

    let summary = json!({
        "op": op_label,
        "total": counters.total,
        "ok": counters.ok,
        "missing": counters.missing,
        "conflict": counters.conflict,
        "error": counters.error,
        "deleted": counters.deleted,
        "hardFailures": counters.hard_failures,
        "progress": serde_json::to_value(&final_progress).ok(),
    });

    let final_status = if counters.hard_failures > 0 {
        "failed"
    } else {
        "success"
    };

    state
        .db
        .finish_job(&job_id, final_status, &finished_at, &summary)
        .await?;

    emit_job_event(
        &state,
        &job_id,
        &json!({
            "type": "ghcr_webhook_job_finished",
            "jobId": job_id,
            "op": op_label,
            "status": final_status,
            "summary": summary,
            "ts": finished_at,
        }),
    )
    .await;

    let scheduled_audit = matches!(
        parsed.kind,
        ParsedGhcrWebhookJobKind::Legacy(GhcrWebhookOp::AuditAll)
    ) && job.created_by == "schedule"
        && job.reason == "schedule";
    let anomaly_total = counters.missing + counters.conflict + counters.error;
    if scheduled_audit && anomaly_total > 0 {
        match state.db.list_github_packages_repos().await {
            Ok(rows) => {
                let anomaly_repos = rows
                    .into_iter()
                    .filter(|row| row.selected)
                    .filter(|row| {
                        matches!(row.webhook_state.as_str(), "missing" | "conflict" | "error")
                    })
                    .map(|row| notify::GhcrWebhookAnomalyRepo {
                        owner: row.owner,
                        repo: row.repo,
                        state: row.webhook_state,
                        last_error: row.last_error,
                    })
                    .collect::<Vec<_>>();

                let notify_state = state.clone();
                let notify_job_id = job_id.clone();
                let notify_finished_at = finished_at.clone();
                tokio::spawn(async move {
                    let event = notify::GhcrWebhookAnomalyEvent {
                        job_id: &notify_job_id,
                        status: final_status,
                        counts: notify::GhcrWebhookAnomalyCounts {
                            missing: counters.missing,
                            conflict: counters.conflict,
                            error: counters.error,
                        },
                        repos: &anomaly_repos,
                    };
                    let _ = notify::notify_ghcr_webhook_anomaly(
                        notify_state.as_ref(),
                        &notify_finished_at,
                        event,
                    )
                    .await;
                });
            }
            Err(err) => {
                tracing::warn!(
                    job_id = %job_id,
                    error = %err,
                    "ghcr webhook anomaly notify: failed to list repos"
                );
            }
        }
    }

    Ok(())
}

async fn run_register_job(
    state: &Arc<AppState>,
    job_id: &str,
    repos: &[(String, String)],
    phase_label: &'static str,
    max_concurrency: usize,
) -> GhcrJobCounters {
    let mut counters = GhcrJobCounters {
        total: repos.len() as u32,
        ..Default::default()
    };

    let settings = match load_settings(state).await {
        Ok(s) => s,
        Err(err) => {
            for (owner, repo) in repos {
                let msg = format!("settings unavailable: {err}");
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    if !settings.enabled {
        let msg = "github packages webhook is disabled".to_string();
        for (owner, repo) in repos {
            mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
        }
        return counters;
    }

    let callback_url = match settings.callback_url.as_deref() {
        Some(v) if Url::parse(v).is_ok() => v.to_string(),
        _ => {
            let msg = "callbackUrl is missing or invalid".to_string();
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let pat = match settings.pat {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            let msg = "pat is required".to_string();
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let secret = match settings.webhook_secret {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            let msg = "webhook secret is missing".to_string();
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let client = match github::GitHubClient::new(&pat) {
        Ok(client) => client,
        Err(err) => {
            let msg = format!("failed to create github client: {err}");
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Register, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let total = repos.len() as u32;
    if total == 0 {
        return counters;
    }
    let concurrency = max_concurrency.max(1).min(repos.len());
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut tasks = tokio::task::JoinSet::new();

    for (owner, repo) in repos.iter().cloned() {
        let state = state.clone();
        let client = client.clone();
        let callback_url = callback_url.clone();
        let secret = secret.clone();
        let semaphore = semaphore.clone();
        let job_id = job_id.to_string();
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("ghcr register semaphore closed");
            let result = run_register_repo_once(
                &state,
                &job_id,
                &owner,
                &repo,
                &client,
                &callback_url,
                &secret,
            )
            .await;
            (owner, repo, result)
        });
    }

    let mut completed = 0_u32;
    while let Some(joined) = tasks.join_next().await {
        let (owner, repo, item) = match joined {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(error = %err, "ghcr register task join failed");
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
                continue;
            }
        };

        counters.ok = counters.ok.saturating_add(item.ok);
        counters.missing = counters.missing.saturating_add(item.missing);
        counters.conflict = counters.conflict.saturating_add(item.conflict);
        counters.error = counters.error.saturating_add(item.error);
        counters.deleted = counters.deleted.saturating_add(item.deleted);
        counters.hard_failures = counters.hard_failures.saturating_add(item.hard_failures);

        completed = completed.saturating_add(1);
        let progress = make_progress(
            phase_label,
            format!("{phase_label} ({completed}/{total})"),
            completed,
            total,
            Some(format!("{owner}/{repo}")),
            now_rfc3339(),
        );
        persist_progress(state, job_id, &progress).await;
    }

    counters
}

async fn run_register_repo_once(
    state: &Arc<AppState>,
    job_id: &str,
    owner: &str,
    repo: &str,
    client: &github::GitHubClient,
    callback_url: &str,
    secret: &str,
) -> GhcrJobCounters {
    let mut counters = GhcrJobCounters::default();
    let _repo_guard = lock_repo_sync(owner, repo).await;
    let current_target = format!("{owner}/{repo}");

    match state.db.get_github_packages_repo(owner, repo).await {
        Ok(None) => {
            tracing::info!(
                target = %current_target,
                job_id = %job_id,
                "skip ghcr sync for repo that is no longer tracked"
            );
            return counters;
        }
        Ok(Some(current_repo))
            if repo_unregistration_in_progress(
                &current_repo.webhook_state,
                current_repo.last_op.as_deref(),
            ) =>
        {
            tracing::info!(
                target = %current_target,
                job_id = %job_id,
                "skip ghcr sync while unregister job is in progress"
            );
            return counters;
        }
        Ok(Some(_)) => {}
        Err(err) => {
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
            mark_repo_error(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::Register,
                &err.to_string(),
            )
            .await;
            return counters;
        }
    }

    let _ = state
        .db
        .set_github_packages_repo_webhook_job_state(
            owner,
            repo,
            "running",
            Some(job_id),
            Some(GhcrWebhookOp::Register.as_str()),
            &now_rfc3339(),
        )
        .await;

    let hooks_res =
        github_call_with_retry(state, job_id, "list_hooks", &current_target, || async {
            client.list_repo_hooks(owner, repo).await
        })
        .await;

    let hooks = match hooks_res {
        Ok(v) => v,
        Err(err) => {
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
            mark_repo_error(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::Register,
                &err.to_string(),
            )
            .await;
            return counters;
        }
    };

    let mut matches = Vec::new();
    for hook in &hooks {
        let Some(url) = hook.config.url.as_deref() else {
            continue;
        };
        if !urls_match(url, callback_url) {
            continue;
        }
        if !hook.events.iter().any(|event| event == "package") {
            continue;
        }
        matches.push(hook);
    }

    if matches.len() > 1 {
        counters.conflict = counters.conflict.saturating_add(1);
        counters.hard_failures = counters.hard_failures.saturating_add(1);
        let message = format!(
            "multiple matching webhooks found ({}); remove duplicates on GitHub then retry",
            matches.len()
        );
        mark_repo_state(
            state,
            owner,
            repo,
            job_id,
            GhcrWebhookOp::Register,
            "conflict",
            None,
            None,
            Some(&message),
        )
        .await;
        return counters;
    }

    if matches.is_empty() {
        let created =
            github_call_with_retry(state, job_id, "create_hook", &current_target, || async {
                client
                    .create_repo_hook(
                        owner,
                        repo,
                        &github::CreateWebhookRequest {
                            name: "web",
                            active: true,
                            events: vec!["package"],
                            config: github::CreateWebhookConfig {
                                url: callback_url,
                                content_type: "json",
                                secret,
                                insecure_ssl: "0",
                            },
                        },
                    )
                    .await
            })
            .await;

        match created {
            Ok(hook) => {
                counters.ok = counters.ok.saturating_add(1);
                mark_repo_state(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::Register,
                    "ok",
                    Some(hook.id),
                    Some(&now_rfc3339()),
                    None,
                )
                .await;
            }
            Err(err) => {
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::Register,
                    &err.to_string(),
                )
                .await;
            }
        }
        return counters;
    }

    let existing = matches[0];
    let updated = github_call_with_retry(state, job_id, "update_hook", &current_target, || async {
        client
            .update_repo_hook(
                owner,
                repo,
                existing.id,
                &github::UpdateWebhookRequest {
                    active: true,
                    events: vec!["package"],
                    config: github::UpdateWebhookConfig {
                        url: callback_url,
                        content_type: "json",
                        secret,
                        insecure_ssl: "0",
                    },
                },
            )
            .await
    })
    .await;

    match updated {
        Ok(hook) => {
            counters.ok = counters.ok.saturating_add(1);
            mark_repo_state(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::Register,
                "ok",
                Some(hook.id),
                Some(&now_rfc3339()),
                None,
            )
            .await;
        }
        Err(err) => {
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
            mark_repo_error(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::Register,
                &err.to_string(),
            )
            .await;
        }
    }

    counters
}

async fn run_unregister_job(
    state: &Arc<AppState>,
    job_id: &str,
    repos: &[(String, String)],
) -> GhcrJobCounters {
    let mut counters = GhcrJobCounters {
        total: repos.len() as u32,
        ..Default::default()
    };

    let settings = match load_settings(state).await {
        Ok(s) => s,
        Err(err) => {
            for (owner, repo) in repos {
                let msg = format!("settings unavailable: {err}");
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Unregister, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let pat = match settings.pat {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            let msg = "pat is required".to_string();
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Unregister, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let callback_url = settings
        .callback_url
        .as_deref()
        .filter(|v| Url::parse(v).is_ok())
        .map(ToString::to_string);

    let client = match github::GitHubClient::new(&pat) {
        Ok(client) => client,
        Err(err) => {
            let msg = format!("failed to create github client: {err}");
            for (owner, repo) in repos {
                mark_repo_error(state, owner, repo, job_id, GhcrWebhookOp::Unregister, &msg).await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    for (index, (owner, repo)) in repos.iter().enumerate() {
        let current_target = format!("{owner}/{repo}");
        let progress = make_progress(
            "unregister",
            format!("unregistering webhook ({}/{})", index + 1, repos.len()),
            index as u32,
            repos.len() as u32,
            Some(current_target.clone()),
            now_rfc3339(),
        );
        persist_progress(state, job_id, &progress).await;
        let _repo_guard = lock_repo_sync(owner, repo).await;

        let _ = state
            .db
            .set_github_packages_repo_webhook_job_state(
                owner,
                repo,
                "running",
                Some(job_id),
                Some(GhcrWebhookOp::Unregister.as_str()),
                &now_rfc3339(),
            )
            .await;

        let repo_row = state
            .db
            .get_github_packages_repo(owner, repo)
            .await
            .ok()
            .flatten();
        let mut errors: Vec<String> = Vec::new();

        if let Some(hook_id) = repo_row.as_ref().and_then(|row| row.hook_id) {
            let deleted = github_call_with_retry(
                state,
                job_id,
                "delete_hook",
                &format!("{current_target}#{hook_id}"),
                || async { client.delete_repo_hook(owner, repo, hook_id).await },
            )
            .await;
            if let Err(err) = deleted
                && github_http_status_from_error(&err) != Some(404)
            {
                errors.push(format!("hook {hook_id}: {err}"));
            }
        }

        if let Some(callback_url) = callback_url.as_deref() {
            let hooks =
                github_call_with_retry(state, job_id, "list_hooks", &current_target, || async {
                    client.list_repo_hooks(owner, repo).await
                })
                .await;

            match hooks {
                Ok(hooks) => {
                    for hook in hooks {
                        let Some(url) = hook.config.url.as_deref() else {
                            continue;
                        };
                        if !urls_match(url, callback_url) {
                            continue;
                        }
                        if !hook.events.iter().any(|event| event == "package") {
                            continue;
                        }

                        let deleted = github_call_with_retry(
                            state,
                            job_id,
                            "delete_hook",
                            &format!("{current_target}#{}", hook.id),
                            || async { client.delete_repo_hook(owner, repo, hook.id).await },
                        )
                        .await;
                        if let Err(err) = deleted
                            && github_http_status_from_error(&err) != Some(404)
                        {
                            errors.push(format!("hook {}: {}", hook.id, err));
                        }
                    }
                }
                Err(err) => errors.push(format!("list hooks: {err}")),
            }
        }

        if errors.is_empty() {
            let _ = state.db.delete_github_packages_repo(owner, repo).await;
            counters.deleted = counters.deleted.saturating_add(1);
            counters.ok = counters.ok.saturating_add(1);
            emit_job_event(
                state,
                job_id,
                &json!({
                    "type": "ghcr_repo_deleted",
                    "jobId": job_id,
                    "op": GhcrWebhookOp::Unregister.as_str(),
                    "target": current_target,
                    "ts": now_rfc3339(),
                }),
            )
            .await;
        } else {
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
            mark_repo_error(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::Unregister,
                &errors.join("; "),
            )
            .await;
        }
    }

    counters
}

async fn run_audit_job(state: &Arc<AppState>, job_id: &str) -> GhcrJobCounters {
    let mut counters = GhcrJobCounters::default();
    let repos = match state.db.list_github_packages_repos().await {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.selected)
            .map(|row| (row.owner, row.repo))
            .collect::<Vec<_>>(),
        Err(err) => {
            tracing::warn!(error = %err, "list repos for audit failed");
            return counters;
        }
    };

    counters.total = repos.len() as u32;

    let settings = match load_settings(state).await {
        Ok(s) => s,
        Err(err) => {
            for (owner, repo) in &repos {
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::AuditAll,
                    &format!("settings unavailable: {err}"),
                )
                .await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    if !settings.enabled {
        for (owner, repo) in &repos {
            mark_repo_error(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::AuditAll,
                "github packages webhook is disabled",
            )
            .await;
            counters.error = counters.error.saturating_add(1);
            counters.hard_failures = counters.hard_failures.saturating_add(1);
        }
        return counters;
    }

    let callback_url = match settings.callback_url.as_deref() {
        Some(v) if Url::parse(v).is_ok() => v.to_string(),
        _ => {
            for (owner, repo) in &repos {
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::AuditAll,
                    "callbackUrl is missing or invalid",
                )
                .await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let pat = match settings.pat {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            for (owner, repo) in &repos {
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::AuditAll,
                    "pat is required",
                )
                .await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    let client = match github::GitHubClient::new(&pat) {
        Ok(client) => client,
        Err(err) => {
            for (owner, repo) in &repos {
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::AuditAll,
                    &format!("failed to create github client: {err}"),
                )
                .await;
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
            }
            return counters;
        }
    };

    for (index, (owner, repo)) in repos.iter().enumerate() {
        let current_target = format!("{owner}/{repo}");
        let progress = make_progress(
            "audit",
            format!("auditing webhook drift ({}/{})", index + 1, repos.len()),
            index as u32,
            repos.len() as u32,
            Some(current_target.clone()),
            now_rfc3339(),
        );
        persist_progress(state, job_id, &progress).await;

        let _ = state
            .db
            .set_github_packages_repo_webhook_job_state(
                owner,
                repo,
                "running",
                Some(job_id),
                Some(GhcrWebhookOp::AuditAll.as_str()),
                &now_rfc3339(),
            )
            .await;

        let hooks =
            github_call_with_retry(state, job_id, "list_hooks", &current_target, || async {
                client.list_repo_hooks(owner, repo).await
            })
            .await;

        let hooks = match hooks {
            Ok(hooks) => hooks,
            Err(err) => {
                counters.error = counters.error.saturating_add(1);
                counters.hard_failures = counters.hard_failures.saturating_add(1);
                mark_repo_error(
                    state,
                    owner,
                    repo,
                    job_id,
                    GhcrWebhookOp::AuditAll,
                    &err.to_string(),
                )
                .await;
                continue;
            }
        };

        let mut matches = Vec::new();
        for hook in &hooks {
            let Some(url) = hook.config.url.as_deref() else {
                continue;
            };
            if !urls_match(url, &callback_url) {
                continue;
            }
            if !hook.events.iter().any(|event| event == "package") {
                continue;
            }
            matches.push(hook);
        }

        if matches.is_empty() {
            counters.missing = counters.missing.saturating_add(1);
            mark_repo_state(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::AuditAll,
                "missing",
                None,
                None,
                Some("webhook missing"),
            )
            .await;
            continue;
        }

        if matches.len() > 1 {
            counters.conflict = counters.conflict.saturating_add(1);
            mark_repo_state(
                state,
                owner,
                repo,
                job_id,
                GhcrWebhookOp::AuditAll,
                "conflict",
                None,
                None,
                Some("multiple matching webhooks found"),
            )
            .await;
            continue;
        }

        counters.ok = counters.ok.saturating_add(1);
        mark_repo_state(
            state,
            owner,
            repo,
            job_id,
            GhcrWebhookOp::AuditAll,
            "ok",
            Some(matches[0].id),
            None,
            None,
        )
        .await;
    }

    counters
}
