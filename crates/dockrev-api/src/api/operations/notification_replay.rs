use super::*;

pub(crate) async fn replay_pending_check_notifications(state: &Arc<AppState>) {
    loop {
        let pending = match state.db.list_pending_check_notification_dispatches().await {
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
        let mut processed_any = false;
        for dispatch in pending {
            match maybe_notify_check_new_versions(
                state,
                &dispatch.job_id,
                &dispatch.reason,
                &dispatch.finished_at,
                &dispatch.summary,
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
        if batch_len < 256 || !processed_any {
            return;
        }
    }
}
