use super::*;

fn log_get_service_rollback_target_resolution(resolved: &ResolvedServiceRollbackTarget) {
    let diagnostics =
        build_rollback_resolution_diagnostics(RollbackResolutionRequestKind::GetTarget, resolved);
    if diagnostics.available {
        tracing::debug!(
            request_kind = %diagnostics.request_kind.as_str(),
            service_id = %diagnostics.service_id,
            stack_id = %diagnostics.stack_id,
            current_digest = %diagnostics.current_digest,
            target_digest = ?diagnostics.target_digest,
            source_update_job_id = ?diagnostics.source_update_job_id,
            scanned_successful_updates = diagnostics.scanned_successful_updates,
            "service rollback target resolved"
        );
    } else {
        tracing::debug!(
            request_kind = %diagnostics.request_kind.as_str(),
            service_id = %diagnostics.service_id,
            stack_id = %diagnostics.stack_id,
            current_digest = %diagnostics.current_digest,
            unavailable_reason = ?diagnostics.unavailable_reason,
            active_job_id = ?diagnostics.active_job_id,
            active_job_status = ?diagnostics.active_job_status,
            scanned_successful_updates = diagnostics.scanned_successful_updates,
            "service rollback target unavailable"
        );
    }
}

fn log_trigger_service_rollback_conflict(resolved: &ResolvedServiceRollbackTarget) {
    let diagnostics = build_rollback_resolution_diagnostics(
        RollbackResolutionRequestKind::TriggerRollback,
        resolved,
    );
    tracing::info!(
        request_kind = %diagnostics.request_kind.as_str(),
        service_id = %diagnostics.service_id,
        stack_id = %diagnostics.stack_id,
        current_digest = %diagnostics.current_digest,
        unavailable_reason = ?diagnostics.unavailable_reason,
        target_digest = ?diagnostics.target_digest,
        source_update_job_id = ?diagnostics.source_update_job_id,
        active_job_id = ?diagnostics.active_job_id,
        active_job_status = ?diagnostics.active_job_status,
        scanned_successful_updates = diagnostics.scanned_successful_updates,
        "service rollback request rejected"
    );
}

pub(crate) async fn trigger_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<Json<TriggerUpdateResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;

    validate_scope(
        &req.scope,
        req.stack_id.as_deref(),
        req.service_id.as_deref(),
    )?;

    match req.scope {
        JobScope::Service => {
            if req.targets.is_none() {
                if req
                    .target_tag
                    .as_deref()
                    .is_none_or(|t| t.trim().is_empty())
                {
                    return Err(ApiError::invalid_argument(
                        "targetTag is required for scope=service",
                    ));
                }
                if req
                    .target_digest
                    .as_deref()
                    .is_none_or(|d| d.trim().is_empty())
                {
                    return Err(ApiError::invalid_argument(
                        "targetDigest is required for scope=service",
                    ));
                }
                if req.pull_tags.is_none() {
                    return Err(ApiError::invalid_argument(
                        "pullTags is required for scope=service",
                    ));
                }
            } else if req.target_tag.is_some()
                || req.target_digest.is_some()
                || req.pull_tags.is_some()
            {
                return Err(ApiError::invalid_argument(
                    "targetTag/targetDigest/pullTags must be omitted when scope=service uses targets",
                ));
            }
        }
        JobScope::Stack | JobScope::All => {
            if req.target_tag.is_some() || req.target_digest.is_some() || req.pull_tags.is_some() {
                return Err(ApiError::invalid_argument(
                    "targetTag/targetDigest/pullTags is only supported for scope=service",
                ));
            }
            if req.targets.is_none() {
                return Err(ApiError::invalid_argument(
                    "targets is required for scope=stack/all",
                ));
            }
        }
    }

    let job_id = enqueue_update_job(
        state,
        user.principal,
        req.reason.as_str().to_string(),
        req,
        now,
    )
    .await?;

    Ok(Json(TriggerUpdateResponse { job_id }))
}

