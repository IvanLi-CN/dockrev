use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use anyhow::Context as _;

use crate::{
    api::types::{
        DiscoveryAction, DiscoveryActionKind, DiscoveryScanSummary, JobLogLine, JobProgress,
        TriggerDiscoveryScanResponse,
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

fn progress_percent(current: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / total).min(100)
}

fn make_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent: progress_percent(current, total),
        planned_current: Some(current),
        planned_total: Some(total),
        planned_percent: Some(progress_percent(current, total)),
        current_target,
        updated_at,
    }
}

async fn persist_job_progress(
    state: &AppState,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let evt = serde_json::json!({
        "type": "job_progress",
        "jobId": job_id,
        "ts": progress.updated_at,
        "phase": progress.phase,
        "message": progress.message,
        "current": progress.current,
        "total": progress.total,
        "percent": progress.percent,
        "plannedCurrent": progress.planned_current,
        "plannedTotal": progress.planned_total,
        "plannedPercent": progress.planned_percent,
        "currentTarget": progress.current_target,
        "updatedAt": progress.updated_at,
    });

    state
        .db
        .insert_job_log(
            job_id,
            &JobLogLine {
                ts: progress.updated_at.clone(),
                level: "event".to_string(),
                msg: evt.to_string(),
            },
        )
        .await?;
    Ok(())
}

async fn emit_job_progress_best_effort(
    state: &AppState,
    progress_job_id: Option<&str>,
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
) {
    let Some(job_id) = progress_job_id else {
        return;
    };

    let updated_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    let progress = make_job_progress(phase, message, current, total, current_target, updated_at);
    if let Err(e) = persist_job_progress(state, job_id, &progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist discovery progress");
    }
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
                (
                    svc.image.reference.clone(),
                    svc.image.tag.clone(),
                    svc.homepage.clone(),
                    svc.update_guard.clone(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let expected = specs
        .iter()
        .map(|svc| {
            (
                svc.name.clone(),
                (
                    svc.image_ref.clone(),
                    svc.image_tag.clone(),
                    svc.homepage.clone(),
                    svc.update_guard.clone(),
                ),
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

fn is_dockrev_generated_override_path(
    path: &str,
    project: &str,
    expected_self_upgrade_override: Option<&Path>,
) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }

    let path = Path::new(trimmed);
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    // Dockrev currently generates overrides in two places:
    // - updater temp files under the host temp dir: dockrev-override-<project>-<ulid>.yml
    // - supervisor self-upgrade overrides next to the configured state path
    if file_name == "self-upgrade.override.yml" && expected_self_upgrade_override == Some(path) {
        return true;
    }

    is_expected_dockrev_temp_override_path(path, file_name, project)
}

fn is_expected_dockrev_temp_override_path(path: &Path, file_name: &str, project: &str) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if parent != std::env::temp_dir().as_path() {
        return false;
    }

    let prefix = format!("dockrev-override-{}-", sanitize_project_name(project));
    let Some(ulid_part) = file_name
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(".yml"))
    else {
        return false;
    };

    ulid::Ulid::from_string(ulid_part).is_ok()
}

fn sanitize_project_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "dockrev".to_string()
    } else {
        out
    }
}

fn configured_self_upgrade_override_path() -> Option<PathBuf> {
    let state_path = std::env::var_os("DOCKREV_SUPERVISOR_STATE_PATH")?;
    if state_path.is_empty() {
        return None;
    }

    expected_self_upgrade_override_path(Path::new(&state_path))
}

fn expected_self_upgrade_override_path(state_path: &Path) -> Option<PathBuf> {
    if !state_path.is_absolute() {
        return None;
    }

    Some(state_path.parent()?.join("self-upgrade.override.yml"))
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
    let expected_self_upgrade_override = configured_self_upgrade_override_path();
    resolve_project_compose_files_with_expected_override(
        project,
        observed,
        expected_self_upgrade_override.as_deref(),
    )
    .await
}

