use super::*;

pub(crate) async fn persist_job_progress(
    state: &Arc<AppState>,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let mut evt = json!({
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
    if let Some(download) = progress.download.as_ref()
        && let Some(obj) = evt.as_object_mut()
    {
        obj.insert("download".to_string(), serde_json::to_value(download)?);
    }
    if let Some(backup) = progress.backup.as_ref()
        && let Some(obj) = evt.as_object_mut()
    {
        obj.insert("backup".to_string(), serde_json::to_value(backup)?);
    }

    state
        .job_live_log_hub
        .publish_progress(job_id, progress.clone());
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
