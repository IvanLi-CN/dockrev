use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
    time::Duration,
};

use anyhow::Context as _;

use crate::{
    api::types::{
        DiscoveryAction, DiscoveryActionKind, DiscoveryScanSummary, TriggerDiscoveryScanResponse,
    },
    compose,
    db::{ComposeServiceSpec, DiscoveredComposeProjectUpsert},
    ids,
    runner::CommandSpec,
    state::AppState,
};

static DISCOVERY_SCAN_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

fn stack_services_match_specs(
    stack: &crate::api::types::StackRecord,
    specs: &[ComposeServiceSpec],
) -> bool {
    let existing = stack
        .services
        .iter()
        .map(|svc| {
            (
                svc.name.clone(),
                (svc.image.reference.clone(), svc.image.tag.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let expected = specs
        .iter()
        .map(|svc| {
            (
                svc.name.clone(),
                (svc.image_ref.clone(), svc.image_tag.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    existing == expected
}

fn parse_labels_json_line(line: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let v: serde_json::Value = serde_json::from_str(line).context("parse docker labels json")?;
    let Some(obj) = v.as_object() else {
        return Ok(BTreeMap::new());
    };

    let mut out = BTreeMap::<String, String>::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            out.insert(k.clone(), s.to_string());
        }
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub enum NormalizeConfigFilesError {
    RelativePathRejected,
    Empty,
}

pub fn normalize_config_files(raw: &str) -> Result<Vec<String>, NormalizeConfigFilesError> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();

    for part in raw
        .split([',', '\n', '\r'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if !part.starts_with('/') {
            return Err(NormalizeConfigFilesError::RelativePathRejected);
        }
        if seen.insert(part.to_string()) {
            out.push(part.to_string());
        }
    }

    if out.is_empty() {
        return Err(NormalizeConfigFilesError::Empty);
    }

    Ok(out)
}

#[derive(Clone, Debug)]
struct ObservedComposeContainer {
    service: String,
    config_files_raw: Option<String>,
}

async fn list_compose_projects_from_docker(
    state: &AppState,
) -> anyhow::Result<BTreeMap<String, Vec<ObservedComposeContainer>>> {
    let ps = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "--filter".to_string(),
                    "label=com.docker.compose.project".to_string(),
                    "-q".to_string(),
                ],
                env: Vec::new(),
            },
            Duration::from_secs(8),
        )
        .await
        .context("docker ps")?;

    if ps.status != 0 {
        return Err(anyhow::anyhow!(
            "docker ps failed status={} stderr={}",
            ps.status,
            ps.stderr
        ));
    }

    let ids: Vec<String> = ps
        .stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut by_project = BTreeMap::<String, Vec<ObservedComposeContainer>>::new();

    for chunk in ids.chunks(64) {
        let mut args = vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{json .Config.Labels}}".to_string(),
        ];
        args.extend(chunk.iter().cloned());

        let out = state
            .runner
            .run(
                CommandSpec {
                    program: "docker".to_string(),
                    args,
                    env: Vec::new(),
                },
                Duration::from_secs(12),
            )
            .await
            .context("docker inspect")?;

        if out.status != 0 {
            return Err(anyhow::anyhow!(
                "docker inspect failed status={} stderr={}",
                out.status,
                out.stderr
            ));
        }

        for line in out.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let labels = parse_labels_json_line(line)?;

            let Some(project) = labels.get("com.docker.compose.project").cloned() else {
                continue;
            };

            let service = labels
                .get("com.docker.compose.service")
                .cloned()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "unknown".to_string());

            let config_files_raw = labels
                .get("com.docker.compose.project.config_files")
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());

            by_project
                .entry(project)
                .or_default()
                .push(ObservedComposeContainer {
                    service,
                    config_files_raw,
                });
        }
    }

    Ok(by_project)
}

