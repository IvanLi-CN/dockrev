use super::*;
use std::{
    collections::HashSet,
    path::{Path as FsPath, PathBuf},
    sync::{Mutex, OnceLock},
};

static STACK_RECORD_DELETION_GUARDS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

struct StackRecordDeletionGuard(String);

impl Drop for StackRecordDeletionGuard {
    fn drop(&mut self) {
        if let Some(guards) = STACK_RECORD_DELETION_GUARDS.get()
            && let Ok(mut guards) = guards.lock()
        {
            guards.remove(&self.0);
        }
    }
}

fn acquire_stack_record_deletion_guard(
    stack_id: &str,
) -> Result<StackRecordDeletionGuard, ApiError> {
    let guards = STACK_RECORD_DELETION_GUARDS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guards = guards
        .lock()
        .map_err(|_| ApiError::internal("stack deletion guard unavailable"))?;
    if !guards.insert(stack_id.to_string()) {
        return Err(ApiError::conflict(
            "stack record deletion already in progress",
        ));
    }
    Ok(StackRecordDeletionGuard(stack_id.to_string()))
}

#[derive(Clone, Debug)]
struct StagedStackRecordArtifact {
    artifact_key: PathBuf,
    tombstone_key: PathBuf,
    artifact_path: String,
    present: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StackRecordDeletionAuditArtifact {
    artifact_path: String,
    tombstone_key: String,
    present: bool,
}

fn validate_record_deletion_tombstone_key(
    artifact_key: &FsPath,
    tombstone_key: &str,
) -> anyhow::Result<PathBuf> {
    let tombstone_key = FsPath::new(tombstone_key);
    if !tombstone_key
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(anyhow::anyhow!(
            "record deletion tombstone path is not relative and safe"
        ));
    }
    if tombstone_key.parent() != artifact_key.parent() {
        return Err(anyhow::anyhow!(
            "record deletion tombstone path has a different parent"
        ));
    }
    let artifact_name = artifact_key
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("backup artifact has no valid file name"))?;
    let tombstone_name = tombstone_key
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("record deletion tombstone has no valid file name"))?;
    if !tombstone_name.starts_with(".dockrev-record-delete-")
        || !tombstone_name.ends_with(&format!("-{artifact_name}"))
    {
        return Err(anyhow::anyhow!(
            "record deletion tombstone path has an unexpected name"
        ));
    }
    Ok(tombstone_key.to_path_buf())
}

async fn compose_files_are_missing(paths: &[String]) -> anyhow::Result<bool> {
    if paths.is_empty() {
        return Ok(false);
    }
    for path in paths {
        match tokio::fs::metadata(path).await {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

async fn resolve_stack_record_artifacts(
    state: &Arc<AppState>,
    snapshot: &crate::db::StackRecordDeletionSnapshot,
) -> anyhow::Result<(
    crate::backup_storage::BackupStorage,
    Vec<StagedStackRecordArtifact>,
)> {
    let storage =
        crate::backup_storage::resolve_backup_storage(&*state.runner, &state.config.db_path)
            .await?;
    let token = ulid::Ulid::new().to_string().to_lowercase();
    let mut staged = Vec::new();
    let mut seen = HashSet::new();
    for artifact in &snapshot.backup_artifacts {
        let artifact_key = crate::backup::legacy_artifact_key(&storage, &artifact.artifact_path)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "backup artifact path is outside managed storage: {}",
                    artifact.artifact_path
                )
            })?;
        if !seen.insert(artifact_key.clone()) {
            continue;
        }
        let file_name = artifact_key
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("backup artifact has no file name"))?
            .to_string_lossy();
        let tombstone_key = artifact_key
            .parent()
            .unwrap_or_else(|| FsPath::new(""))
            .join(format!(".dockrev-record-delete-{token}-{file_name}"));
        let present = crate::backup::artifact_exists(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &artifact_key,
        )
        .await?;
        staged.push(StagedStackRecordArtifact {
            artifact_key,
            tombstone_key,
            artifact_path: artifact.artifact_path.clone(),
            present,
        });
    }
    Ok((storage, staged))
}

