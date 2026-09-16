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
