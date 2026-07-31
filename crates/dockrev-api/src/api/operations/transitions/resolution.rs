use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitionJobKind {
    Update,
    Rollback,
}

impl TransitionJobKind {
    pub(crate) fn from_job_type(job_type: &JobType) -> Self {
        match job_type {
            JobType::Rollback => Self::Rollback,
            _ => Self::Update,
        }
    }

    pub(crate) fn summary_key(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Rollback => "rollback",
        }
    }

    pub(crate) fn summary_mode(self, update_mode: &UpdateMode) -> &'static str {
        match self {
            Self::Update => update_mode.as_str(),
            Self::Rollback => "rollback",
        }
    }

    pub(crate) fn initial_log_message(self) -> &'static str {
        match self {
            Self::Update => "update started",
            Self::Rollback => "rollback started",
        }
    }

    pub(crate) fn initial_progress_message(self) -> &'static str {
        match self {
            Self::Update => "preparing update job",
            Self::Rollback => "preparing rollback job",
        }
    }

    pub(crate) fn preparing_targets_message(self, total_stacks: u32) -> String {
        match self {
            Self::Update => format!("preparing update targets ({total_stacks} stacks)"),
            Self::Rollback => format!("preparing rollback targets ({total_stacks} stacks)"),
        }
    }

    pub(crate) fn processing_stack_message(self, stack_id: &str) -> String {
        match self {
            Self::Update => format!("processing stack {stack_id}"),
            Self::Rollback => format!("processing rollback for stack {stack_id}"),
        }
    }

    pub(crate) fn applying_stack_message(self, stack_id: &str) -> String {
        match self {
            Self::Update => format!("applying updates for stack {stack_id}"),
            Self::Rollback => format!("applying rollback for stack {stack_id}"),
        }
    }

    pub(crate) fn processed_stacks_message(
        self,
        processed_stacks: u32,
        total_stacks: u32,
    ) -> String {
        match self {
            Self::Update => format!("processed stacks ({processed_stacks}/{total_stacks})"),
            Self::Rollback => {
                format!("processed rollback stacks ({processed_stacks}/{total_stacks})")
            }
        }
    }

    pub(crate) fn failed_message(self) -> &'static str {
        match self {
            Self::Update => "update failed",
            Self::Rollback => "rollback failed",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PendingRollbackConflict {
    pub(crate) reason: String,
    pub(crate) job: JobListItem,
}

pub(crate) fn service_operation_conflict_reason(job: &JobListItem) -> &'static str {
    match job.r#type.as_str() {
        "rollback" => "rollback_in_progress",
        "service_lifecycle" => "service_lifecycle_in_progress",
        "update" => match job.scope {
            JobScope::All => "global_update_in_progress",
            JobScope::Stack => "stack_update_in_progress",
            JobScope::Service => "service_update_in_progress",
        },
        _ => "service_operation_in_progress",
    }
}