pub(crate) async fn get_service_rollback_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceRollbackTargetResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let resolved = resolve_service_rollback_target(&state, &service_id).await?;
    log_get_service_rollback_target_resolution(&resolved);
    Ok(Json(resolved.response))
}

pub(crate) async fn trigger_service_rollback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<TriggerRollbackResponse>, ApiError> {
    let user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let resolved = resolve_service_rollback_target(&state, &service_id).await?;
    if !resolved.response.available {
        log_trigger_service_rollback_conflict(&resolved);
        return Err(rollback_unavailable_error(&resolved.response));
    }
    crate::compose_capability::require_v2_api(&*state.runner, &state.config).await?;
    let job_id =
        enqueue_service_rollback_job(state, user.principal, "ui".to_string(), resolved, now)
            .await?;
    tracing::info!(
        request_kind = %RollbackResolutionRequestKind::TriggerRollback.as_str(),
        service_id = %service_id,
        job_id = %job_id,
        "service rollback request accepted"
    );

    Ok(Json(TriggerRollbackResponse { job_id }))
}

pub(crate) async fn enqueue_update_job(
    state: Arc<AppState>,
    created_by: String,
    reason: String,
    mut req: TriggerUpdateRequest,
    now: String,
) -> Result<String, ApiError> {
    let stack_ids = resolve_stack_ids_for_update(&state, &req)
        .await
        .map_err(map_internal)?;
    let validated_targets = resolve_validated_update_targets(&state, &req, &stack_ids).await?;
    req.targets = Some(validated_targets);
    if matches!(&req.mode, UpdateMode::Apply) {
        crate::compose_capability::require_v2_api(&*state.runner, &state.config).await?;
    }

    let operation_targets = if req.mode.as_str() == "apply" {
        let mut operation_targets = Vec::new();
        for target in req.targets.as_deref().unwrap_or_default() {
            let stack_id = state
                .db
                .get_service_stack_id(&target.service_id)
                .await
                .map_err(map_internal)?
                .ok_or_else(|| ApiError::not_found("service not found"))?;
            operation_targets.push(crate::db::ServiceOperationTarget {
                service_id: target.service_id.clone(),
                stack_id,
            });
        }
        operation_targets
    } else {
        Vec::new()
    };

    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::Update,
        req.scope.clone(),
        req.stack_id.clone(),
        req.service_id.clone(),
        &now,
    );
    job.allow_arch_mismatch = req.allow_arch_mismatch;
    job.backup_mode = req.backup_mode.as_str().to_string();
    job.summary_json = json!({
        "mode": req.mode.as_str(),
        "targets": &req.targets,
    });

    let mut job_db = job.to_db();
    job_db.created_by = created_by;
    job_db.reason = reason;
    let has_operation_targets = !operation_targets.is_empty();
    if !has_operation_targets {
        state.db.insert_job(job_db).await.map_err(map_internal)?;
    } else if let Some(conflict) = state
        .db
        .insert_service_operation_job_if_unblocked(
            job_db,
            operation_targets,
            Some(JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: "update started".to_string(),
            }),
        )
        .await
        .map_err(map_internal)?
    {
        return Err(service_operation_conflict_error(&conflict));
    }

    if !has_operation_targets {
        state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: now.clone(),
                    level: "info".to_string(),
                    msg: "update started".to_string(),
                },
            )
            .await
            .map_err(map_internal)?;
    }
    let init_progress = make_job_progress(
        "prepare",
        "preparing update job".to_string(),
        0,
        0,
        None,
        now.clone(),
    );
    if let Err(e) = persist_job_progress(&state, &job_id, &init_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial update progress");
    }

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    let run_req = req.clone();
    tokio::spawn(async move {
        let _ = run_update_job(run_state, run_job_id, run_req).await;
    });

    Ok(job_id)
}

