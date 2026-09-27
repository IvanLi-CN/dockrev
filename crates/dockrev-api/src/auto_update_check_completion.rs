fn configured_tag_observation_candidates(
    summary: &serde_json::Value,
    existing: &[notify::NewVersionDiscoveredService],
    matched_service_ids: Option<&std::collections::HashSet<String>>,
) -> Vec<(String, String, serde_json::Value)> {
    let Some(observations) = summary
        .get("configuredTagObservations")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = existing
        .iter()
        .map(|candidate| {
            (
                candidate.service_id.clone(),
                crate::snapshot_worker::normalize_digest(&candidate.candidate_digest)
                    .unwrap_or_else(|| candidate.candidate_digest.clone()),
            )
        })
        .collect::<std::collections::HashSet<_>>();
    let mut candidates = Vec::new();
    for observation in observations {
        let Some(service_id) = observation.get("serviceId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(digest) = observation
            .get("digest")
            .and_then(|v| v.as_str())
            .and_then(crate::snapshot_worker::normalize_digest)
        else {
            continue;
        };
        if matched_service_ids.is_some_and(|ids| !ids.contains(service_id))
            || !seen.insert((service_id.to_string(), digest.clone()))
        {
            continue;
        }
        candidates.push((service_id.to_string(), digest, observation.clone()));
    }
    candidates
}

async fn reobserved_configured_tag_candidates(
    state: &Arc<AppState>,
    summary: &serde_json::Value,
    existing: &[notify::NewVersionDiscoveredService],
    source: &str,
) -> anyhow::Result<Vec<(notify::NewVersionDiscoveredService, Option<String>)>> {
    let matched_service_ids = if source == "github_webhook" {
        api::summary_matched_service_ids(summary)
    } else {
        None
    };
    let identities = configured_tag_observation_candidates(
        summary,
        existing,
        matched_service_ids.as_ref(),
    );
    let mut services_by_stack = HashMap::new();
    let mut candidates = Vec::new();
    for (service_id, observed_digest, observation) in identities {
        let Some(stack_id) = state.db.get_service_stack_id(&service_id).await? else {
            continue;
        };
        if !services_by_stack.contains_key(&stack_id) {
            services_by_stack.insert(
                stack_id.clone(),
                state.db.list_services_for_check(&stack_id).await?,
            );
        }
        let Some(service) = services_by_stack
            .get(&stack_id)
            .and_then(|services| services.iter().find(|service| service.id == service_id))
        else {
            continue;
        };
        let Some(observed_repo) = observation.get("imageRepo").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(configured_tag) = observation
            .get("configuredTag")
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let Some(image_repo) = crate::snapshot_worker::image_repo_from_image_ref(&service.image_ref)
        else {
            continue;
        };
        let Some(candidate_digest) = service
            .candidate_digest
            .as_deref()
            .and_then(crate::snapshot_worker::normalize_digest)
        else {
            continue;
        };
        let current_digest = service
            .current_digest
            .as_deref()
            .and_then(crate::snapshot_worker::normalize_digest);
        if observed_repo != image_repo
            || configured_tag != service.image_tag
            || service.candidate_tag.as_deref() != Some(configured_tag)
            || candidate_digest != observed_digest
            || current_digest.as_deref() == Some(candidate_digest.as_str())
        {
            continue;
        }
        let candidate_display_tag = observation
            .get("version")
            .and_then(|v| v.as_str())
            .filter(|version| crate::ignore::is_strict_semver(version))
            .or_else(|| {
                service
                    .candidate_resolved_tag
                    .as_deref()
                    .filter(|version| crate::ignore::is_strict_semver(version))
            })
            .unwrap_or(configured_tag);
        let digest_bound_version = observation
            .get("version")
            .and_then(|v| v.as_str())
            .filter(|version| crate::ignore::is_strict_semver(version))
            .map(str::to_string);
        candidates.push((
            notify::NewVersionDiscoveredService {
                stack_id,
                service_id: service.id.clone(),
                image_ref: service.image_ref.clone(),
                current_tag: service.image_tag.clone(),
                current_digest,
                current_display_tag: service
                    .current_resolved_tag
                    .clone()
                    .unwrap_or_else(|| service.image_tag.clone()),
                candidate_tag: configured_tag.to_string(),
                candidate_display_tag: candidate_display_tag.to_string(),
                candidate_digest,
            },
            digest_bound_version,
        ));
    }
    Ok(candidates)
}

pub async fn handle_completed_check(
    state: &Arc<AppState>,
    job_id: &str,
    reason: &str,
    finished_at: &str,
    summary: &serde_json::Value,
) -> anyhow::Result<()> {
    let created_by = state.db.get_job(job_id).await?.map(|job| job.created_by);
    let Some(source) = auto_policy_source(reason, summary, created_by.as_deref()) else {
        return Ok(());
    };
    let mut discovered_services = notify::extract_new_versions_discovered(summary);
    if api::summary_emits_new_version_notification(summary)
        && let Some(matched_service_ids) = api::summary_matched_service_ids(summary)
    {
        discovered_services.retain(|service| matched_service_ids.contains(&service.service_id));
    }
    // A prior check can materialize the configured-tag candidate before this qualified check.
    let reobserved =
        reobserved_configured_tag_candidates(state, summary, &discovered_services, source).await?;
    if discovered_services.is_empty() && reobserved.is_empty() {
        return Ok(());
    }
    let mut candidates = discovered_services
        .into_iter()
        .map(|candidate| (candidate, None))
        .collect::<Vec<_>>();
    candidates.extend(reobserved);

    let discovered_at = state
        .db
        .get_job(job_id)
        .await?
        .map(|job| job.created_at)
        .unwrap_or_else(|| finished_at.to_string());

    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    for (candidate, digest_bound_version) in &candidates {
        state
            .db
            .reopen_auto_update_candidate_inference(
                &candidate.service_id,
                &candidate.candidate_digest,
                "qualified_check_reopened_inference",
                finished_at,
            )
            .await?;
        evaluate_candidate(
            state,
            job_id,
            &discovered_at,
            finished_at,
            candidate,
            Some(source),
            digest_bound_version.as_deref(),
        )
        .await?;
        if let Some(image_repo) =
            crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)
        {
            reconcile_inference_for_digest(
                state,
                &image_repo,
                &candidate.candidate_digest,
                &host_platform,
                finished_at,
            )
            .await?;
        }
    }
    process_due_pending(state, finished_at, 50).await?;
    Ok(())
}
