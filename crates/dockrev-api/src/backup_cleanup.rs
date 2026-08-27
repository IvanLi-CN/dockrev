use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::runner::{CommandRunner, CommandSpec};

pub(crate) async fn stop_interrupted_helper(
    runner: &dyn CommandRunner,
    job_id: &str,
) -> anyhow::Result<()> {
    let out = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=cc.ivanli.dockrev.job-id={job_id}"),
                    "--filter".to_string(),
                    "label=cc.ivanli.dockrev.stop-mode=stop".to_string(),
                ],
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "list interrupted backup helper failed: {}",
            out.stderr
        ));
    }
    let ids = out.stdout.split_whitespace().collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let mut args = vec!["stop".to_string(), "--time".to_string(), "2".to_string()];
    args.extend(ids.into_iter().map(str::to_string));
    let stopped = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args,
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;
    if stopped.status != 0 {
        return Err(anyhow::anyhow!(
            "stop interrupted backup helper failed: {}",
            stopped.stderr
        ));
    }
    Ok(())
}

pub(crate) async fn run_to_string(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
) -> anyhow::Result<String> {
    let out = runner.run(spec, timeout).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "command failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(out.stdout)
}

pub(crate) fn timestamp_slug(now_rfc3339: &str) -> String {
    let cleaned = now_rfc3339.replace(['-', ':'], "");
    if let Some((date, rest)) = cleaned.split_once('T') {
        let time = rest.trim_end_matches('Z');
        let time = if time.len() >= 6 { &time[..6] } else { time };
        return format!("{}-{}Z", &date[..8.min(date.len())], time);
    }
    "backup".to_string()
}

pub(crate) async fn record_cleanup_error(
    db: &crate::db::Db,
    backup_id: &str,
    attempted_at: &str,
    error: &str,
) -> anyhow::Result<()> {
    db.mark_backup_cleanup_failed_retriable(backup_id, attempted_at, error)
        .await
}

pub(crate) async fn mark_cleanup_delete_completed(
    db: &crate::db::Db,
    backup_id: &str,
    intent: &str,
    completed: &str,
) -> anyhow::Result<bool> {
    db.mark_backup_cleanup_delete_completed(backup_id, intent, completed)
        .await
}

pub(crate) async fn record_cleanup_intent_error(
    db: &crate::db::Db,
    backup_id: &str,
    attempted_at: &str,
    intent: &str,
    error: &str,
) -> anyhow::Result<()> {
    db.mark_backup_cleanup_failed_for_intent(backup_id, attempted_at, intent, error)
        .await
}

pub(crate) async fn record_cleanup_completed_error(
    db: &crate::db::Db,
    backup_id: &str,
    attempted_at: &str,
    completed: &str,
    error: &str,
) -> anyhow::Result<()> {
    db.mark_backup_cleanup_failed_for_completed(backup_id, attempted_at, completed, error)
        .await
}

pub(crate) async fn record_cleanup_state_error(
    db: &crate::db::Db,
    backup_id: &str,
    attempted_at: &str,
    recovery_marker: Option<&str>,
    error: &str,
) -> anyhow::Result<()> {
    if has_cleanup_delete_completed(recovery_marker) {
        record_cleanup_completed_error(
            db,
            backup_id,
            attempted_at,
            &cleanup_delete_completed_marker(recovery_marker.unwrap()),
            error,
        )
        .await
    } else if has_cleanup_delete_intent(recovery_marker) {
        record_cleanup_intent_error(
            db,
            backup_id,
            attempted_at,
            recovery_marker.unwrap_or_default(),
            error,
        )
        .await
    } else {
        record_cleanup_error(db, backup_id, attempted_at, error).await
    }
}

pub(crate) fn has_cleanup_delete_intent(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value == crate::db::BACKUP_CLEANUP_DELETE_INTENT_LEGACY
            || value.starts_with(crate::db::BACKUP_CLEANUP_DELETE_INTENT_PREFIX)
    })
}

pub(crate) fn is_legacy_cleanup_delete_intent(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value == crate::db::BACKUP_CLEANUP_DELETE_INTENT_LEGACY
            || value.starts_with(&format!(
                "{}\n",
                crate::db::BACKUP_CLEANUP_DELETE_INTENT_LEGACY
            ))
    })
}

pub(crate) fn has_cleanup_delete_completed(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.starts_with(crate::db::BACKUP_CLEANUP_DELETE_COMPLETED_PREFIX))
}

pub(crate) fn cleanup_tombstone_key(artifact_key: &Path, marker: &str) -> PathBuf {
    let token = cleanup_marker_token(marker);
    let artifact_name = artifact_key
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    artifact_key
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!(".dockrev-delete-{token}-{artifact_name}"))
}

pub(crate) fn cleanup_delete_completed_marker(marker: &str) -> String {
    format!(
        "{}{}",
        crate::db::BACKUP_CLEANUP_DELETE_COMPLETED_PREFIX,
        cleanup_marker_token(marker)
    )
}

pub(crate) fn cleanup_delete_intent_marker(marker: &str) -> String {
    format!(
        "{}{}",
        crate::db::BACKUP_CLEANUP_DELETE_INTENT_PREFIX,
        cleanup_marker_token(marker)
    )
}

fn cleanup_marker_token(marker: &str) -> &str {
    marker
        .split_once('\n')
        .map_or(marker, |(marker, _)| marker)
        .rsplit(':')
        .next()
        .filter(|token| !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("unknown")
}
