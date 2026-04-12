use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Context as _;
use ring::digest::{SHA256, digest};
use serde::Deserialize;
use serde_json::json;

use crate::api::types::{
    CleanupPreset, CleanupResourceItem, CleanupResourceKind, CleanupScanReason, CleanupScanRequest,
    CleanupScanResponse, CleanupScope, CleanupServiceGroup, CleanupStackGroup, CleanupUnownedGroup,
    JobLogLine, JobProgress,
};
use crate::db::ArchivedFilter;
use crate::runner::{CommandOutput, CommandSpec};
use crate::state::AppState;

const DOCKER_TIMEOUT: Duration = Duration::from_secs(15);

mod parse;

use parse::{
    ensure_success, fingerprint_hint_from_buildx_text_output, fingerprint_hint_from_output,
    parse_buildx_du_json_lines, parse_buildx_du_text_summary, resolve_volume_fingerprint,
    scan_volume_size_from_mountpoint, scan_volume_sizes_from_system_df,
};

#[derive(Clone, Debug)]
pub struct CleanupExecutionPlan {
    request: CleanupPlanRequest,
    scanned_at: String,
    estimated_reclaimable_bytes: u64,
    has_unknown_size: bool,
    stack_groups: Vec<CleanupStackGroup>,
    unowned_group: Option<CleanupUnownedGroup>,
    confirmation_fingerprint: String,
    commands: Vec<CleanupCommandAction>,
}

impl CleanupExecutionPlan {
    pub fn to_response(&self, reason: CleanupScanReason) -> CleanupScanResponse {
        CleanupScanResponse {
            reason,
            preset: self.request.preset.clone(),
            scope: self.request.scope.clone(),
            scanned_at: self.scanned_at.clone(),
            estimated_reclaimable_bytes: self.estimated_reclaimable_bytes,
            has_unknown_size: self.has_unknown_size,
            stack_groups: self.stack_groups.clone(),
            unowned_group: self.unowned_group.clone(),
            confirmation_fingerprint: Some(self.confirmation_fingerprint.clone()),
        }
    }

    pub fn confirmation_fingerprint(&self) -> &str {
        &self.confirmation_fingerprint
    }

    pub fn estimated_reclaimable_bytes(&self) -> u64 {
        self.estimated_reclaimable_bytes
    }

    pub fn has_unknown_size(&self) -> bool {
        self.has_unknown_size
    }

    pub fn target_count(&self) -> usize {
        self.commands.len()
    }

