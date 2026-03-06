use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::Context as _;
use chrono::Local;
use cron::Schedule;
use serde_json::{Value, json};

use crate::{
    api,
    api::types::{JobLogLine, JobRecord, JobScope, JobType},
    ghcr_webhook_jobs, ids, notify, registry,
    state::AppState,
};

const SETTINGS_REFRESH_INTERVAL_SECONDS: u64 = 30;

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

fn next_fire_time_local(expr: &str) -> anyhow::Result<chrono::DateTime<Local>> {
    let normalized = crate::cron_expr::normalize_cron(expr)?;
    let schedule = Schedule::from_str(&normalized)?;
    schedule
        .upcoming(Local)
        .next()
        .ok_or_else(|| anyhow::anyhow!("cron produced no upcoming fire times"))
}

fn extract_discovered_new_versions(summary: &Value) -> Vec<notify::NewVersionDiscoveredService> {
    let Some(items) = summary
        .get("newVersions")
        .and_then(|v| v.get("services"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(stack_id) = item.get("stackId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(service_id) = item.get("serviceId").and_then(|v| v.as_str()) else {
            continue;
        };
        out.push(notify::NewVersionDiscoveredService {
            stack_id: stack_id.to_string(),
            service_id: service_id.to_string(),
            current_tag: item
                .get("currentTag")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            candidate_tag: item
                .get("candidateTag")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
        });
    }
    out
}

pub fn spawn_tasks(state: Arc<AppState>) {
    spawn_update_check_scheduler(state.clone());
    spawn_ghcr_webhook_audit_scheduler(state);
}

fn spawn_update_check_scheduler(state: Arc<AppState>) {
    tokio::spawn(async move {
        let refresh = Duration::from_secs(SETTINGS_REFRESH_INTERVAL_SECONDS);
        loop {
            let settings = match state.db.get_schedule_settings().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "update check scheduler: settings unavailable");
                    tokio::time::sleep(refresh).await;
                    continue;
                }
            };

            let spec = settings.update_check;
            if !spec.enabled {
                tokio::time::sleep(refresh).await;
                continue;
            }

            let next = match next_fire_time_local(&spec.cron) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, cron = %spec.cron, "update check scheduler: invalid cron");
                    tokio::time::sleep(refresh).await;
                    continue;
                }
            };

            let now_local = Local::now();
            let until = (next - now_local)
                .to_std()
                .unwrap_or_else(|_| Duration::from_secs(0));

            if until <= refresh {
                tokio::time::sleep(until).await;
                // Re-check settings before firing to avoid a "last-second disable" from triggering.
                match state.db.get_schedule_settings().await {
                    Ok(latest) => {
                        if !latest.update_check.enabled {
                            continue;
                        }
                        if crate::cron_expr::canonicalize_for_store(&latest.update_check.cron)
                            != crate::cron_expr::canonicalize_for_store(&spec.cron)
                        {
                            continue;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "update check scheduler: settings unavailable before fire");
                        continue;
                    }
                }

                if let Err(e) = trigger_scheduled_check(state.clone()).await {
                    tracing::warn!(error = %e, "update check scheduler: tick failed");
                }
                continue;
            }

            tokio::time::sleep(refresh).await;
        }
    });
}

