use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::Context as _;
use serde_json::json;
use tokio::sync::Semaphore;
use url::Url;

use crate::{
    api::types::{
        GitHubPackagesWebhookOverviewResponse, GitHubPackagesWebhookOverviewSummary, JobListItem,
        JobLogLine, JobProgress, JobScope, JobType,
    },
    github, ids,
    state::AppState,
};

const WORKER_IDLE_POLL_MS: u64 = 400;
const RETRY_MAX_ATTEMPTS: u32 = 3;
const GHCR_SYNC_ALL_MAX_CONCURRENCY: usize = 5;
const GHCR_SYNC_REPO_WORKERS: usize = 5;
const GHCR_SYNC_REPO_LOCK_STRIPES: usize = 128;

static GHCR_SYNC_ENQUEUE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static GHCR_SYNC_REPO_LOCKS: OnceLock<Vec<Arc<tokio::sync::Mutex<()>>>> = OnceLock::new();

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

#[derive(Clone, Debug)]
struct ParsedGhcrWebhookJob {
    kind: ParsedGhcrWebhookJobKind,
    repos: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsedGhcrWebhookJobKind {
    Legacy(GhcrWebhookOp),
    SyncAll,
    SyncRepo,
}

#[derive(Clone, Debug)]
struct GhcrWebhookSettings {
    enabled: bool,
    callback_url: Option<String>,
    pat: Option<String>,
    webhook_secret: Option<String>,
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

fn sync_enqueue_lock() -> &'static tokio::sync::Mutex<()> {
    GHCR_SYNC_ENQUEUE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn repo_sync_locks() -> &'static [Arc<tokio::sync::Mutex<()>>] {
    GHCR_SYNC_REPO_LOCKS
        .get_or_init(|| {
            (0..GHCR_SYNC_REPO_LOCK_STRIPES)
                .map(|_| Arc::new(tokio::sync::Mutex::new(())))
                .collect()
        })
        .as_slice()
}

fn repo_sync_lock_index(key: &str) -> usize {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % GHCR_SYNC_REPO_LOCK_STRIPES
}

async fn lock_repo_sync(owner: &str, repo: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let key = format!("{owner}/{repo}").to_ascii_lowercase();
    let repo_lock = repo_sync_locks()[repo_sync_lock_index(&key)].clone();
    repo_lock.lock_owned().await
}

fn repo_unregistration_in_progress(webhook_state: &str, last_op: Option<&str>) -> bool {
    matches!(webhook_state, "queued" | "running")
        && last_op == Some(GhcrWebhookOp::Unregister.as_str())
}

fn repo_registration_in_progress(webhook_state: &str, last_op: Option<&str>) -> bool {
    matches!(webhook_state, "queued" | "running")
        && last_op == Some(GhcrWebhookOp::Register.as_str())
}

fn is_pending_status(status: &str) -> bool {
    status == "queued" || status == "running"
}

fn is_legacy_register_job(job: &JobListItem) -> bool {
    job.r#type.as_str() == JobType::GitHubPackagesWebhook.as_str()
        && job.summary_json.get("op").and_then(|v| v.as_str())
            == Some(GhcrWebhookOp::Register.as_str())
}

pub fn spawn_tasks(state: Arc<AppState>) {
    spawn_worker_loops(&state, JobType::GitHubPackagesWebhook, 1);
    spawn_worker_loops(&state, JobType::GitHubPackagesWebhookSyncAll, 1);
    spawn_worker_loops(
        &state,
        JobType::GitHubPackagesWebhookSyncRepo,
        GHCR_SYNC_REPO_WORKERS,
    );

    let scheduler_state = state.clone();
    tokio::spawn(async move {
        run_audit_scheduler(scheduler_state).await;
    });
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
    let mut summary = GitHubPackagesWebhookOverviewSummary {
        tracked: 0,
        ok: 0,
        missing: 0,
        error: 0,
        conflict: 0,
        queued: 0,
        running: 0,
        unknown: 0,
    };

    let mut last_audit_at: Option<String> = None;
    for (webhook_state, audit_at) in state
        .db
        .list_github_packages_repos_for_job_state_summary()
        .await?
    {
        summary.tracked = summary.tracked.saturating_add(1);
        match webhook_state.as_str() {
            "ok" => summary.ok = summary.ok.saturating_add(1),
            "missing" => summary.missing = summary.missing.saturating_add(1),
            "error" => summary.error = summary.error.saturating_add(1),
            "conflict" => summary.conflict = summary.conflict.saturating_add(1),
            "queued" => summary.queued = summary.queued.saturating_add(1),
            "running" => summary.running = summary.running.saturating_add(1),
            _ => summary.unknown = summary.unknown.saturating_add(1),
        }

        if let Some(audit_at) = audit_at {
            let replace = last_audit_at
                .as_deref()
                .is_none_or(|current| audit_at.as_str() > current);
            if replace {
                last_audit_at = Some(audit_at);
            }
        }
    }

    let ghcr_job_types = [
        JobType::GitHubPackagesWebhook,
        JobType::GitHubPackagesWebhookSyncAll,
        JobType::GitHubPackagesWebhookSyncRepo,
    ];

    let mut jobs_queued = 0_u32;
    let mut jobs_running = 0_u32;
    let mut running_job: Option<JobListItem> = None;

    for job_type in &ghcr_job_types {
        jobs_queued = jobs_queued.saturating_add(
            state
                .db
                .count_jobs_by_type_and_status(job_type.clone(), "queued")
                .await?,
        );
        jobs_running = jobs_running.saturating_add(
            state
                .db
                .count_jobs_by_type_and_status(job_type.clone(), "running")
                .await?,
        );

        if let Some(candidate) = state
            .db
            .list_jobs_by_type_and_statuses(job_type.clone(), &["running"], 1)
            .await?
            .first()
            .cloned()
            && running_job
                .as_ref()
                .is_none_or(|current| candidate.created_at > current.created_at)
        {
            running_job = Some(candidate);
        }
    }
    let running_job_id = running_job.map(|job| job.id);

    Ok(GitHubPackagesWebhookOverviewResponse {
        summary,
        jobs_queued,
        jobs_running,
        running_job_id,
        last_audit_at,
    })
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

async fn run_audit_scheduler(state: Arc<AppState>) {
    let interval = state.config.ghcr_webhook_audit_interval_seconds.max(60);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    // Skip the immediate first tick so schedule cadence starts after one full interval.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if let Err(err) = enqueue_audit_job(&state, "schedule", "schedule").await {
            tracing::warn!(error = %err, "enqueue ghcr audit job failed");
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

async fn load_settings(state: &Arc<AppState>) -> anyhow::Result<GhcrWebhookSettings> {
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

async fn mark_repo_error(
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
async fn mark_repo_state(
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

fn parse_job_payload(job: &JobListItem) -> anyhow::Result<ParsedGhcrWebhookJob> {
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

async fn github_call_with_retry<T, F, Fut>(
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

async fn persist_progress(state: &Arc<AppState>, job_id: &str, progress: &JobProgress) {
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

async fn emit_job_event(state: &Arc<AppState>, job_id: &str, payload: &serde_json::Value) {
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

fn make_progress(
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
        planned_percent: Some(percent),
        current_target,
        updated_at,
    }
}

fn parse_full_name(full_name: &str) -> anyhow::Result<(String, String)> {
    let mut parts = full_name.trim().split('/');
    let owner = parts.next().unwrap_or_default().trim();
    let repo = parts.next().unwrap_or_default().trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        anyhow::bail!("invalid fullName");
    }
    Ok((owner.to_string(), repo.to_string()))
}

fn urls_match(a: &str, b: &str) -> bool {
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

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string())
}
