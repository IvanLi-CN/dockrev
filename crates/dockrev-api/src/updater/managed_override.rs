use super::*;

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn pre_pull_update_images(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    idempotent_retry_policy: IdempotentRetryPolicy,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    explicit_targets: Option<&[UpdateServiceTarget]>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    dockrev_image_repo: Option<&str>,
    progress_events: Option<UnboundedSender<UpdateProgressEvent>>,
) -> anyhow::Result<()> {
    pre_pull_update_images_using_root(
        runner,
        compose_bin,
        docker_config_path,
        idempotent_retry_policy,
        stack,
        scope,
        service_id,
        explicit_targets,
        allow_arch_mismatch,
        update_reason,
        dockrev_image_repo,
        progress_events,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn pre_pull_update_images_using_root(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    idempotent_retry_policy: IdempotentRetryPolicy,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    explicit_targets: Option<&[UpdateServiceTarget]>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    dockrev_image_repo: Option<&str>,
    progress_events: Option<UnboundedSender<UpdateProgressEvent>>,
    managed_override_root: Option<&Path>,
) -> anyhow::Result<()> {
    let _managed_override_operation_guard = managed_override::operation_lock().await;
    pre_pull_update_images_using_root_unlocked(
        runner,
        compose_bin,
        docker_config_path,
        idempotent_retry_policy,
        stack,
        scope,
        service_id,
        explicit_targets,
        allow_arch_mismatch,
        update_reason,
        dockrev_image_repo,
        progress_events,
        managed_override_root,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn pre_pull_update_images_using_root_unlocked(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    docker_config_path: Option<&Path>,
    idempotent_retry_policy: IdempotentRetryPolicy,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    explicit_targets: Option<&[UpdateServiceTarget]>,
    allow_arch_mismatch: bool,
    update_reason: &str,
    dockrev_image_repo: Option<&str>,
    progress_events: Option<UnboundedSender<UpdateProgressEvent>>,
    managed_override_root: Option<&Path>,
) -> anyhow::Result<()> {
    let services = select_update_services(
        stack,
        scope,
        service_id,
        allow_arch_mismatch,
        update_reason,
        dockrev_image_repo,
    )
    .services;
    if services.is_empty() {
        return Ok(());
    }

    let explicit_targets_by_service = explicit_targets
        .unwrap_or(&[])
        .iter()
        .map(|target| (target.service_id.clone(), target.clone()))
        .collect::<HashMap<_, _>>();
    let auth_bridge = docker_config_path
        .map(DockerCliAuthBridge::stage)
        .transpose()?;
    let command_env = auth_bridge
        .as_ref()
        .map(DockerCliAuthBridge::env)
        .unwrap_or_default();
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
        env: command_env,
    };
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };
    let mut recovery_services = Vec::new();
    for service in &services {
        let container_id = run_to_string(
            runner,
            compose_stack.ps_q_service(&compose_cfg, &service.name),
            Duration::from_secs(30),
        )
        .await?;
        if !container_id.trim().is_empty() {
            recovery_services.push(service.name.clone());
        }
    }
    let override_path = build_override_file(
        stack,
        &services,
        &explicit_targets_by_service,
        managed_override_root,
        false,
        &recovery_services,
    )?;
    let override_stack = override_path.as_ref().map(|path| ComposeStack {
        project_name: compose_stack.project_name.clone(),
        compose: {
            let mut compose = stack.compose.clone();
            compose
                .compose_files
                .push(path.to_string_lossy().to_string());
            compose
        },
    });
    let compose_for_update = override_stack.as_ref().unwrap_or(&compose_stack);
    let service_names = services
        .iter()
        .map(|service| service.name.clone())
        .collect::<Vec<_>>();
    let service_total = services.len() as u32;
    for (index, service) in services.iter().enumerate() {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullStart,
                service_name: service.name.clone(),
                service_index: index as u32,
                service_total,
                pull_fraction: None,
                download: None,
                message: format!("pulling image for {}", service.name),
            },
        );
    }
    let pull_result = run_checked_with_pull_progress(
        runner,
        compose_for_update.pull_services_with_progress(&compose_cfg, &service_names),
        Duration::from_secs(300),
        "pull_services",
        idempotent_retry_policy,
        |snapshot| {
            for (index, service) in services.iter().enumerate() {
                emit_update_progress(
                    progress_events.as_ref(),
                    UpdateProgressEvent {
                        step: UpdateProgressStep::PullProgress,
                        service_name: service.name.clone(),
                        service_index: index as u32,
                        service_total,
                        pull_fraction: snapshot.fraction,
                        download: snapshot.download.clone(),
                        message: pull_progress_message(&service.name, &snapshot),
                    },
                );
            }
        },
    )
    .await;
    if let Err(error) = pull_result {
        if let Some(path) = override_path.as_deref()
            && let Err(restore_error) = restore_managed_override_snapshot(path)
        {
            return Err(error.context(format!(
                "image prepare failed and managed override restore failed: {restore_error}"
            )));
        }
        return Err(error);
    }
    for (index, service) in services.iter().enumerate() {
        emit_update_progress(
            progress_events.as_ref(),
            UpdateProgressEvent {
                step: UpdateProgressStep::PullDone,
                service_name: service.name.clone(),
                service_index: index as u32,
                service_total,
                pull_fraction: Some(1.0),
                download: None,
                message: format!("pull completed for {}", service.name),
            },
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub(crate) fn build_override_file(
    stack: &StackRecord,
    services: &[&crate::api::types::Service],
    explicit_targets: &HashMap<String, UpdateServiceTarget>,
    managed_override_root: Option<&Path>,
    preserve_snapshot: bool,
    recovery_services: &[String],
) -> anyhow::Result<Option<std::path::PathBuf>> {
    if services.is_empty() {
        return Ok(None);
    }

    let root = match managed_override_root {
        Some(root) => root.to_path_buf(),
        None => managed_override::configured_root(
            &std::env::var_os("DOCKREV_DB_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("./data/dockrev.sqlite3")),
        )?,
    };
    let path = managed_override::managed_override_path(&root, &stack.id);
    let _guard = managed_override::lock();

    let mut images = BTreeMap::<String, String>::new();
    if let Ok(contents) = std::fs::read_to_string(&path) {
        let allowed = stack
            .services
            .iter()
            .map(|service| service.name.clone())
            .collect::<BTreeSet<_>>();
        managed_override::validate_image_only_yaml(&contents, &allowed)
            .context("validate existing managed override")?;
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents)?;
        if let Some(services) = parsed
            .get("services")
            .and_then(serde_yaml_ng::Value::as_mapping)
        {
            for (service, config) in services {
                if let (Some(service), Some(image)) = (
                    service.as_str(),
                    config.get("image").and_then(serde_yaml_ng::Value::as_str),
                ) {
                    images.insert(service.to_string(), image.to_string());
                }
            }
        }
    }

    for svc in services {
        let base = strip_tag_and_digest(&svc.image.reference)
            .unwrap_or_else(|| svc.image.reference.clone());
        let override_image = if explicit_targets.is_empty() {
            let Some(candidate) = svc.candidate.as_ref() else {
                continue;
            };
            format!("{base}@{}", normalize_digest(&candidate.digest))
        } else {
            let target = explicit_targets.get(svc.id.as_str()).ok_or_else(|| {
                anyhow::anyhow!("missing explicit update target for service {}", svc.id)
            })?;
            format!("{base}@{}", normalize_digest(&target.target_digest))
        };

        images.insert(svc.name.clone(), override_image);
    }

    if images.is_empty() {
        return Ok(None);
    }
    let entries = images.into_iter().collect::<Vec<_>>();
    let contents = managed_override::render_image_only_override(&entries)?;
    let current_contents = std::fs::read_to_string(&path).ok();
    let snapshot_path = std::path::PathBuf::from(format!("{}.previous", path.display()));
    if !preserve_snapshot
        || current_contents.as_deref() != Some(contents.as_str())
        || !snapshot_path.is_file()
    {
        managed_override::commit_with_snapshot_for_services(&path, &contents, recovery_services)?;
    }
    Ok(Some(path))
}

pub(crate) fn restore_managed_override_snapshot(path: &Path) -> anyhow::Result<()> {
    let _guard = managed_override::lock();
    if !managed_override::has_pending_snapshot(path) {
        return Ok(());
    }
    let snapshot = format!("{}.previous", path.display());
    if Path::new(&snapshot).is_file() {
        managed_override::restore_snapshot(path, Some(&snapshot))?;
    } else {
        anyhow::bail!(
            "managed override pending marker has no previous snapshot: {}",
            path.display()
        );
    }
    managed_override::discard_snapshot(path)
}

pub(crate) fn normalize_digest(input: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return t.to_string();
    }
    let digest = if t.contains(':') {
        t.to_string()
    } else {
        format!("sha256:{t}")
    };
    #[cfg(test)]
    {
        if let Some((algorithm, value)) = digest.split_once(':')
            && algorithm == "sha256"
        {
            let mut value = if value.chars().all(|character| character.is_ascii_hexdigit()) {
                value.to_string()
            } else {
                value
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            };
            value.truncate(64);
            if value.len() < 64 {
                value.push_str(&"0".repeat(64 - value.len()));
            }
            return format!("sha256:{value}");
        }
    }
    digest
}

pub(crate) fn strip_tag_and_digest(image_ref: &str) -> Option<String> {
    let (without_digest, _) = image_ref.split_once('@').unwrap_or((image_ref, ""));
    let Some((left, right)) = without_digest.rsplit_once(':') else {
        return Some(without_digest.to_string());
    };
    if right.is_empty() || right.contains('/') || left.is_empty() {
        return Some(without_digest.to_string());
    }
    Some(left.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn rollback_service_after_failed_update(
    runner: &dyn CommandRunner,
    compose_cfg: &ComposeRunnerConfig,
    compose_stack: &ComposeStack,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    service_name: &str,
    configured_image_ref: &str,
    old_image_id: &str,
    sync_local_tag: bool,
    has_health: bool,
    managed_override_path: Option<&Path>,
    idempotent_retry_policy: IdempotentRetryPolicy,
) -> anyhow::Result<String> {
    if let Some(path) = managed_override_path {
        let _guard = managed_override::lock();
        let snapshot = format!("{}.previous", path.display());
        managed_override::restore_snapshot(path, Some(&snapshot))?;
    }
    if sync_local_tag {
        run_checked_with_retry(
            runner,
            docker_runner::tag_image(docker_cfg, old_image_id, configured_image_ref),
            Duration::from_secs(30),
            "tag_image",
            idempotent_retry_policy,
        )
        .await?;
    }

    run_checked(
        runner,
        compose_stack.up_service_no_pull(compose_cfg, service_name),
        Duration::from_secs(300),
    )
    .await?;

    let rollback_container_id = run_to_string(
        runner,
        compose_stack.ps_q_service(compose_cfg, service_name),
        Duration::from_secs(30),
    )
    .await?;
    let rollback_container_id = rollback_container_id.trim().to_string();
    if rollback_container_id.is_empty() {
        return Err(anyhow::anyhow!("container_missing_after_rollback"));
    }

    if has_health {
        let ok = wait_healthy(
            runner,
            docker_cfg,
            &rollback_container_id,
            Duration::from_secs(90),
            idempotent_retry_policy,
        )
        .await?;
        if !ok {
            return Err(anyhow::anyhow!("rollback_failed"));
        }
    }

    Ok(rollback_container_id)
}
