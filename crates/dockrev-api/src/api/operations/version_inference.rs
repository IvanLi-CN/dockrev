use super::*;

pub(super) fn preferred_display_tag(raw_tag: &str, resolved_tag: Option<&str>) -> String {
    resolved_tag
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .unwrap_or_else(|| raw_tag.trim())
        .to_string()
}

#[derive(Default)]
pub(super) struct NotificationSnapshotDisplay {
    pub(super) display_tag: Option<String>,
    pub(super) ready: bool,
}

pub(super) async fn notification_snapshot_display_for_digest(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    raw_tag: &str,
) -> Result<NotificationSnapshotDisplay, ApiError> {
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, checked_at, _updated_at)) = snapshot else {
        return Ok(NotificationSnapshotDisplay::default());
    };
    let Some(snapshot_entry) =
        super::super::stacks::parse_digest_snapshot_row(&snapshot_json, &checked_at)
    else {
        return Ok(NotificationSnapshotDisplay::default());
    };
    let ready = crate::notify::notification_snapshot_is_ready(
        &snapshot_entry.snapshot,
        snapshot_entry.snapshot.checked_at.as_str(),
    );
    let display_tag = ready
        .then(|| {
            super::super::stacks::infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, raw_tag)
                .into_iter()
                .next()
        })
        .flatten();
    Ok(NotificationSnapshotDisplay { display_tag, ready })
}

async fn notification_snapshot_ready_for_digest(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> Result<Option<bool>, ApiError> {
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, checked_at, _updated_at)) = snapshot else {
        return Ok(None);
    };
    Ok(Some(
        crate::notify::notification_snapshot_is_ready_from_row(&snapshot_json, &checked_at)
            .unwrap_or(false),
    ))
}

pub(super) async fn should_enqueue_new_version_inference(
    state: &AppState,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
) -> Result<bool, ApiError> {
    Ok(
        notification_snapshot_ready_for_digest(state, image_repo, digest, host_platform).await?
            != Some(true),
    )
}