pub(crate) async fn enqueue_service_rollback_job(
    state: Arc<AppState>,
    created_by: String,
    reason: String,
    resolved: ResolvedServiceRollbackTarget,
    now: String,
) -> Result<String, ApiError> {
    let target = resolved
        .target
        .clone()
        .ok_or_else(|| rollback_unavailable_error(&resolved.response))?;
    let operation_target = crate::db::ServiceOperationTarget {
        service_id: target.service_id.clone(),
        stack_id: resolved.stack_id.clone(),
    };

    let req = TriggerUpdateRequest {
        scope: JobScope::Service,
        stack_id: Some(resolved.stack_id.clone()),
        service_id: Some(target.service_id.clone()),
        target_tag: None,
        target_digest: None,
        pull_tags: None,
        targets: Some(vec![target]),
        mode: UpdateMode::Apply,
        allow_arch_mismatch: false,
        backup_mode: BackupMode::Inherit,
        reason: UpdateReason::Ui,
    };

    let job_id = ids::new_job_id();
    let mut job = JobRecord::new_running(
        job_id.clone(),
        JobType::Rollback,
        JobScope::Service,
        Some(resolved.stack_id.clone()),
        req.service_id.clone(),
        &now,
    );
    job.backup_mode = BackupMode::Inherit.as_str().to_string();
    job.summary_json = json!({
        "mode": "rollback",
        "currentDigest": resolved.response.current_digest,
        "currentDisplayTag": resolved.response.current_display_tag,
        "targetDigest": resolved.response.target_digest,
        "targetDisplayTag": resolved.response.target_display_tag,
        "sourceUpdateJobId": resolved.response.source_update_job_id,
        "sourceFinishedAt": resolved.response.source_finished_at,
    });

    let mut job_db = job.to_db();
    job_db.created_by = created_by;
    job_db.reason = reason;
    if let Some(conflict) = state
        .db
        .insert_service_operation_job_if_unblocked(
            job_db,
            vec![operation_target],
            Some(JobLogLine {
                ts: now.clone(),
                level: "info".to_string(),
                msg: TransitionJobKind::Rollback
                    .initial_log_message()
                    .to_string(),
            }),
        )
        .await
        .map_err(map_internal)?
    {
        return Err(service_operation_conflict_error(&conflict));
    }

    let init_progress = make_job_progress(
        "prepare",
        TransitionJobKind::Rollback
            .initial_progress_message()
            .to_string(),
        0,
        0,
        None,
        now.clone(),
    );
    if let Err(e) = persist_job_progress(&state, &job_id, &init_progress).await {
        tracing::warn!(job_id = %job_id, error = %e, "failed to persist initial rollback progress");
    }

    let run_state = state.clone();
    let run_job_id = job_id.clone();
    tokio::spawn(async move {
        let _ = run_update_job(run_state, run_job_id, req).await;
    });

    Ok(job_id)
}

pub(crate) fn normalize_digest_for_compare(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(':') {
        return Some(trimmed.to_string());
    }
    Some(format!("sha256:{trimmed}"))
}

