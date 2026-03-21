use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use serde_json::json;

use crate::{
    api::types::{JobLogLine, JobProgress, JobRecord, JobScope, JobType, RuntimeScanReason},
    ids, ignore, registry,
    runner::CommandSpec,
    service_check,
    state::AppState,
};

#[derive(Clone, Debug)]
pub struct RuntimeScanJobArgs {
    pub job_id: String,
    pub scope: JobScope,
    pub stack_id: Option<String>,
    pub service_id: Option<String>,
    pub host_platform: String,
    pub started_at: String,
    pub reason: String,
}

fn progress_percent(current: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / total).min(100)
}

fn make_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent: progress_percent(current, total),
        planned_current: Some(current),
        planned_total: Some(total),
        planned_percent: Some(progress_percent(current, total)),
        current_target,
        updated_at,
    }
}

fn needs_version_inference_for_tags(current_tag: &str, candidate_tag: Option<&str>) -> bool {
    if !ignore::is_strict_semver(current_tag) {
        return true;
    }
    candidate_tag.is_some_and(|tag| !ignore::is_strict_semver(tag))
}

async fn persist_job_progress(
    state: &Arc<AppState>,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let evt = json!({
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
    });
    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: progress.updated_at.clone(),
                level: "event".to_string(),
                msg: evt.to_string(),
            },
        )
        .await?;

    Ok(())
}

async fn persist_job_progress_best_effort(
    state: &Arc<AppState>,
    job_id: &str,
    progress: &JobProgress,
) {
    if let Err(e) = persist_job_progress(state, job_id, progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist runtime scan progress");
    }
}

pub fn spawn_task(state: Arc<AppState>) {
    let interval = state.config.runtime_scan_interval_seconds;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval));
        loop {
            ticker.tick().await;
            if let Err(e) = enqueue_scheduled_scan(state.clone()).await {
                tracing::warn!(error = %e, "runtime scan tick failed");
            }
        }
    });
}

async fn enqueue_scheduled_scan(state: Arc<AppState>) -> anyhow::Result<()> {
    // Skip if another runtime scan is still running.
    if state
        .db
        .find_latest_running_runtime_scan_job(&JobScope::All, None, None)
        .await?
        .is_some()
        || state
            .db
            .find_latest_running_runtime_scan_job(&JobScope::Stack, None, None)
            .await?
            .is_some()
        || state
            .db
            .find_latest_running_runtime_scan_job(&JobScope::Service, None, None)
            .await?
            .is_some()
    {
        return Ok(());
    }

    let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    let job_id = ids::new_job_id();
    let job = JobRecord::new_running(
        job_id.clone(),
        JobType::RuntimeScan,
        JobScope::All,
        None,
        None,
        &now,
    );

    let mut job_db = job.to_db();
    job_db.created_by = "schedule".to_string();
    job_db.reason = RuntimeScanReason::Schedule.as_str().to_string();
    state.db.insert_job(job_db).await?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    tokio::spawn(run_job(
        state.clone(),
        RuntimeScanJobArgs {
            job_id: job_id.clone(),
            scope: JobScope::All,
            stack_id: None,
            service_id: None,
            host_platform,
            started_at: now,
            reason: RuntimeScanReason::Schedule.as_str().to_string(),
        },
    ));

    Ok(())
}

