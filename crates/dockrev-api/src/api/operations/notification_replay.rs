use super::*;

pub(crate) async fn replay_pending_check_notifications(state: &Arc<AppState>) {
    let mut after = None::<(String, String)>;
    loop {
        let pending = match state
            .db
            .list_pending_check_notification_dispatches_after(
                after
                    .as_ref()
                    .map(|(finished_at, job_id)| (finished_at.as_str(), job_id.as_str())),
            )
            .await
        {
            Ok(items) => items,
            Err(error) => {
                tracing::warn!(error = %error, "failed to load pending check notification dispatches");
                return;
            }
        };
        if pending.is_empty() {
            return;
        }
        let batch_len = pending.len();
        let last_cursor = pending
            .last()
            .map(|dispatch| (dispatch.finished_at.clone(), dispatch.job_id.clone()));
        let mut processed_any = false;
        for dispatch in pending {
            if !dispatch.event_enabled {
                if let Err(error) = state
                    .db
                    .mark_check_notification_dispatch_processed(
                        &dispatch.job_id,
                        &dispatch.finished_at,
                    )
                    .await
                {
                    tracing::warn!(
                        job_id = %dispatch.job_id,
                        error = %error,
                        "failed to mark disabled check notification dispatch processed"
                    );
                } else {
                    processed_any = true;
                }
                continue;
            }
            match maybe_notify_check_new_versions(
                state,
                &dispatch.job_id,
                &dispatch.reason,
                &dispatch.finished_at,
                &dispatch.summary,
                dispatch.event_enabled,
            )
            .await
            {
                Ok(()) => {
                    if let Err(error) = state
                        .db
                        .mark_check_notification_dispatch_processed(
                            &dispatch.job_id,
                            &dispatch.finished_at,
                        )
                        .await
                    {
                        tracing::warn!(
                            job_id = %dispatch.job_id,
                            error = %error,
                            "failed to mark check notification dispatch processed"
                        );
                    } else {
                        processed_any = true;
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        job_id = %dispatch.job_id,
                        error = %error,
                        "failed to replay check notification dispatch"
                    );
                }
            }
        }
        if batch_len < 256 {
            return;
        }
        if !processed_any {
            tracing::debug!("notification dispatch replay advanced past a failed batch");
        }
        let Some(last_cursor) = last_cursor else {
            return;
        };
        after = Some(last_cursor);
    }
}
