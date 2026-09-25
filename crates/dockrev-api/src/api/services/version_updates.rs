use super::*;

async fn load_version_update_service(
    state: &AppState,
    service_id: &str,
) -> Result<(crate::models::StackRecord, Service, String), ApiError> {
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
    let service = stack
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("service not found"))?;

    if updater::is_dockrev_image_ref(
        &service.image.reference,
        Some(state.config.dockrev_image_repo.as_str()),
    ) {
        return Err(ApiError::invalid_argument(
            "selected-version updates are not available for Dockrev",
        ));
    }
    if stack.archived || service.archived.unwrap_or(false) {
        return Err(ApiError::conflict("archived services cannot be updated"));
    }
    if service.ignore.as_ref().is_some_and(|ignore| ignore.matched) {
        return Err(ApiError::conflict("ignored services cannot be updated"));
    }
    if service
        .update_guard
        .as_ref()
        .is_some_and(|guard| guard.blocked)
    {
        return Err(ApiError::conflict("service update is blocked by its guard"));
    }

    let image_repo = snapshot_worker::image_repo_from_image_ref(&service.image.reference)
        .ok_or_else(|| ApiError::invalid_argument("service image repository is invalid"))?;
    Ok((stack, service, image_repo))
}

async fn build_service_version_update_preview(
    state: &AppState,
    service_id: &str,
    release_tag: &str,
) -> Result<PreviewServiceVersionUpdateResponse, ApiError> {
    let release_tag = release_tag.trim();
    if release_tag.is_empty() {
        return Err(ApiError::invalid_argument("releaseTag is required"));
    }
    let (_, service, image_repo) = load_version_update_service(state, service_id).await?;
    let current_digest = service
        .image
        .digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .ok_or_else(|| ApiError::conflict("current service digest is unavailable"))?;
    let observations = state
        .db
        .list_service_version_tag_observations(service_id, &image_repo, &service.image.tag)
        .await
        .map_err(map_internal)?;
    let observed_current_version = observations
        .iter()
        .find(|observation| {
            snapshot_worker::normalize_digest(&observation.digest).as_deref()
                == Some(current_digest.as_str())
        })
        .and_then(|observation| observation.version.as_deref())
        .map(str::trim)
        .filter(|tag| crate::ignore::is_strict_semver(tag));
    let current_version = service
        .image
        .resolved_tag
        .as_deref()
        .map(str::trim)
        .filter(|tag| crate::ignore::is_strict_semver(tag))
        .or(observed_current_version)
        .unwrap_or(service.image.tag.trim());
    if !updater::strict_semver_tag_is_newer(release_tag, current_version) {
        return Err(ApiError::invalid_argument(
            "releaseTag must be a strictly newer comparable SemVer version",
        ));
    }
    let image = registry::ImageRef::parse(&service.image.reference)
        .map_err(|_| ApiError::invalid_argument("service image reference is invalid"))?;
    let associated = observations.iter().find(|observation| {
        observation
            .version
            .as_deref()
            .is_some_and(|version| updater::strict_semver_tags_equivalent(version, release_tag))
    });
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let (classification, target_digest, manifest) = if let Some(observation) = associated {
        let target_digest = observation.digest.clone();
        let manifest = state
            .registry
            .get_manifest(&image, &target_digest, &host_platform)
            .await
            .map_err(|_| ApiError::conflict("observed release digest is no longer resolvable"))?;
        (
            ServiceVersionUpdateClassification::Normal,
            target_digest,
            manifest,
        )
    } else {
        let manifest = state
            .registry
            .get_manifest(&image, release_tag, &host_platform)
            .await
            .map_err(|_| {
                ApiError::conflict("release tag is not resolvable in the service repository")
            })?;
        let target_digest = manifest
            .digest
            .clone()
            .or(manifest.platform_digest.clone())
            .and_then(|digest| snapshot_worker::normalize_digest(&digest))
            .ok_or_else(|| ApiError::conflict("release tag did not resolve to a valid digest"))?;
        (
            ServiceVersionUpdateClassification::Forced,
            target_digest,
            manifest,
        )
    };

    if registry::compute_arch_match(&host_platform, &manifest.arch).as_str() == "mismatch" {
        return Err(ApiError::conflict(
            "selected release does not support the host architecture",
        ));
    }
    let target_digest = snapshot_worker::normalize_digest(&target_digest)
        .ok_or_else(|| ApiError::conflict("selected release digest is invalid"))?;

    Ok(PreviewServiceVersionUpdateResponse {
        release_tag: release_tag.to_string(),
        classification,
        target_digest,
        current_digest,
        current_version: current_version.to_string(),
        image_reference: service.image.reference,
        image_repo,
        configured_tag: service.image.tag,
    })
}