pub async fn run_job(state: Arc<AppState>, args: RuntimeScanJobArgs) {
    let RuntimeScanJobArgs {
        job_id,
        scope,
        stack_id,
        service_id,
        host_platform,
        started_at,
        reason,
    } = args;

    let started = json!({
        "type": "runtime_scan_started",
        "jobId": job_id,
        "ts": started_at,
        "scope": scope.as_str(),
        "stackId": stack_id,
        "serviceId": service_id,
        "reason": reason,
        "hostPlatform": host_platform,
    });
    let _ = state
        .db
        .insert_job_log(
            &job_id,
            &JobLogLine {
                ts: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                level: "event".to_string(),
                msg: started.to_string(),
            },
        )
        .await;

    let outcome = run_runtime_scan_for_job(
        &state,
        &job_id,
        &scope,
        stack_id.as_deref(),
        service_id.as_deref(),
        &host_platform,
        &started_at,
    )
    .await;

    let finished_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    match outcome {
        Ok(summary) => {
            let finished_evt = json!({
                "type": "runtime_scan_finished",
                "jobId": job_id,
                "ts": finished_at,
                "summary": summary,
            });
            let _ = state
                .db
                .insert_job_log(
                    &job_id,
                    &JobLogLine {
                        ts: time::OffsetDateTime::now_utc()
                            .format(&time::format_description::well_known::Rfc3339)
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                        level: "event".to_string(),
                        msg: finished_evt.to_string(),
                    },
                )
                .await;
            let _ = state
                .db
                .finish_job(&job_id, "success", &finished_at, &summary)
                .await;
        }
        Err(e) => {
            // Always emit a terminal event so SSE clients can close their EventSource even on failures.
            let progress = make_job_progress(
                "done",
                "runtime scan failed".to_string(),
                0,
                0,
                None,
                finished_at.clone(),
            );
            persist_job_progress_best_effort(&state, &job_id, &progress).await;
            let summary = json!({
                "error": e.to_string(),
                "progress": serde_json::to_value(&progress).unwrap_or_else(|_| json!({})),
            });
            let finished_evt = json!({
                "type": "runtime_scan_finished",
                "jobId": job_id,
                "ts": finished_at,
                "summary": summary,
                "status": "failed",
            });
            let _ = state
                .db
                .insert_job_log(
                    &job_id,
                    &JobLogLine {
                        ts: finished_at.clone(),
                        level: "event".to_string(),
                        msg: finished_evt.to_string(),
                    },
                )
                .await;

            let _ = state
                .db
                .insert_job_log(
                    &job_id,
                    &JobLogLine {
                        ts: finished_at.clone(),
                        level: "error".to_string(),
                        msg: format!("runtime scan failed: {e}"),
                    },
                )
                .await;
            let _ = state
                .db
                .finish_job(&job_id, "failed", &finished_at, &summary)
                .await;
        }
    }
}

