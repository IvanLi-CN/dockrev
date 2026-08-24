use super::*;

pub fn try_managed_reconcile_lock() -> Option<tokio::sync::MutexGuard<'static, ()>> {
    managed_override::try_operation_lock()
}

pub async fn run_managed_override_reconcile(
    state: &AppState,
    job_id: &str,
    stack_id: &str,
    reconcile_guard: tokio::sync::MutexGuard<'static, ()>,
) {
    let result = reconcile_managed_override(state, stack_id, reconcile_guard).await;
    let finished_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    match result {
        Ok(summary) => {
            let _ = state
                .db
                .finish_job(job_id, "success", &finished_at, &summary)
                .await;
        }
        Err(error) => {
            let _ = state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: finished_at.clone(),
                        level: "error".to_string(),
                        msg: format!("managed override reconciliation failed: {error}"),
                    },
                )
                .await;
            let _ = state
                .db
                .finish_job(
                    job_id,
                    "failed",
                    &finished_at,
                    &serde_json::json!({"error": error.to_string()}),
                )
                .await;
        }
    }
}

async fn reconcile_managed_override(
    state: &AppState,
    stack_id: &str,
    _reconcile_guard: tokio::sync::MutexGuard<'static, ()>,
) -> anyhow::Result<serde_json::Value> {
    let discovered = state
        .db
        .list_discovered_compose_projects(crate::db::ArchivedFilter::Include)
        .await?
        .into_iter()
        .find(|project| project.stack_id.as_deref() == Some(stack_id))
        .ok_or_else(|| anyhow::anyhow!("discovery project is not associated with stack"))?;
    if !discovered
        .last_error
        .as_deref()
        .is_some_and(|error| error.starts_with(managed_override::STALE_TEMP_WARNING))
    {
        anyhow::bail!(
            "managed override reconciliation requires a stale Dockrev temporary override warning"
        );
    }

    let stack = state
        .db
        .get_stack(stack_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stack not found"))?;
    let affected_services = discovered
        .last_error
        .as_deref()
        .and_then(parse_affected_services)
        .unwrap_or_else(|| {
            stack
                .services
                .iter()
                .map(|service| service.name.clone())
                .collect()
        });
    let selected_services = stack
        .services
        .iter()
        .filter(|service| affected_services.iter().any(|name| name == &service.name))
        .collect::<Vec<_>>();
    if selected_services.is_empty() {
        anyhow::bail!("stale warning did not identify any services in the stack");
    }
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: state.config.compose_bin.clone(),
        env: Vec::new(),
    };
    let base_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };
    let docker_cfg = docker_runner::DockerRunnerConfig::default();
    let mut old_image_ids = BTreeMap::new();
    let mut images = Vec::new();
    for service in &selected_services {
        let container_id =
            command_text(state, base_stack.ps_q_service(&compose_cfg, &service.name)).await?;
        let container_id = container_id.trim();
        if container_id.is_empty() {
            anyhow::bail!("service container is not running: {}", service.name);
        }
        let image_id = command_text(
            state,
            docker_runner::inspect_image_id(&docker_cfg, container_id),
        )
        .await?;
        let image_id = image_id.trim().to_string();
        if image_id.is_empty() {
            anyhow::bail!("running image id is empty: {}", service.name);
        }
        let repo = base_repository(&service.image.reference);
        let repo_digest = command_text(
            state,
            docker_runner::inspect_repo_digests(&docker_cfg, &image_id),
        )
        .await?
        .split(',')
        .map(str::trim)
        .find(|digest| repo_digest_matches(digest, &repo))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no RepoDigest matching base repository for {}",
                service.name
            )
        })?;
        old_image_ids.insert(service.name.clone(), image_id);
        images.push((service.name.clone(), repo_digest));
    }

    let path =
        managed_override::managed_override_path(&state.config.managed_override_dir, stack_id);
    let _override_guard = managed_override::lock();
    managed_override::recover_interrupted(&path)?;
    let allowed_services = stack
        .services
        .iter()
        .map(|service| service.name.clone())
        .collect::<BTreeSet<_>>();
    let existing_contents = std::fs::read_to_string(&path).ok();
    let contents =
        merge_managed_override_images(existing_contents.as_deref(), &allowed_services, &images)?;
    let service_names = selected_services
        .iter()
        .map(|service| service.name.clone())
        .collect::<Vec<_>>();
    managed_override::commit_with_snapshot_for_services(&path, &contents, &service_names)?;
    drop(_override_guard);

    let mut managed_stack = base_stack.clone();
    managed_stack
        .compose
        .compose_files
        .push(path.to_string_lossy().to_string());
    if let Err(error) = run_checked_command(
        state,
        managed_stack.up_services_no_pull_no_deps_force_recreate(&compose_cfg, &service_names),
    )
    .await
    {
        return Err(rollback_reconciliation(
            state,
            &managed_stack,
            &compose_cfg,
            &service_names,
            &path,
            error,
        )
        .await);
    }

    for service in &selected_services {
        let verification = async {
            let container_id = command_text(
                state,
                managed_stack.ps_q_service(&compose_cfg, &service.name),
            )
            .await?;
            let container_id = container_id.trim().to_string();
            if container_id.is_empty() {
                anyhow::bail!("service container missing after reconciliation");
            }
            let running = command_text(
                state,
                docker_runner::inspect_is_running(&docker_cfg, &container_id),
            )
            .await?;
            let image_id = command_text(
                state,
                docker_runner::inspect_image_id(&docker_cfg, &container_id),
            )
            .await?;
            if running.trim() != "1" || image_id.trim() != old_image_ids[&service.name] {
                anyhow::bail!("service image or running-state verification failed");
            }
            let has_health = command_text(
                state,
                docker_runner::inspect_has_healthcheck(&docker_cfg, &container_id),
            )
            .await?;
            if has_health.trim() == "1" {
                let health = command_text(
                    state,
                    docker_runner::inspect_health_status(&docker_cfg, &container_id),
                )
                .await?;
                if health.trim() != "healthy" {
                    anyhow::bail!("service health verification failed");
                }
            }
            Ok::<(), anyhow::Error>(())
        }
        .await;
        if let Err(error) = verification {
            return Err(rollback_reconciliation(
                state,
                &managed_stack,
                &compose_cfg,
                &service_names,
                &path,
                anyhow::anyhow!(
                    "service verification failed after reconciliation for {}: {error}",
                    service.name
                ),
            )
            .await);
        }
    }

    managed_override::mark_snapshot_applied(&path)?;
    managed_override::discard_snapshot(&path)?;
    let scan = run_scan(state).await?;
    Ok(serde_json::json!({
        "managedOverridePath": path,
        "services": service_names,
        "pull": "never",
        "recreate": "--no-deps --force-recreate",
        "rescan": scan.summary,
    }))
}