pub(crate) async fn get_service_version_tag_observations(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceVersionTagObservationsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let (_, service, image_repo) = load_version_update_service(&state, &service_id).await?;
    let observations = state
        .db
        .list_service_version_tag_observations(&service_id, &image_repo, &service.image.tag)
        .await
        .map_err(map_internal)?
        .into_iter()
        .filter_map(|observation| {
            observation
                .version
                .map(|version| ServiceVersionTagObservationItem {
                    version,
                    digest: observation.digest,
                    observed_at: observation.observed_at,
                })
        })
        .collect();
    Ok(Json(ServiceVersionTagObservationsResponse {
        image_repo,
        configured_tag: service.image.tag,
        observations,
    }))
}

pub(crate) async fn preview_service_version_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<PreviewServiceVersionUpdateRequest>,
) -> Result<Json<PreviewServiceVersionUpdateResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    Ok(Json(
        build_service_version_update_preview(&state, &service_id, &req.release_tag).await?,
    ))
}

pub(crate) async fn trigger_service_version_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<TriggerServiceVersionUpdateRequest>,
) -> Result<Json<TriggerServiceVersionUpdateResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let preview =
        build_service_version_update_preview(&state, &service_id, &req.release_tag).await?;
    let same_baseline = req.release_tag.trim() == preview.release_tag
        && req.classification == preview.classification
        && req.image_repo == preview.image_repo
        && req.configured_tag == preview.configured_tag
        && snapshot_worker::normalize_digest(&req.target_digest).as_deref()
            == Some(preview.target_digest.as_str())
        && snapshot_worker::normalize_digest(&req.current_digest).as_deref()
            == Some(preview.current_digest.as_str());
    if !same_baseline || req.image_reference != preview.image_reference {
        return Err(ApiError::conflict(
            "selected version preview is stale; preview the release again",
        ));
    }
    if preview.classification == ServiceVersionUpdateClassification::Forced && !req.force_confirmed
    {
        return Err(ApiError::invalid_argument(
            "forceConfirmed is required for an unobserved release",
        ));
    }

    let now = now_rfc3339().map_err(map_internal)?;
    let target = UpdateServiceTarget {
        service_id: service_id.clone(),
        target_tag: preview.configured_tag.clone(),
        target_digest: preview.target_digest.clone(),
        pull_tags: Some(Vec::new()),
        skip_tag_followups: false,
        skip_target_tag_pull: true,
        auto_policy_context: None,
    };
    let update_request = TriggerUpdateRequest {
        scope: JobScope::Service,
        stack_id: None,
        service_id: Some(service_id),
        target_tag: None,
        target_digest: None,
        pull_tags: None,
        targets: Some(vec![target.clone()]),
        mode: UpdateMode::Apply,
        allow_arch_mismatch: false,
        backup_mode: req.backup_mode,
        reason: UpdateReason::Ui,
    };
    let job_id = super::super::operations::enqueue_selected_version_update_job(
        state,
        user.principal,
        update_request,
        target,
        preview.current_digest,
        preview.image_reference,
        preview.configured_tag,
        now,
    )
    .await?;
    Ok(Json(TriggerServiceVersionUpdateResponse { job_id }))
}