pub(crate) fn normalize_required_update_string(
    field: &str,
    value: Option<&str>,
) -> Result<String, ApiError> {
    let trimmed = value.unwrap_or_default().trim();
    if trimmed.is_empty() {
        return Err(ApiError::invalid_argument(format!("{field} is required")));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn normalize_required_pull_tags(
    field: &str,
    pull_tags: Option<&Vec<String>>,
    target_tag: &str,
) -> Result<Vec<String>, ApiError> {
    let values =
        pull_tags.ok_or_else(|| ApiError::invalid_argument(format!("{field} is required")))?;
    let target_tag = target_tag.trim();
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (idx, tag) in values.iter().enumerate() {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(ApiError::invalid_argument(format!(
                "{field}[{idx}] must not be empty"
            )));
        }
        if trimmed == target_tag {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            normalized.push(trimmed.to_string());
        }
    }
    Ok(normalized)
}

pub(crate) fn normalize_update_service_target(
    target: &UpdateServiceTarget,
    field_prefix: &str,
) -> Result<UpdateServiceTarget, ApiError> {
    let target_tag = normalize_required_update_string(
        &format!("{field_prefix}.targetTag"),
        Some(&target.target_tag),
    )?;
    let target_digest = normalize_required_update_string(
        &format!("{field_prefix}.targetDigest"),
        Some(&target.target_digest),
    )?;
    let normalized_digest = normalize_digest_for_compare(&target_digest).ok_or_else(|| {
        ApiError::invalid_argument(format!("{field_prefix}.targetDigest is required"))
    })?;
    Ok(UpdateServiceTarget {
        service_id: normalize_required_update_string(
            &format!("{field_prefix}.serviceId"),
            Some(&target.service_id),
        )?,
        target_tag: target_tag.clone(),
        target_digest: normalized_digest,
        pull_tags: Some(normalize_required_pull_tags(
            &format!("{field_prefix}.pullTags"),
            target.pull_tags.as_ref(),
            &target_tag,
        )?),
        skip_tag_followups: target.skip_tag_followups,
    })
}

pub(crate) fn requested_update_targets(
    req: &TriggerUpdateRequest,
) -> Result<Vec<UpdateServiceTarget>, ApiError> {
    match req.scope {
        JobScope::Service => {
            if let Some(targets) = req.targets.as_ref() {
                let mut normalized = Vec::with_capacity(targets.len());
                for (idx, target) in targets.iter().enumerate() {
                    normalized.push(normalize_update_service_target(
                        target,
                        &format!("targets[{idx}]"),
                    )?);
                }
                return Ok(normalized);
            }

            let target_tag =
                normalize_required_update_string("targetTag", req.target_tag.as_deref())?;
            let target_digest =
                normalize_required_update_string("targetDigest", req.target_digest.as_deref())?;
            let normalized_digest = normalize_digest_for_compare(&target_digest)
                .ok_or_else(|| ApiError::invalid_argument("targetDigest is required"))?;

            Ok(vec![UpdateServiceTarget {
                service_id: normalize_required_update_string(
                    "serviceId",
                    req.service_id.as_deref(),
                )?,
                target_tag: target_tag.clone(),
                target_digest: normalized_digest,
                pull_tags: Some(normalize_required_pull_tags(
                    "pullTags",
                    req.pull_tags.as_ref(),
                    &target_tag,
                )?),
                skip_tag_followups: false,
            }])
        }
        JobScope::Stack | JobScope::All => {
            let targets = req.targets.as_ref().ok_or_else(|| {
                ApiError::invalid_argument("targets is required for scope=stack/all")
            })?;
            let mut normalized = Vec::with_capacity(targets.len());
            for (idx, target) in targets.iter().enumerate() {
                normalized.push(normalize_update_service_target(
                    target,
                    &format!("targets[{idx}]"),
                )?);
            }
            Ok(normalized)
        }
    }
}

pub(crate) async fn resolve_stack_ids_for_update(
    state: &AppState,
    req: &TriggerUpdateRequest,
) -> anyhow::Result<Vec<String>> {
    let stack_ids = match req.scope {
        JobScope::All => state.db.list_stack_ids().await?,
        JobScope::Stack => req.stack_id.clone().into_iter().collect(),
        JobScope::Service => {
            let service_id = req.service_id.clone().unwrap_or_default();
            state
                .db
                .get_service_stack_id(&service_id)
                .await?
                .map(|id| vec![id])
                .unwrap_or_default()
        }
    };
    Ok(stack_ids)
}

pub(crate) fn validate_target_against_service(
    svc: &crate::api::types::Service,
    target: &UpdateServiceTarget,
    allow_arch_mismatch: bool,
) -> Result<(), ApiError> {
    if target.target_tag.trim() != svc.image.tag.trim() {
        return Err(ApiError::invalid_argument(
            "cross-tag updates are not supported (targetTag must match service image tag)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedTag": svc.image.tag,
            "gotTag": target.target_tag,
        })));
    }

    let expected_digest = svc
        .candidate
        .as_ref()
        .and_then(|candidate| normalize_digest_for_compare(&candidate.digest));
    let got_digest = normalize_digest_for_compare(&target.target_digest);
    let (Some(expected_digest), Some(got_digest)) = (expected_digest.clone(), got_digest) else {
        return Err(ApiError::conflict(
            "target digest no longer matches latest scan (rescan required)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedDigest": expected_digest,
            "gotDigest": target.target_digest,
        })));
    };
    if expected_digest != got_digest {
        return Err(ApiError::conflict(
            "target digest no longer matches latest scan (rescan required)",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "expectedDigest": expected_digest,
            "gotDigest": got_digest,
        })));
    }

    if !allow_arch_mismatch
        && svc.candidate.as_ref().is_some_and(|candidate| {
            matches!(candidate.arch_match, crate::api::types::ArchMatch::Mismatch)
        })
    {
        return Err(ApiError::invalid_argument(
            "arch mismatch: re-run with allowArchMismatch=true to force update",
        )
        .with_details(json!({
            "serviceId": svc.id,
            "archMatch": "mismatch",
        })));
    }

    Ok(())
}