#[derive(Clone, Debug)]
struct ResolvedProjectComposeFiles {
    compose_files: Vec<String>,
    warning: Option<String>,
    details: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct InvalidProjectComposeFiles {
    reason: String,
    details: Option<serde_json::Value>,
}

fn is_subsequence(sub: &[String], sup: &[String]) -> bool {
    let mut i = 0usize;
    for item in sup {
        if i < sub.len() && sub[i] == *item {
            i += 1;
        }
    }
    i == sub.len()
}

fn variants_details_json(
    variants: &BTreeMap<Vec<String>, BTreeSet<String>>,
    selected: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut entries: Vec<(&Vec<String>, &BTreeSet<String>)> = variants.iter().collect();
    // Keep output deterministic and compact: shortest-first, then lexicographic.
    entries.sort_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

    let variants_json = entries
        .into_iter()
        .map(|(files, services)| {
            serde_json::json!({
                "configFiles": files,
                "services": services.iter().cloned().collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "kind": "config_files_variants",
        "variants": variants_json,
        "selected": selected,
    })
}

fn validate_image_only_override(
    path: &str,
    yaml: &str,
    allowed_services: &BTreeSet<String>,
) -> Result<(), String> {
    use serde_yaml_ng::Value;

    let root: Value = serde_yaml_ng::from_str(yaml).map_err(|e| format!("invalid yaml: {e}"))?;
    let Some(map) = root.as_mapping() else {
        return Err("root must be a mapping".to_string());
    };

    for (k, _v) in map {
        let Some(key) = k.as_str() else {
            return Err("root keys must be strings".to_string());
        };
        if key != "services" {
            return Err(format!("unsupported root key: {key}"));
        }
    }

    let services_key = Value::String("services".to_string());
    let Some(services_val) = map.get(&services_key) else {
        // No services section: treat as a no-op override.
        return Ok(());
    };
    let Some(services_map) = services_val.as_mapping() else {
        return Err("'services' must be a mapping".to_string());
    };

    for (svc_name_key, svc_val) in services_map {
        let Some(svc_name) = svc_name_key.as_str() else {
            return Err("service names must be strings".to_string());
        };
        if !allowed_services.contains(svc_name) {
            return Err(format!(
                "service '{svc_name}' not in allowed services for this variant"
            ));
        }

        let Some(svc_map) = svc_val.as_mapping() else {
            return Err(format!("service '{svc_name}' must be a mapping"));
        };

        for (k, _v) in svc_map {
            let Some(key) = k.as_str() else {
                return Err(format!("service '{svc_name}' keys must be strings"));
            };
            if key != "image" {
                return Err(format!("service '{svc_name}' has unsupported key: {key}"));
            }
        }

        let image_key = Value::String("image".to_string());
        let Some(image_val) = svc_map.get(&image_key) else {
            return Err(format!("service '{svc_name}' missing required key: image"));
        };
        let Some(image_str) = image_val.as_str() else {
            return Err(format!("service '{svc_name}'.image must be a string"));
        };
        if image_str.trim().is_empty() {
            return Err(format!("service '{svc_name}'.image must be non-empty"));
        }
    }

    let _ = path;
    Ok(())
}

async fn resolve_project_compose_files(
    project: &str,
    observed: &[ObservedComposeContainer],
) -> Result<ResolvedProjectComposeFiles, InvalidProjectComposeFiles> {
    // Group by the normalized config_files (paths + order) and collect services that reported it.
    let mut variants = BTreeMap::<Vec<String>, BTreeSet<String>>::new();

    for c in observed {
        let Some(raw) = c.config_files_raw.as_deref() else {
            continue;
        };
        let files = match normalize_config_files(raw) {
            Ok(v) => v,
            Err(NormalizeConfigFilesError::RelativePathRejected) => {
                return Err(InvalidProjectComposeFiles {
                    reason: "config_files_relative_path_rejected".to_string(),
                    details: Some(serde_json::json!({
                        "kind": "config_files_invalid",
                        "service": c.service,
                        "raw": raw,
                    })),
                });
            }
            Err(NormalizeConfigFilesError::Empty) => {
                return Err(InvalidProjectComposeFiles {
                    reason: "config_files_empty".to_string(),
                    details: Some(serde_json::json!({
                        "kind": "config_files_invalid",
                        "service": c.service,
                        "raw": raw,
                    })),
                });
            }
        };

        variants.entry(files).or_default().insert(c.service.clone());
    }

    if variants.is_empty() {
        return Err(InvalidProjectComposeFiles {
            reason: "config_files_missing".to_string(),
            details: None,
        });
    }

    if variants.len() == 1 {
        let (compose_files, _) = variants.iter().next().expect("len=1");
        return Ok(ResolvedProjectComposeFiles {
            compose_files: compose_files.clone(),
            warning: None,
            details: None,
        });
    }

    let keys = variants.keys().cloned().collect::<Vec<_>>();
    let mut candidates = Vec::<Vec<String>>::new();
    for k in &keys {
        if keys.iter().all(|other| is_subsequence(other, k)) {
            candidates.push(k.clone());
        }
    }

    if candidates.is_empty() {
        // No canonical superset across variants. This commonly happens when Dockrev-generated
        // override files (e.g. /tmp/dockrev-override-*.yml) differ per service. If *all* extra
        // files are unreadable, fall back to common compose files and emit a warning instead of
        // marking the project invalid.
        let shortest = variants
            .keys()
            .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
            .expect("variants non-empty");
        let common_files = shortest
            .iter()
            .filter(|p| variants.keys().all(|v| v.contains(*p)))
            .cloned()
            .collect::<Vec<_>>();
        if common_files.is_empty() {
            let details = variants_details_json(&variants, None);
            return Err(InvalidProjectComposeFiles {
                reason: format!(
                    "config_files_conflict: failed to compute common compose files across variants (project={project})"
                ),
                details: Some(details),
            });
        }

        let mut extra_set = BTreeSet::<String>::new();
        for files in variants.keys() {
            for p in files {
                if !common_files.contains(p) {
                    extra_set.insert(p.clone());
                }
            }
        }
        let extra_files = extra_set.into_iter().collect::<Vec<_>>();

        let mut unreadable = Vec::<serde_json::Value>::new();
        let mut readable = Vec::<String>::new();
        for path in &extra_files {
            match tokio::fs::read_to_string(path).await {
                Ok(_) => readable.push(path.clone()),
                Err(e) => unreadable.push(serde_json::json!({
                    "path": path,
                    "error": e.to_string(),
                })),
            }
        }

        if readable.is_empty() {
            let selected = serde_json::json!({
                "mode": "common_fallback_no_superset_all_unreadable",
                "configFiles": common_files.clone(),
                "extraFiles": extra_files.clone(),
                "unreadableExtra": unreadable,
            });
            let details = variants_details_json(&variants, Some(selected));
            return Ok(ResolvedProjectComposeFiles {
                compose_files: common_files.clone(),
                warning: Some(
                    "warning:config_files_conflict_fallback_common: no canonical superset found; all extra files unreadable; using common compose files. Hint: mount the override path into dockrev (same absolute path, read-only), or set DOCKREV_SUPERVISOR_STATE_PATH to a mounted directory".to_string(),
                ),
                details: Some(details),
            });
        }

        let selected = serde_json::json!({
            "mode": "invalid_no_superset",
            "readableExtra": readable,
            "unreadableExtra": unreadable,
        });
        let details = variants_details_json(&variants, Some(selected));
        return Err(InvalidProjectComposeFiles {
            reason: format!(
                "config_files_conflict: multiple distinct config_files variants and no canonical superset found (project={project})"
            ),
            details: Some(details),
        });
    }

    candidates.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    let superset = candidates[0].clone();

    let common_files = superset
        .iter()
        .filter(|p| variants.keys().all(|v| v.contains(*p)))
        .cloned()
        .collect::<Vec<_>>();
    if common_files.is_empty() {
        let details = variants_details_json(&variants, None);
        return Err(InvalidProjectComposeFiles {
            reason: format!(
                "config_files_conflict: failed to compute common compose files across variants (project={project})"
            ),
            details: Some(details),
        });
    }

    let extra_files = superset
        .iter()
        .filter(|p| !common_files.contains(*p))
        .cloned()
        .collect::<Vec<_>>();

    let superset_services = variants
        .get(&superset)
        .cloned()
        .unwrap_or_else(BTreeSet::new);

    // Validate all extra files before accepting the superset canonical list.
    for path in &extra_files {
        let contents = match tokio::fs::read_to_string(path).await {
            Ok(v) => v,
            Err(e) => {
                let selected = serde_json::json!({
                    "mode": "common_fallback",
                    "configFiles": common_files.clone(),
                    "extraFiles": extra_files.clone(),
                    "services": superset_services.iter().cloned().collect::<Vec<_>>(),
                    "unreadableExtra": { "path": path, "error": e.to_string() },
                });
                let details = variants_details_json(&variants, Some(selected));
                return Ok(ResolvedProjectComposeFiles {
                    compose_files: common_files.clone(),
                    warning: Some(format!(
                        "warning:config_files_extra_unreadable_fallback_common: extra compose file unreadable: {path} ({e}); using common compose files. Hint: mount the override path into dockrev (same absolute path, read-only), or set DOCKREV_SUPERVISOR_STATE_PATH to a mounted directory"
                    )),
                    details: Some(details),
                });
            }
        };

        if let Err(err) = validate_image_only_override(path, &contents, &superset_services) {
            let err_msg = err.clone();
            if err.contains("not in allowed services for this variant") {
                // The file is image-only, but it touches services outside the variant that
                // reported the superset. This can happen after self-upgrade or stack updates
                // when extra override files get propagated to unrelated services. In this case,
                // do not accept the superset as canonical; fall back to common files and warn.
                let selected = serde_json::json!({
                    "mode": "common_fallback_unsafe_extra",
                    "configFiles": common_files.clone(),
                    "extraFiles": extra_files.clone(),
                    "services": superset_services.iter().cloned().collect::<Vec<_>>(),
                    "unsafeExtra": { "path": path, "error": err_msg },
                });
                let details = variants_details_json(&variants, Some(selected));
                return Ok(ResolvedProjectComposeFiles {
                    compose_files: common_files.clone(),
                    warning: Some(format!(
                        "warning:config_files_unsafe_extra_fallback_common: extra compose file unsafe for selected superset: {path} ({err}); using common compose files"
                    )),
                    details: Some(details),
                });
            }

            let selected = serde_json::json!({
                "mode": "superset_rejected",
                "configFiles": superset.clone(),
                "extraFiles": extra_files.clone(),
                "services": superset_services.iter().cloned().collect::<Vec<_>>(),
                "unsafeExtra": { "path": path, "error": err_msg },
            });
            let details = variants_details_json(&variants, Some(selected));
            return Err(InvalidProjectComposeFiles {
                reason: format!(
                    "config_files_conflict: superset includes an unsafe override file: {path} ({err})"
                ),
                details: Some(details),
            });
        }
    }

    let selected = serde_json::json!({
        "mode": "superset",
        "configFiles": superset.clone(),
        "extraFiles": extra_files.clone(),
        "services": superset_services.iter().cloned().collect::<Vec<_>>(),
    });
    let details = variants_details_json(&variants, Some(selected));

    Ok(ResolvedProjectComposeFiles {
        compose_files: superset.clone(),
        warning: Some(format!(
            "warning:config_files_superset_selected: selected superset config_files; extra_files=[{}]; services=[{}]",
            extra_files.join(","),
            superset_services
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        )),
        details: Some(details),
    })
}

pub fn spawn_task(state: std::sync::Arc<AppState>) {
    let interval = state.config.discovery_interval_seconds;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval));
        loop {
            ticker.tick().await;
            if let Err(e) = run_scan(state.as_ref()).await {
                tracing::warn!(error = %e, "discovery scan failed");
            }
        }
    });
}

