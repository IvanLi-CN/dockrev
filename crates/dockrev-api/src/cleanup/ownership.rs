use std::collections::{BTreeMap, BTreeSet};

use super::{
    CleanupOwnership, DockerContainerInspect, DockerImageInspect, DockerNetworkInspect,
    DockerVolumeInspect, ManagedContext, ManagedServiceRef, ManagedStackRef,
};
use crate::db::{ArchivedFilter, Db};

pub(super) async fn load_managed_context_from_db(db: &Db) -> anyhow::Result<ManagedContext> {
    let stack_rows = db
        .list_stacks_with_state(ArchivedFilter::Include, crate::db::StackStateFilter::All)
        .await?;
    let mut context = ManagedContext::default();

    for stack_row in stack_rows {
        let Some(stack) = db.get_stack(&stack_row.id).await? else {
            continue;
        };
        let compose_projects = db.get_stack_compose_projects_for_cleanup(&stack.id).await?;
        let stack_ref = ManagedStackRef {
            stack_id: stack.id.clone(),
            stack_name: stack.name.clone(),
        };
        for project in &compose_projects {
            context
                .compose_project_to_stack
                .insert(project.clone(), stack_ref.clone());
        }

        for service in stack.services {
            let service_ref = ManagedServiceRef {
                stack_id: stack.id.clone(),
                stack_name: stack.name.clone(),
                service_id: service.id.clone(),
                service_name: service.name.clone(),
                image_references: image_reference_candidates_from_service(&service),
                image_repo: crate::snapshot_worker::image_repo_from_image_ref(
                    &service.image.reference,
                ),
            };
            for project in &compose_projects {
                context
                    .compose_project_service_to_service
                    .insert((project.clone(), service.name.clone()), service_ref.clone());
            }
            if let Some(repo) = service_ref.image_repo.clone() {
                context
                    .repo_to_services
                    .entry(repo)
                    .or_default()
                    .push(service_ref.clone());
            }
            for image_reference in &service_ref.image_references {
                context
                    .image_reference_to_services
                    .entry(image_reference.clone())
                    .or_default()
                    .push(service_ref.clone());
            }
        }
    }

    Ok(context)
}

pub(super) fn resolve_container_ownership(
    container: &DockerContainerInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    if let Some(labels) = container.config.labels.as_ref()
        && labels
            .get("com.docker.compose.project")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    {
        return resolve_ownership_from_labels(labels, managed, true)
            .unwrap_or(CleanupOwnership::Unowned);
    }

    let mut repos = BTreeSet::new();
    if let Some(image_ref) = container.config.image.as_deref()
        && let Some(repo) = crate::snapshot_worker::image_repo_from_image_ref(image_ref)
    {
        repos.insert(repo);
    }
    resolve_ownership_from_image_repos(&repos, managed)
}

pub(super) fn resolve_image_ownership(
    image: &DockerImageInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    // An unlabelled image cannot safely inherit ownership from a repository alone:
    // a deleted Stack and a retained Stack may use the same repository.
    if let Some(labels) = image.config.labels.as_ref()
        && labels
            .get("com.docker.compose.project")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    {
        return resolve_ownership_from_labels(labels, managed, true)
            .unwrap_or(CleanupOwnership::Unowned);
    }
    resolve_ownership_from_image_references(&image_reference_candidates(image), managed)
}

pub(super) fn resolve_volume_ownership(
    volume: &DockerVolumeInspect,
    managed: &ManagedContext,
) -> CleanupOwnership {
    volume
        .labels
        .as_ref()
        .and_then(|labels| resolve_ownership_from_labels(labels, managed, true))
        .unwrap_or(CleanupOwnership::Unowned)
}

pub(super) fn resolve_network_ownership(
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
    ownership_from_services(services)
}

fn image_reference_candidates(image: &DockerImageInspect) -> BTreeSet<String> {
    image
        .repo_tags
        .iter()
        .chain(image.repo_digests.iter())
        .flat_map(|value| image_reference_candidates_from_ref(value))
        .collect()
}

fn image_reference_candidates_from_ref(image_ref: &str) -> BTreeSet<String> {
    let raw = image_ref.trim();
    let Some(repo) = crate::snapshot_worker::image_repo_from_image_ref(raw) else {
        return BTreeSet::new();
    };
    let mut references = BTreeSet::new();
    if let Some((without_digest, digest)) = raw.split_once('@') {
        if !digest.trim().is_empty() {
            references.insert(format!("{repo}@{}", digest.trim()));
        }
        if let Some((_, tag)) = without_digest.rsplit_once(':')
            && !tag.is_empty()
            && !tag.contains('/')
        {
            references.insert(format!("{repo}:{}", tag.trim()));
        }
    } else if let Some((_, tag)) = raw.rsplit_once(':')
        && !tag.is_empty()
        && !tag.contains('/')
    {
        references.insert(format!("{repo}:{}", tag.trim()));
    }
    references
}

fn image_reference_candidates_from_service(
    service: &crate::api::types::Service,
) -> BTreeSet<String> {
    let reference = service.image.reference.trim();
    if reference.is_empty() {
        return BTreeSet::new();
    }
    let mut candidates = if service.image.tag.trim().is_empty() {
        image_reference_candidates_from_ref(reference)
    } else {
        image_reference_candidates_from_ref(&format!("{reference}:{}", service.image.tag.trim()))
    };
    if let Some(digest) = service.image.digest.as_deref().map(str::trim)
        && !digest.is_empty()
    {
        candidates.extend(image_reference_candidates_from_ref(&format!(
            "{reference}@{digest}"
        )));
    }
    candidates
}

fn resolve_ownership_from_image_references(
    references: &BTreeSet<String>,
    managed: &ManagedContext,
) -> CleanupOwnership {
    let services = references
        .iter()
        .filter_map(|reference| managed.image_reference_to_services.get(reference))
        .flat_map(|matches| matches.iter().cloned())
        .collect();
    ownership_from_services(services)
}

fn ownership_from_services(mut services: Vec<ManagedServiceRef>) -> CleanupOwnership {
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

pub(super) fn snapshot_ownership(
    ownership: CleanupOwnership,
) -> crate::api::types::CleanupInventoryOwnership {
    match ownership {
        CleanupOwnership::Service {
            stack_id,
            stack_name,
            service_id,
            service_name,
        } => crate::api::types::CleanupInventoryOwnership {
            kind: crate::api::types::CleanupInventoryOwnershipType::Service,
            stack_id: Some(stack_id),
            stack_name: Some(stack_name),
            service_id: Some(service_id),
            service_name: Some(service_name),
        },
        CleanupOwnership::StackOrphan {
            stack_id,
            stack_name,
        } => crate::api::types::CleanupInventoryOwnership {
            kind: crate::api::types::CleanupInventoryOwnershipType::StackOrphan,
            stack_id: Some(stack_id),
            stack_name: Some(stack_name),
            service_id: None,
            service_name: None,
        },
        CleanupOwnership::Unowned => crate::api::types::CleanupInventoryOwnership {
            kind: crate::api::types::CleanupInventoryOwnershipType::Unowned,
            stack_id: None,
            stack_name: None,
            service_id: None,
            service_name: None,
        },
    }
}
