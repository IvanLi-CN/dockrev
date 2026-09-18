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
    if discovered_services.is_empty() {
        return Ok(());
    }

    let discovered_at = state
        .db
        .get_job(job_id)
        .await?
        .map(|job| job.created_at)
        .unwrap_or_else(|| finished_at.to_string());

    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    for candidate in &discovered_services {
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