async fn restore_staged_stack_record_artifacts(
    state: &Arc<AppState>,
    storage: &crate::backup_storage::BackupStorage,
    staged: &[StagedStackRecordArtifact],
) -> anyhow::Result<()> {
    for item in staged.iter().rev().filter(|item| item.present) {
        if crate::backup::artifact_exists(
            &*state.runner,
            storage,
            &state.config.dockrev_image_repo,
            &item.tombstone_key,
        )
        .await?
        {
            crate::backup::move_artifact_to_tombstone(
                &*state.runner,
                storage,
                &state.config.dockrev_image_repo,
                &item.tombstone_key,
                &item.artifact_key,
            )
            .await?;
        }
    }
    Ok(())
}

async fn delete_staged_stack_record_artifacts(
    state: &Arc<AppState>,
    storage: &crate::backup_storage::BackupStorage,
    staged: &[StagedStackRecordArtifact],
) -> anyhow::Result<()> {
    for item in staged.iter().filter(|item| item.present) {
        crate::backup::delete_artifact_if_present(
            &*state.runner,
            storage,
            &state.config.dockrev_image_repo,
            &item.tombstone_key,
        )
        .await?;
    }
    Ok(())
}

async fn retry_pending_stack_record_artifact_cleanup(
    state: &Arc<AppState>,
    stack_id: &str,
    audit_json: &str,
) -> anyhow::Result<()> {
    let artifacts = serde_json::from_str::<Vec<StackRecordDeletionAuditArtifact>>(audit_json)
        .context("decode pending stack record deletion audit")?;
    let present_artifacts = artifacts.iter().filter(|artifact| artifact.present);
    let Some(_) = present_artifacts.clone().next() else {
        state
            .db
            .complete_stack_record_deletion_cleanup(stack_id)
            .await?;
        return Ok(());
    };

    let storage =
        crate::backup_storage::resolve_backup_storage(&*state.runner, &state.config.db_path)
            .await?;
    for artifact in present_artifacts {
        let artifact_key = crate::backup::legacy_artifact_key(&storage, &artifact.artifact_path)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "backup artifact path is outside managed storage: {}",
                    artifact.artifact_path
                )
            })?;
        let tombstone_key =
            validate_record_deletion_tombstone_key(&artifact_key, &artifact.tombstone_key)?;
        if crate::backup::artifact_exists(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &tombstone_key,
        )
        .await?
        {
            crate::backup::delete_artifact_if_present(
                &*state.runner,
                &storage,
                &state.config.dockrev_image_repo,
                &tombstone_key,
            )
            .await?;
        } else if crate::backup::artifact_exists(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &artifact_key,
        )
        .await?
        {
            anyhow::bail!(
                "backup artifact reappeared while record deletion cleanup was pending: {}",
                artifact.artifact_path
            );
        }
    }
    state
        .db
        .complete_stack_record_deletion_cleanup(stack_id)
        .await?;
    Ok(())
}

async fn recover_pending_stack_record_deletion_intent(
    state: &Arc<AppState>,
    stack_id: &str,
    audit_json: &str,
) -> anyhow::Result<()> {
    let artifacts = serde_json::from_str::<Vec<StackRecordDeletionAuditArtifact>>(audit_json)
        .context("decode pending stack record deletion intent")?;
    let present_artifacts = artifacts.iter().filter(|artifact| artifact.present);
    let Some(_) = present_artifacts.clone().next() else {
        state
            .db
            .finish_stack_record_deletion_intent(
                stack_id,
                "failed",
                false,
                Some("staging intent recovered before retry"),
            )
            .await?;
        return Ok(());
    };

    let storage =
        crate::backup_storage::resolve_backup_storage(&*state.runner, &state.config.db_path)
            .await?;
    for artifact in present_artifacts {
        let artifact_key = crate::backup::legacy_artifact_key(&storage, &artifact.artifact_path)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "backup artifact path is outside managed storage: {}",
                    artifact.artifact_path
                )
            })?;
        let tombstone_key =
            validate_record_deletion_tombstone_key(&artifact_key, &artifact.tombstone_key)?;
        let original_present = crate::backup::artifact_exists(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &artifact_key,
        )
        .await?;
        let tombstone_present = crate::backup::artifact_exists(
            &*state.runner,
            &storage,
            &state.config.dockrev_image_repo,
            &tombstone_key,
        )
        .await?;
        match (original_present, tombstone_present) {
            (true, true) => {
                anyhow::bail!(
                    "backup artifact and its record-deletion tombstone both exist: {}",
                    artifact.artifact_path
                )
            }
            (false, true) => {
                crate::backup::move_artifact_to_tombstone(
                    &*state.runner,
                    &storage,
                    &state.config.dockrev_image_repo,
                    &tombstone_key,
                    &artifact_key,
                )
                .await?;
            }
            (true, false) | (false, false) => {}
        }
    }

    state
        .db
        .finish_stack_record_deletion_intent(
            stack_id,
            "failed",
            false,
            Some("staging intent recovered before retry"),
        )
        .await?;
    Ok(())
}