async fn run_runtime_scan_for_job(
    state: &Arc<AppState>,
    job_id: &str,
    scope: &JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    host_platform: &str,
    now: &str,
) -> anyhow::Result<serde_json::Value> {
    let stack_ids = match scope {
        JobScope::All => state.db.list_stack_ids().await?,
        JobScope::Stack => stack_id.map(|s| vec![s.to_string()]).unwrap_or_default(),
        JobScope::Service => {
            let service_id = service_id.unwrap_or_default().to_string();
            state
                .db
                .get_service_stack_id(&service_id)
                .await?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };
    let total_stacks = stack_ids.len() as u32;
    let mut progress_current = 0u32;
    let mut latest_progress = make_job_progress(
        "prepare",
        format!("preparing runtime scan ({total_stacks} stacks)"),
        progress_current,
        total_stacks,
        None,
        now_rfc3339().unwrap_or_else(|_| now.to_string()),
    );
    persist_job_progress_best_effort(state, job_id, &latest_progress).await;

    let mut stacks_scanned = 0u32;
    let mut services_with_runtime = 0u32;
    let mut services_drifted = 0u32;
    let mut services_updated = 0u32;
    let mut stacks_with_errors: u32 = 0;

    let manifest_digest_cache = service_check::new_manifest_digest_cache();
    let repo_tags_cache = service_check::new_repo_tags_cache();

    for stack_id in &stack_ids {
        latest_progress = make_job_progress(
            "scanning",
            format!("scanning stack {stack_id}"),
            progress_current,
            total_stacks,
            Some(stack_id.clone()),
            now_rfc3339().unwrap_or_else(|_| now.to_string()),
        );
        persist_job_progress_best_effort(state, job_id, &latest_progress).await;

        let compose_project = state.db.get_stack_compose_project(stack_id).await?;
        let Some(project) = compose_project.as_deref() else {
            progress_current = progress_current.saturating_add(1);
            latest_progress = make_job_progress(
                "scanning",
                format!("scanned stacks ({progress_current}/{total_stacks})"),
                progress_current,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| now.to_string()),
            );
            persist_job_progress_best_effort(state, job_id, &latest_progress).await;
            continue;
        };

        let mut services = state.db.list_services_for_runtime_scan(stack_id).await?;
        if *scope == JobScope::Service {
            services.retain(|s| service_id.is_some_and(|id| id == s.id));
        }
        if services.is_empty() {
            progress_current = progress_current.saturating_add(1);
            latest_progress = make_job_progress(
                "scanning",
                format!("scanned stacks ({progress_current}/{total_stacks})"),
                progress_current,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| now.to_string()),
            );
            persist_job_progress_best_effort(state, job_id, &latest_progress).await;
            continue;
        }

        stacks_scanned += 1;

        let runtime_observations = match docker_compose_project_runtime_digests(
            state.as_ref(),
            project,
            &services,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                stacks_with_errors += 1;
                let _ = state
                    .db
                    .insert_job_log(
                        job_id,
                        &JobLogLine {
                            ts: now.to_string(),
                            level: "warn".to_string(),
                            msg: format!("runtime scan: docker query failed for stack {stack_id} project {project}: {e}"),
                        },
                    )
                    .await;
                progress_current = progress_current.saturating_add(1);
                latest_progress = make_job_progress(
                    "scanning",
                    format!("scanned stacks ({progress_current}/{total_stacks})"),
                    progress_current,
                    total_stacks,
                    Some(stack_id.clone()),
                    now_rfc3339().unwrap_or_else(|_| now.to_string()),
                );
                persist_job_progress_best_effort(state, job_id, &latest_progress).await;
                continue;
            }
        };

        for svc in &services {
            let Some(runtime) = runtime_observations.get(&svc.name).cloned() else {
                continue;
            };
            services_with_runtime += 1;

            let digest_changed = !svc
                .current_digest
                .as_deref()
                .is_some_and(|d| d == runtime.digest.as_str());
            let runtime_started_at_changed =
                service_check::normalize_runtime_started_at(
                    svc.current_runtime_started_at.as_deref(),
                ) != service_check::normalize_runtime_started_at(runtime.started_at.as_deref());

            if !digest_changed && !runtime_started_at_changed {
                continue;
            }
            if digest_changed {
                services_drifted += 1;
            }

            let svc_for_check = crate::db::ServiceForCheck {
                id: svc.id.clone(),
                name: svc.name.clone(),
                image_ref: svc.image_ref.clone(),
                image_tag: svc.image_tag.clone(),
                current_digest: svc.current_digest.clone(),
                current_runtime_started_at: svc.current_runtime_started_at.clone(),
                current_resolved_tag: svc.current_resolved_tag.clone(),
                current_resolved_tags_json: svc.current_resolved_tags_json.clone(),
                candidate_digest: svc.candidate_digest.clone(),
                candidate_resolved_tag: svc.candidate_resolved_tag.clone(),
            };

            let before_digest = svc.current_digest.clone();
            let mut outcome = service_check::check_service_and_persist(
                state,
                job_id,
                &svc_for_check,
                Some(runtime.clone()),
                host_platform,
                now,
                &manifest_digest_cache,
                &repo_tags_cache,
            )
            .await?;

            let mut inference_ok = true;
            if outcome.current_digest.is_none() {
                // For runtime drift recovery, we must not leave the DB stale just because
                // registry tag listing is temporarily unavailable.
                //
                // Keep the "registry inference logic" unchanged by only reusing it when
                // it is available; otherwise, we fall back to persisting the runtime digest
                // and clearing resolved/candidate fields to avoid showing stale data.
                inference_ok = false;
                service_check::persist_runtime_fallback_result(
                    &state.db,
                    &svc.id,
                    &svc.image_ref,
                    &svc.image_tag,
                    &runtime,
                    now,
                )
                .await?;

                outcome.current_digest = Some(runtime.digest.clone());
                outcome.current_resolved_tag = None;
                outcome.current_resolved_tags_json = None;
                outcome.current_resolved_tags = None;
                outcome.candidate_tag = None;
                outcome.candidate_resolved_tag = None;
                outcome.candidate_digest = None;
                outcome.candidate_arch_match = None;
                outcome.candidate_arch_json = None;
                outcome.ignore_rule_id = None;
                outcome.ignore_reason = None;
                outcome.candidate_present = false;
            }

            services_updated += 1;
            if outcome.candidate_digest_changed
                && outcome.candidate_digest.is_some()
                && needs_version_inference_for_tags(
                    &svc.image_tag,
                    outcome.candidate_tag.as_deref(),
                )
                && let Some(repo) =
                    crate::snapshot_worker::image_repo_from_image_ref(&svc.image_ref)
                && let Some(candidate_digest) = outcome
                    .candidate_digest
                    .as_deref()
                    .and_then(crate::snapshot_worker::normalize_digest)
            {
                let _ = state
                    .snapshot_worker
                    .enqueue(&repo, &candidate_digest, host_platform, "new_version")
                    .await;
            }
            let changed = before_digest.as_deref() != Some(runtime.digest.as_str());
            if changed
                && let Some(d) = outcome.current_digest.as_deref()
                && let Some(repo) =
                    crate::snapshot_worker::image_repo_from_image_ref(&svc.image_ref)
                && let Some(normalized) = crate::snapshot_worker::normalize_digest(d)
            {
                state
                    .snapshot_worker
                    .enqueue(
                        &repo,
                        &normalized,
                        host_platform,
                        "runtime_scan_digest_changed",
                    )
                    .await;
            }
            let evt = json!({
                "type": "runtime_scan_service",
                "jobId": job_id,
                "ts": now,
                "stackId": stack_id,
                "serviceId": svc.id,
                "serviceName": svc.name,
                "imageRef": svc.image_ref,
                "rawTag": svc.image_tag,
                "runtimeDigest": runtime.digest,
                "runtimeStartedAt": runtime.started_at,
                "dbDigestBefore": before_digest,
                "updated": {
                    "currentDigest": outcome.current_digest,
                    "resolvedTag": outcome.current_resolved_tag,
                    "resolvedTags": outcome.current_resolved_tags,
                },
                "changed": changed,
                "inferenceOk": inference_ok,
            });
            let _ = state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: now.to_string(),
                        level: "event".to_string(),
                        msg: evt.to_string(),
                    },
                )
                .await;
        }

        // Expose recency in the UI: this is still a scan that refreshes current_* for drifted services.
        state.db.update_stack_last_check_at(stack_id, now).await?;

        progress_current = progress_current.saturating_add(1);
        latest_progress = make_job_progress(
            "scanning",
            format!("scanned stacks ({progress_current}/{total_stacks})"),
            progress_current,
            total_stacks,
            Some(stack_id.clone()),
            now_rfc3339().unwrap_or_else(|_| now.to_string()),
        );
        persist_job_progress_best_effort(state, job_id, &latest_progress).await;
    }

    latest_progress = make_job_progress(
        "done",
        "runtime scan finished".to_string(),
        progress_current,
        total_stacks,
        None,
        now_rfc3339().unwrap_or_else(|_| now.to_string()),
    );
    persist_job_progress_best_effort(state, job_id, &latest_progress).await;
    let progress_json = serde_json::to_value(&latest_progress)?;

    Ok(json!({
        "hostPlatform": host_platform,
        "scope": scope.as_str(),
        "stackIds": stack_ids,
        "stacksScanned": stacks_scanned,
        "stacksWithErrors": stacks_with_errors,
        "servicesWithRuntimeDigest": services_with_runtime,
        "servicesDrifted": services_drifted,
        "servicesUpdated": services_updated,
        "progress": progress_json,
    }))
}

