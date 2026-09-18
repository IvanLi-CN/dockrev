fn candidate_event_targets(
    candidates: &[AutoUpdateCandidateRow],
    host_platform: &str,
) -> (Vec<String>, HashMap<String, (String, String)>) {
    let mut targets = HashMap::new();
    let keys = candidates
        .iter()
        .filter_map(|candidate| {
            let image_repo =
                crate::snapshot_worker::image_repo_from_image_ref(&candidate.image_ref)?;
            let key = crate::snapshot_worker::snapshot_task_key(
                &image_repo,
                &candidate.candidate_digest,
                host_platform,
            )?;
            targets.insert(
                key.clone(),
                (image_repo, candidate.candidate_digest.clone()),
            );
            Some(key)
        })
        .collect::<Vec<_>>();
    (keys, targets)
}

fn candidate_match_values<'a>(
    candidate: &'a notify::NewVersionDiscoveredService,
    resolved_tags: Option<&'a [String]>,
) -> Vec<&'a str> {
    let mut values = resolved_tags
        .into_iter()
        .flatten()
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    let display_tag = candidate.candidate_display_tag.trim();
    let raw_tag = candidate.candidate_tag.trim();
    if !display_tag.is_empty() && display_tag != raw_tag {
        values.push(display_tag);
    }
    if !raw_tag.is_empty() {
        values.push(raw_tag);
    }
    values
}

async fn reconcile_auto_update_policy_candidates(
    state: &Arc<AppState>,
    now: &str,
) -> anyhow::Result<()> {
    for candidate in state
        .db
        .list_auto_update_candidates_for_policy_reconciliation(50)
        .await?
    {
        evaluate_candidate(
            state,
            &candidate.source_job_id,
            &candidate.discovered_at,
            now,
            &candidate_from_row(&candidate),
            Some(&candidate.source),
        )
        .await?;
    }
    Ok(())
}

fn auto_policy_source(
    reason: &str,
    summary: &serde_json::Value,
    created_by: Option<&str>,
) -> Option<&'static str> {
    if reason.eq_ignore_ascii_case("schedule")
        && created_by.is_some_and(|value| value.eq_ignore_ascii_case("schedule"))
    {
        return Some("schedule");
    }
    if summary
        .get("source")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|source| source.eq_ignore_ascii_case("github_webhook"))
        && created_by.is_some_and(|value| {
            value.eq_ignore_ascii_case("webhook") || value.eq_ignore_ascii_case("github")
        })
        && api::summary_emits_new_version_notification(summary)
    {
        return Some("github_webhook");
    }
    None
}

fn is_qualified_auto_policy_source(source: Option<&str>) -> bool {
    matches!(source, Some("schedule" | "github_webhook"))
}

async fn has_valid_auto_policy_source(
    db: &crate::db::Db,
    source_job_id: &str,
    source: &str,
    service_id: Option<&str>,
    stack_id: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(job) = db.get_job(source_job_id).await? else {
        return Ok(false);
    };
    if !job.status.eq_ignore_ascii_case("success") {
        return Ok(false);
    }
    if stack_id.is_some_and(|stack_id| job.stack_id.as_deref() != Some(stack_id))
        || service_id.is_some_and(|service_id| job.service_id.as_deref() != Some(service_id))
    {
        return Ok(false);
    }
    Ok(auto_policy_source(&job.reason, &job.summary_json, Some(&job.created_by)) == Some(source))
}

pub fn spawn_tasks(state: Arc<AppState>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(PENDING_POLL_INTERVAL_SECONDS);
        loop {
            let now = match time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
            {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error = %error, "auto update policy scheduler: clock unavailable");
                    continue;
                }
            };
            if let Err(error) = process_due_pending(&state, &now, 50).await {
                tracing::warn!(error = %error, "auto update policy scheduler failed");
            }
            if let Err(error) = reconcile_pending_inference(&state, &now).await {
                tracing::warn!(error = %error, "auto update candidate reconciliation failed");
            }

            let after_id = state.snapshot_worker.latest_event_id().await;
            let host_platform =
                crate::registry::host_platform_override(state.config.host_platform.as_deref())
                    .unwrap_or_else(|| "linux/amd64".to_string());
            let (candidate_keys, candidate_targets) = match state
                .db
                .list_auto_update_candidates_for_events(50)
                .await
            {
                Ok(candidates) => candidate_event_targets(&candidates, &host_platform),
                Err(error) => {
                    tracing::debug!(error = %error, "auto update candidate event wait list failed");
                    (Vec::new(), HashMap::new())
                }
            };
            if candidate_keys.is_empty() {
                tokio::time::sleep(interval).await;
            } else {
                let outcomes = state
                    .snapshot_worker
                    .wait_for_task_finished_keys_since(after_id, &candidate_keys, interval)
                    .await;
                if outcomes.keys().next().is_some() {
                    for key in outcomes.keys() {
                        let Some((image_repo, digest)) = candidate_targets.get(key) else {
                            continue;
                        };
                        if let Err(error) = reconcile_inference_for_digest(
                            &state,
                            image_repo,
                            digest,
                            &host_platform,
                            &now,
                        )
                        .await
                        {
                            tracing::warn!(
                                image_repo,
                                digest,
                                error = %error,
                                "auto update candidate settlement after snapshot event failed"
                            );
                        }
                    }
                }
            }
        }
    });
}
