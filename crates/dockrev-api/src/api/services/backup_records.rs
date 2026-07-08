use super::*;

pub(super) async fn get_service_backup_records(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<ServiceBackupRecordsResponse, ApiError> {
    let stack_id = state
        .db
        .get_service_stack_id(service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let stack = state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    if !stack.services.iter().any(|svc| svc.id == service_id) {
        return Err(ApiError::not_found("service not found"));
    }

    let rows = state
        .db
        .list_service_backup_records(&stack_id, service_id)
        .await
        .map_err(map_internal)?;
    Ok(ServiceBackupRecordsResponse {
        records: rows
            .into_iter()
            .filter(|row| !is_pure_noise_backup_record(row, &stack_id))
            .map(|row| map_backup_record_row(row, &stack_id))
            .collect(),
    })
}

fn is_pure_noise_backup_record(row: &crate::db::ServiceBackupRecordRow, stack_id: &str) -> bool {
    if row.status != "skipped" {
        return false;
    }
    current_stack_backup_summary(&row.job_summary_json, stack_id)
        .and_then(|backup| backup.get("reason"))
        .and_then(serde_json::Value::as_str)
        == Some("no_included_targets")
}

fn map_backup_record_row(
    row: crate::db::ServiceBackupRecordRow,
    stack_id: &str,
) -> ServiceBackupRecordItem {
    ServiceBackupRecordItem {
        backup_id: row.backup_id,
        job_id: row.job_id,
        scope: row.scope,
        status: row.status,
        created_at: row.created_at,
        finished_at: row.finished_at,
        artifact_path: row.artifact_path,
        size_bytes: row.size_bytes,
        cleanup_after: row.cleanup_after,
        deleted_at: row.deleted_at,
        error: row.error,
        assets: extract_backup_assets(&row.job_summary_json, stack_id),
    }
}

fn extract_backup_assets(
    summary: &serde_json::Value,
    stack_id: &str,
) -> Vec<ServiceBackupRecordAsset> {
    current_stack_backup_summary(summary, stack_id)
        .and_then(|backup| {
            backup
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .map(|targets| {
                    targets
                        .iter()
                        .filter_map(map_backup_asset_value)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default()
}

fn current_stack_backup_summary<'a>(
    summary: &'a serde_json::Value,
    stack_id: &str,
) -> Option<&'a serde_json::Value> {
    summary
        .get("stacks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|stack| {
            stack
                .get("stackId")
                .and_then(serde_json::Value::as_str)
                .map(|value| value == stack_id)
                .unwrap_or(false)
        })
        .and_then(|stack| stack.get("backup"))
}

fn map_backup_asset_value(value: &serde_json::Value) -> Option<ServiceBackupRecordAsset> {
    let object = value.as_object()?;
    let target_value = object.get("target")?.clone();
    let target = serde_json::from_value::<BackupTarget>(target_value).ok()?;
    let status = match object.get("status").and_then(serde_json::Value::as_str) {
        Some("included") => ServiceBackupRecordAssetStatus::Included,
        Some("skipped") => ServiceBackupRecordAssetStatus::Skipped,
        _ => return None,
    };
    let policy = object
        .get("policy")
        .and_then(serde_json::Value::as_str)
        .map(BackupTargetPolicy::from_str);
    let size_bytes = object.get("sizeBytes").and_then(serde_json::Value::as_u64);
    let reason = object
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    Some(ServiceBackupRecordAsset {
        target,
        status,
        policy,
        size_bytes,
        reason,
    })
}
