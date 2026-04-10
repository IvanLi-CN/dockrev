use std::sync::Arc;

use crate::{
    api::types::{
        GitHubPackagesWebhookOverviewResponse, GitHubPackagesWebhookOverviewSummary, JobListItem,
        JobType,
    },
    state::AppState,
};

pub(super) async fn get_overview(
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
