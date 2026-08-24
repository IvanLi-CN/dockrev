use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Context as _;
use serde::Deserialize;
use serde_json::json;

use crate::api::types::{
    CleanupInventoryCandidate, CleanupInventoryCategory, CleanupInventoryOwnership,
    CleanupInventoryOwnershipType, CleanupInventorySnapshot, CleanupPreset, CleanupResourceKind,
    CleanupScanReason, CleanupScanRequest, CleanupScanResponse, CleanupScanStatus, CleanupScope,
    CleanupServerDiskUsage, CleanupStackGroup, CleanupUnownedGroup, JobLogLine, JobProgress,
};
use crate::db::{ArchivedFilter, Db};
use crate::runner::{CommandOutput, CommandSpec};
use crate::state::AppState;

const DOCKER_TIMEOUT: Duration = Duration::from_secs(15);

mod parse;
mod planning;

use parse::{
    ensure_success, fingerprint_hint_from_buildx_text_output, fingerprint_hint_from_output,
    parse_buildx_du_json_lines, parse_buildx_du_text_summary,
};
use planning::{
    build_grouped_response, candidate_matches_request, compute_confirmation_fingerprint,
    grouped_targets_json, image_is_dangling, is_builtin_network, preferred_image_label,
};

#[derive(Clone, Debug)]
pub struct CleanupExecutionPlan {
    request: CleanupPlanRequest,
    scanned_at: String,
    estimated_reclaimable_bytes: u64,
    has_unknown_size: bool,
    server_disk_usage: Option<CleanupServerDiskUsage>,
    stack_groups: Vec<CleanupStackGroup>,
    unowned_group: Option<CleanupUnownedGroup>,
    confirmation_fingerprint: String,
    commands: Vec<CleanupCommandAction>,
}

