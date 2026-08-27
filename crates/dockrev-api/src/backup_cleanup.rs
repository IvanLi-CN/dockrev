use std::path::{Path, PathBuf};

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

pub(crate) fn has_cleanup_delete_intent(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value == crate::db::BACKUP_CLEANUP_DELETE_INTENT_LEGACY
            || value.starts_with(crate::db::BACKUP_CLEANUP_DELETE_INTENT_PREFIX)
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

fn cleanup_marker_token(marker: &str) -> &str {
    marker
        .rsplit(':')
        .next()
        .filter(|token| !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("unknown")
}