pub(crate) async fn docker_compose_project_runtime_digests(
    state: &AppState,
    compose_project: &str,
    services: &[crate::db::ServiceForRuntimeScan],
) -> anyhow::Result<BTreeMap<String, service_check::RuntimeServiceObservation>> {
    // docker ps (all containers in the compose project)
    let ps = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.project={compose_project}"),
                ],
                env: Vec::new(),
            },
            Duration::from_secs(8),
        )
        .await?;

    if ps.status != 0 {
        return Err(anyhow::anyhow!(
            "docker ps failed status={} stderr={}",
            ps.status,
            ps.stderr
        ));
    }

    let container_ids = ps
        .stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if container_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    // docker inspect (service label + image id + container startedAt per container)
    let inspect = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{index .Config.Labels \"com.docker.compose.service\"}}\t{{.Image}}\t{{.State.StartedAt}}"
                            .to_string(),
                    ];
                    args.extend(container_ids);
                    args
                },
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;

    if inspect.status != 0 {
        return Err(anyhow::anyhow!(
            "docker inspect failed status={} stderr={}",
            inspect.status,
            inspect.stderr
        ));
    }

    let mut container_images: Vec<(String, String, Option<String>)> = Vec::new();
    let mut image_ids: BTreeSet<String> = BTreeSet::new();
    for line in inspect.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(svc_name) = parts.next() else {
            continue;
        };
        let Some(image_id) = parts.next() else {
            continue;
        };
        let started_at_raw = parts.next();
        let svc_name = svc_name.trim().to_string();
        let image_id = image_id.trim().to_string();
        if svc_name.is_empty() || image_id.is_empty() {
            continue;
        }
        container_images.push((
            svc_name,
            image_id.clone(),
            service_check::normalize_runtime_started_at(started_at_raw),
        ));
        image_ids.insert(image_id);
    }

    if image_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    // docker image inspect (RepoDigests per image id)
    let img_inspect = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec!["image".to_string(), "inspect".to_string()];
                    args.extend(image_ids.iter().cloned());
                    args.push("--format".to_string());
                    args.push("{{.Id}}\t{{json .RepoDigests}}".to_string());
                    args
                },
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;

    if img_inspect.status != 0 {
        return Err(anyhow::anyhow!(
            "docker image inspect failed status={} stderr={}",
            img_inspect.status,
            img_inspect.stderr
        ));
    }

    let mut repo_digests_by_image_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in img_inspect.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((image_id, json_raw)) = line.split_once('\t') else {
            continue;
        };
        let image_id = image_id.trim().to_string();
        if image_id.is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<Vec<String>>(json_raw.trim()).unwrap_or_default();
        repo_digests_by_image_id.insert(image_id, parsed);
    }

    // Build lookup: service name -> (repo candidates)
    let mut repo_candidates_by_service: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for svc in services {
        if let Ok(img) = registry::ImageRef::parse(&svc.image_ref) {
            repo_candidates_by_service.insert(svc.name.clone(), repo_candidates(&img));
        }
    }

    let mut digests_by_service: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut started_ats_by_service: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (svc_name, image_id, started_at) in container_images {
        let Some(repo_candidates) = repo_candidates_by_service.get(&svc_name) else {
            continue;
        };
        let Some(repo_digests) = repo_digests_by_image_id.get(&image_id) else {
            continue;
        };
        if let Some(started_at) = started_at {
            started_ats_by_service
                .entry(svc_name.clone())
                .or_default()
                .insert(started_at);
        }
        let entry = digests_by_service.entry(svc_name).or_default();
        for d in repo_digests {
            for repo in repo_candidates {
                if let Some(rest) = d.strip_prefix(&format!("{repo}@"))
                    && !rest.trim().is_empty()
                {
                    entry.insert(rest.trim().to_string());
                }
            }
        }
    }

    let mut out: BTreeMap<String, service_check::RuntimeServiceObservation> = BTreeMap::new();
    for (svc_name, digests) in digests_by_service {
        if digests.len() == 1 {
            out.insert(
                svc_name.clone(),
                service_check::RuntimeServiceObservation {
                    digest: digests.iter().next().cloned().unwrap_or_default(),
                    started_at: started_ats_by_service
                        .get(&svc_name)
                        .and_then(|values| values.iter().next().cloned()),
                },
            );
        }
    }
    Ok(out)
}

fn repo_candidates(img: &registry::ImageRef) -> Vec<String> {
    let mut out = Vec::<String>::new();
    out.push(format!("{}/{}", img.registry, img.name));
    if img.registry == "docker.io" {
        out.push(img.name.clone());
        if let Some(short) = img.name.strip_prefix("library/") {
            out.push(short.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_candidates_support_digest_only_image_refs() {
        let image = crate::registry::ImageRef::parse("ghcr.io/acme/web@sha256:deadbeef").unwrap();
        assert_eq!(
            repo_candidates(&image),
            vec!["ghcr.io/acme/web".to_string()]
        );
    }
}