async fn resolve_project_compose_files_with_expected_override(
    project: &str,
    observed: &[ObservedComposeContainer],
    expected_self_upgrade_override: Option<&Path>,
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
        let (compose_files, services) = variants.iter().next().expect("len=1");
        let mut readable_files = Vec::<String>::new();
        let mut unreadable_dockrev_generated = Vec::<serde_json::Value>::new();
        let mut unreadable_other = Vec::<serde_json::Value>::new();
        for path in compose_files {
            match tokio::fs::read_to_string(path).await {
                Ok(_) => readable_files.push(path.clone()),
                Err(e) => {
                    let entry = serde_json::json!({
                        "path": path,
                        "error": e.to_string(),
                    });
                    if is_dockrev_generated_override_path(
                        path,
                        project,
                        expected_self_upgrade_override,
                    ) {
                        unreadable_dockrev_generated.push(entry);
                    } else {
                        unreadable_other.push(entry);
                    }
                }
            }
        }

        if unreadable_dockrev_generated.is_empty() && unreadable_other.is_empty() {
            return Ok(ResolvedProjectComposeFiles {
                compose_files: compose_files.clone(),
                warning: None,
                details: None,
            });
        }

        if unreadable_other.is_empty()
            && !unreadable_dockrev_generated.is_empty()
            && !readable_files.is_empty()
        {
            let selected = serde_json::json!({
                "mode": "single_variant_dockrev_generated_override_fallback",
                "configFiles": readable_files.clone(),
                "services": services.iter().cloned().collect::<Vec<_>>(),
                "ignoredExtra": unreadable_dockrev_generated,
            });
            let details = variants_details_json(&variants, Some(selected));
            return Ok(ResolvedProjectComposeFiles {
                compose_files: readable_files,
                warning: Some(
                    "warning:config_files_single_variant_dockrev_generated_override_fallback: unreadable dockrev-generated override ignored; using readable compose files. Hint: mount the override path into dockrev (same absolute path, read-only), and set DOCKREV_SUPERVISOR_STATE_PATH to the same mounted absolute path in both dockrev and supervisor".to_string(),
                ),
                details: Some(details),
            });
        }

        let unreadable = unreadable_other
            .first()
            .or_else(|| unreadable_dockrev_generated.first())
            .cloned();
        let reason = unreadable
            .as_ref()
            .and_then(|entry| {
                let path = entry.get("path")?.as_str()?;
                let error = entry.get("error")?.as_str()?;
                Some(format!(
                    "compose_file_unreadable: {path} ({error}) (mount missing? ensure host path is mounted read-only at the same absolute path)"
                ))
            })
            .unwrap_or_else(|| "compose_file_unreadable".to_string());
        let selected = serde_json::json!({
            "mode": "single_variant_invalid",
            "configFiles": compose_files.clone(),
            "services": services.iter().cloned().collect::<Vec<_>>(),
            "unreadable": unreadable_other,
            "ignoredDockrevGenerated": unreadable_dockrev_generated,
        });
        let details = variants_details_json(&variants, Some(selected));
        return Err(InvalidProjectComposeFiles {
            reason,
            details: Some(details),
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
                    "warning:config_files_conflict_fallback_common: no canonical superset found; all extra files unreadable; using common compose files. Hint: mount the override path into dockrev (same absolute path, read-only), and set DOCKREV_SUPERVISOR_STATE_PATH to the same mounted absolute path in both dockrev and supervisor".to_string(),
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
                        "warning:config_files_extra_unreadable_fallback_common: extra compose file unreadable: {path} ({e}); using common compose files. Hint: mount the override path into dockrev (same absolute path, read-only), and set DOCKREV_SUPERVISOR_STATE_PATH to the same mounted absolute path in both dockrev and supervisor"
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
    run_scan_inner(state, None).await
}

pub async fn run_scan_for_job(
    state: &AppState,
    job_id: &str,
) -> anyhow::Result<TriggerDiscoveryScanResponse> {
    run_scan_inner(state, Some(job_id)).await
}

async fn run_scan_inner(
    state: &AppState,
    progress_job_id: Option<&str>,
) -> anyhow::Result<TriggerDiscoveryScanResponse> {
    let _scan_guard = DISCOVERY_SCAN_LOCK.lock().await;
    let started_at = now_rfc3339()?;
    let start = std::time::Instant::now();
    let now = started_at.clone();

    // Discovery setup can involve docker inspect and compose-file inference where total work
    // is not yet known. Expose this as indeterminate progress (total=0).
    emit_job_progress_best_effort(
        state,
        progress_job_id,
        "prepare",
        "discovering compose projects".to_string(),
        0,
        0,
        None,
    )
    .await;

    let projects = list_compose_projects_from_docker(state).await?;
    let total_projects = projects.len() as u32;

    emit_job_progress_best_effort(
        state,
        progress_job_id,
        "scan",
        format!("scanning projects (0/{total_projects})"),
        0,
        total_projects,
        None,
    )
    .await;

    let mut seen_projects = Vec::<String>::new();
    let mut actions = Vec::<DiscoveryAction>::new();
    let mut projects_processed = 0u32;

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
        emit_job_progress_best_effort(
            state,
            progress_job_id,
            "scan",
            format!("scanning project {project}"),
            projects_processed,
            total_projects,
            Some(project.clone()),
        )
        .await;

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
                projects_processed = projects_processed.saturating_add(1);
                emit_job_progress_best_effort(
                    state,
                    progress_job_id,
                    "scan",
                    format!("scanned projects ({projects_processed}/{total_projects})"),
                    projects_processed,
                    total_projects,
                    Some(project.clone()),
                )
                .await;
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
            projects_processed = projects_processed.saturating_add(1);
            emit_job_progress_best_effort(
                state,
                progress_job_id,
                "scan",
                format!("scanned projects ({projects_processed}/{total_projects})"),
                projects_processed,
                total_projects,
                Some(project.clone()),
            )
            .await;
            continue;
        }

        let svc_specs: Vec<ComposeServiceSpec> = merged
            .values()
            .map(|svc| ComposeServiceSpec {
                name: svc.name.clone(),
                image_ref: svc.image_ref.clone(),
                image_tag: svc.image_tag.clone(),
                homepage: svc.homepage.clone(),
                update_guard: svc.update_guard.clone(),
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
                    homepage: svc.homepage.clone(),
                    update_guard: svc.update_guard.clone(),
                    auto_rollback: true,
                    backup_bind_paths: BTreeMap::new(),
                    backup_volume_names: BTreeMap::new(),
                });
            }

            state.db.insert_stack(&stack, &seeds, &now).await?;
            stack_id = Some(new_stack_id.clone());
            summary.stacks_created += 1;
            if let Err(err) = crate::repo_link_backfill::enqueue_stack_backfill_if_needed(
                state,
                &new_stack_id,
                "discovery_create",
            )
            .await
            {
                tracing::warn!(
                    error = %err,
                    stack_id = %new_stack_id,
                    "failed to enqueue repo link backfill after discovery create"
                );
            }
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
            projects_processed = projects_processed.saturating_add(1);
            emit_job_progress_best_effort(
                state,
                progress_job_id,
                "scan",
                format!("scanned projects ({projects_processed}/{total_projects})"),
                projects_processed,
                total_projects,
                Some(project.clone()),
            )
            .await;
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
            if let Err(err) = crate::repo_link_backfill::enqueue_stack_backfill_if_needed(
                state,
                &stack_id,
                "discovery_sync",
            )
            .await
            {
                tracing::warn!(
                    error = %err,
                    stack_id = %stack_id,
                    "failed to enqueue repo link backfill after discovery sync"
                );
            }
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

        projects_processed = projects_processed.saturating_add(1);
        emit_job_progress_best_effort(
            state,
            progress_job_id,
            "scan",
            format!("scanned projects ({projects_processed}/{total_projects})"),
            projects_processed,
            total_projects,
            Some(project.clone()),
        )
        .await;
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

    emit_job_progress_best_effort(
        state,
        progress_job_id,
        "done",
        "discovery scan finished".to_string(),
        projects_processed,
        total_projects,
        None,
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;
    Ok(TriggerDiscoveryScanResponse {
        started_at,
        duration_ms,
        summary,
        actions,
    })
}

#[cfg(test)]
mod tests;