pub async fn run_scan(state: &AppState) -> anyhow::Result<TriggerDiscoveryScanResponse> {
    let _scan_guard = DISCOVERY_SCAN_LOCK.lock().await;
    let started_at = now_rfc3339()?;
    let start = std::time::Instant::now();
    let now = started_at.clone();

    let projects = list_compose_projects_from_docker(state).await?;

    let mut seen_projects = Vec::<String>::new();
    let mut actions = Vec::<DiscoveryAction>::new();

    let mut summary = DiscoveryScanSummary {
        projects_seen: 0,
        stacks_created: 0,
        stacks_updated: 0,
        stacks_skipped: 0,
        stacks_failed: 0,
        stacks_marked_missing: 0,
    };

    for (project, observed) in &projects {
        seen_projects.push(project.clone());
        summary.projects_seen += 1;

        let resolved = match resolve_project_compose_files(project, observed).await {
            Ok(v) => v,
            Err(e) => {
                summary.stacks_failed += 1;
                state
                    .db
                    .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                        project: project.clone(),
                        stack_id: None,
                        status: "invalid".to_string(),
                        last_seen_at: Some(now.clone()),
                        last_scan_at: now.clone(),
                        last_error: Some(e.reason.clone()),
                        last_config_files: None,
                        unarchive_if_active: false,
                    })
                    .await?;
                actions.push(DiscoveryAction {
                    project: project.clone(),
                    action: DiscoveryActionKind::Failed,
                    stack_id: None,
                    reason: Some(e.reason),
                    details: e.details,
                });
                continue;
            }
        };

        let warning = resolved.warning.clone();
        let action_details = resolved.details.clone();
        let config_files = resolved.compose_files;

        let mut merged: BTreeMap<String, compose::ServiceFromCompose> = BTreeMap::new();
        let mut failure_reason: Option<String> = None;

        for path in &config_files {
            let contents = match tokio::fs::read_to_string(path).await {
                Ok(v) => v,
                Err(e) => {
                    failure_reason = Some(format!(
                        "compose_file_unreadable: {path} ({e}) (mount missing? ensure host path is mounted read-only at the same absolute path)"
                    ));
                    break;
                }
            };

            match compose::parse_services(&contents) {
                Ok(parsed) => {
                    merged = compose::merge_services(merged, parsed);
                }
                Err(e) => {
                    failure_reason = Some(format!("compose_file_invalid: {path} ({e})"));
                    break;
                }
            }
        }

        if failure_reason.is_none() && merged.is_empty() {
            failure_reason = Some("compose_no_services".to_string());
        }

        if let Some(msg) = failure_reason {
            summary.stacks_failed += 1;
            state
                .db
                .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                    project: project.clone(),
                    stack_id: None,
                    status: "invalid".to_string(),
                    last_seen_at: Some(now.clone()),
                    last_scan_at: now.clone(),
                    last_error: Some(msg.clone()),
                    last_config_files: Some(config_files.clone()),
                    unarchive_if_active: false,
                })
                .await?;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Failed,
                stack_id: None,
                reason: Some(msg),
                details: action_details,
            });
            continue;
        }

        let svc_specs: Vec<ComposeServiceSpec> = merged
            .values()
            .map(|svc| ComposeServiceSpec {
                name: svc.name.clone(),
                image_ref: svc.image_ref.clone(),
                image_tag: svc.image_tag.clone(),
            })
            .collect();

        let existing = state.db.get_discovered_compose_project(project).await?;
        let mut stack_id = existing.as_ref().and_then(|r| r.stack_id.clone());
        let mut stack_exists = false;

        if let Some(id) = stack_id.as_deref() {
            stack_exists = state.db.get_stack(id).await?.is_some();
        }

        if stack_id.is_none() || !stack_exists {
            let new_stack_id = ids::new_stack_id();
            let stack = crate::api::types::StackRecord {
                id: new_stack_id.clone(),
                name: project.clone(),
                archived: false,
                compose: crate::api::types::ComposeConfig {
                    kind: "path".to_string(),
                    compose_files: config_files.clone(),
                    env_file: None,
                },
                backup: crate::api::types::StackBackupConfig::default(),
                services: Vec::new(),
            };

            let mut seeds = Vec::new();
            for svc in merged.values() {
                seeds.push(crate::api::types::ServiceSeed {
                    id: ids::new_service_id(),
                    name: svc.name.clone(),
                    image_ref: svc.image_ref.clone(),
                    image_tag: svc.image_tag.clone(),
                    auto_rollback: true,
                    backup_bind_paths: BTreeMap::new(),
                    backup_volume_names: BTreeMap::new(),
                });
            }

            state.db.insert_stack(&stack, &seeds, &now).await?;
            stack_id = Some(new_stack_id.clone());
            summary.stacks_created += 1;
            state
                .db
                .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                    project: project.clone(),
                    stack_id: stack_id.clone(),
                    status: "active".to_string(),
                    last_seen_at: Some(now.clone()),
                    last_scan_at: now.clone(),
                    last_error: warning.clone(),
                    last_config_files: Some(config_files.clone()),
                    unarchive_if_active: true,
                })
                .await?;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Created,
                stack_id: stack_id.clone(),
                reason: warning.clone(),
                details: action_details,
            });
            continue;
        }

        let stack_id = stack_id.expect("stack id missing after create path");
        let stack = state
            .db
            .get_stack(&stack_id)
            .await?
            .context("stack missing")?;
        let needs_update = stack.compose.compose_files != config_files;
        let needs_service_sync = !stack_services_match_specs(&stack, &svc_specs);
        let needs_sync = needs_update || needs_service_sync;

        if needs_sync {
            state
                .db
                .sync_stack_from_compose(&stack_id, &config_files, &svc_specs, &now)
                .await?;
            summary.stacks_updated += 1;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Updated,
                stack_id: Some(stack_id.clone()),
                reason: warning.clone(),
                details: action_details.clone(),
            });
        } else {
            summary.stacks_skipped += 1;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Skipped,
                stack_id: Some(stack_id.clone()),
                reason: warning.clone(),
                details: action_details.clone(),
            });
        }

        state
            .db
            .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                project: project.clone(),
                stack_id: Some(stack_id.clone()),
                status: "active".to_string(),
                last_seen_at: Some(now.clone()),
                last_scan_at: now.clone(),
                last_error: warning,
                last_config_files: Some(config_files),
                unarchive_if_active: true,
            })
            .await?;
    }

    let newly_missing = state
        .db
        .mark_discovered_compose_projects_missing_except(&seen_projects, &now)
        .await?;
    summary.stacks_marked_missing = newly_missing.len() as u32;

    for project in newly_missing {
        actions.push(DiscoveryAction {
            project,
            action: DiscoveryActionKind::MarkedMissing,
            stack_id: None,
            reason: None,
            details: None,
        });
    }

    if actions.len() > state.config.discovery_max_actions as usize {
        actions.truncate(state.config.discovery_max_actions as usize);
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    Ok(TriggerDiscoveryScanResponse {
        started_at,
        duration_ms,
        summary,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dockrev-discovery-test-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_labels_json_line_null_is_empty() {
        let out = parse_labels_json_line("null").unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_labels_json_line_non_object_is_empty() {
        let out = parse_labels_json_line("[]").unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_labels_json_line_object_extracts_strings() {
        let out = parse_labels_json_line(r#"{"a":"b","n":123}"#).unwrap();
        assert_eq!(out.get("a").map(String::as_str), Some("b"));
        assert_eq!(out.get("n"), None);
    }

    #[test]
    fn stack_services_match_specs_detects_changes() {
        let stack = crate::api::types::StackRecord {
            id: "stk_1".to_string(),
            name: "demo".to_string(),
            archived: false,
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![crate::api::types::Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: crate::api::types::ComposeRef {
                    reference: "ghcr.io/acme/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                    resolved_tag: None,
                    resolved_tags: None,
                },
                candidate: None,
                ignore: None,
                settings: crate::api::types::ServiceSettings {
                    auto_rollback: true,
                    backup_targets: crate::api::types::BackupTargetOverrides {
                        bind_paths: BTreeMap::new(),
                        volume_names: BTreeMap::new(),
                    },
                },
                archived: None,
            }],
        };

        let specs_ok = vec![ComposeServiceSpec {
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web:1.0".to_string(),
            image_tag: "1.0".to_string(),
        }];
        assert!(stack_services_match_specs(&stack, &specs_ok));

        let specs_changed = vec![ComposeServiceSpec {
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web:1.1".to_string(),
            image_tag: "1.1".to_string(),
        }];
        assert!(!stack_services_match_specs(&stack, &specs_changed));
    }

    #[test]
    fn normalize_config_files_splits_dedupes_preserves_order() {
        let raw = " /a.yml,\n/b.yml\r\n/a.yml\n\n/c.yml ";
        let out = normalize_config_files(raw).unwrap();
        assert_eq!(out, vec!["/a.yml", "/b.yml", "/c.yml"]);
    }

    #[test]
    fn normalize_config_files_rejects_relative() {
        let raw = "compose.yml,/abs.yml";
        assert!(matches!(
            normalize_config_files(raw),
            Err(NormalizeConfigFilesError::RelativePathRejected)
        ));
    }

    #[test]
    fn is_subsequence_preserves_order_semantics() {
        assert!(is_subsequence(&[] as &[String], &[] as &[String]));
        assert!(is_subsequence(&["/a".to_string()], &["/a".to_string()]));
        assert!(is_subsequence(
            &["/a".to_string(), "/c".to_string()],
            &["/a".to_string(), "/b".to_string(), "/c".to_string()]
        ));
        assert!(!is_subsequence(
            &["/b".to_string(), "/a".to_string()],
            &["/a".to_string(), "/b".to_string()]
        ));
    }

    #[tokio::test]
    async fn resolve_project_compose_files_superset_is_warning_and_selects_superset() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let override_yml = dir.join("self-upgrade.override.yml");

        // The override file must be readable and "image-only" for the superset to be accepted.
        std::fs::write(
            &override_yml,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
        )
        .unwrap();

        let base_s = base.display().to_string();
        let override_s = override_yml.display().to_string();

        let observed = vec![
            ObservedComposeContainer {
                service: "dockrev".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev-supervisor".to_string(),
                config_files_raw: Some(base_s.clone()),
            },
        ];

        let resolved = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap();
        assert_eq!(resolved.compose_files, vec![base_s, override_s]);
        assert!(
            resolved
                .warning
                .as_deref()
                .is_some_and(|w| w.contains("warning:config_files_superset_selected"))
        );
        assert!(resolved.details.is_some());
    }

    #[tokio::test]
    async fn resolve_project_compose_files_dedupes_duplicate_paths_in_labels() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let override_yml = dir.join("self-upgrade.override.yml");

        std::fs::write(
            &override_yml,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
        )
        .unwrap();

        let base_s = base.display().to_string();
        let override_s = override_yml.display().to_string();

        let observed = vec![
            ObservedComposeContainer {
                service: "dockrev".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s},{override_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev-supervisor".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s}")),
            },
        ];

        let resolved = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap();
        assert_eq!(resolved.compose_files, vec![base_s, override_s]);
        assert!(resolved.warning.is_none());
    }

    #[tokio::test]
    async fn resolve_project_compose_files_non_subset_conflict_is_invalid_with_details() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let a = dir.join("a.yml");
        let b = dir.join("b.yml");

        // Ensure extra files are readable, otherwise the resolver will fall back to common files.
        std::fs::write(&a, "# a\n").unwrap();
        std::fs::write(&b, "# b\n").unwrap();

        let base_s = base.display().to_string();
        let a_s = a.display().to_string();
        let b_s = b.display().to_string();

        let observed = vec![
            ObservedComposeContainer {
                service: "svc-a".to_string(),
                config_files_raw: Some(format!("{base_s},{a_s}")),
            },
            ObservedComposeContainer {
                service: "svc-b".to_string(),
                config_files_raw: Some(format!("{base_s},{b_s}")),
            },
        ];

        let err = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap_err();
        assert!(err.reason.contains("config_files_conflict"));
        assert!(err.details.is_some());
        assert_eq!(
            err.details
                .as_ref()
                .and_then(|d| d.get("variants"))
                .and_then(|v| v.as_array())
                .map(|v| v.len()),
            Some(2)
        );
    }

    #[tokio::test]
    async fn resolve_project_compose_files_no_superset_all_extras_unreadable_falls_back_to_common()
    {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let a = dir.join("missing-a.yml");
        let b = dir.join("missing-b.yml");

        let base_s = base.display().to_string();
        let a_s = a.display().to_string();
        let b_s = b.display().to_string();

        // No canonical superset: two distinct variants with different extra files.
        // Since all extra files are unreadable, fall back to common files (base only) with a warning.
        let observed = vec![
            ObservedComposeContainer {
                service: "svc-a".to_string(),
                config_files_raw: Some(format!("{base_s},{a_s}")),
            },
            ObservedComposeContainer {
                service: "svc-b".to_string(),
                config_files_raw: Some(format!("{base_s},{b_s}")),
            },
        ];

        let resolved = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap();
        assert_eq!(resolved.compose_files, vec![base_s]);
        assert!(
            resolved
                .warning
                .as_deref()
                .is_some_and(|w| { w.contains("warning:config_files_conflict_fallback_common") })
        );
        assert!(resolved.details.is_some());
    }

    #[tokio::test]
    async fn resolve_project_compose_files_superset_unsafe_extra_falls_back_to_common() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let override_yml = dir.join("self-upgrade.override.yml");
        let tmp_override = dir.join("dockrev-override.yml");

        std::fs::write(
            &override_yml,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
        )
        .unwrap();

        let base_s = base.display().to_string();
        let override_s = override_yml.display().to_string();
        let tmp_s = tmp_override.display().to_string();

        // Superset candidate is reported by "dozzle", but the extra self-upgrade override touches
        // a different service ("dockrev"). Treat as unsafe for the superset and fall back to common.
        let observed = vec![
            ObservedComposeContainer {
                service: "dozzle".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s},{tmp_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev-supervisor".to_string(),
                config_files_raw: Some(base_s.clone()),
            },
        ];

        let resolved = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap();
        assert_eq!(resolved.compose_files, vec![base_s]);
        assert!(
            resolved.warning.as_deref().is_some_and(|w| {
                w.contains("warning:config_files_unsafe_extra_fallback_common")
            })
        );
        assert!(resolved.details.is_some());
    }

    #[tokio::test]
    async fn resolve_project_compose_files_unreadable_extra_falls_back_to_common() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let override_yml = dir.join("missing.override.yml");

        let base_s = base.display().to_string();
        let override_s = override_yml.display().to_string();

        let observed = vec![
            ObservedComposeContainer {
                service: "dockrev".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev-supervisor".to_string(),
                config_files_raw: Some(base_s.clone()),
            },
        ];

        let resolved = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap();
        assert_eq!(resolved.compose_files, vec![base_s]);
        assert!(
            resolved.warning.as_deref().is_some_and(
                |w| w.contains("warning:config_files_extra_unreadable_fallback_common")
            )
        );
        assert!(resolved.details.is_some());
    }

    #[tokio::test]
    async fn resolve_project_compose_files_unsafe_override_is_invalid() {
        let dir = make_temp_dir();
        let base = dir.join("docker-compose.yml");
        let override_yml = dir.join("unsafe.override.yml");

        std::fs::write(
            &override_yml,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n    environment:\n      A: B\n",
        )
        .unwrap();

        let base_s = base.display().to_string();
        let override_s = override_yml.display().to_string();

        let observed = vec![
            ObservedComposeContainer {
                service: "dockrev".to_string(),
                config_files_raw: Some(format!("{base_s},{override_s}")),
            },
            ObservedComposeContainer {
                service: "dockrev-supervisor".to_string(),
                config_files_raw: Some(base_s.clone()),
            },
        ];

        let err = resolve_project_compose_files("dockrev", &observed)
            .await
            .unwrap_err();
        assert!(err.reason.contains("unsafe override"));
        assert!(err.details.is_some());
    }
}
