use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finalize_update_job(
    state: Arc<AppState>,
    job_id: String,
    req: TriggerUpdateRequest,
    job_kind: TransitionJobKind,
    final_status: String,
    stack_summaries: UpdateStackSummaries,
    backups_to_cleanup: UpdateBackupsToCleanup,
    mut final_summary: serde_json::Value,
    finished_at: String,
    mut settlements: Vec<crate::db::ServiceAcceptedStateSettlement>,
    observed_healthcheck_failures: usize,
    evidence: Option<crate::rollback_evidence::RollbackEvidenceContext>,
    evidence_setup_error: Option<String>,
) -> anyhow::Result<()> {
    let has_update_targets = req
        .targets
        .as_deref()
        .is_some_and(|targets| !targets.is_empty());
    let update_apply = job_kind == TransitionJobKind::Update && req.mode.as_str() == "apply";
    let recovery_error = find_recovery_error(&final_summary);
    let mut preserve_fence = false;
    let mut preserve_reason = None;
    if update_apply {
        let baselines = state
            .db
            .baseline_service_operation_settlements(&job_id)
            .await?;
        let stop_control = state.db.get_update_stop_control(&job_id).await?;
        let apply_committed = stop_control
            .as_ref()
            .is_some_and(|control| control.apply_committed_at.is_some());
        if recovery_error.is_some() && stop_control.is_some() {
            // A failed override/runtime restore leaves the odd generation as the
            // recovery fence. Do not publish a terminal job until a later recovery
            // pass can establish authoritative runtime state.
            preserve_fence = true;
            preserve_reason = recovery_error.clone();
        } else if final_status != "success"
            && recovery_error.is_none()
            && stop_control
                .as_ref()
                .is_some_and(|control| control.apply_committed_at.is_none())
        {
            if has_update_targets
                && baselines.len() != req.targets.as_deref().unwrap_or_default().len()
            {
                preserve_fence = true;
                preserve_reason = Some(
                    "update settlement deferred: service baselines are incomplete".to_string(),
                );
            } else {
                settlements = baselines.clone();
            }
        }
        if !apply_committed && !baselines.is_empty() {
            // No apply commit means the persisted baseline remains authoritative, including
            // successful no-op updates whose runtime has nothing new to observe.
            settlements = baselines.clone();
        }
        let baseline_ids = baselines
            .iter()
            .map(|settlement| settlement.service_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let settlement_ids = settlements
            .iter()
            .map(|settlement| settlement.service_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        if !preserve_fence
            && apply_committed
            && !baselines.is_empty()
            && (baseline_ids != settlement_ids || baselines.len() != settlements.len())
        {
            preserve_fence = true;
            preserve_reason = Some(
                "update settlement deferred: authoritative runtime observations are incomplete"
                    .to_string(),
            );
        }
    }

    if preserve_fence {
        let diagnostic = preserve_reason
            .or_else(|| recovery_error.clone())
            .unwrap_or_else(|| {
                "update settlement deferred: runtime state is not authoritative".to_string()
            });
        let _ = state
            .db
            .record_update_stop_recovery_error(&job_id, &diagnostic, &finished_at)
            .await;
        let _ = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: finished_at.clone(),
                    level: "warn".to_string(),
                    msg: diagnostic.clone(),
                },
            )
            .await;
        state.update_stop_hub.remove(&job_id);
        return Err(anyhow::anyhow!(diagnostic));
    }

    if let Some(summary) = final_summary.as_object_mut() {
        let refresh = if final_status == "success" {
            "fresh"
        } else {
            "deferred"
        };
        let reason = if final_status == "success" {
            "settled from final runtime observation"
        } else {
            "baseline candidate retained until a later authoritative observation"
        };
        let services = req
            .targets
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|target| {
                json!({
                    "serviceId": target.service_id,
                    "candidateRefresh": refresh,
                    "reason": reason,
                })
            })
            .collect::<Vec<_>>();
        let settlement_status = if update_apply && has_update_targets && settlements.is_empty() {
            "deferred"
        } else {
            "settled"
        };
        summary.insert(
            "stateSettlement".to_string(),
            json!({
                "status": settlement_status,
                "services": services,
            }),
        );
    }

    let force_notify = final_status != "success";
    let mut should_notify = true;
    let mut notify_summary = final_summary.clone();
    let mut notify_skip_reason: Option<String> = None;
    if !force_notify {
        match req.scope {
            JobScope::Service => {
                if let Some(service_id) = req.service_id.as_deref()
                    && let Some(true) = state.db.is_service_archived(service_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived service".to_string());
                }
                if should_notify
                    && let Some(service_id) = req.service_id.as_deref()
                    && let Some(stack_id) = state.db.get_service_stack_id(service_id).await?
                    && let Some(true) = state.db.is_stack_archived(&stack_id).await?
                {
                    should_notify = false;
                    notify_skip_reason = Some("archived stack".to_string());
                }
            }
            JobScope::Stack | JobScope::All => {
                let mut filtered = Vec::<serde_json::Value>::new();
                for s in &stack_summaries {
                    let Some(stack_id) = s.get("stackId").and_then(|v| v.as_str()) else {
                        continue;
                    };

                    if let Some(true) = state.db.is_stack_archived(stack_id).await? {
                        continue;
                    }

                    let include = if let Some(update) = s.get("update")
                        && let Some(changed_ids) = extract_changed_service_ids(update)
                    {
                        state.db.has_unarchived_services(&changed_ids).await?
                    } else {
                        state.db.has_unarchived_services_in_stack(stack_id).await?
                    };

                    if include {
                        filtered.push(s.clone());
                    }
                }

                if filtered.is_empty() {
                    should_notify = false;
                    notify_skip_reason =
                        Some("all stacks archived or only archived services touched".to_string());
                } else {
                    notify_summary = json!({
                        "mode": req.mode.as_str(),
                        "stacks": filtered,
                    });
                }
            }
        }
    }

    if !should_notify {
        let _ = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: finished_at.clone(),
                    level: "info".to_string(),
                    msg: format!(
                        "notify skipped ({})",
                        notify_skip_reason.as_deref().unwrap_or("filtered")
                    ),
                },
            )
            .await;
    }
    let archive_path = if let Some(evidence) = evidence.as_ref() {
        let mut evidence_summary = evidence.finalize().await;
        let mut archive_path = None;
        if evidence_summary.status == "available" {
            match tokio::fs::metadata(evidence.archive_path()).await {
                Ok(_) => archive_path = Some(evidence.archive_path()),
                Err(error) => {
                    mark_archive_metadata_unavailable(&mut evidence_summary, error);
                }
            }
        }
        if evidence_summary.status != "absent"
            && let Some(summary) = final_summary.as_object_mut()
        {
            summary.insert(
                "rollbackEvidence".to_string(),
                serde_json::to_value(&evidence_summary)?,
            );
        }
        archive_path
    } else {
        let failed_candidates = transition_failed_candidate_count(
            job_kind,
            req.mode.as_str(),
            &stack_summaries,
            observed_healthcheck_failures,
        );
        crate::rollback_evidence_finalize::record_spool_setup_failure(
            &mut final_summary,
            evidence_setup_error.as_deref(),
            failed_candidates,
        );
        None
    };
    let notification = notify::prepare_job_notification_item_for_finish(
        state.as_ref(),
        should_notify,
        &job_id,
        &final_status,
        &finished_at,
        &notify_summary,
    )
    .await?;
    let notification_event_enabled =
        crate::rollback_evidence_finalize::finish_update_stop_job_with_evidence_archive(
            &state.db,
            &job_id,
            &final_status,
            &finished_at,
            &mut final_summary,
            archive_path,
            evidence.as_ref(),
            (!settlements.is_empty()).then_some(settlements.as_slice()),
            notification.as_ref(),
        )
        .await?;
    if should_record_update_tag_history(&req, &final_status) {
        record_update_tag_history(state.as_ref(), &req, &finished_at).await;
    }
    if final_status == "success"
        && let Ok(now_dt) = time::OffsetDateTime::parse(
            &finished_at,
            &time::format_description::well_known::Rfc3339,
        )
    {
        for (backup_id, after_seconds) in backups_to_cleanup {
            let cleanup_after = now_dt + time::Duration::seconds(after_seconds as i64);
            if let Ok(cleanup_after) =
                cleanup_after.format(&time::format_description::well_known::Rfc3339)
            {
                let _ = state
                    .db
                    .schedule_backup_cleanup(&backup_id, &cleanup_after)
                    .await;
            }
        }
    }
    if notification_event_enabled == Some(true)
        && let Err(error) = notify::notify_job_updated_with_event_enabled(
            state.as_ref(),
            &job_id,
            &final_status,
            &finished_at,
            &notify_summary,
            notification_event_enabled,
        )
        .await
    {
        let _ = state
            .db
            .insert_job_log(
                &job_id,
                &JobLogLine {
                    ts: finished_at.clone(),
                    level: "warn".to_string(),
                    msg: format!("notification delivery failed: {error}"),
                },
            )
            .await;
    }
    state.update_stop_hub.remove(&job_id);
    Ok(())
}