    pub fn initial_job_summary(&self) -> serde_json::Value {
        json!({
            "preset": self.request.preset.as_str(),
            "scope": self.request.scope.as_str(),
            "reclaimedBytesEstimated": self.estimated_reclaimable_bytes,
            "hasUnknownSize": self.has_unknown_size,
            "deletedCountsByKind": {},
            "skippedInUse": [],
            "groupedTargets": grouped_targets_json(&self.commands),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CleanupPlanRequest {
    preset: CleanupPreset,
    scope: CleanupScope,
    stack_id: Option<String>,
    service_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CleanupOwnership {
    Service {
        stack_id: String,
        stack_name: String,
        service_id: String,
        service_name: String,
    },
    StackOrphan {
        stack_id: String,
        stack_name: String,
    },
    Unowned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CleanupCandidateCategory {
    StoppedContainer,
    DanglingImage,
    ManagedUnusedImage,
    UnusedNetwork,
    ManagedUnusedVolume,
    GlobalUnusedImage,
    GlobalUnusedVolume,
    BuilderCache,
}

#[derive(Clone, Debug)]
struct CleanupCandidate {
    key: String,
    resource_id: String,
    kind: CleanupResourceKind,
    label: String,
    estimated_reclaimable_bytes: Option<u64>,
    estimate_unknown: bool,
    requires_ephemeral_confirmation: bool,
    ownership: CleanupOwnership,
    category: CleanupCandidateCategory,
}

#[derive(Clone, Debug)]
struct CleanupCommandAction {
    kind: CleanupResourceKind,
    resource_id: String,
    label: String,
    ownership: CleanupOwnership,
}

#[derive(Clone, Debug)]
struct ManagedStackRef {
    stack_id: String,
    stack_name: String,
}

#[derive(Clone, Debug)]
struct ManagedServiceRef {
    stack_id: String,
    stack_name: String,
    service_id: String,
    service_name: String,
    image_repo: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct ManagedContext {
    compose_project_to_stack: BTreeMap<String, ManagedStackRef>,
    compose_project_service_to_service: BTreeMap<(String, String), ManagedServiceRef>,
    repo_to_services: BTreeMap<String, Vec<ManagedServiceRef>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerInspectConfig {
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerInspectState {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerContainerMount {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerContainerInspect {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    size_rw: Option<i64>,
    #[serde(default)]
    config: DockerInspectConfig,
    #[serde(default)]
    state: DockerInspectState,
    #[serde(default)]
    mounts: Vec<DockerContainerMount>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerImageInspect {
    #[serde(default)]
    id: String,
    #[serde(default)]
    repo_tags: Vec<String>,
    #[serde(default)]
    repo_digests: Vec<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    config: DockerInspectConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerVolumeUsageData {
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerVolumeInspect {
    #[serde(default)]
    name: String,
    #[serde(default)]
    mountpoint: Option<String>,
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    usage_data: Option<DockerVolumeUsageData>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct DockerNetworkInspect {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    driver: Option<String>,
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(default)]
    containers: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BuilderCacheEstimate {
    reclaimable_bytes: Option<u64>,
    estimate_unknown: bool,
    fingerprint_hint: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BuildxDuRecord {
    #[serde(default, rename = "Reclaimable")]
    reclaimable: bool,
    #[serde(default, rename = "Shared")]
    shared: bool,
    #[serde(default, rename = "Size")]
    size: serde_json::Value,
}

pub async fn build_execution_plan(
    state: &AppState,
    req: &CleanupScanRequest,
    scanned_at: &str,
) -> anyhow::Result<CleanupExecutionPlan> {
    let managed = load_managed_context(state).await?;
    let mut candidates = scan_candidates(state, &managed).await?;
    if matches!(
        req.preset,
        CleanupPreset::Balanced | CleanupPreset::ProjectDeepClean | CleanupPreset::Aggressive
    ) && let Some(builder_cache) = scan_builder_cache_candidate(state).await
    {
        candidates.push(builder_cache);
    }

    let request = CleanupPlanRequest {
        preset: req.preset.clone(),
        scope: req.scope.clone(),
        stack_id: req.stack_id.clone(),
        service_id: req.service_id.clone(),
    };

    let mut selected = candidates
        .into_iter()
        .filter(|candidate| candidate_matches_request(candidate, &request))
        .collect::<Vec<_>>();
    selected.sort_by(|a, b| a.key.cmp(&b.key));

    let (stack_groups, unowned_group, estimated_reclaimable_bytes, has_unknown_size) =
        build_grouped_response(&selected);
    let confirmation_fingerprint = compute_confirmation_fingerprint(
        &request,
        &selected,
        scanned_at,
        estimated_reclaimable_bytes,
        has_unknown_size,
    )?;
    let commands = selected
        .into_iter()
        .map(|candidate| CleanupCommandAction {
            kind: candidate.kind,
            resource_id: candidate.resource_id,
            label: candidate.label,
            ownership: candidate.ownership,
        })
        .collect::<Vec<_>>();

    Ok(CleanupExecutionPlan {
        request,
        scanned_at: scanned_at.to_string(),
        estimated_reclaimable_bytes,
        has_unknown_size,
        stack_groups,
        unowned_group,
        confirmation_fingerprint,
        commands,
    })
}

pub async fn run_cleanup_job(
    state: std::sync::Arc<AppState>,
    job_id: &str,
    plan: CleanupExecutionPlan,
) -> anyhow::Result<()> {
    let total = plan.commands.len() as u32;
    let mut deleted_counts = BTreeMap::<String, u32>::new();
    let mut skipped_in_use = Vec::<serde_json::Value>::new();
    let mut unexpected_failures = Vec::<String>::new();

    if total == 0 {
        let finished_at = now_rfc3339()?;
        let summary = json!({
            "preset": plan.request.preset.as_str(),
            "scope": plan.request.scope.as_str(),
            "reclaimedBytesEstimated": plan.estimated_reclaimable_bytes,
            "hasUnknownSize": plan.has_unknown_size,
            "deletedCountsByKind": {},
            "skippedInUse": [],
            "groupedTargets": grouped_targets_json(&plan.commands),
        });
        state
            .db
            .finish_job(job_id, "success", &finished_at, &summary)
            .await?;
        return Ok(());
    }

    for (idx, action) in plan.commands.iter().enumerate() {
        let updated_at = now_rfc3339()?;
        let current = (idx as u32).saturating_add(1);
        let progress = make_cleanup_job_progress(
            "apply",
            format!("cleaning {}", action.label),
            current.saturating_sub(1),
            total,
            Some(action.label.clone()),
            updated_at.clone(),
        );
        persist_cleanup_job_progress(state.as_ref(), job_id, &progress).await?;

        let spec = command_spec_for_action(action);
        let out = state.runner.run(spec.clone(), DOCKER_TIMEOUT).await?;
        let combined_output = format!("{}\n{}", out.stdout, out.stderr);

        if out.status == 0 {
            *deleted_counts
                .entry(action.kind.as_str().to_string())
                .or_default() += 1;
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: updated_at.clone(),
                        level: "info".to_string(),
                        msg: format!("deleted {} {}", action.kind.as_str(), action.label),
                    },
                )
                .await?;
        } else if is_in_use_error(action.kind.clone(), &combined_output) {
            skipped_in_use.push(json!({
                "kind": action.kind.as_str(),
                "label": action.label,
                "reason": "still_attached",
            }));
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: updated_at.clone(),
                        level: "warn".to_string(),
                        msg: format!(
                            "skipped in-use {} {}: {}",
                            action.kind.as_str(),
                            action.label,
                            combined_output.trim()
                        ),
                    },
                )
                .await?;
        } else if is_not_found_error(&combined_output) {
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: updated_at.clone(),
                        level: "info".to_string(),
                        msg: format!(
                            "resource already gone for {} {}",
                            action.kind.as_str(),
                            action.label
                        ),
                    },
                )
                .await?;
        } else {
            unexpected_failures.push(format!(
                "{} {} failed: status={} output={}",
                action.kind.as_str(),
                action.label,
                out.status,
                combined_output.trim()
            ));
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: updated_at.clone(),
                        level: "error".to_string(),
                        msg: unexpected_failures.last().cloned().unwrap_or_default(),
                    },
                )
                .await?;
        }

        let updated_at = now_rfc3339()?;
        let progress = make_cleanup_job_progress(
            "apply",
            format!("processed {}", action.label),
            current,
            total,
            Some(action.label.clone()),
            updated_at,
        );
        persist_cleanup_job_progress(state.as_ref(), job_id, &progress).await?;
    }

    let finished_at = now_rfc3339()?;
    let status = if unexpected_failures.is_empty() {
        "success"
    } else {
        "failed"
    };
    let summary = json!({
        "preset": plan.request.preset.as_str(),
        "scope": plan.request.scope.as_str(),
        "reclaimedBytesEstimated": plan.estimated_reclaimable_bytes,
        "hasUnknownSize": plan.has_unknown_size,
        "deletedCountsByKind": deleted_counts,
        "skippedInUse": skipped_in_use,
        "groupedTargets": grouped_targets_json(&plan.commands),
        "errors": unexpected_failures,
    });
    state
        .db
        .finish_job(job_id, status, &finished_at, &summary)
        .await?;

    Ok(())
}

async fn load_managed_context(state: &AppState) -> anyhow::Result<ManagedContext> {
    let stack_rows = state.db.list_stacks(ArchivedFilter::Exclude).await?;
    let mut context = ManagedContext::default();

    for stack_row in stack_rows {
        let Some(stack) = state.db.get_stack(&stack_row.id).await? else {
            continue;
        };
        if stack.archived {
            continue;
        }
        let compose_project = state.db.get_stack_compose_project(&stack.id).await?;
        let stack_ref = ManagedStackRef {
            stack_id: stack.id.clone(),
            stack_name: stack.name.clone(),
        };
        if let Some(project) = compose_project.clone() {
            context
                .compose_project_to_stack
                .insert(project, stack_ref.clone());
        }

        for service in stack.services {
            if service.archived.unwrap_or(false) {
                continue;
            }
            let service_ref = ManagedServiceRef {
                stack_id: stack.id.clone(),
                stack_name: stack.name.clone(),
                service_id: service.id.clone(),
                service_name: service.name.clone(),
                image_repo: crate::snapshot_worker::image_repo_from_image_ref(
                    &service.image.reference,
                ),
            };
            if let Some(project) = compose_project.clone() {
                context
                    .compose_project_service_to_service
                    .insert((project, service.name.clone()), service_ref.clone());
            }
            if let Some(repo) = service_ref.image_repo.clone() {
                context
                    .repo_to_services
                    .entry(repo)
                    .or_default()
                    .push(service_ref.clone());
            }
        }
    }

    Ok(context)
}

async fn scan_candidates(
    state: &AppState,
    managed: &ManagedContext,
) -> anyhow::Result<Vec<CleanupCandidate>> {
    let container_ids = docker_list_ids(state, vec!["container", "ls", "-aq"]).await?;
    let mut containers = Vec::<DockerContainerInspect>::new();
    for container_id in container_ids {
        let inspect = docker_inspect_json::<DockerContainerInspect>(
            state,
            vec![
                "inspect".to_string(),
                "--size".to_string(),
                "--format".to_string(),
                "{{json .}}".to_string(),
                container_id,
            ],
        )
        .await?;
        containers.push(inspect);
    }

    let used_image_ids = containers
        .iter()
        .map(|container| container.image.trim().to_string())
        .filter(|image| !image.is_empty())
        .collect::<BTreeSet<_>>();
    let used_volume_names = containers
        .iter()
        .flat_map(|container| container.mounts.iter())
        .filter(|mount| mount.r#type.as_deref() == Some("volume"))
        .filter_map(|mount| mount.name.as_ref().map(|name| name.trim().to_string()))
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>();

    let mut candidates = Vec::<CleanupCandidate>::new();

    for container in &containers {
        if container.state.status.as_deref() == Some("running") {
            continue;
        }
        let owner = resolve_container_ownership(container, managed);
        let label = container
            .name
            .as_deref()
            .unwrap_or(container.id.as_str())
            .trim()
            .trim_start_matches('/')
            .to_string();
        candidates.push(CleanupCandidate {
            key: format!("container:{}", container.id),
            resource_id: container.id.clone(),
            kind: CleanupResourceKind::Container,
            label,
            estimated_reclaimable_bytes: container
                .size_rw
                .and_then(|size| u64::try_from(size).ok()),
            estimate_unknown: container
                .size_rw
                .and_then(|size| u64::try_from(size).ok())
                .is_none(),
            requires_ephemeral_confirmation: false,
            ownership: owner,
            category: CleanupCandidateCategory::StoppedContainer,
        });
    }

    let image_ids = docker_list_ids(state, vec!["image", "ls", "-aq", "--no-trunc"]).await?;
    let mut dedup_image_ids = BTreeSet::new();
    for image_id in image_ids {
        if !dedup_image_ids.insert(image_id.clone()) || used_image_ids.contains(&image_id) {
            continue;
        }
        let inspect = docker_inspect_json::<DockerImageInspect>(
            state,
            vec![
                "image".to_string(),
                "inspect".to_string(),
                "--format".to_string(),
                "{{json .}}".to_string(),
                image_id.clone(),
            ],
        )
        .await?;
        let repos = image_repo_candidates(&inspect);
        let dangling = image_is_dangling(&inspect);
        let ownership = resolve_image_ownership(&inspect, &repos, managed);
        let category = if dangling {
            CleanupCandidateCategory::DanglingImage
        } else if matches!(ownership, CleanupOwnership::Unowned) {
            CleanupCandidateCategory::GlobalUnusedImage
        } else {
            CleanupCandidateCategory::ManagedUnusedImage
        };
        let label = preferred_image_label(&inspect);
        candidates.push(CleanupCandidate {
            key: format!("image:{}", inspect.id),
            resource_id: inspect.id,
            kind: CleanupResourceKind::Image,
            label,
            estimated_reclaimable_bytes: inspect.size,
            estimate_unknown: inspect.size.is_none(),
            requires_ephemeral_confirmation: false,
            ownership,
            category,
        });
    }

    let volume_names = docker_list_ids(state, vec!["volume", "ls", "-q"]).await?;
    let mut system_df_volume_sizes: Option<BTreeMap<String, u64>> = None;
    let mut dedup_volume_names = BTreeSet::new();
    for volume_name in volume_names {
        if !dedup_volume_names.insert(volume_name.clone())
            || used_volume_names.contains(&volume_name)
        {
            continue;
        }
        let inspect = docker_inspect_json::<DockerVolumeInspect>(
            state,
            vec![
                "volume".to_string(),
                "inspect".to_string(),
                "--format".to_string(),
                "{{json .}}".to_string(),
                volume_name.clone(),
            ],
        )
        .await?;
        let ownership = resolve_volume_ownership(&inspect, managed);
        let category = if matches!(ownership, CleanupOwnership::Unowned) {
            CleanupCandidateCategory::GlobalUnusedVolume
        } else {
            CleanupCandidateCategory::ManagedUnusedVolume
        };
        let volume_fingerprint = resolve_volume_fingerprint(state, &inspect).await;
        if volume_fingerprint.is_none() {
            continue;
        }
        let mut estimated_reclaimable_bytes =
            inspect.usage_data.as_ref().and_then(|usage| usage.size);
        if estimated_reclaimable_bytes.is_none() {
            if system_df_volume_sizes.is_none() {
                system_df_volume_sizes = Some(scan_volume_sizes_from_system_df(state).await);
            }
            estimated_reclaimable_bytes = system_df_volume_sizes
                .as_ref()
                .and_then(|sizes| sizes.get(&inspect.name).copied());
        }
        if estimated_reclaimable_bytes.is_none()
            && let Some(mountpoint) = inspect.mountpoint.as_deref()
        {
            estimated_reclaimable_bytes = scan_volume_size_from_mountpoint(state, mountpoint).await;
        }
        candidates.push(CleanupCandidate {
            key: volume_fingerprint.unwrap_or_else(|| format!("volume:{}", inspect.name)),
            resource_id: inspect.name.clone(),
            kind: CleanupResourceKind::Volume,
            label: inspect.name.clone(),
            estimated_reclaimable_bytes,
            estimate_unknown: estimated_reclaimable_bytes.is_none(),
            requires_ephemeral_confirmation: false,
            ownership,
            category,
        });
    }

    let network_ids = docker_list_ids(state, vec!["network", "ls", "-q"]).await?;
    let mut dedup_network_ids = BTreeSet::new();
    for network_id in network_ids {
        if !dedup_network_ids.insert(network_id.clone()) {
            continue;
        }
        let inspect = docker_inspect_json::<DockerNetworkInspect>(
            state,
            vec![
                "network".to_string(),
                "inspect".to_string(),
                "--format".to_string(),
                "{{json .}}".to_string(),
                network_id.clone(),
            ],
        )
        .await?;
        if is_builtin_network(&inspect) || !inspect.containers.is_empty() {
            continue;
        }
        candidates.push(CleanupCandidate {
            key: format!("network:{}", inspect.id),
            resource_id: inspect.id.clone(),
            kind: CleanupResourceKind::Network,
            label: inspect.name.clone(),
            estimated_reclaimable_bytes: Some(0),
            estimate_unknown: false,
            requires_ephemeral_confirmation: false,
            ownership: resolve_network_ownership(&inspect, managed),
            category: CleanupCandidateCategory::UnusedNetwork,
        });
    }

    Ok(candidates)
}

async fn scan_builder_cache_candidate(state: &AppState) -> Option<CleanupCandidate> {
    let estimate = scan_builder_cache_estimate(state)
        .await
        .unwrap_or(BuilderCacheEstimate {
            reclaimable_bytes: None,
            estimate_unknown: true,
            fingerprint_hint: None,
        });
    let fingerprint_hint = estimate.fingerprint_hint?;
    Some(CleanupCandidate {
        key: format!("builder_cache:global:{fingerprint_hint}"),
        resource_id: "global-builder-cache".to_string(),
        kind: CleanupResourceKind::BuilderCache,
        label: "global builder cache".to_string(),
        estimated_reclaimable_bytes: estimate.reclaimable_bytes,
        estimate_unknown: estimate.estimate_unknown,
        requires_ephemeral_confirmation: false,
        ownership: CleanupOwnership::Unowned,
        category: CleanupCandidateCategory::BuilderCache,
    })
}

async fn scan_builder_cache_estimate(state: &AppState) -> Option<BuilderCacheEstimate> {
    let json_out = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "buildx".to_string(),
                    "du".to_string(),
                    "--format=json".to_string(),
                ],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await
        .ok();
    if let Some(out) = json_out
        && out.status == 0
        && let Some(estimate) = parse_buildx_du_json_lines(&out.stdout)
    {
        let fingerprint_hint = fingerprint_hint_from_output(&out.stdout);
        if !estimate.estimate_unknown {
            return Some(BuilderCacheEstimate {
                fingerprint_hint,
                ..estimate
            });
        }
        if let Some(summary_estimate) = scan_builder_cache_text_summary(state).await
            && summary_estimate.reclaimable_bytes.unwrap_or_default()
                >= estimate.reclaimable_bytes.unwrap_or_default()
        {
            return Some(BuilderCacheEstimate {
                reclaimable_bytes: summary_estimate.reclaimable_bytes,
                estimate_unknown: false,
                fingerprint_hint,
            });
        }
        return Some(BuilderCacheEstimate {
            fingerprint_hint,
            ..estimate
        });
    }

    scan_builder_cache_text_summary(state).await
}

async fn scan_builder_cache_text_summary(state: &AppState) -> Option<BuilderCacheEstimate> {
    let out = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec!["buildx".to_string(), "du".to_string()],
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await
        .ok()?;
    if out.status != 0 {
        return None;
    }
    let reclaimable = parse_buildx_du_text_summary(&out.stdout);
    Some(BuilderCacheEstimate {
        reclaimable_bytes: reclaimable,
        estimate_unknown: reclaimable.is_none(),
        fingerprint_hint: fingerprint_hint_from_buildx_text_output(&out.stdout),
    })
}

fn resolve_container_ownership(
    container: &DockerContainerInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    if let Some(labels) = container.config.labels.as_ref()
        && let Some(owner) = resolve_ownership_from_labels(labels, managed, true)
    {
        return owner;
    }

    let mut repos = BTreeSet::new();
    if let Some(image_ref) = container.config.image.as_deref()
        && let Some(repo) = crate::snapshot_worker::image_repo_from_image_ref(image_ref)
    {
        repos.insert(repo);
    }
    resolve_ownership_from_image_repos(&repos, managed)
}

fn resolve_image_ownership(
    image: &DockerImageInspect,
    repos: &BTreeSet<String>,
    managed: &ManagedContext,
) -> CleanupOwnership {
    if let Some(labels) = image.config.labels.as_ref()
        && let Some(owner) = resolve_ownership_from_labels(labels, managed, true)
    {
        return owner;
    }
    resolve_ownership_from_image_repos(repos, managed)
}

fn resolve_volume_ownership(
    volume: &DockerVolumeInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    volume
        .labels
        .as_ref()
        .and_then(|labels| resolve_ownership_from_labels(labels, managed, true))
        .unwrap_or(CleanupOwnership::Unowned)
}

fn resolve_network_ownership(
    network: &DockerNetworkInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    network
        .labels
        .as_ref()
        .and_then(|labels| resolve_ownership_from_labels(labels, managed, false))
        .unwrap_or(CleanupOwnership::Unowned)
}

fn resolve_ownership_from_labels(
    labels: &BTreeMap<String, String>,
    managed: &ManagedContext,
    allow_service: bool,
) -> Option<CleanupOwnership> {
    let project = labels
        .get("com.docker.compose.project")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;
    let stack = managed.compose_project_to_stack.get(project)?;
    if allow_service
        && let Some(service_name) = labels
            .get("com.docker.compose.service")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        && let Some(service) = managed
            .compose_project_service_to_service
            .get(&(project.to_string(), service_name.to_string()))
    {
        return Some(CleanupOwnership::Service {
            stack_id: service.stack_id.clone(),
            stack_name: service.stack_name.clone(),
            service_id: service.service_id.clone(),
            service_name: service.service_name.clone(),
        });
    }
    Some(CleanupOwnership::StackOrphan {
        stack_id: stack.stack_id.clone(),
        stack_name: stack.stack_name.clone(),
    })
}

fn resolve_ownership_from_image_repos(
    repos: &BTreeSet<String>,
    managed: &ManagedContext,
) -> CleanupOwnership {
    let mut services = Vec::<ManagedServiceRef>::new();
    for repo in repos {
        if let Some(matches) = managed.repo_to_services.get(repo) {
            services.extend(matches.iter().cloned());
        }
    }

    services.sort_by(|a, b| {
        (a.stack_id.as_str(), a.service_id.as_str())
            .cmp(&(b.stack_id.as_str(), b.service_id.as_str()))
    });
    services.dedup_by(|a, b| a.stack_id == b.stack_id && a.service_id == b.service_id);

    if services.len() == 1 {
        let service = services.remove(0);
        return CleanupOwnership::Service {
            stack_id: service.stack_id,
            stack_name: service.stack_name,
            service_id: service.service_id,
            service_name: service.service_name,
        };
    }

    let unique_stack_ids = services
        .iter()
        .map(|service| service.stack_id.clone())
        .collect::<BTreeSet<_>>();
    if unique_stack_ids.len() == 1
        && let Some(service) = services.first()
    {
        return CleanupOwnership::StackOrphan {
            stack_id: service.stack_id.clone(),
            stack_name: service.stack_name.clone(),
        };
    }

    CleanupOwnership::Unowned
}

fn image_repo_candidates(image: &DockerImageInspect) -> BTreeSet<String> {
    image
        .repo_tags
        .iter()
        .chain(image.repo_digests.iter())
        .filter_map(|value| crate::snapshot_worker::image_repo_from_image_ref(value))
        .collect::<BTreeSet<_>>()
}

fn image_is_dangling(image: &DockerImageInspect) -> bool {
    if image.repo_tags.is_empty() {
        return true;
    }
    image.repo_tags.iter().all(|tag| tag == "<none>:<none>")
}

fn preferred_image_label(image: &DockerImageInspect) -> String {
    image
        .repo_tags
        .iter()
        .find(|tag| tag.as_str() != "<none>:<none>")
        .cloned()
        .or_else(|| image.repo_digests.first().cloned())
        .unwrap_or_else(|| image.id.clone())
}

fn is_builtin_network(network: &DockerNetworkInspect) -> bool {
    matches!(network.name.as_str(), "bridge" | "host" | "none")
        || matches!(network.driver.as_deref(), Some("host" | "null"))
}

fn candidate_matches_request(candidate: &CleanupCandidate, request: &CleanupPlanRequest) -> bool {
    if !preset_includes_candidate(&request.preset, &candidate.category) {
        return false;
    }

    match request.scope {
        CleanupScope::All => match candidate.category {
            CleanupCandidateCategory::BuilderCache => true,
            _ => true,
        },
        CleanupScope::Stack => match &candidate.ownership {
            CleanupOwnership::Service { stack_id, .. }
            | CleanupOwnership::StackOrphan { stack_id, .. } => {
                Some(stack_id.as_str()) == request.stack_id.as_deref()
            }
            CleanupOwnership::Unowned => false,
        },
        CleanupScope::Service => match &candidate.ownership {
            CleanupOwnership::Service {
                stack_id,
                service_id,
                ..
            } => {
                Some(stack_id.as_str()) == request.stack_id.as_deref()
                    && Some(service_id.as_str()) == request.service_id.as_deref()
            }
            CleanupOwnership::StackOrphan { .. } | CleanupOwnership::Unowned => false,
        },
    }
}

fn preset_includes_candidate(preset: &CleanupPreset, category: &CleanupCandidateCategory) -> bool {
    match preset {
        CleanupPreset::Conservative => matches!(
            category,
            CleanupCandidateCategory::StoppedContainer
                | CleanupCandidateCategory::DanglingImage
                | CleanupCandidateCategory::UnusedNetwork
        ),
        CleanupPreset::Balanced => matches!(
            category,
            CleanupCandidateCategory::StoppedContainer
                | CleanupCandidateCategory::DanglingImage
                | CleanupCandidateCategory::UnusedNetwork
                | CleanupCandidateCategory::ManagedUnusedImage
                | CleanupCandidateCategory::BuilderCache
        ),
        CleanupPreset::ProjectDeepClean => matches!(
            category,
            CleanupCandidateCategory::StoppedContainer
                | CleanupCandidateCategory::DanglingImage
                | CleanupCandidateCategory::UnusedNetwork
                | CleanupCandidateCategory::ManagedUnusedImage
                | CleanupCandidateCategory::ManagedUnusedVolume
                | CleanupCandidateCategory::BuilderCache
        ),
        CleanupPreset::Aggressive => true,
    }
}

fn build_grouped_response(
    candidates: &[CleanupCandidate],
) -> (
    Vec<CleanupStackGroup>,
    Option<CleanupUnownedGroup>,
    u64,
    bool,
) {
    #[derive(Default)]
    struct StackAccum {
        stack_name: String,
        stack_orphans: Vec<CleanupResourceItem>,
        services: BTreeMap<String, CleanupServiceGroup>,
        bytes: u64,
        has_unknown: bool,
    }

    let mut stacks = BTreeMap::<String, StackAccum>::new();
    let mut unowned_resources = Vec::<CleanupResourceItem>::new();
    let mut total_bytes = 0_u64;
    let mut total_unknown = false;

    for candidate in candidates {
        let item = CleanupResourceItem {
            resource_id: candidate.resource_id.clone(),
            kind: candidate.kind.clone(),
            label: candidate.label.clone(),
            reason: candidate_reason(&candidate.category).to_string(),
            min_preset: minimum_preset_for_category(&candidate.category),
            estimated_reclaimable_bytes: candidate.estimated_reclaimable_bytes,
            estimate_unknown: candidate.estimate_unknown,
        };
        let known = candidate.estimated_reclaimable_bytes.unwrap_or_default();
        let unknown = candidate.estimate_unknown;
        total_bytes = total_bytes.saturating_add(known);
        total_unknown |= unknown;

        match &candidate.ownership {
            CleanupOwnership::Service {
                stack_id,
                stack_name,
                service_id,
                service_name,
            } => {
                let stack = stacks.entry(stack_id.clone()).or_default();
                stack.stack_name = stack_name.clone();
                stack.bytes = stack.bytes.saturating_add(known);
                stack.has_unknown |= unknown;
                let service = stack.services.entry(service_id.clone()).or_insert_with(|| {
                    CleanupServiceGroup {
                        service_id: service_id.clone(),
                        service_name: service_name.clone(),
                        estimated_reclaimable_bytes: 0,
                        has_unknown_size: false,
                        resources: Vec::new(),
                    }
                });
                service.estimated_reclaimable_bytes =
                    service.estimated_reclaimable_bytes.saturating_add(known);
                service.has_unknown_size |= unknown;
                service.resources.push(item);
            }
            CleanupOwnership::StackOrphan {
                stack_id,
                stack_name,
            } => {
                let stack = stacks.entry(stack_id.clone()).or_default();
                stack.stack_name = stack_name.clone();
                stack.bytes = stack.bytes.saturating_add(known);
                stack.has_unknown |= unknown;
                stack.stack_orphans.push(item);
            }
            CleanupOwnership::Unowned => {
                unowned_resources.push(item);
            }
        }
    }

    let mut stack_groups = stacks
        .into_iter()
        .map(|(stack_id, mut stack)| {
            for service in stack.services.values_mut() {
                service.resources.sort_by(|a, b| {
                    (a.kind.as_str(), a.label.as_str()).cmp(&(b.kind.as_str(), b.label.as_str()))
                });
            }
            let mut services = stack.services.into_values().collect::<Vec<_>>();
            services.sort_by(|a, b| a.service_name.cmp(&b.service_name));
            stack.stack_orphans.sort_by(|a, b| {
                (a.kind.as_str(), a.label.as_str()).cmp(&(b.kind.as_str(), b.label.as_str()))
            });
            CleanupStackGroup {
                stack_id,
                stack_name: stack.stack_name,
                estimated_reclaimable_bytes: stack.bytes,
                has_unknown_size: stack.has_unknown,
                stack_orphans: stack.stack_orphans,
                services,
            }
        })
        .collect::<Vec<_>>();
    stack_groups.sort_by(|a, b| a.stack_name.cmp(&b.stack_name));

    let unowned_group = if unowned_resources.is_empty() {
        None
    } else {
        let bytes = unowned_resources
            .iter()
            .map(|item| item.estimated_reclaimable_bytes.unwrap_or_default())
            .sum::<u64>();
        let has_unknown = unowned_resources.iter().any(|item| item.estimate_unknown);
        let mut resources = unowned_resources;
        resources.sort_by(|a, b| {
            (a.kind.as_str(), a.label.as_str()).cmp(&(b.kind.as_str(), b.label.as_str()))
        });
        Some(CleanupUnownedGroup {
            title: "未归属资源".to_string(),
            estimated_reclaimable_bytes: bytes,
            has_unknown_size: has_unknown,
            resources,
        })
    };

    (stack_groups, unowned_group, total_bytes, total_unknown)
}

fn minimum_preset_for_category(category: &CleanupCandidateCategory) -> CleanupPreset {
    match category {
        CleanupCandidateCategory::StoppedContainer
        | CleanupCandidateCategory::DanglingImage
        | CleanupCandidateCategory::UnusedNetwork => CleanupPreset::Conservative,
        CleanupCandidateCategory::ManagedUnusedImage | CleanupCandidateCategory::BuilderCache => {
            CleanupPreset::Balanced
        }
        CleanupCandidateCategory::ManagedUnusedVolume => CleanupPreset::ProjectDeepClean,
        CleanupCandidateCategory::GlobalUnusedImage
        | CleanupCandidateCategory::GlobalUnusedVolume => CleanupPreset::Aggressive,
    }
}

fn candidate_reason(category: &CleanupCandidateCategory) -> &'static str {
    match category {
        CleanupCandidateCategory::StoppedContainer => "容器已退出",
        CleanupCandidateCategory::DanglingImage => "悬空镜像，未被容器使用",
        CleanupCandidateCategory::ManagedUnusedImage => "旧镜像未被任何容器使用",
        CleanupCandidateCategory::ManagedUnusedVolume => "卷未挂载到任何容器",
        CleanupCandidateCategory::UnusedNetwork => "网络没有活动容器连接",
        CleanupCandidateCategory::BuilderCache => "Builder cache 可回收",
        CleanupCandidateCategory::GlobalUnusedImage => "未归属镜像未被任何容器使用",
        CleanupCandidateCategory::GlobalUnusedVolume => "未归属卷未挂载到任何容器",
    }
}

fn compute_confirmation_fingerprint(
    request: &CleanupPlanRequest,
    candidates: &[CleanupCandidate],
    _scanned_at: &str,
    estimated_reclaimable_bytes: u64,
    has_unknown_size: bool,
) -> anyhow::Result<String> {
    let mut selected = candidates
        .iter()
        .map(|candidate| -> anyhow::Result<_> {
            let ownership = ownership_json(&candidate.ownership);
            let ownership_key = serde_json::to_string(&ownership)?;
            let category = format!("{:?}", candidate.category);
            let entry = json!({
                "key": candidate.key,
                "kind": candidate.kind.as_str(),
                "resourceId": candidate.resource_id,
                "label": candidate.label,
                "estimatedReclaimableBytes": candidate.estimated_reclaimable_bytes,
                "estimateUnknown": candidate.estimate_unknown,
                "requiresEphemeralConfirmation": candidate.requires_ephemeral_confirmation,
                "ownership": ownership,
                "category": category,
            });
            Ok((
                (
                    candidate.key.as_str().to_string(),
                    candidate.kind.as_str().to_string(),
                    candidate.resource_id.as_str().to_string(),
                    candidate.label.as_str().to_string(),
                    candidate.estimated_reclaimable_bytes,
                    candidate.estimate_unknown,
                    candidate.requires_ephemeral_confirmation,
                    ownership_key,
                    category,
                ),
                entry,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    selected.sort_by(|a, b| a.0.cmp(&b.0));
    let selected = selected
        .into_iter()
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();
    let payload = json!({
        "preset": request.preset.as_str(),
        "scope": request.scope.as_str(),
        "stackId": request.stack_id,
        "serviceId": request.service_id,
        "estimatedReclaimableBytes": estimated_reclaimable_bytes,
        "hasUnknownSize": has_unknown_size,
        "selected": selected,
    });
    let encoded = serde_json::to_vec(&payload)?;
    let hashed = digest(&SHA256, &encoded);
    Ok(format!("sha256:{}", hex::encode(hashed.as_ref())))
}

fn ownership_json(ownership: &CleanupOwnership) -> serde_json::Value {
    match ownership {
        CleanupOwnership::Service {
            stack_id,
            service_id,
            ..
        } => json!({
            "type": "service",
            "stackId": stack_id,
            "serviceId": service_id,
        }),
        CleanupOwnership::StackOrphan { stack_id, .. } => json!({
            "type": "stack_orphan",
            "stackId": stack_id,
        }),
        CleanupOwnership::Unowned => json!({
            "type": "unowned",
        }),
    }
}

fn command_spec_for_action(action: &CleanupCommandAction) -> CommandSpec {
    let args = match action.kind {
        CleanupResourceKind::Container => vec![
            "container".to_string(),
            "rm".to_string(),
            action.resource_id.clone(),
        ],
        CleanupResourceKind::Image => vec![
            "image".to_string(),
            "rm".to_string(),
            action.resource_id.clone(),
        ],
        CleanupResourceKind::Network => vec![
            "network".to_string(),
            "rm".to_string(),
            action.resource_id.clone(),
        ],
        CleanupResourceKind::Volume => vec![
            "volume".to_string(),
            "rm".to_string(),
            action.resource_id.clone(),
        ],
        CleanupResourceKind::BuilderCache => vec![
            "builder".to_string(),
            "prune".to_string(),
            "-a".to_string(),
            "-f".to_string(),
        ],
    };
    CommandSpec {
        program: "docker".to_string(),
        args,
        env: Vec::new(),
    }
}

fn grouped_targets_json(commands: &[CleanupCommandAction]) -> serde_json::Value {
    let mut stacks = BTreeMap::<String, BTreeSet<String>>::new();
    for command in commands {
        match &command.ownership {
            CleanupOwnership::Service {
                stack_id,
                service_id,
                ..
            } => {
                stacks
                    .entry(stack_id.clone())
                    .or_default()
                    .insert(service_id.clone());
            }
            CleanupOwnership::StackOrphan { stack_id, .. } => {
                stacks.entry(stack_id.clone()).or_default();
            }
            CleanupOwnership::Unowned => {}
        }
    }
    json!(
        stacks
            .into_iter()
            .map(|(stack_id, service_ids)| {
                json!({
                    "stackId": stack_id,
                    "serviceIds": service_ids.into_iter().collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>()
    )
}

fn is_in_use_error(kind: CleanupResourceKind, output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    match kind {
        CleanupResourceKind::Container => {
            lowered.contains("container is running") || lowered.contains("is not stopped")
        }
        CleanupResourceKind::Image => {
            lowered.contains("being used by running container")
                || lowered.contains("must be forced")
                || lowered.contains("is being used by stopped container")
        }
        CleanupResourceKind::Network => lowered.contains("has active endpoints"),
        CleanupResourceKind::Volume => {
            lowered.contains("volume is in use") || lowered.contains("still in use")
        }
        CleanupResourceKind::BuilderCache => false,
    }
}

fn is_not_found_error(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("no such")
        || lowered.contains("not found")
        || lowered.contains("already in progress")
}

fn make_cleanup_job_progress(
    phase: &str,
    message: String,
    current: u32,
    total: u32,
    current_target: Option<String>,
    updated_at: String,
) -> JobProgress {
    let percent = if total == 0 {
        0
    } else {
        ((current.saturating_mul(100)) / total).min(100)
    };
    JobProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
        percent,
        planned_current: Some(current),
        planned_total: Some(total),
        planned_percent: Some(percent),
        current_target,
        updated_at,
    }
}

async fn persist_cleanup_job_progress(
    state: &AppState,
    job_id: &str,
    progress: &JobProgress,
) -> anyhow::Result<()> {
    let progress_json = serde_json::to_value(progress)?;
    state.db.set_job_progress(job_id, &progress_json).await?;

    let evt = json!({
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

async fn docker_list_ids(state: &AppState, args: Vec<&str>) -> anyhow::Result<Vec<String>> {
    let out = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: args.into_iter().map(ToString::to_string).collect(),
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await?;
    ensure_success("docker list", &out)?;
    Ok(out
        .stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>())
}

async fn docker_inspect_json<T>(state: &AppState, args: Vec<String>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let out = state
        .runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args,
                env: Vec::new(),
            },
            DOCKER_TIMEOUT,
        )
        .await?;
    ensure_success("docker inspect", &out)?;
    serde_json::from_str(out.stdout.trim()).context("parse docker inspect json")
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests;
