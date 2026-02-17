use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use serde_json::json;

use crate::{
    api::types::{JobLogLine, JobRecord, JobScope, JobType, RuntimeScanReason},
    ids, registry,
    runner::CommandSpec,
    service_check,
    state::AppState,
};

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
        job_id.clone(),
        JobScope::All,
        None,
        None,
        host_platform,
        now,
        RuntimeScanReason::Schedule.as_str().to_string(),
    ));

    Ok(())
}

pub async fn run_job(
    state: Arc<AppState>,
    job_id: String,
    scope: JobScope,
    stack_id: Option<String>,
    service_id: Option<String>,
    host_platform: String,
    started_at: String,
    reason: String,
) {
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
            let summary = json!({ "error": e.to_string() });
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

    let mut stacks_scanned = 0u32;
    let mut services_with_runtime = 0u32;
    let mut services_drifted = 0u32;
    let mut services_updated = 0u32;
    let mut stacks_with_errors: u32 = 0;

    let mut manifest_digest_cache: HashMap<String, (Option<String>, Option<String>)> =
        HashMap::new();

    for stack_id in &stack_ids {
        let compose_project = state.db.get_stack_compose_project(stack_id).await?;
        let Some(project) = compose_project.as_deref() else {
            continue;
        };

        let mut services = state.db.list_services_for_runtime_scan(stack_id).await?;
        if *scope == JobScope::Service {
            services.retain(|s| service_id.is_some_and(|id| id == s.id));
        }
        if services.is_empty() {
            continue;
        }

        stacks_scanned += 1;

        let runtime_digests = match docker_compose_project_runtime_digests(
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
                continue;
            }
        };

        for svc in &services {
            let Some(runtime_digest) = runtime_digests.get(&svc.name).cloned() else {
                continue;
            };
            services_with_runtime += 1;

            if svc
                .current_digest
                .as_deref()
                .is_some_and(|d| d == runtime_digest.as_str())
            {
                continue;
            }
            services_drifted += 1;

            let svc_for_check = crate::db::ServiceForCheck {
                id: svc.id.clone(),
                name: svc.name.clone(),
                image_ref: svc.image_ref.clone(),
                image_tag: svc.image_tag.clone(),
            };

            let before_digest = svc.current_digest.clone();
            let mut outcome = service_check::check_service_and_persist(
                state,
                job_id,
                &svc_for_check,
                Some(runtime_digest.clone()),
                host_platform,
                now,
                &mut manifest_digest_cache,
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
                state
                    .db
                    .update_service_check_result(
                        &svc.id,
                        Some(runtime_digest.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        now,
                        now,
                    )
                    .await?;

                outcome.current_digest = Some(runtime_digest.clone());
                outcome.current_resolved_tag = None;
                outcome.current_resolved_tags_json = None;
                outcome.current_resolved_tags = None;
                outcome.candidate_tag = None;
                outcome.candidate_digest = None;
                outcome.candidate_arch_match = None;
                outcome.candidate_arch_json = None;
                outcome.ignore_rule_id = None;
                outcome.ignore_reason = None;
                outcome.candidate_present = false;
            }

            services_updated += 1;
            let changed = before_digest.as_deref() != Some(runtime_digest.as_str());
            let evt = json!({
                "type": "runtime_scan_service",
                "jobId": job_id,
                "ts": now,
                "stackId": stack_id,
                "serviceId": svc.id,
                "serviceName": svc.name,
                "imageRef": svc.image_ref,
                "rawTag": svc.image_tag,
                "runtimeDigest": runtime_digest,
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
    }

    Ok(json!({
        "hostPlatform": host_platform,
        "scope": scope.as_str(),
        "stackIds": stack_ids,
        "stacksScanned": stacks_scanned,
        "stacksWithErrors": stacks_with_errors,
        "servicesWithRuntimeDigest": services_with_runtime,
        "servicesDrifted": services_drifted,
        "servicesUpdated": services_updated,
    }))
}

async fn docker_compose_project_runtime_digests(
    state: &AppState,
    compose_project: &str,
    services: &[crate::db::ServiceForRuntimeScan],
) -> anyhow::Result<BTreeMap<String, String>> {
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

    // docker inspect (service label + image id per container)
    let inspect = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{index .Config.Labels \"com.docker.compose.service\"}}\t{{.Image}}"
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

    let mut container_images: Vec<(String, String)> = Vec::new();
    let mut image_ids: BTreeSet<String> = BTreeSet::new();
    for line in inspect.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((svc_name, image_id)) = line.split_once('\t') else {
            continue;
        };
        let svc_name = svc_name.trim().to_string();
        let image_id = image_id.trim().to_string();
        if svc_name.is_empty() || image_id.is_empty() {
            continue;
        }
        container_images.push((svc_name, image_id.clone()));
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
    for (svc_name, image_id) in container_images {
        let Some(repo_candidates) = repo_candidates_by_service.get(&svc_name) else {
            continue;
        };
        let Some(repo_digests) = repo_digests_by_image_id.get(&image_id) else {
            continue;
        };
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

    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (svc_name, digests) in digests_by_service {
        if digests.len() == 1 {
            out.insert(svc_name, digests.iter().next().cloned().unwrap_or_default());
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
