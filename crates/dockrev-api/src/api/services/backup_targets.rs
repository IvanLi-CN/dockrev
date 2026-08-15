use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) async fn get_service_backup_targets(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<ServiceBackupTargetsResponse, ApiError> {
    let context = load_service_backup_context(state, service_id).await?;
    Ok(build_service_backup_targets_response(
        &context.service_id,
        &context.stack,
        &context.compose_services,
        &context.backup_settings,
    ))
}

pub(super) async fn put_service_backup_targets(
    state: &Arc<AppState>,
    service_id: &str,
    req: PutServiceBackupTargetsRequest,
) -> Result<PutServiceBackupTargetsResponse, ApiError> {
    let context = load_service_backup_context(state, service_id).await?;
    let update = build_service_backup_targets_update(&context, req)?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .put_service_backup_targets(service_id, &update, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("service not found"));
    }
    state
        .management_events
        .publish_change(
            "services",
            "service",
            service_id.to_string(),
            serde_json::json!({ "operation": "backup_targets_updated" }),
        )
        .await;
    Ok(PutServiceBackupTargetsResponse { ok: true })
}

pub(super) async fn read_compose_service_specs(
    compose_files: &[String],
) -> anyhow::Result<Vec<crate::db::ComposeServiceSpec>> {
    let mut merged = std::collections::BTreeMap::new();
    for path in compose_files {
        let contents = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("read compose file {path}"))?;
        let parsed = crate::compose::parse_services(&contents)
            .with_context(|| format!("parse compose file {path}"))?;
        let backup_targets =
            crate::compose::parse_backup_targets(&contents, std::path::Path::new(path))
                .with_context(|| format!("parse compose mounts {path}"))?;
        let parsed = parsed
            .into_iter()
            .map(|mut svc| {
                if let Some(targets) = backup_targets.get(&svc.name) {
                    svc.backup_bind_paths = targets.bind_paths.clone();
                    svc.backup_volume_names = targets.volume_names.clone();
                }
                svc
            })
            .collect();
        merged = crate::compose::merge_services(merged, parsed);
    }
    Ok(merged
        .into_values()
        .map(|svc| crate::db::ComposeServiceSpec {
            name: svc.name,
            image_ref: svc.image_ref,
            image_tag: svc.image_tag,
            homepage: svc.homepage,
            update_guard: svc.update_guard,
            backup_bind_paths: svc.backup_bind_paths,
            backup_volume_names: svc.backup_volume_names,
        })
        .collect())
}

struct ServiceBackupContext {
    service_id: String,
    stack: crate::api::types::StackRecord,
    compose_services: Vec<crate::db::ComposeServiceSpec>,
    backup_settings: crate::api::types::BackupSettings,
}