async fn trigger_scheduled_check(state: Arc<AppState>) -> anyhow::Result<()> {
    let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());

    // Skip if a check is already running. If the existing job is stale, terminate it and proceed.
    let stale_threshold = time::Duration::hours(2);
    if let Some(existing) = state
        .db
        .find_latest_running_check_job(&JobScope::All, None, None)
        .await?
    {
        let started_at = existing
            .started_at
            .as_deref()
            .unwrap_or(existing.created_at.as_str());
        let existing_stale =
            time::OffsetDateTime::parse(started_at, &time::format_description::well_known::Rfc3339)
                .ok()
                .and_then(|started| {
                    time::OffsetDateTime::parse(
                        &now,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .ok()
                    .map(|cur| cur - started)
                })
                .is_some_and(|age| age > stale_threshold);

        if existing_stale {
            let _ = state
                .db
                .terminate_job_as_failed(&existing.id, &now, "stale_check")
                .await;
        } else {
            return Ok(());
        }
    }

    let check_id = ids::new_check_id();
    let job = JobRecord::new_running(
        check_id.clone(),
        JobType::Check,
        JobScope::All,
        None,
        None,
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = "schedule".to_string();
    job_db.reason = "schedule".to_string();
    state.db.insert_job(job_db).await?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let run_state = state.clone();
    let run_check_id = check_id.clone();
    let run_host_platform = host_platform.clone();
    let run_started_at = now.clone();
    tokio::spawn(async move {
        if let Err(e) = run_state
            .db
            .insert_job_log(
                &run_check_id,
                &JobLogLine {
                    ts: run_started_at.clone(),
                    level: "info".to_string(),
                    msg: "check started".to_string(),
                },
            )
            .await
        {
            tracing::warn!(job_id = %run_check_id, error = %e, "failed to insert check started log");
        }

        let scope = JobScope::All;
        let outcome = api::run_check_for_job(
            &run_state,
            &run_check_id,
            &scope,
            None,
            None,
            &run_host_platform,
            &run_started_at,
        )
        .await;

        let finished_at = match now_rfc3339() {
            Ok(ts) => ts,
            Err(err) => {
                tracing::warn!(
                    job_id = %run_check_id,
                    error = %err,
                    "failed to format finished_at as RFC3339; falling back to started_at"
                );
                run_started_at.clone()
            }
        };
        match outcome {
            Ok(summary) => {
                if let Err(e) = run_state
                    .db
                    .finish_job(&run_check_id, "success", &finished_at, &summary)
                    .await
                {
                    tracing::error!(job_id = %run_check_id, error = %e, "failed to finish check job");
                } else {
                    let discovered_services = extract_discovered_new_versions(&summary);
                    if !discovered_services.is_empty() {
                        let services_checked = summary
                            .get("servicesChecked")
                            .and_then(|v| v.as_u64())
                            .unwrap_or_default()
                            .min(u32::MAX as u64)
                            as u32;
                        let notify_state = run_state.clone();
                        let notify_job_id = run_check_id.clone();
                        let notify_finished_at = finished_at.clone();
                        tokio::spawn(async move {
                            let _ = notify::notify_new_versions_discovered(
                                notify_state.as_ref(),
                                &notify_job_id,
                                &notify_finished_at,
                                services_checked,
                                &discovered_services,
                            )
                            .await;
                        });
                    }
                }
            }
            Err(e) => {
                if let Err(err) = run_state
                    .db
                    .insert_job_log(
                        &run_check_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: format!("check failed: {e:?}"),
                        },
                    )
                    .await
                {
                    tracing::warn!(job_id = %run_check_id, error = %err, "failed to insert check failure log");
                }
                let summary = json!({"error": format!("{e:?}")});
                if let Err(err) = run_state
                    .db
                    .finish_job(&run_check_id, "failed", &finished_at, &summary)
                    .await
                {
                    tracing::error!(job_id = %run_check_id, error = %err, "failed to finish failed check job");
                }
            }
        }
    });

    Ok(())
}

fn spawn_ghcr_webhook_audit_scheduler(state: Arc<AppState>) {
    tokio::spawn(async move {
        let refresh = Duration::from_secs(SETTINGS_REFRESH_INTERVAL_SECONDS);
        loop {
            let settings = match state.db.get_schedule_settings().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "ghcr webhook audit scheduler: settings unavailable");
                    tokio::time::sleep(refresh).await;
                    continue;
                }
            };

            let spec = settings.ghcr_webhook_audit;
            if !spec.enabled {
                tokio::time::sleep(refresh).await;
                continue;
            }

            let next = match next_fire_time_local(&spec.cron) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, cron = %spec.cron, "ghcr webhook audit scheduler: invalid cron");
                    tokio::time::sleep(refresh).await;
                    continue;
                }
            };

            let now_local = Local::now();
            let until = (next - now_local)
                .to_std()
                .unwrap_or_else(|_| Duration::from_secs(0));

            if until <= refresh {
                tokio::time::sleep(until).await;
                match state.db.get_schedule_settings().await {
                    Ok(latest) => {
                        if !latest.ghcr_webhook_audit.enabled {
                            continue;
                        }
                        if crate::cron_expr::canonicalize_for_store(&latest.ghcr_webhook_audit.cron)
                            != crate::cron_expr::canonicalize_for_store(&spec.cron)
                        {
                            continue;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "ghcr webhook audit scheduler: settings unavailable before fire");
                        continue;
                    }
                }

                if let Err(e) = trigger_scheduled_ghcr_audit(&state).await {
                    tracing::warn!(error = %e, "ghcr webhook audit scheduler: tick failed");
                }
                continue;
            }

            tokio::time::sleep(refresh).await;
        }
    });
}

async fn trigger_scheduled_ghcr_audit(state: &Arc<AppState>) -> anyhow::Result<()> {
    if state
        .db
        .has_pending_job_by_type_created_by_reason(
            JobType::GitHubPackagesWebhook,
            "schedule",
            "schedule",
        )
        .await
        .context("check existing pending ghcr schedule job")?
    {
        return Ok(());
    }

    ghcr_webhook_jobs::enqueue_audit_job(state, "schedule", "schedule")
        .await
        .context("enqueue scheduled ghcr audit job")?;
    Ok(())
}
