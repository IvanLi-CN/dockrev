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
struct ProjectLabels {
    config_files_raw: Option<String>,
    working_dir_raw: Option<String>,
}

async fn list_compose_projects_from_docker(
    state: &AppState,
) -> anyhow::Result<BTreeMap<String, ProjectLabels>> {
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

    let mut by_project = BTreeMap::<String, ProjectLabels>::new();

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

            let config_files_raw = labels
                .get("com.docker.compose.project.config_files")
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            let working_dir_raw = labels
                .get("com.docker.compose.project.working_dir")
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());

            let entry = by_project.entry(project).or_insert(ProjectLabels {
                config_files_raw: None,
                working_dir_raw: None,
            });

            if let Some(v) = config_files_raw {
                match &entry.config_files_raw {
                    None => entry.config_files_raw = Some(v),
                    Some(prev) if prev == &v => {}
                    Some(_) => {
                        // conflict marker: keep a sentinel distinct value to signal conflict later
                        entry.config_files_raw = Some("__CONFLICT__".to_string());
                    }
                }
            }

            if let Some(v) = working_dir_raw
                && entry.working_dir_raw.is_none()
            {
                entry.working_dir_raw = Some(v);
            }
        }
    }

    Ok(by_project)
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

    for (project, labels) in &projects {
        seen_projects.push(project.clone());
        summary.projects_seen += 1;

        let config_files_raw = match labels.config_files_raw.as_deref() {
            None => {
                summary.stacks_failed += 1;
                state
                    .db
                    .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                        project: project.clone(),
                        stack_id: None,
                        status: "invalid".to_string(),
                        last_seen_at: Some(now.clone()),
                        last_scan_at: now.clone(),
                        last_error: Some("config_files_missing".to_string()),
                        last_config_files: None,
                        unarchive_if_active: false,
                    })
                    .await?;
                actions.push(DiscoveryAction {
                    project: project.clone(),
                    action: DiscoveryActionKind::Failed,
                    stack_id: None,
                    reason: Some("config_files_missing".to_string()),
                    details: None,
                });
                continue;
            }
            Some("__CONFLICT__") => {
                summary.stacks_failed += 1;
                state
                    .db
                    .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                        project: project.clone(),
                        stack_id: None,
                        status: "invalid".to_string(),
                        last_seen_at: Some(now.clone()),
                        last_scan_at: now.clone(),
                        last_error: Some("config_files_conflict".to_string()),
                        last_config_files: None,
                        unarchive_if_active: false,
                    })
                    .await?;
                actions.push(DiscoveryAction {
                    project: project.clone(),
                    action: DiscoveryActionKind::Failed,
                    stack_id: None,
                    reason: Some("config_files_conflict".to_string()),
                    details: None,
                });
                continue;
            }
            Some(v) => v,
        };

        let config_files = match normalize_config_files(config_files_raw) {
            Ok(v) => v,
            Err(NormalizeConfigFilesError::RelativePathRejected) => {
                summary.stacks_failed += 1;
                state
                    .db
                    .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                        project: project.clone(),
                        stack_id: None,
                        status: "invalid".to_string(),
                        last_seen_at: Some(now.clone()),
                        last_scan_at: now.clone(),
                        last_error: Some("config_files_relative_path_rejected".to_string()),
                        last_config_files: None,
                        unarchive_if_active: false,
                    })
                    .await?;
                actions.push(DiscoveryAction {
                    project: project.clone(),
                    action: DiscoveryActionKind::Failed,
                    stack_id: None,
                    reason: Some("config_files_relative_path_rejected".to_string()),
                    details: None,
                });
                continue;
            }
            Err(NormalizeConfigFilesError::Empty) => {
                summary.stacks_failed += 1;
                state
                    .db
                    .upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                        project: project.clone(),
                        stack_id: None,
                        status: "invalid".to_string(),
                        last_seen_at: Some(now.clone()),
                        last_scan_at: now.clone(),
                        last_error: Some("config_files_empty".to_string()),
                        last_config_files: None,
                        unarchive_if_active: false,
                    })
                    .await?;
                actions.push(DiscoveryAction {
                    project: project.clone(),
                    action: DiscoveryActionKind::Failed,
                    stack_id: None,
                    reason: Some("config_files_empty".to_string()),
                    details: None,
                });
                continue;
            }
        };

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
                details: None,
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
                    last_error: None,
                    last_config_files: Some(config_files.clone()),
                    unarchive_if_active: true,
                })
                .await?;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Created,
                stack_id: stack_id.clone(),
                reason: None,
                details: None,
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
                reason: None,
                details: None,
            });
        } else {
            summary.stacks_skipped += 1;
            actions.push(DiscoveryAction {
                project: project.clone(),
                action: DiscoveryActionKind::Skipped,
                stack_id: Some(stack_id.clone()),
                reason: None,
                details: None,
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
                last_error: None,
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
}