impl CleanupExecutionPlan {
    pub fn to_response(&self, reason: CleanupScanReason) -> CleanupScanResponse {
        CleanupScanResponse {
            status: CleanupScanStatus::Ready,
            reason,
            preset: self.request.preset.clone(),
            scope: self.request.scope.clone(),
            scanned_at: Some(self.scanned_at.clone()),
            refreshing: false,
            retry_after_ms: None,
            estimated_reclaimable_bytes: Some(self.estimated_reclaimable_bytes),
            has_unknown_size: self.has_unknown_size,
            server_disk_usage: self.server_disk_usage.clone(),
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

#[derive(Clone, Debug)]
struct CleanupCommandAction {
    kind: CleanupResourceKind,
    resource_id: String,
    label: String,
    instance_id: Option<String>,
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

fn volume_instance_identity(volume: &DockerVolumeInspect) -> Option<String> {
    volume
        .created_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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

pub async fn build_inventory_snapshot(
    db: Db,
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> anyhow::Result<CleanupInventorySnapshot> {
    build_inventory_snapshot_with_progress(db, runner, |_| {}).await
}

pub async fn build_inventory_snapshot_with_progress(
    db: Db,
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
    mut on_partial: impl FnMut(CleanupInventorySnapshot) + Send,
) -> anyhow::Result<CleanupInventorySnapshot> {
    let managed = load_managed_context_from_db(&db).await?;
    let mut candidates = scan_candidates_with_progress(&runner, &db, &managed, |candidates| {
        on_partial(CleanupInventorySnapshot {
            scanned_at: now_rfc3339()
                .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
            server_disk_usage: None,
            candidates,
        });
    })
    .await?;
    if let Some(builder_cache) = scan_builder_cache_candidate(runner.clone()).await {
        candidates.push(builder_cache);
        on_partial(CleanupInventorySnapshot {
            scanned_at: now_rfc3339()
                .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
            server_disk_usage: None,
            candidates: candidates.clone(),
        });
    }
    let scanned_at = now_rfc3339()?;
    let server_disk_usage = scan_server_disk_usage_with_runner(runner.clone())
        .await
        .map(CleanupServerDiskUsage::from);
    Ok(CleanupInventorySnapshot {
        scanned_at,
        server_disk_usage,
        candidates,
    })
}

pub fn build_execution_plan_from_snapshot(
    snapshot: &CleanupInventorySnapshot,
    req: &CleanupScanRequest,
    scanned_at: &str,
) -> anyhow::Result<CleanupExecutionPlan> {
    let candidates = snapshot
        .candidates
        .iter()
        .cloned()
        .map(candidate_from_snapshot)
        .collect::<Vec<_>>();

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
            instance_id: candidate.instance_id,
            ownership: execution_ownership_from_snapshot(&candidate.ownership),
        })
        .collect::<Vec<_>>();

    Ok(CleanupExecutionPlan {
        request,
        scanned_at: scanned_at.to_string(),
        estimated_reclaimable_bytes,
        has_unknown_size,
        server_disk_usage: snapshot.server_disk_usage.clone(),
        stack_groups,
        unowned_group,
        confirmation_fingerprint,
        commands,
    })
}

impl CleanupExecutionPlan {
    pub async fn validate_volume_identities(
        &self,
        runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    ) -> bool {
        for action in &self.commands {
            if action.kind != CleanupResourceKind::Volume {
                continue;
            }
            if !validate_volume_action_identity(action, runner).await {
                return false;
            }
        }
        true
    }
}

async fn validate_volume_action_identity(
    action: &CleanupCommandAction,
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> bool {
    let Some(expected) = action.instance_id.as_deref() else {
        return false;
    };
    let current = docker_inspect_json::<DockerVolumeInspect>(
        runner,
        vec![
            "volume".to_string(),
            "inspect".to_string(),
            "--format".to_string(),
            "{{json .}}".to_string(),
            action.resource_id.clone(),
        ],
    )
    .await
    .ok()
    .and_then(|volume| volume_instance_identity(&volume));
    current.as_deref() == Some(expected)
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

        if action.kind == CleanupResourceKind::Volume
            && !validate_volume_action_identity(action, &state.runner).await
        {
            skipped_in_use.push(json!({
                "kind": action.kind.as_str(),
                "label": action.label,
                "reason": "snapshot_changed",
            }));
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: updated_at.clone(),
                        level: "warn".to_string(),
                        msg: format!(
                            "skipped changed volume {}: snapshot identity no longer matches",
                            action.label
                        ),
                    },
                )
                .await?;
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
            continue;
        }

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

async fn load_managed_context_from_db(db: &Db) -> anyhow::Result<ManagedContext> {
    let stack_rows = db.list_stacks(ArchivedFilter::Exclude).await?;
    let mut context = ManagedContext::default();

    for stack_row in stack_rows {
        let Some(stack) = db.get_stack(&stack_row.id).await? else {
            continue;
        };
        if stack.archived {
            continue;
        }
        let compose_project = db.get_stack_compose_project(&stack.id).await?;
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

async fn scan_candidates_with_progress(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    db: &Db,
    managed: &ManagedContext,
    mut on_partial: impl FnMut(Vec<CleanupInventoryCandidate>) + Send,
) -> anyhow::Result<Vec<CleanupInventoryCandidate>> {
    let container_ids = docker_list_ids(runner, vec!["container", "ls", "-aq"]).await?;
    let mut containers = Vec::<DockerContainerInspect>::new();
    for container_id in container_ids {
        let inspect = docker_inspect_json::<DockerContainerInspect>(
            runner,
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

    let mut candidates = Vec::<CleanupInventoryCandidate>::new();

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
        candidates.push(CleanupInventoryCandidate {
            key: format!("container:{}", container.id),
            resource_id: container.id.clone(),
            kind: CleanupResourceKind::Container,
            label,
            instance_id: None,
            estimated_reclaimable_bytes: container
                .size_rw
                .and_then(|size| u64::try_from(size).ok()),
            estimate_unknown: container
                .size_rw
                .and_then(|size| u64::try_from(size).ok())
                .is_none(),
            requires_ephemeral_confirmation: false,
            ownership: snapshot_ownership(owner),
            category: CleanupInventoryCategory::StoppedContainer,
        });
    }
    on_partial(candidates.clone());

    let image_ids = docker_list_ids(runner, vec!["image", "ls", "-aq", "--no-trunc"]).await?;
    let mut dedup_image_ids = BTreeSet::new();
    for image_id in image_ids {
        if !dedup_image_ids.insert(image_id.clone()) || used_image_ids.contains(&image_id) {
            continue;
        }
        let inspect = docker_inspect_json::<DockerImageInspect>(
            runner,
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
            CleanupInventoryCategory::DanglingImage
        } else if matches!(ownership, CleanupOwnership::Unowned) {
            CleanupInventoryCategory::GlobalUnusedImage
        } else {
            CleanupInventoryCategory::ManagedUnusedImage
        };
        let label = preferred_image_label(&inspect);
        candidates.push(CleanupInventoryCandidate {
            key: format!("image:{}", inspect.id),
            resource_id: inspect.id,
            kind: CleanupResourceKind::Image,
            label,
            instance_id: None,
            estimated_reclaimable_bytes: inspect.size,
            estimate_unknown: inspect.size.is_none(),
            requires_ephemeral_confirmation: false,
            ownership: snapshot_ownership(ownership),
            category,
        });
    }
    on_partial(candidates.clone());

    let volume_names = docker_list_ids(runner, vec!["volume", "ls", "-q"]).await?;
    let mut system_df_volume_sizes: Option<BTreeMap<String, u64>> = None;
    let mut dedup_volume_names = BTreeSet::new();
    for volume_name in volume_names {
        if !dedup_volume_names.insert(volume_name.clone())
            || used_volume_names.contains(&volume_name)
        {
            continue;
        }
        let inspect = docker_inspect_json::<DockerVolumeInspect>(
            runner,
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
            CleanupInventoryCategory::GlobalUnusedVolume
        } else {
            CleanupInventoryCategory::ManagedUnusedVolume
        };
        let volume_fingerprint = resolve_volume_fingerprint_with_runner(runner, &inspect).await;
        if volume_fingerprint.is_none() {
            continue;
        }
        let mut estimated_reclaimable_bytes =
            inspect.usage_data.as_ref().and_then(|usage| usage.size);
        if estimated_reclaimable_bytes.is_none() {
            if system_df_volume_sizes.is_none() {
                system_df_volume_sizes =
                    Some(scan_volume_sizes_from_system_df_with_runner(runner).await);
            }
            estimated_reclaimable_bytes = system_df_volume_sizes
                .as_ref()
                .and_then(|sizes| sizes.get(&inspect.name).copied());
        }
        if estimated_reclaimable_bytes.is_none()
            && let Some(mountpoint) = inspect.mountpoint.as_deref()
        {
            estimated_reclaimable_bytes =
                scan_volume_size_from_mountpoint_with_runner(runner, mountpoint).await;
        }
        candidates.push(CleanupInventoryCandidate {
            key: volume_fingerprint.unwrap_or_else(|| format!("volume:{}", inspect.name)),
            resource_id: inspect.name.clone(),
            kind: CleanupResourceKind::Volume,
            label: inspect.name.clone(),
            instance_id: volume_instance_identity(&inspect),
            estimated_reclaimable_bytes,
            estimate_unknown: estimated_reclaimable_bytes.is_none(),
            requires_ephemeral_confirmation: false,
            ownership: snapshot_ownership(ownership),
            category,
        });
    }
    on_partial(candidates.clone());

    let network_ids = docker_list_ids(runner, vec!["network", "ls", "-q"]).await?;
    let mut dedup_network_ids = BTreeSet::new();
    for network_id in network_ids {
        if !dedup_network_ids.insert(network_id.clone()) {
            continue;
        }
        let inspect = docker_inspect_json::<DockerNetworkInspect>(
            runner,
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
        candidates.push(CleanupInventoryCandidate {
            key: format!("network:{}", inspect.id),
            resource_id: inspect.id.clone(),
            kind: CleanupResourceKind::Network,
            label: inspect.name.clone(),
            instance_id: None,
            estimated_reclaimable_bytes: Some(0),
            estimate_unknown: false,
            requires_ephemeral_confirmation: false,
            ownership: snapshot_ownership(resolve_network_ownership(&inspect, managed)),
            category: CleanupInventoryCategory::UnusedNetwork,
        });
    }
    on_partial(candidates.clone());

    let _ = db;
    Ok(candidates)
}

async fn scan_builder_cache_candidate(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> Option<CleanupInventoryCandidate> {
    let estimate =
        scan_builder_cache_estimate(runner.clone())
            .await
            .unwrap_or(BuilderCacheEstimate {
                reclaimable_bytes: None,
                estimate_unknown: true,
                fingerprint_hint: None,
            });
    let fingerprint_hint = estimate.fingerprint_hint?;
    Some(CleanupInventoryCandidate {
        key: format!("builder_cache:global:{fingerprint_hint}"),
        resource_id: "global-builder-cache".to_string(),
        kind: CleanupResourceKind::BuilderCache,
        label: "global builder cache".to_string(),
        instance_id: None,
        estimated_reclaimable_bytes: estimate.reclaimable_bytes,
        estimate_unknown: estimate.estimate_unknown,
        requires_ephemeral_confirmation: false,
        ownership: CleanupInventoryOwnership {
            kind: CleanupInventoryOwnershipType::Unowned,
            stack_id: None,
            stack_name: None,
            service_id: None,
            service_name: None,
        },
        category: CleanupInventoryCategory::BuilderCache,
    })
}

async fn scan_builder_cache_estimate(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> Option<BuilderCacheEstimate> {
    let json_out = runner
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
        if let Some(summary_estimate) = scan_builder_cache_text_summary(runner.clone()).await
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

    scan_builder_cache_text_summary(runner).await
}

async fn scan_builder_cache_text_summary(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> Option<BuilderCacheEstimate> {
    let out = runner
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

fn snapshot_ownership(ownership: CleanupOwnership) -> CleanupInventoryOwnership {
    match ownership {
        CleanupOwnership::Service {
            stack_id,
            stack_name,
            service_id,
            service_name,
        } => CleanupInventoryOwnership {
            kind: CleanupInventoryOwnershipType::Service,
            stack_id: Some(stack_id),
            stack_name: Some(stack_name),
            service_id: Some(service_id),
            service_name: Some(service_name),
        },
        CleanupOwnership::StackOrphan {
            stack_id,
            stack_name,
        } => CleanupInventoryOwnership {
            kind: CleanupInventoryOwnershipType::StackOrphan,
            stack_id: Some(stack_id),
            stack_name: Some(stack_name),
            service_id: None,
            service_name: None,
        },
        CleanupOwnership::Unowned => CleanupInventoryOwnership {
            kind: CleanupInventoryOwnershipType::Unowned,
            stack_id: None,
            stack_name: None,
            service_id: None,
            service_name: None,
        },
    }
}

fn candidate_from_snapshot(candidate: CleanupInventoryCandidate) -> CleanupInventoryCandidate {
    candidate
}

fn execution_ownership_from_snapshot(ownership: &CleanupInventoryOwnership) -> CleanupOwnership {
    match ownership.kind {
        CleanupInventoryOwnershipType::Service => CleanupOwnership::Service {
            stack_id: ownership.stack_id.clone().unwrap_or_default(),
            stack_name: ownership.stack_name.clone().unwrap_or_default(),
            service_id: ownership.service_id.clone().unwrap_or_default(),
            service_name: ownership.service_name.clone().unwrap_or_default(),
        },
        CleanupInventoryOwnershipType::StackOrphan => CleanupOwnership::StackOrphan {
            stack_id: ownership.stack_id.clone().unwrap_or_default(),
            stack_name: ownership.stack_name.clone().unwrap_or_default(),
        },
        CleanupInventoryOwnershipType::Unowned => CleanupOwnership::Unowned,
    }
}

async fn docker_list_ids(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    args: Vec<&str>,
) -> anyhow::Result<Vec<String>> {
    let out = runner
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

async fn docker_inspect_json<T>(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    args: Vec<String>,
) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let out = runner
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

async fn resolve_volume_fingerprint_with_runner(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    volume: &DockerVolumeInspect,
) -> Option<String> {
    parse::resolve_volume_fingerprint_with_runner(runner.clone(), volume).await
}

async fn scan_volume_sizes_from_system_df_with_runner(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> BTreeMap<String, u64> {
    parse::scan_volume_sizes_from_system_df_with_runner(runner.clone()).await
}

async fn scan_volume_size_from_mountpoint_with_runner(
    runner: &std::sync::Arc<dyn crate::runner::CommandRunner>,
    mountpoint: &str,
) -> Option<u64> {
    parse::scan_volume_size_from_mountpoint_with_runner(runner.clone(), mountpoint).await
}

async fn scan_server_disk_usage_with_runner(
    runner: std::sync::Arc<dyn crate::runner::CommandRunner>,
) -> Option<(u64, u64)> {
    parse::scan_server_disk_usage_with_runner(runner).await
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
        planned_percent: Some(Some(percent)),
        current_target,
        download: None,
        backup: None,
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

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests;