async fn load_service_backup_context(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<ServiceBackupContext, ApiError> {
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
    let compose_services = read_compose_service_specs(&stack.compose.compose_files)
        .await
        .map_err(map_internal)?;
    let mut backup_settings = state.db.get_backup_settings().await.map_err(map_internal)?;
    backup_settings.base_dir = crate::backup_storage::logical_backup_root(&state.config.db_path)
        .map_err(map_internal)?
        .to_string_lossy()
        .to_string();
    Ok(ServiceBackupContext {
        service_id: service_id.to_string(),
        stack,
        compose_services,
        backup_settings,
    })
}

fn build_service_backup_targets_response(
    service_id: &str,
    stack: &crate::api::types::StackRecord,
    compose_services: &[crate::db::ComposeServiceSpec],
    backup_settings: &crate::api::types::BackupSettings,
) -> ServiceBackupTargetsResponse {
    let storage = ServiceBackupStorageInfo {
        base_dir: backup_settings.base_dir.clone(),
        artifact_pattern: format!("{}/<stackId>/<timestamp>.tar.zst", backup_settings.base_dir),
        compression: "zstd".to_string(),
        keep_last: stack.backup.retention.keep_last,
        delete_after_stable_seconds: stack.backup.retention.delete_after_stable_seconds,
    };
    let service_by_name = compose_services
        .iter()
        .map(|svc| (svc.name.as_str(), svc))
        .collect::<BTreeMap<_, _>>();
    let current_service = stack
        .services
        .iter()
        .find(|svc| svc.id == service_id)
        .expect("service existence checked earlier");
    let bind_paths = service_by_name
        .get(current_service.name.as_str())
        .map(|declared| {
            declared
                .backup_bind_paths
                .iter()
                .map(|key| {
                    build_service_backup_target_item(
                        service_id,
                        stack,
                        compose_services,
                        key,
                        false,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let volume_names = service_by_name
        .get(current_service.name.as_str())
        .map(|declared| {
            declared
                .backup_volume_names
                .iter()
                .map(|key| {
                    build_service_backup_target_item(service_id, stack, compose_services, key, true)
                })
                .collect()
        })
        .unwrap_or_default();

    ServiceBackupTargetsResponse {
        bind_paths,
        volume_names,
        storage,
    }
}

fn build_service_backup_target_item(
    service_id: &str,
    stack: &crate::api::types::StackRecord,
    compose_services: &[crate::db::ComposeServiceSpec],
    key: &str,
    is_volume: bool,
) -> ServiceBackupTargetItem {
    let service = stack
        .services
        .iter()
        .find(|svc| svc.id == service_id)
        .expect("service existence checked earlier");
    let policy = match if is_volume {
        service.settings.backup_targets.volume_names.get(key)
    } else {
        service.settings.backup_targets.bind_paths.get(key)
    } {
        Some(crate::api::types::TernaryChoice::Force) => {
            crate::api::types::BackupTargetPolicy::StopRelatedServices
        }
        Some(crate::api::types::TernaryChoice::Inherit) => {
            crate::api::types::BackupTargetPolicy::LiveBackup
        }
        Some(crate::api::types::TernaryChoice::Skip) | None => {
            crate::api::types::BackupTargetPolicy::Disabled
        }
    };
    let related_service_ids =
        count_declared_services_for_target(is_volume, key, stack, compose_services);

    ServiceBackupTargetItem {
        key: key.to_string(),
        policy,
        related_service_count: related_service_ids.len() as u32,
        related_service_ids,
    }
}

fn build_service_backup_targets_update(
    context: &ServiceBackupContext,
    req: PutServiceBackupTargetsRequest,
) -> Result<crate::db::ServiceBackupTargetsUpdate, ApiError> {
    let service = context
        .stack
        .services
        .iter()
        .find(|svc| svc.id == context.service_id)
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let compose_service = context
        .compose_services
        .iter()
        .find(|svc| svc.name == service.name)
        .ok_or_else(|| ApiError::not_found("service compose spec not found"))?;

    let bind_paths =
        reconcile_backup_category(false, &compose_service.backup_bind_paths, req.bind_paths)?;
    let volume_names =
        reconcile_backup_category(true, &compose_service.backup_volume_names, req.volume_names)?;

    let stack_targets = merge_stack_backup_targets(&bind_paths, &volume_names);

    Ok(crate::db::ServiceBackupTargetsUpdate {
        stack_targets,
        bind_paths,
        volume_names,
    })
}

fn reconcile_backup_category(
    is_volume: bool,
    declared_keys: &[String],
    items: Vec<PutServiceBackupTargetItem>,
) -> Result<Vec<crate::db::ServiceBackupTargetPolicyRow>, ApiError> {
    let declared = declared_keys.iter().cloned().collect::<BTreeSet<_>>();
    let requested = items
        .into_iter()
        .map(|item| (item.key.clone(), item))
        .collect::<BTreeMap<_, _>>();
    if requested.len() != declared.len() {
        return Err(ApiError::invalid_argument(
            "backup target selection is incomplete",
        ));
    }
    for key in requested.keys() {
        if !declared.contains(key) {
            return Err(ApiError::invalid_argument(
                "backup target is not declared by this service",
            ));
        }
    }

    let mut next = Vec::new();
    for key in declared {
        let item = requested
            .get(&key)
            .ok_or_else(|| ApiError::invalid_argument("backup target selection is incomplete"))?;
        if is_volume == key.starts_with('/') {
            return Err(ApiError::invalid_argument(
                "backup target kind does not match key",
            ));
        }
        next.push(crate::db::ServiceBackupTargetPolicyRow {
            key: key.clone(),
            policy: item.policy,
        });
    }
    Ok(next)
}

fn merge_stack_backup_targets(
    next_bind_paths: &[crate::db::ServiceBackupTargetPolicyRow],
    next_volume_names: &[crate::db::ServiceBackupTargetPolicyRow],
) -> Vec<crate::api::types::BackupTarget> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for row in next_volume_names {
        if row.policy.is_enabled() && seen.insert(("volume", row.key.clone())) {
            out.push(crate::api::types::BackupTarget::DockerVolume {
                name: row.key.clone(),
            });
        }
    }
    for row in next_bind_paths {
        if row.policy.is_enabled() && seen.insert(("bind", row.key.clone())) {
            out.push(crate::api::types::BackupTarget::BindMount {
                path: row.key.clone(),
            });
        }
    }
    out
}

fn count_declared_services_for_target(
    is_volume: bool,
    key: &str,
    stack: &crate::api::types::StackRecord,
    compose_services: &[crate::db::ComposeServiceSpec],
) -> Vec<String> {
    let mut ids = compose_services
        .iter()
        .filter(|svc| {
            if is_volume {
                svc.backup_volume_names.iter().any(|item| item == key)
            } else {
                svc.backup_bind_paths.iter().any(|item| item == key)
            }
        })
        .filter_map(|svc| stack.services.iter().find(|live| live.name == svc.name))
        .map(|svc| svc.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}