async fn record_stack_record_deletion_attempt(
    state: &Arc<AppState>,
    stack_id: &str,
    actor_principal: &str,
    outcome: &str,
) -> Result<(), ApiError> {
    let occurred_at = now_rfc3339().map_err(map_internal)?;
    state
        .db
        .record_stack_record_deletion_attempt(stack_id, actor_principal, outcome, &occurred_at)
        .await
        .map_err(map_internal)
}

async fn stack_record_deletion_preview(
    state: &Arc<AppState>,
    stack_id: &str,
) -> Result<StackRecordDeletionPreviewResponse, ApiError> {
    let Some(snapshot) = state
        .db
        .stack_record_deletion_snapshot(stack_id)
        .await
        .map_err(map_internal)?
    else {
        return Err(ApiError::not_found("stack not found"));
    };

    let mut blocking_reason = None;
    if !snapshot.missing {
        blocking_reason = Some("stack_is_not_missing".to_string());
    } else if snapshot.has_pending_job {
        blocking_reason = Some("stack_has_pending_operation".to_string());
    } else {
        match compose_files_are_missing(&snapshot.compose_files).await {
            Ok(true) => {}
            Ok(false) => blocking_reason = Some("compose_files_reappeared".to_string()),
            Err(error) => blocking_reason = Some(format!("compose_files_unreadable: {error}")),
        }
    }

    let mut backup_artifacts = Vec::new();
    if blocking_reason.is_none() && !snapshot.backup_artifacts.is_empty() {
        match resolve_stack_record_artifacts(state, &snapshot).await {
            Ok((_storage, staged)) => {
                backup_artifacts = staged
                    .into_iter()
                    .map(|item| StackRecordDeletionArtifactPreview {
                        artifact_path: item.artifact_path,
                        present: item.present,
                    })
                    .collect();
            }
            Err(error) => blocking_reason = Some(format!("backup_artifact_unsafe: {error}")),
        }
    }

    Ok(StackRecordDeletionPreviewResponse {
        stack_id: snapshot.stack_id,
        stack_name: snapshot.stack_name,
        missing: snapshot.missing,
        can_delete: blocking_reason.is_none(),
        blocking_reason,
        backup_artifacts,
        docker_resources: StackRecordDeletionDockerResources {
            status: "unknown".to_string(),
            containers: 0,
            images: 0,
            volumes: 0,
            networks: 0,
        },
    })
}

pub(super) async fn get_stack_record_deletion_preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<StackRecordDeletionPreviewResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    Ok(Json(
        stack_record_deletion_preview(&state, &stack_id).await?,
    ))
}

