use super::*;

pub(crate) async fn record_update_tag_history(
    state: &AppState,
    req: &TriggerUpdateRequest,
    now: &str,
) {
    let Ok(targets) = requested_update_targets(req) else {
        return;
    };
    if targets.is_empty() {
        return;
    }
    let targets_by_service = targets
        .into_iter()
        .map(|target| (target.service_id.clone(), target))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut stack_ids = std::collections::BTreeSet::new();
    for service_id in targets_by_service.keys() {
        match state.db.get_service_stack_id(service_id).await {
            Ok(Some(stack_id)) => {
                stack_ids.insert(stack_id);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    service_id = %service_id,
                    "failed to resolve service stack for tag history"
                );
            }
        }
    }
    for stack_id in stack_ids {
        let Ok(Some(stack)) = state.db.get_stack(&stack_id).await else {
            continue;
        };
        for service in stack.services {
            let Some(target) = targets_by_service.get(&service.id) else {
                continue;
            };
            let Some(image_repo) =
                crate::snapshot_worker::image_repo_from_image_ref(&service.image.reference)
            else {
                continue;
            };
            let tag = target.target_tag.trim();
            if tag.is_empty() {
                continue;
            }
            if let Err(err) = state
                .db
                .upsert_service_tag_history(&service.id, &image_repo, tag, "update", now)
                .await
            {
                tracing::warn!(
                    error = %err,
                    service_id = %service.id,
                    "failed to record update tag history"
                );
            }
        }
    }
}