pub(crate) fn merge_managed_override_images(
    existing_contents: Option<&str>,
    allowed_services: &BTreeSet<String>,
    replacements: &[(String, String)],
) -> anyhow::Result<String> {
    let mut images = BTreeMap::<String, String>::new();
    if let Some(contents) = existing_contents {
        managed_override::validate_image_only_yaml(contents, allowed_services)
            .context("validate existing managed override")?;
        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(contents)?;
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
    images.extend(replacements.iter().cloned());
    managed_override::render_image_only_override(&images.into_iter().collect::<Vec<_>>())
}

async fn command_text(state: &AppState, command: CommandSpec) -> anyhow::Result<String> {
    let output = state.runner.run(command, Duration::from_secs(60)).await?;
    if output.status != 0 {
        anyhow::bail!(
            "command failed status={} stderr={}",
            output.status,
            output.stderr.trim()
        );
    }
    Ok(output.stdout)
}

async fn run_checked_command(state: &AppState, command: CommandSpec) -> anyhow::Result<()> {
    let output = state.runner.run(command, Duration::from_secs(300)).await?;
    if output.status != 0 {
        anyhow::bail!(
            "compose reconciliation failed status={} stderr={}",
            output.status,
            output.stderr.trim()
        );
    }
    Ok(())
}

fn restore_managed_override(path: &Path) -> anyhow::Result<()> {
    let _guard = managed_override::lock();
    let snapshot = format!("{}.previous", path.display());
    managed_override::restore_snapshot(path, Some(&snapshot))
}

async fn rollback_reconciliation(
    state: &AppState,
    managed_stack: &ComposeStack,
    compose_cfg: &ComposeRunnerConfig,
    service_names: &[String],
    path: &Path,
    original_error: anyhow::Error,
) -> anyhow::Error {
    if let Err(restore_error) = restore_managed_override(path) {
        return original_error.context(format!(
            "failed to restore managed override: {restore_error}"
        ));
    }
    if let Err(compose_error) = run_checked_command(
        state,
        managed_stack.up_services_no_pull_no_deps_force_recreate(compose_cfg, service_names),
    )
    .await
    {
        return original_error.context(format!(
            "failed to restore running services: {compose_error}"
        ));
    }
    if let Err(discard_error) = managed_override::discard_snapshot(path) {
        return original_error.context(format!(
            "failed to discard managed override snapshot after rollback: {discard_error}"
        ));
    }
    original_error
}

fn base_repository(image: &str) -> String {
    let without_digest = image.split_once('@').map_or(image, |(repo, _)| repo);
    without_digest
        .rsplit_once(':')
        .filter(|(_, tag)| !tag.contains('/') && !tag.is_empty())
        .map_or(without_digest, |(repo, _)| repo)
        .to_string()
}

pub(crate) fn repo_digest_matches(repo_digest: &str, base_repo: &str) -> bool {
    let Some((repo, digest)) = repo_digest.split_once('@') else {
        return false;
    };
    if !digest.starts_with("sha256:")
        || digest.len() != 71
        || !digest["sha256:".len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return false;
    }
    repo == base_repo
}

pub(crate) fn parse_affected_services(warning: &str) -> Option<Vec<String>> {
    let raw = warning.split_once("services=[")?.1.split_once(']')?.0;
    let services = raw
        .split(',')
        .map(str::trim)
        .filter(|service| !service.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    (!services.is_empty()).then_some(services)
}