pub(super) async fn delete_stack_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<StackRecordDeletionResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let _guard = acquire_stack_record_deletion_guard(&stack_id)?;
    if let Some(intent_json) = state
        .db
        .pending_stack_record_deletion_intent(&stack_id)
        .await
        .map_err(map_internal)?
    {
        recover_pending_stack_record_deletion_intent(&state, &stack_id, &intent_json)
            .await
            .map_err(map_internal)?;
    }

    let Some(snapshot) = state
        .db
        .stack_record_deletion_snapshot(&stack_id)
        .await
        .map_err(map_internal)?
    else {
        if let Some(audit_json) = state
            .db
            .pending_stack_record_deletion_cleanup(&stack_id)
            .await
            .map_err(map_internal)?
            && let Err(error) =
                retry_pending_stack_record_artifact_cleanup(&state, &stack_id, &audit_json).await
        {
            state
                .db
                .mark_stack_record_deletion_cleanup_error(&stack_id, &error.to_string())
                .await
                .map_err(map_internal)?;
            return Err(ApiError::internal(
                "Stack record was deleted; backup artifact cleanup remains pending",
            ));
        }
        let occurred_at = now_rfc3339().map_err(map_internal)?;
        state
            .db
            .record_stack_record_deletion_already_absent(&stack_id, &user.principal, &occurred_at)
            .await
            .map_err(map_internal)?;
        return Ok(Json(StackRecordDeletionResponse {
            stack_id,
            outcome: "already_absent".to_string(),
        }));
    };

    if !snapshot.missing {
        record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
            .await?;
        return Err(
            ApiError::conflict("stack is no longer missing").with_details(json!({
                "reason": "stack_is_not_missing"
            })),
        );
    }
    if snapshot.has_pending_job {
        record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
            .await?;
        return Err(
            ApiError::conflict("stack has a pending operation").with_details(json!({
                "reason": "stack_has_pending_operation"
            })),
        );
    }
    match compose_files_are_missing(&snapshot.compose_files).await {
        Ok(true) => {}
        Ok(false) => {
            record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
                .await?;
            return Err(
                ApiError::conflict("Compose files reappeared").with_details(json!({
                    "reason": "compose_files_reappeared"
                })),
            );
        }
        Err(error) => {
            record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
                .await?;
            return Err(
                ApiError::conflict("Compose files cannot be verified").with_details(
                    json!({ "reason": "compose_files_unreadable", "cause": error.to_string() }),
                ),
            );
        }
    }

    let (storage, staged) = if snapshot.backup_artifacts.is_empty() {
        (None, Vec::new())
    } else {
        let (storage, staged) = match resolve_stack_record_artifacts(&state, &snapshot).await {
            Ok(value) => value,
            Err(error) => {
                record_stack_record_deletion_attempt(
                    &state,
                    &stack_id,
                    &user.principal,
                    "conflict",
                )
                .await?;
                return Err(
                    ApiError::conflict("backup artifact cannot be safely verified").with_details(
                        json!({
                            "reason": "backup_artifact_unsafe",
                            "cause": error.to_string()
                        }),
                    ),
                );
            }
        };
        (Some(storage), staged)
    };

    let artifact_audit = serde_json::to_string(
        &staged
            .iter()
            .map(|item| {
                json!({
                    "artifactPath": item.artifact_path,
                    "tombstoneKey": item.tombstone_key.to_string_lossy(),
                    "present": item.present,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|error| map_internal(error.into()))?;
    let now = now_rfc3339().map_err(map_internal)?;
    state
        .db
        .begin_stack_record_deletion(
            &stack_id,
            &user.principal,
            &now,
            &artifact_audit,
            staged.iter().any(|item| item.present),
        )
        .await
        .map_err(map_internal)?;

    let mut moved = Vec::new();
    if let Some(storage) = &storage {
        for item in staged.iter().filter(|item| item.present) {
            if let Err(error) = crate::backup::move_artifact_to_tombstone(
                &*state.runner,
                storage,
                &state.config.dockrev_image_repo,
                &item.artifact_key,
                &item.tombstone_key,
            )
            .await
            {
                let restore_error = restore_staged_stack_record_artifacts(&state, storage, &moved)
                    .await
                    .err()
                    .map(|restore| format!("; restore failed: {restore}"))
                    .unwrap_or_default();
                if restore_error.is_empty() {
                    state
                        .db
                        .finish_stack_record_deletion_intent(
                            &stack_id,
                            "failed",
                            false,
                            Some("backup artifact staging failed"),
                        )
                        .await
                        .map_err(map_internal)?;
                }
                return Err(ApiError::internal(format!(
                    "backup artifact staging failed: {error}{restore_error}"
                )));
            }
            moved.push(item.clone());
        }
    }

    let final_snapshot = state
        .db
        .stack_record_deletion_snapshot(&stack_id)
        .await
        .map_err(map_internal)?;
    let Some(final_snapshot) = final_snapshot else {
        state
            .db
            .finish_stack_record_deletion_intent(
                &stack_id,
                "already_absent",
                !moved.is_empty(),
                None,
            )
            .await
            .map_err(map_internal)?;
        if let Some(storage) = &storage
            && let Err(error) = delete_staged_stack_record_artifacts(&state, storage, &moved).await
        {
            state
                .db
                .mark_stack_record_deletion_cleanup_error(&stack_id, &error.to_string())
                .await
                .map_err(map_internal)?;
            return Err(ApiError::internal(
                "Stack record was deleted; backup artifact cleanup is pending",
            ));
        }
        state
            .db
            .complete_stack_record_deletion_cleanup(&stack_id)
            .await
            .map_err(map_internal)?;
        return Ok(Json(StackRecordDeletionResponse {
            stack_id,
            outcome: "already_absent".to_string(),
        }));
    };
    if !final_snapshot.missing || final_snapshot.has_pending_job {
        if let Some(storage) = &storage {
            restore_staged_stack_record_artifacts(&state, storage, &moved)
                .await
                .map_err(map_internal)?;
        }
        state
            .db
            .finish_stack_record_deletion_intent(
                &stack_id,
                "conflict",
                false,
                Some("stack deletion preconditions changed"),
            )
            .await
            .map_err(map_internal)?;
        record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
            .await?;
        return Err(ApiError::conflict("stack deletion preconditions changed"));
    }
    if !compose_files_are_missing(&final_snapshot.compose_files)
        .await
        .map_err(map_internal)?
    {
        if let Some(storage) = &storage {
            restore_staged_stack_record_artifacts(&state, storage, &moved)
                .await
                .map_err(map_internal)?;
        }
        state
            .db
            .finish_stack_record_deletion_intent(
                &stack_id,
                "conflict",
                false,
                Some("Compose files reappeared"),
            )
            .await
            .map_err(map_internal)?;
        record_stack_record_deletion_attempt(&state, &stack_id, &user.principal, "conflict")
            .await?;
        return Err(ApiError::conflict("Compose files reappeared"));
    }

    let outcome = state
        .db
        .commit_stack_record_deletion(
            &stack_id,
            &user.principal,
            &now,
            &artifact_audit,
            !moved.is_empty(),
        )
        .await
        .map_err(map_internal)?;

    match outcome {
        crate::db::StackRecordDeletionCommit::Conflict(reason) => {
            if let Some(storage) = &storage {
                restore_staged_stack_record_artifacts(&state, storage, &moved)
                    .await
                    .map_err(map_internal)?;
            }
            state
                .db
                .finish_stack_record_deletion_intent(
                    &stack_id,
                    "conflict",
                    false,
                    Some("database deletion preconditions changed"),
                )
                .await
                .map_err(map_internal)?;
            Err(ApiError::conflict(reason))
        }
        crate::db::StackRecordDeletionCommit::AlreadyAbsent => {
            state
                .db
                .finish_stack_record_deletion_intent(
                    &stack_id,
                    "already_absent",
                    !moved.is_empty(),
                    None,
                )
                .await
                .map_err(map_internal)?;
            if let Some(storage) = &storage
                && let Err(error) =
                    delete_staged_stack_record_artifacts(&state, storage, &moved).await
            {
                state
                    .db
                    .mark_stack_record_deletion_cleanup_error(&stack_id, &error.to_string())
                    .await
                    .map_err(map_internal)?;
                return Err(ApiError::internal(
                    "Stack record was deleted; backup artifact cleanup is pending",
                ));
            }
            state
                .db
                .complete_stack_record_deletion_cleanup(&stack_id)
                .await
                .map_err(map_internal)?;
            Ok(Json(StackRecordDeletionResponse {
                stack_id,
                outcome: "already_absent".to_string(),
            }))
        }
        crate::db::StackRecordDeletionCommit::Deleted => {
            if let Some(storage) = &storage
                && let Err(error) =
                    delete_staged_stack_record_artifacts(&state, storage, &moved).await
            {
                state
                    .db
                    .mark_stack_record_deletion_cleanup_error(&stack_id, &error.to_string())
                    .await
                    .map_err(map_internal)?;
                return Err(ApiError::internal(
                    "Stack record deleted; backup artifact cleanup is pending",
                ));
            }
            if let Err(error) = state
                .db
                .complete_stack_record_deletion_cleanup(&stack_id)
                .await
            {
                state
                    .db
                    .mark_stack_record_deletion_cleanup_error(&stack_id, &error.to_string())
                    .await
                    .map_err(map_internal)?;
                return Err(ApiError::internal(
                    "Stack record deleted; backup artifact cleanup is pending",
                ));
            }
            state
                .management_events
                .publish_change(
                    "stacks",
                    "stack",
                    stack_id.clone(),
                    json!({ "operation": "record_deleted" }),
                )
                .await;
            Ok(Json(StackRecordDeletionResponse {
                stack_id,
                outcome: "deleted".to_string(),
            }))
        }
    }
}
