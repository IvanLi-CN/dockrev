use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use ring::digest::{SHA256, digest};
use serde_json::json;

use super::{
    CleanupCommandAction, CleanupOwnership, CleanupPlanRequest, DockerImageInspect,
    DockerNetworkInspect,
};
use crate::api::types::{
    CleanupInventoryCandidate, CleanupInventoryCategory, CleanupInventoryOwnership,
    CleanupInventoryOwnershipType, CleanupPreset, CleanupResourceItem, CleanupScope,
    CleanupServiceGroup, CleanupStackGroup, CleanupUnownedGroup,
};

pub(super) fn image_is_dangling(image: &DockerImageInspect) -> bool {
    if image.repo_tags.is_empty() {
        return true;
    }
    image.repo_tags.iter().all(|tag| tag == "<none>:<none>")
}

pub(super) fn preferred_image_label(image: &DockerImageInspect) -> String {
    image
        .repo_tags
        .iter()
        .find(|tag| tag.as_str() != "<none>:<none>")
        .cloned()
        .or_else(|| image.repo_digests.first().cloned())
        .unwrap_or_else(|| image.id.clone())
}

pub(super) fn is_builtin_network(network: &DockerNetworkInspect) -> bool {
    matches!(network.name.as_str(), "bridge" | "host" | "none")
        || matches!(network.driver.as_deref(), Some("host" | "null"))
}

pub(super) fn candidate_matches_request(
    candidate: &CleanupInventoryCandidate,
    request: &CleanupPlanRequest,
) -> bool {
    if !preset_includes_candidate(&request.preset, &candidate.category) {
        return false;
    }

    match request.scope {
        CleanupScope::All => true,
        CleanupScope::Stack => match candidate.ownership.kind {
            CleanupInventoryOwnershipType::Service | CleanupInventoryOwnershipType::StackOrphan => {
                candidate.ownership.stack_id.as_deref() == request.stack_id.as_deref()
            }
            CleanupInventoryOwnershipType::Unowned => false,
        },
        CleanupScope::Service => match candidate.ownership.kind {
            CleanupInventoryOwnershipType::Service => {
                candidate.ownership.stack_id.as_deref() == request.stack_id.as_deref()
                    && candidate.ownership.service_id.as_deref() == request.service_id.as_deref()
            }
            CleanupInventoryOwnershipType::StackOrphan | CleanupInventoryOwnershipType::Unowned => {
                false
            }
        },
    }
}

pub(super) fn preset_includes_candidate(
    preset: &CleanupPreset,
    category: &CleanupInventoryCategory,
) -> bool {
    match preset {
        CleanupPreset::Conservative => matches!(
            category,
            CleanupInventoryCategory::StoppedContainer
                | CleanupInventoryCategory::DanglingImage
                | CleanupInventoryCategory::UnusedNetwork
        ),
        CleanupPreset::Balanced => matches!(
            category,
            CleanupInventoryCategory::StoppedContainer
                | CleanupInventoryCategory::DanglingImage
                | CleanupInventoryCategory::UnusedNetwork
                | CleanupInventoryCategory::ManagedUnusedImage
                | CleanupInventoryCategory::BuilderCache
        ),
        CleanupPreset::ProjectDeepClean => matches!(
            category,
            CleanupInventoryCategory::StoppedContainer
                | CleanupInventoryCategory::DanglingImage
                | CleanupInventoryCategory::UnusedNetwork
                | CleanupInventoryCategory::ManagedUnusedImage
                | CleanupInventoryCategory::ManagedUnusedVolume
                | CleanupInventoryCategory::BuilderCache
        ),
        CleanupPreset::Aggressive => true,
    }
}

pub(super) fn build_grouped_response(
    candidates: &[CleanupInventoryCandidate],
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

        match candidate.ownership.kind {
            CleanupInventoryOwnershipType::Service => {
                let stack_id = candidate.ownership.stack_id.clone().unwrap_or_default();
                let stack_name = candidate.ownership.stack_name.clone().unwrap_or_default();
                let service_id = candidate.ownership.service_id.clone().unwrap_or_default();
                let service_name = candidate.ownership.service_name.clone().unwrap_or_default();
                let stack = stacks.entry(stack_id.clone()).or_default();
                stack.stack_name = stack_name;
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
            CleanupInventoryOwnershipType::StackOrphan => {
                let stack_id = candidate.ownership.stack_id.clone().unwrap_or_default();
                let stack_name = candidate.ownership.stack_name.clone().unwrap_or_default();
                let stack = stacks.entry(stack_id.clone()).or_default();
                stack.stack_name = stack_name;
                stack.bytes = stack.bytes.saturating_add(known);
                stack.has_unknown |= unknown;
                stack.stack_orphans.push(item);
            }
            CleanupInventoryOwnershipType::Unowned => {
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

fn minimum_preset_for_category(category: &CleanupInventoryCategory) -> CleanupPreset {
    match category {
        CleanupInventoryCategory::StoppedContainer
        | CleanupInventoryCategory::DanglingImage
        | CleanupInventoryCategory::UnusedNetwork => CleanupPreset::Conservative,
        CleanupInventoryCategory::ManagedUnusedImage | CleanupInventoryCategory::BuilderCache => {
            CleanupPreset::Balanced
        }
        CleanupInventoryCategory::ManagedUnusedVolume => CleanupPreset::ProjectDeepClean,
        CleanupInventoryCategory::GlobalUnusedImage
        | CleanupInventoryCategory::GlobalUnusedVolume => CleanupPreset::Aggressive,
    }
}

fn candidate_reason(category: &CleanupInventoryCategory) -> &'static str {
    match category {
        CleanupInventoryCategory::StoppedContainer => "容器已退出",
        CleanupInventoryCategory::DanglingImage => "悬空镜像，未被容器使用",
        CleanupInventoryCategory::ManagedUnusedImage => "旧镜像未被任何容器使用",
        CleanupInventoryCategory::ManagedUnusedVolume => "卷未挂载到任何容器",
        CleanupInventoryCategory::UnusedNetwork => "网络没有活动容器连接",
        CleanupInventoryCategory::BuilderCache => "Builder cache 可回收",
        CleanupInventoryCategory::GlobalUnusedImage => "未归属镜像未被任何容器使用",
        CleanupInventoryCategory::GlobalUnusedVolume => "未归属卷未挂载到任何容器",
    }
}

pub(super) fn compute_confirmation_fingerprint(
    request: &CleanupPlanRequest,
    candidates: &[CleanupInventoryCandidate],
    _scanned_at: &str,
    estimated_reclaimable_bytes: u64,
    has_unknown_size: bool,
) -> Result<String> {
    let mut selected = candidates
        .iter()
        .map(|candidate| -> Result<_> {
            let ownership = snapshot_ownership_json(&candidate.ownership);
            let ownership_key = serde_json::to_string(&ownership)?;
            let category = serde_json::to_string(&candidate.category)?;
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
                    category.clone(),
                ),
                entry,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
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

fn snapshot_ownership_json(ownership: &CleanupInventoryOwnership) -> serde_json::Value {
    json!({
        "type": match ownership.kind {
            CleanupInventoryOwnershipType::Service => "service",
            CleanupInventoryOwnershipType::StackOrphan => "stack_orphan",
            CleanupInventoryOwnershipType::Unowned => "unowned",
        },
        "stackId": ownership.stack_id,
        "serviceId": ownership.service_id,
    })
}

pub(super) fn grouped_targets_json(commands: &[CleanupCommandAction]) -> serde_json::Value {
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