pub(crate) async fn resolve_validated_update_targets(
    state: &AppState,
    req: &TriggerUpdateRequest,
    stack_ids: &[String],
) -> Result<Vec<UpdateServiceTarget>, ApiError> {
    let normalized_targets = requested_update_targets(req)?;
    let mut requested_by_service = std::collections::BTreeMap::<String, UpdateServiceTarget>::new();
    for target in normalized_targets {
        if requested_by_service
            .insert(target.service_id.clone(), target)
            .is_some()
        {
            return Err(ApiError::invalid_argument(
                "targets contains duplicate serviceId",
            ));
        }
    }

    if req.scope == JobScope::Service {
        let service_id = req.service_id.as_deref().unwrap_or_default().trim();
        if requested_by_service.len() != 1 {
            return Err(ApiError::invalid_argument(
                "exactly one target is required for scope=service",
            ));
        }
        let Some(target) = requested_by_service.get(service_id) else {
            return Err(ApiError::invalid_argument(
                "scope=service target serviceId must match request serviceId",
            ));
        };

        let mut found_service = false;
        for stack_id in stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
                continue;
            };
            for svc in &stack.services {
                if svc.id != service_id {
                    continue;
                }
                validate_target_against_service(svc, target, req.allow_arch_mismatch)?;
                found_service = true;
            }
        }

        if !found_service {
            return Ok(requested_by_service.into_values().collect());
        }

        return Ok(requested_by_service.into_values().collect());
    }

    let mut expected_service_ids = std::collections::BTreeSet::<String>::new();
    let mut actionable_services =
        std::collections::BTreeMap::<String, crate::api::types::Service>::new();
    for stack_id in stack_ids {
        let Some(stack) = state.db.get_stack(stack_id).await.map_err(map_internal)? else {
            continue;
        };

        let selection = updater::select_update_services(
            &stack,
            &req.scope,
            req.service_id.as_deref(),
            req.allow_arch_mismatch,
            req.reason.as_str(),
            Some(state.config.dockrev_image_repo.as_str()),
        );

        for svc in selection.services {
            expected_service_ids.insert(svc.id.clone());
            actionable_services.insert(svc.id.clone(), svc.clone());
        }
    }

    let missing_service_ids = expected_service_ids
        .iter()
        .filter(|service_id| !requested_by_service.contains_key(*service_id))
        .cloned()
        .collect::<Vec<_>>();
    let extra_service_ids = requested_by_service
        .keys()
        .filter(|service_id| !expected_service_ids.contains(*service_id))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_service_ids.is_empty() || !extra_service_ids.is_empty() {
        return Err(ApiError::invalid_argument(
            "targets must exactly cover the selected services for this scope",
        )
        .with_details(json!({
            "missingServiceIds": missing_service_ids,
            "extraServiceIds": extra_service_ids,
        })));
    }

    for (service_id, target) in &requested_by_service {
        if let Some(svc) = actionable_services.get(service_id) {
            validate_target_against_service(svc, target, req.allow_arch_mismatch)?;
        }
    }

    Ok(requested_by_service.into_values().collect())
}