pub(crate) fn service_operation_conflict_error(job: &JobListItem) -> ApiError {
    ApiError::conflict("service operation in progress").with_details(json!({
        "reason": service_operation_conflict_reason(job),
        "existingJobId": job.id,
    }))
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedServiceRollbackTarget {
    pub(crate) stack_id: String,
    pub(crate) service_id: String,
    pub(crate) scanned_successful_updates: usize,
    pub(crate) response: ServiceRollbackTargetResponse,
    pub(crate) target: Option<UpdateServiceTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RollbackResolutionRequestKind {
    GetTarget,
    TriggerRollback,
}

impl RollbackResolutionRequestKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::GetTarget => "get_target",
            Self::TriggerRollback => "trigger_rollback",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RollbackResolutionDiagnostics {
    pub(crate) service_id: String,
    pub(crate) stack_id: String,
    pub(crate) request_kind: RollbackResolutionRequestKind,
    pub(crate) current_digest: String,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
    pub(crate) target_digest: Option<String>,
    pub(crate) source_update_job_id: Option<String>,
    pub(crate) active_job_id: Option<String>,
    pub(crate) active_job_status: Option<String>,
    pub(crate) scanned_successful_updates: usize,
}

pub(crate) fn better_pending_job(candidate: &JobListItem, current: Option<&JobListItem>) -> bool {
    let Some(current) = current else {
        return true;
    };
    let candidate_rank = if candidate.status == "running" { 2 } else { 1 };
    let current_rank = if current.status == "running" { 2 } else { 1 };
    candidate_rank > current_rank
        || (candidate_rank == current_rank
            && (candidate.created_at > current.created_at
                || (candidate.created_at == current.created_at && candidate.id > current.id)))
}

pub(crate) async fn resolve_service_for_transition(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<(String, crate::api::types::Service), ApiError> {
    let Some(stack_id) = state
        .db
        .get_service_stack_id(service_id)
        .await
        .map_err(map_internal)?
    else {
        return Err(ApiError::not_found("service not found"));
    };
    let Some(stack) = state.db.get_stack(&stack_id).await.map_err(map_internal)? else {
        return Err(ApiError::not_found("stack not found"));
    };
    let Some(service) = stack.services.into_iter().find(|svc| svc.id == service_id) else {
        return Err(ApiError::not_found("service not found"));
    };
    Ok((stack_id, service))
}

pub(crate) async fn resolve_service_display_tag_for_digest(
    state: &Arc<AppState>,
    service: &crate::api::types::Service,
    digest: Option<&str>,
    persisted_resolved_tag: Option<&str>,
    fallback_to_raw_tag: bool,
) -> Result<Option<String>, ApiError> {
    match resolve_authoritative_service_display_tag_from_snapshot(state, service, digest).await? {
        Some(Some(resolved)) => return Ok(Some(resolved)),
        Some(None) => {}
        None => {
            if let Some(resolved) = persisted_resolved_tag
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
            {
                return Ok(Some(resolved));
            }
        }
    }
    if fallback_to_raw_tag {
        let raw = service.image.tag.trim();
        if !raw.is_empty() {
            return Ok(Some(raw.to_string()));
        }
    }
    Ok(None)
}

async fn resolve_authoritative_service_display_tag_from_snapshot(
    state: &Arc<AppState>,
    service: &crate::api::types::Service,
    digest: Option<&str>,
) -> Result<Option<Option<String>>, ApiError> {
    if crate::ignore::is_strict_semver(&service.image.tag) {
        return Ok(None);
    }
    let Some(image_repo) =
        crate::snapshot_worker::image_repo_from_image_ref(&service.image.reference)
    else {
        return Ok(None);
    };
    let Some(digest) = digest.and_then(crate::snapshot_worker::normalize_digest) else {
        return Ok(None);
    };
    let host_platform =
        crate::registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, checked_at, _updated_at)) = snapshot else {
        return Ok(None);
    };
    let Some(snapshot_entry) =
        super::stacks::parse_digest_snapshot_row(&snapshot_json, &checked_at)
    else {
        return Ok(None);
    };
    if !crate::notify::notification_snapshot_is_ready(
        &snapshot_entry.snapshot,
        snapshot_entry.snapshot.checked_at.as_str(),
    ) {
        return Ok(Some(None));
    }
    let scan = &snapshot_entry.snapshot.scan;
    let scan_has_failures = scan.manifests_timeout > 0 || scan.manifests_error > 0;
    let scan_is_complete = scan.repo_tags_considered >= scan.repo_tags_total;
    if scan_has_failures || !scan_is_complete {
        return Ok(None);
    }
    Ok(Some(
        super::stacks::infer_semver_tags_from_snapshot(
            &snapshot_entry.snapshot,
            &service.image.tag,
        )
        .into_iter()
        .next(),
    ))
}

pub(crate) async fn find_pending_rollback_conflict(
    state: &Arc<AppState>,
    stack_id: &str,
    service_id: &str,
) -> Result<Option<PendingRollbackConflict>, ApiError> {
    find_pending_service_operation_conflict(state, stack_id, service_id).await
}

pub(crate) async fn find_pending_service_operation_conflict(
    state: &Arc<AppState>,
    stack_id: &str,
    service_id: &str,
) -> Result<Option<PendingRollbackConflict>, ApiError> {
    let mut best: Option<PendingRollbackConflict> = None;
    for job_type in [JobType::Rollback, JobType::ServiceLifecycle] {
        if let Some(job) = state
            .db
            .find_latest_pending_job_by_type_and_service_id(job_type, service_id)
            .await
            .map_err(map_internal)?
            && better_pending_job(&job, best.as_ref().map(|item| &item.job))
        {
            best = Some(PendingRollbackConflict {
                reason: service_operation_conflict_reason(&job).to_string(),
                job,
            });
        }
    }

    if let Some(job) = state
        .db
        .find_latest_pending_update_blocking_service(stack_id, service_id)
        .await
        .map_err(map_internal)?
        && better_pending_job(&job, best.as_ref().map(|item| &item.job))
    {
        best = Some(PendingRollbackConflict {
            reason: service_operation_conflict_reason(&job).to_string(),
            job,
        });
    }

    Ok(best)
}

pub(crate) fn find_matching_update_history_target(
    summary: &serde_json::Value,
    service_id: &str,
    current_digest: &str,
) -> Option<String> {
    let stacks = summary.get("stacks")?.as_array()?;
    for stack in stacks {
        let update = stack.get("update")?;
        let final_digest = update
            .get("finalDigests")
            .and_then(|value| value.get(service_id))
            .and_then(|value| value.as_str())
            .and_then(normalize_digest_for_compare);
        let old_digest = update
            .get("oldDigests")
            .and_then(|value| value.get(service_id))
            .and_then(|value| value.as_str())
            .and_then(normalize_digest_for_compare);
        if final_digest.as_deref() == Some(current_digest) && old_digest.is_some() {
            return old_digest;
        }
    }
    None
}

pub(crate) fn build_rollback_resolution_diagnostics(
    request_kind: RollbackResolutionRequestKind,
    resolved: &ResolvedServiceRollbackTarget,
) -> RollbackResolutionDiagnostics {
    RollbackResolutionDiagnostics {
        service_id: resolved.service_id.clone(),
        stack_id: resolved.stack_id.clone(),
        request_kind,
        current_digest: resolved.response.current_digest.clone(),
        available: resolved.response.available,
        unavailable_reason: resolved.response.unavailable_reason.clone(),
        target_digest: resolved.response.target_digest.clone(),
        source_update_job_id: resolved.response.source_update_job_id.clone(),
        active_job_id: resolved.response.active_job_id.clone(),
        active_job_status: resolved.response.active_job_status.clone(),
        scanned_successful_updates: resolved.scanned_successful_updates,
    }
}

pub(crate) async fn resolve_service_rollback_target(
    state: &Arc<AppState>,
    service_id: &str,
) -> Result<ResolvedServiceRollbackTarget, ApiError> {
    let (stack_id, service) = resolve_service_for_transition(state, service_id).await?;
    let current_digest =
        normalize_digest_for_compare(service.image.digest.as_deref().unwrap_or_default())
            .unwrap_or_default();
    let current_display_tag = resolve_service_display_tag_for_digest(
        state,
        &service,
        service.image.digest.as_deref(),
        service.image.resolved_tag.as_deref(),
        true,
    )
    .await?;

    let conflict = find_pending_rollback_conflict(state, &stack_id, service_id).await?;
    let active_job_id = conflict.as_ref().map(|item| item.job.id.clone());
    let active_job_status = conflict.as_ref().map(|item| item.job.status.clone());
    let mut unavailable_reason = conflict.as_ref().map(|item| item.reason.clone());

    let mut target_digest: Option<String> = None;
    let mut target_display_tag: Option<String> = None;
    let mut source_update_job_id: Option<String> = None;
    let mut source_finished_at: Option<String> = None;
    let mut target: Option<UpdateServiceTarget> = None;
    let mut scanned_successful_updates = 0usize;

    if updater::is_dockrev_image_ref(
        &service.image.reference,
        Some(state.config.dockrev_image_repo.as_str()),
    ) {
        unavailable_reason
            .get_or_insert_with(|| "dockrev_service_managed_via_supervisor".to_string());
    } else if current_digest.is_empty() {
        unavailable_reason.get_or_insert_with(|| "current_digest_missing".to_string());
    } else {
        let successful_updates = state
            .db
            .list_jobs_by_type_and_statuses(JobType::Update, &["success"], 500)
            .await
            .map_err(map_internal)?;
        scanned_successful_updates = successful_updates.len();
        for job in successful_updates {
            if job
                .summary_json
                .get("mode")
                .and_then(|value| value.as_str())
                != Some("apply")
            {
                continue;
            }
            let Some(found_target_digest) =
                find_matching_update_history_target(&job.summary_json, service_id, &current_digest)
            else {
                continue;
            };
            if found_target_digest == current_digest {
                unavailable_reason
                    .get_or_insert_with(|| "target_digest_matches_current".to_string());
                break;
            }
            target_display_tag = resolve_service_display_tag_for_digest(
                state,
                &service,
                Some(found_target_digest.as_str()),
                None,
                false,
            )
            .await?;
            source_update_job_id = Some(job.id.clone());
            source_finished_at = job.finished_at.clone();
            target_digest = Some(found_target_digest.clone());
            target = Some(UpdateServiceTarget {
                service_id: service_id.to_string(),
                target_tag: service.image.tag.clone(),
                target_digest: found_target_digest,
                pull_tags: Some(Vec::new()),
                skip_tag_followups: true,
            });
            break;
        }
        if target.is_none() && unavailable_reason.is_none() {
            unavailable_reason = Some("no_matching_update_history".to_string());
        }
    }

    let available = unavailable_reason.is_none() && target.is_some();
    Ok(ResolvedServiceRollbackTarget {
        stack_id,
        service_id: service_id.to_string(),
        scanned_successful_updates,
        response: ServiceRollbackTargetResponse {
            available,
            current_digest,
            current_display_tag,
            target_digest,
            target_display_tag,
            source_update_job_id,
            source_finished_at,
            unavailable_reason,
            active_job_id,
            active_job_status,
        },
        target,
    })
}

pub(crate) fn rollback_unavailable_error(payload: &ServiceRollbackTargetResponse) -> ApiError {
    ApiError::conflict("service rollback is unavailable").with_details(json!({
        "reason": payload.unavailable_reason,
        "existingJobId": payload.active_job_id,
        "activeJobStatus": payload.active_job_status,
        "currentDigest": payload.current_digest,
        "currentDisplayTag": payload.current_display_tag,
        "targetDigest": payload.target_digest,
        "targetDisplayTag": payload.target_display_tag,
        "sourceUpdateJobId": payload.source_update_job_id,
        "sourceFinishedAt": payload.source_finished_at,
    }))
}
