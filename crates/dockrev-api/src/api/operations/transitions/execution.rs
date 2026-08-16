use super::*;
use crate::backup::BackupRecoveryStore;
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) type UpdateStackSummaries = Vec<serde_json::Value>;
pub(crate) type UpdateBackupsToCleanup = Vec<(String, u32)>;
pub(crate) type UpdateJobOutcome = (
    String,
    UpdateStackSummaries,
    UpdateBackupsToCleanup,
    JobProgress,
);

pub(crate) fn extract_changed_service_ids(update: &serde_json::Value) -> Option<Vec<String>> {
    let ids = update
        .get("newDigests")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect::<Vec<_>>())?;
    if ids.is_empty() { None } else { Some(ids) }
}

pub(crate) fn extract_stack_transition_summary(
    stack: &serde_json::Value,
    kind: TransitionJobKind,
) -> Option<&serde_json::Value> {
    stack.get(kind.summary_key())
}

pub(crate) fn transition_failure_step(
    kind: TransitionJobKind,
    stack_summaries: &[serde_json::Value],
) -> Option<&str> {
    stack_summaries.iter().find_map(|stack| {
        extract_stack_transition_summary(stack, kind)
            .and_then(|summary| summary.get("failureStep"))
            .and_then(|value| value.as_str())
    })
}

pub(crate) fn transition_terminal_message(
    kind: TransitionJobKind,
    final_status: &str,
    stack_summaries: &[serde_json::Value],
) -> String {
    match kind {
        TransitionJobKind::Update => {
            if final_status == "success" {
                return "update finished".to_string();
            }
            if final_status == "cancelled" {
                return "update cancelled".to_string();
            }
            if final_status == "rolled_back" {
                return match transition_failure_step(kind, stack_summaries) {
                    Some("healthcheck") => {
                        "update rolled back after healthcheck failure".to_string()
                    }
                    Some("pull_target_tag") => {
                        "update rolled back after target tag pull failure".to_string()
                    }
                    Some("sync_configured_tag") => {
                        "update rolled back after compose tag sync failure".to_string()
                    }
                    _ => "update rolled back".to_string(),
                };
            }
            "update finished with failures".to_string()
        }
        TransitionJobKind::Rollback => {
            if final_status == "rolled_back" {
                return "rollback finished".to_string();
            }
            match transition_failure_step(kind, stack_summaries) {
                Some("healthcheck") => "rollback failed after healthcheck failure".to_string(),
                Some("pull_target_tag") => {
                    "rollback failed after target tag pull failure".to_string()
                }
                Some("sync_configured_tag") => {
                    "rollback failed after compose tag sync failure".to_string()
                }
                _ => "rollback failed".to_string(),
            }
        }
    }
}

pub(crate) fn normalize_transition_outcome_status(
    kind: TransitionJobKind,
    outcome_status: &str,
) -> String {
    match kind {
        TransitionJobKind::Update => outcome_status.to_string(),
        TransitionJobKind::Rollback => match outcome_status {
            "success" => "rolled_back".to_string(),
            _ => "failed".to_string(),
        },
    }
}

pub(crate) async fn run_update_job(
    state: Arc<AppState>,
    job_id: String,
    req: TriggerUpdateRequest,
) -> anyhow::Result<()> {
    let _live_log_cleanup = crate::job_live_logs::JobLiveLogCleanupGuard::new(
        state.job_live_log_hub.clone(),
        job_id.clone(),
    );
    let job_kind = state
        .db
        .get_job(&job_id)
        .await?
        .map(|job| TransitionJobKind::from_job_type(&job.r#type))
        .unwrap_or(TransitionJobKind::Update);
    let stop_signal = (job_kind == TransitionJobKind::Update && req.mode.as_str() == "apply")
        .then(|| state.update_stop_hub.subscribe(&job_id));
    let outcome: anyhow::Result<UpdateJobOutcome> = async {
        if matches!(job_kind, TransitionJobKind::Rollback)
            || matches!(&req.mode, UpdateMode::Apply)
        {
            crate::compose_capability::require_v2(&*state.runner, &state.config).await?;
        }
        let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
            .unwrap_or_else(|| "linux/amd64".to_string());
        let backup_settings = state.db.get_backup_settings().await?;
        let stack_ids = resolve_stack_ids_for_update(state.as_ref(), &req).await?;
        let manifest_digest_cache = service_check::new_manifest_digest_cache();
        let repo_tags_cache = service_check::new_repo_tags_cache();
        let total_stacks = stack_ids.len() as u32;

        let mut final_status = "success".to_string();
        let mut stack_summaries = Vec::new();
        let mut backups_to_cleanup: Vec<(String, u32)> = Vec::new();
        let mut processed_stacks = 0u32;
        let mut latest_progress = make_job_progress(
            "prepare",
            job_kind.preparing_targets_message(total_stacks),
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        for stack_id in &stack_ids {
            let Some(stack) = state.db.get_stack(stack_id).await? else {
                processed_stacks = processed_stacks.saturating_add(1);
                latest_progress = make_job_progress(
                    "apply",
                    format!("skipped missing stack {stack_id}"),
                    processed_stacks,
                    total_stacks,
                    Some(stack_id.clone()),
                    now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                );
                if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "failed to persist update progress"
                    );
                }
                continue;
            };
            latest_progress = make_job_progress_with_percent(
                "backup",
                job_kind.processing_stack_message(stack_id),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, 0.08),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let logging_runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
                live_log_hub: state.job_live_log_hub.clone(),
                stop_signal: stop_signal.clone(),
            };
            let recovery_runner = DbLoggingRunner {
                db: state.db.clone(),
                inner: state.runner.clone(),
                job_id: job_id.clone(),
                live_log_hub: state.job_live_log_hub.clone(),
                stop_signal: None,
            };
            let recovery_store = DbBackupRecoveryStore {
                db: state.db.clone(),
                job_id: job_id.clone(),
            };

            let mut stack_summary = serde_json::Map::new();
            stack_summary.insert("stackId".to_string(), json!(stack_id));
            let planned_selection = updater::select_update_services(
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
                Some(state.config.dockrev_image_repo.as_str()),
            );
            let skipped_version_anomaly = planned_selection.skipped_version_anomaly.clone();
            let planned_service_ids = planned_selection
                .services
                .iter()
                .map(|svc| svc.id.clone())
                .collect::<Vec<_>>();
            let no_actionable_services =
                req.mode.as_str() == "apply" && planned_selection.services.is_empty();
            let no_actionable_services_after_anomaly_skip = no_actionable_services
                && !req.reason.as_str().eq_ignore_ascii_case("ui")
                && !skipped_version_anomaly.is_empty();
            let backup_requested = req.mode.as_str() == "apply"
                && !no_actionable_services
                && backup::should_run_backup(&backup_settings, req.backup_mode.as_str());
            let backup_requires_stop = backup_requested
                && backup::requires_service_stop(
                    &stack,
                    &req.scope,
                    req.service_id.as_deref(),
                );
            let pull_branch_percent = Arc::new(AtomicU32::new(0));
            let backup_branch_percent = Arc::new(AtomicU32::new(0));
            let mut services_kept_stopped_for_apply = Vec::<String>::new();
            let mut prepare_error: Option<anyhow::Error> = None;
            let mut prepare_stop_requested = false;

            if backup_requested && backup_requires_stop {
                let (pull_progress_tx, mut pull_progress_rx) =
                    tokio::sync::mpsc::unbounded_channel::<updater::UpdateProgressEvent>();
                let pull_progress_state = state.clone();
                let pull_progress_job_id = job_id.clone();
                let pull_progress_stack_id = stack_id.clone();
                let pull_branch_percent_for_task = pull_branch_percent.clone();
                let backup_branch_percent_for_task = backup_branch_percent.clone();
                let pull_progress_task = tokio::spawn(async move {
                    let mut last_fraction = 0.0_f64;
                    while let Some(event) = pull_progress_rx.recv().await {
                        let fraction = match event.step {
                            updater::UpdateProgressStep::PullDone => 1.0,
                            updater::UpdateProgressStep::PullProgress => {
                                event.pull_fraction.unwrap_or(last_fraction)
                            }
                            _ => last_fraction,
                        }
                        .clamp(last_fraction, 1.0);
                        last_fraction = fraction;
                        pull_branch_percent_for_task
                            .store((fraction * 10_000.0).round() as u32, Ordering::Relaxed);
                        let percent = combined_backup_pull_percent(
                            processed_stacks,
                            total_stacks,
                            &pull_branch_percent_for_task,
                            &backup_branch_percent_for_task,
                        );
                        let mut progress = make_job_progress_with_percent(
                            "pull",
                            event.message,
                            processed_stacks,
                            total_stacks,
                            Some(pull_progress_stack_id.clone()),
                            now_rfc3339().unwrap_or_else(|_| {
                                time::OffsetDateTime::now_utc().to_string()
                            }),
                            percent,
                        );
                        progress.download = event.download;
                        let _ = persist_job_progress(
                            &pull_progress_state,
                            &pull_progress_job_id,
                            &progress,
                        )
                        .await;
                    }
                });
                let pre_pull_result = updater::pre_pull_update_images(
                    &logging_runner,
                    &state.config.compose_bin,
                    state.config.docker_config_path.as_deref(),
                    updater::IdempotentRetryPolicy {
                        max_attempts: state.config.update_idempotent_retry_max_attempts,
                        base_ms: state.config.update_idempotent_retry_base_ms,
                        max_ms: state.config.update_idempotent_retry_max_ms,
                    },
                    &stack,
                    &req.scope,
                    req.service_id.as_deref(),
                    req.targets.as_deref(),
                    req.allow_arch_mismatch,
                    req.reason.as_str(),
                    Some(state.config.dockrev_image_repo.as_str()),
                    Some(pull_progress_tx),
                )
                .await;
                let _ = pull_progress_task.await;
                pre_pull_result?;
            }

            let mut backup_id_for_cleanup: Option<(String, u32)> = None;
            if backup_requested {
                let backup_id = ids::new_backup_id();
                let now = now_rfc3339()?;
                state
                    .db
                    .insert_backup(&backup_id, stack_id, &job_id, &now)
                    .await?;
                state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: now.clone(),
                            level: "info".to_string(),
                            msg: format!("backup started: {backup_id}"),
                        },
                    )
                    .await?;

                let (backup_progress_tx, mut backup_progress_rx) =
                    tokio::sync::mpsc::unbounded_channel::<backup::BackupProgressEvent>();
                let progress_state = state.clone();
                let progress_job_id = job_id.clone();
                let progress_stack_id = stack_id.clone();
                let pull_branch_percent_for_backup = pull_branch_percent.clone();
                let backup_branch_percent_for_task = backup_branch_percent.clone();
                let backup_command_seq = state.job_live_log_hub.begin_command(&job_id);
                let backup_progress_task = tokio::spawn(async move {
                    let mut last_checkpoint = std::time::Instant::now();
                    let mut last_checkpoint_percent = 0_u32;
                    while let Some(event) = backup_progress_rx.recv().await {
                        backup_branch_percent_for_task
                            .store(event.percent.min(100) * 100, Ordering::Relaxed);
                        let overall_percent = combined_backup_pull_percent(
                            processed_stacks,
                            total_stacks,
                            &pull_branch_percent_for_backup,
                            &backup_branch_percent_for_task,
                        );
                        let updated_at = now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                        let mut progress = make_job_progress_with_percent(
                            "backup",
                            format!("backing up {}", progress_stack_id),
                            processed_stacks,
                            total_stacks,
                            Some(progress_stack_id.clone()),
                            updated_at.clone(),
                            overall_percent,
                        );
                        progress.backup = Some(crate::api::types::JobProgressBackup {
                            phase: event.phase.clone(),
                            estimated_total_bytes: Some(event.total_bytes),
                            processed_bytes: event.processed_bytes,
                            compressed_bytes: event.compressed_bytes,
                            throughput_bps: Some(event.throughput_bps),
                            eta_seconds: event.eta_seconds,
                        });
                        progress_state
                            .job_live_log_hub
                            .publish_progress(&progress_job_id, progress.clone());

                        let width = 10_usize;
                        let filled = ((event.percent.min(100) as usize) * width) / 100;
                        let eta = event
                            .eta_seconds
                            .map(|seconds| format!("{seconds}s"))
                            .unwrap_or_else(|| "--".to_string());
                        let line = format!(
                            "[{}{}] {:>3}% {} bytes {}/s ETA {} zstd-size {}",
                            "#".repeat(filled),
                            "-".repeat(width - filled),
                            event.percent.min(100),
                            event.processed_bytes,
                            event.throughput_bps,
                            eta,
                            event.compressed_bytes,
                        );
                        progress_state.job_live_log_hub.publish_terminal(
                            &progress_job_id,
                            crate::job_live_logs::JobLiveTerminal {
                                ts: updated_at,
                                command_seq: backup_command_seq,
                                lines: vec![crate::job_live_logs::JobLiveTerminalLine {
                                    segments: vec![crate::job_live_logs::JobLiveTerminalSegment {
                                        text: line,
                                        fg: None,
                                        bg: None,
                                        bold: false,
                                        dim: false,
                                        underline: false,
                                    }],
                                }],
                            },
                        );

                        let checkpoint_due = last_checkpoint.elapsed() >= Duration::from_secs(2)
                            || event.percent >= last_checkpoint_percent.saturating_add(1)
                            || event.percent == 100;
                        if checkpoint_due {
                            let _ = persist_job_progress(
                                &progress_state,
                                &progress_job_id,
                                &progress,
                            )
                            .await;
                            last_checkpoint = std::time::Instant::now();
                            last_checkpoint_percent = event.percent;
                        }
                    }
                    progress_state.job_live_log_hub.publish_command_complete(
                        &progress_job_id,
                        backup_command_seq,
                        true,
                        false,
                    );
                });

                let services_to_keep_stopped = planned_selection
                    .services
                    .iter()
                    .map(|service| service.name.clone())
                    .collect::<Vec<_>>();
                let backup_future = backup::run_pre_update_backup(
                    &logging_runner,
                    &*state.runner,
                    &recovery_runner,
                    &recovery_store,
                    &backup_settings,
                    &state.config.db_path,
                    &state.config.dockrev_image_repo,
                    &backup_id,
                    &job_id,
                    &state.config.compose_bin,
                    state.config.docker_config_path.as_deref(),
                    &stack,
                    &req.scope,
                    req.service_id.as_deref(),
                    &services_to_keep_stopped,
                    &now,
                    Some(&state.db),
                    Some(backup_progress_tx),
                );
                let backup_result = if backup_requires_stop {
                    backup_future.await
                } else {
                    let (pull_progress_tx, mut pull_progress_rx) =
                        tokio::sync::mpsc::unbounded_channel::<updater::UpdateProgressEvent>();
                    let pull_progress_state = state.clone();
                    let pull_progress_job_id = job_id.clone();
                    let pull_progress_stack_id = stack_id.clone();
                    let pull_branch_percent_for_task = pull_branch_percent.clone();
                    let backup_branch_percent_for_pull = backup_branch_percent.clone();
                    let pull_progress_task = tokio::spawn(async move {
                        let mut last_fraction = 0.0_f64;
                        while let Some(event) = pull_progress_rx.recv().await {
                            let fraction = match event.step {
                                updater::UpdateProgressStep::PullDone => 1.0,
                                updater::UpdateProgressStep::PullProgress => {
                                    event.pull_fraction.unwrap_or(last_fraction)
                                }
                                _ => last_fraction,
                            }
                            .clamp(last_fraction, 1.0);
                            last_fraction = fraction;
                            pull_branch_percent_for_task
                                .store((fraction * 10_000.0).round() as u32, Ordering::Relaxed);
                            let percent = combined_backup_pull_percent(
                                processed_stacks,
                                total_stacks,
                                &pull_branch_percent_for_task,
                                &backup_branch_percent_for_pull,
                            );
                            let mut progress = make_job_progress_with_percent(
                                "pull",
                                event.message,
                                processed_stacks,
                                total_stacks,
                                Some(pull_progress_stack_id.clone()),
                                now_rfc3339().unwrap_or_else(|_| {
                                    time::OffsetDateTime::now_utc().to_string()
                                }),
                                percent,
                            );
                            progress.download = event.download;
                            let _ = persist_job_progress(
                                &pull_progress_state,
                                &pull_progress_job_id,
                                &progress,
                            )
                            .await;
                        }
                    });
                    let pull_future = updater::pre_pull_update_images(
                        &logging_runner,
                        &state.config.compose_bin,
                        state.config.docker_config_path.as_deref(),
                        updater::IdempotentRetryPolicy {
                            max_attempts: state.config.update_idempotent_retry_max_attempts,
                            base_ms: state.config.update_idempotent_retry_base_ms,
                            max_ms: state.config.update_idempotent_retry_max_ms,
                        },
                        &stack,
                        &req.scope,
                        req.service_id.as_deref(),
                        req.targets.as_deref(),
                        req.allow_arch_mismatch,
                        req.reason.as_str(),
                        Some(state.config.dockrev_image_repo.as_str()),
                        Some(pull_progress_tx),
                    );
                    let (backup_result, pull_result) = tokio::join!(backup_future, pull_future);
                    let _ = pull_progress_task.await;
                    match pull_result {
                        Ok(()) => backup_result,
                        Err(error) => {
                            prepare_stop_requested = crate::update_stop::is_requested(&error);
                            prepare_error = Some(anyhow::anyhow!("image prepare failed: {error}"));
                            backup_result
                        }
                    }
                };
                let _ = backup_progress_task.await;
                match backup_result {
                    Ok(res) => {
                        for msg in &res.log_lines {
                            let _ = state
                                .db
                                .insert_job_log(
                                    &job_id,
                                    &JobLogLine {
                                        ts: now.clone(),
                                        level: "info".to_string(),
                                        msg: msg.clone(),
                                    },
                                )
                                .await;
                        }

                        let _ = state
                            .db
                            .finish_backup(
                                &backup_id,
                                &res.status,
                                &now,
                                res.artifact_path.as_deref(),
                                res.size_bytes,
                                None,
                            )
                            .await;

                        stack_summary.insert("backup".to_string(), res.summary_json);
                        services_kept_stopped_for_apply = res.services_kept_stopped;

                        if res.status == "success" {
                            backup_id_for_cleanup = Some((
                                backup_id,
                                stack.backup.retention.delete_after_stable_seconds,
                            ));
                        }
                    }
                    Err(e) => {
                        let err = e.to_string();
                        let _ = state
                            .db
                            .finish_backup(&backup_id, "failed", &now, None, None, Some(&err))
                            .await;
                        let _ = state
                            .db
                            .insert_job_log(
                                &job_id,
                                &JobLogLine {
                                    ts: now.clone(),
                                    level: "warn".to_string(),
                                    msg: format!("backup failed: {err}"),
                                },
                            )
                            .await;

                        stack_summary
                            .insert("backup".to_string(), json!({"status":"failed","error":err}));

                        if crate::update_stop::is_requested(&e) {
                            final_status = if recovery_store.clear().await.is_ok() {
                                "cancelled".to_string()
                            } else {
                                "failed".to_string()
                            };
                            stack_summary.insert(
                                job_kind.summary_key().to_string(),
                                json!({"stopRequested": true}),
                            );
                            stack_summaries.push(serde_json::Value::Object(stack_summary));
                            processed_stacks = processed_stacks.saturating_add(1);
                            break;
                        }

                        if backup_settings.require_success || backup::is_safety_failure(&e) {
                            final_status = "failed".to_string();
                            stack_summaries.push(serde_json::Value::Object(stack_summary));
                            processed_stacks = processed_stacks.saturating_add(1);
                            latest_progress = make_job_progress(
                                "apply",
                                job_kind.processed_stacks_message(processed_stacks, total_stacks),
                                processed_stacks,
                                total_stacks,
                                Some(stack_id.clone()),
                                now_rfc3339().unwrap_or_else(|_| {
                                    time::OffsetDateTime::now_utc().to_string()
                                }),
                            );
                            if let Err(err) =
                                persist_job_progress(&state, &job_id, &latest_progress).await
                            {
                                tracing::warn!(
                                    job_id = %job_id,
                                    error = %err,
                                    "failed to persist update progress"
                                );
                            }
                            break;
                        }
                    }
                }
                if let Some(error) = prepare_error.take() {
                    let restored = if services_kept_stopped_for_apply.is_empty() {
                        Ok(())
                    } else {
                        backup::restore_services_after_failed_apply(
                            &*state.runner,
                            &state.config.compose_bin,
                            state.config.docker_config_path.as_deref(),
                            &stack,
                            &services_kept_stopped_for_apply,
                        )
                        .await
                    };
                    let cleanup = recovery_store.clear().await;
                    final_status = if prepare_stop_requested && restored.is_ok() && cleanup.is_ok() {
                        "cancelled".to_string()
                    } else {
                        "failed".to_string()
                    };
                    stack_summary.insert(
                        job_kind.summary_key().to_string(),
                        json!({
                            "error": error.to_string(),
                            "failureStep": "pull_services",
                            "stopRequested": prepare_stop_requested,
                            "recoveryError": restored.err().or_else(|| cleanup.err()).map(|error| error.to_string()),
                        }),
                    );
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    break;
                }
            } else {
                stack_summary.insert(
                    "backup".to_string(),
                    if no_actionable_services_after_anomaly_skip {
                        json!({"status":"skipped","reason":"no_actionable_services_after_anomaly_skip"})
                    } else if no_actionable_services {
                        json!({"status":"skipped","reason":"no_actionable_services"})
                    } else if req.mode.as_str() != "apply" {
                        json!({"status":"skipped","reason":"dry_run"})
                    } else {
                        json!({"status":"skipped","reason":"disabled"})
                    },
                );
            }

            let apply_base_progress = if backup_requested {
                0.75
            } else {
                UPDATE_STACK_BASE_PROGRESS
            };
            let apply_span = if backup_requested {
                0.20
            } else {
                UPDATE_STACK_APPLY_SPAN
            };
            latest_progress = make_job_progress_with_percent(
                "apply",
                job_kind.applying_stack_message(stack_id),
                processed_stacks,
                total_stacks,
                Some(stack_id.clone()),
                now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                update_progress_percent(processed_stacks, total_stacks, apply_base_progress),
            );
            if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
            }

            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::unbounded_channel::<updater::UpdateProgressEvent>();
            let progress_state = state.clone();
            let progress_job_id = job_id.clone();
            let progress_stack_id = stack_id.clone();
            let processed_stacks_for_progress = processed_stacks;
            let total_stacks_for_progress = total_stacks;
            let apply_base_progress_for_task = apply_base_progress;
            let apply_span_for_task = apply_span;
            let progress_semantics = if job_kind == TransitionJobKind::Update {
                UpdateProgressSemantics::VerifiedOnlyBatch
            } else {
                UpdateProgressSemantics::Legacy
            };
            let progress_task = tokio::spawn(async move {
                let mut last_percent = update_progress_percent(
                    processed_stacks_for_progress,
                    total_stacks_for_progress,
                    apply_base_progress_for_task,
                );
                let mut last_planned_percent = Some(Some(last_percent));
                let mut last_emit = std::time::Instant::now()
                    .checked_sub(Duration::from_secs(5))
                    .unwrap_or_else(std::time::Instant::now);

                while let Some(evt) = progress_rx.recv().await {
                    let snapshot = update_progress_snapshot(
                        &evt,
                        progress_semantics,
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        last_percent,
                        apply_base_progress_for_task,
                        apply_span_for_task,
                    );
                    let next_percent = snapshot.percent;
                    let next_planned_percent = snapshot.planned_percent;

                    let force_emit = matches!(
                        evt.step,
                        updater::UpdateProgressStep::PullDone
                            | updater::UpdateProgressStep::UpDone
                            | updater::UpdateProgressStep::HealthFailed
                            | updater::UpdateProgressStep::HealthDone
                            | updater::UpdateProgressStep::TargetTagPullDone
                            | updater::UpdateProgressStep::SyncTagDone
                            | updater::UpdateProgressStep::PullTagsDone
                            | updater::UpdateProgressStep::ServiceDone
                    ) || (progress_semantics == UpdateProgressSemantics::VerifiedOnlyBatch
                        && matches!(evt.step, updater::UpdateProgressStep::PullStart))
                        || evt.download.is_some();
                    let planned_changed = next_planned_percent != last_planned_percent;
                    let should_emit = force_emit
                        || planned_changed
                        || next_percent > last_percent
                        || last_emit.elapsed() >= Duration::from_millis(600);
                    if !should_emit {
                        continue;
                    }

                    last_percent = next_percent;
                    last_planned_percent = next_planned_percent;
                    last_emit = std::time::Instant::now();
                    let updated_at = now_rfc3339()
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                    let progress_message = if evt.message.contains(&evt.service_name) {
                        evt.message
                    } else {
                        format!("{} · {}", evt.service_name, evt.message)
                    };
                    let mut progress = make_job_progress_with_optional_plan(
                        "apply",
                        progress_message,
                        processed_stacks_for_progress,
                        total_stacks_for_progress,
                        Some(progress_stack_id.clone()),
                        updated_at,
                        next_percent,
                        Some(processed_stacks_for_progress),
                        Some(total_stacks_for_progress),
                        next_planned_percent,
                    );
                    progress.download = evt.download;
                    if let Err(e) =
                        persist_job_progress(&progress_state, &progress_job_id, &progress).await
                    {
                        tracing::warn!(
                            job_id = %progress_job_id,
                            error = %e,
                            "failed to persist streamed update progress"
                        );
                    }
                }
            });

            let retry_policy = updater::IdempotentRetryPolicy {
                max_attempts: state.config.update_idempotent_retry_max_attempts,
                base_ms: state.config.update_idempotent_retry_base_ms,
                max_ms: state.config.update_idempotent_retry_max_ms,
            };
            let apply_gate = DbUpdateApplyGate {
                db: state.db.clone(),
                job_id: job_id.clone(),
            };
            let update_outcome = updater::run_update_job_with_gate(
                &logging_runner,
                &state.config.compose_bin,
                state.config.docker_config_path.as_deref(),
                retry_policy,
                &stack,
                &req.scope,
                req.service_id.as_deref(),
                req.mode.as_str(),
                req.targets.as_deref(),
                req.allow_arch_mismatch,
                req.reason.as_str(),
                Some(state.config.dockrev_image_repo.as_str()),
                Some(progress_tx),
                backup_requested,
                &services_kept_stopped_for_apply,
                (job_kind == TransitionJobKind::Update && req.mode.as_str() == "apply")
                    .then_some(&apply_gate as &dyn updater::UpdateApplyGate),
            )
            .await;
            let _ = progress_task.await;
            match update_outcome {
                Ok(mut outcome) => {
                    if outcome.status != "success"
                    {
                        let restored = if services_kept_stopped_for_apply.is_empty() {
                            Ok(())
                        } else {
                            backup::restore_services_after_failed_apply(
                                &*state.runner,
                                &state.config.compose_bin,
                                state.config.docker_config_path.as_deref(),
                                &stack,
                                &services_kept_stopped_for_apply,
                            )
                            .await
                        };
                        let cleanup = recovery_store.clear().await;
                        if let Some(error) = restored.err().or_else(|| cleanup.err()) {
                            outcome.status = "failed".to_string();
                            if let Some(summary) = outcome.summary_json.as_object_mut() {
                                summary.insert("recoveryError".to_string(), json!(error.to_string()));
                            }
                        }
                    } else if let Err(error) = recovery_store.clear().await {
                        outcome.status = "failed".to_string();
                        if let Some(summary) = outcome.summary_json.as_object_mut() {
                            summary.insert("recoveryError".to_string(), json!(error.to_string()));
                        }
                    }
                    if outcome.status == "success"
                        && !planned_service_ids.is_empty()
                        && let Some(project) = state.db.get_stack_compose_project(stack_id).await?
                    {
                        let settled_at = now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
                        let settle_services_by_id = state
                            .db
                            .list_services_for_check(stack_id)
                            .await?
                            .into_iter()
                            .map(|service| (service.id.clone(), service))
                            .collect::<std::collections::HashMap<_, _>>();
                        let mut settled_services = 0usize;
                        for changed_service_id in &planned_service_ids {
                            let Some(svc_for_check) =
                                settle_services_by_id.get(changed_service_id).cloned()
                            else {
                                continue;
                            };
                            let Ok(img) = registry::ImageRef::parse(&svc_for_check.image_ref) else {
                                continue;
                            };
                            let runtime_observation = docker_compose_service_runtime_digest(
                                state.as_ref(),
                                &project,
                                &svc_for_check.name,
                                &repo_candidates(&img),
                            )
                            .await
                            .ok()
                            .flatten();
                            let Some(runtime_observation) = runtime_observation else {
                                continue;
                            };

                            let mut settle_outcome = service_check::check_service_and_persist(
                                &state,
                                &job_id,
                                &svc_for_check,
                                Some(runtime_observation.clone()),
                                &host_platform,
                                &settled_at,
                                &manifest_digest_cache,
                                &repo_tags_cache,
                            )
                            .await?;
                            let mut inference_ok = true;
                            if settle_outcome.current_digest.is_none() {
                                inference_ok = false;
                                service_check::persist_runtime_fallback_result(
                                    &state.db,
                                    &svc_for_check.id,
                                    &svc_for_check.image_ref,
                                    &svc_for_check.image_tag,
                                    &runtime_observation,
                                    &settled_at,
                                )
                                .await?;
                                settle_outcome.current_digest =
                                    Some(runtime_observation.digest.clone());
                                settle_outcome.current_resolved_tag = None;
                                settle_outcome.current_resolved_tags_json = None;
                                settle_outcome.candidate_tag = None;
                                settle_outcome.candidate_resolved_tag = None;
                                settle_outcome.candidate_digest = None;
                                settle_outcome.candidate_arch_match = None;
                                settle_outcome.candidate_arch_json = None;
                                settle_outcome.ignore_rule_id = None;
                                settle_outcome.ignore_reason = None;
                                settle_outcome.candidate_present = false;
                            }
                            let evt = json!({
                                "type": "update_state_settled",
                                "jobId": job_id,
                                "ts": settled_at,
                                "stackId": stack_id,
                                "serviceId": svc_for_check.id,
                                "serviceName": svc_for_check.name,
                                "runtimeDigest": runtime_observation.digest,
                                "runtimeStartedAt": runtime_observation.started_at,
                                "candidatePresent": settle_outcome.candidate_present,
                                "inferenceOk": inference_ok,
                            });
                            state
                                .db
                                .insert_job_log(
                                    &job_id,
                                    &JobLogLine {
                                        ts: settled_at.clone(),
                                        level: "event".to_string(),
                                        msg: evt.to_string(),
                                    },
                                )
                                .await?;
                            settled_services += 1;

                            enqueue_snapshot_for_image_ref(
                                &state,
                                &svc_for_check.image_ref,
                                &runtime_observation.digest,
                                &host_platform,
                                "update_digest_changed",
                            )
                            .await;
                        }
                        if settled_services > 0 {
                            state.db.update_stack_last_check_at(stack_id, &settled_at).await?;
                        }
                    }
                    final_status = outcome.status.clone();
                    stack_summary.insert(job_kind.summary_key().to_string(), outcome.summary_json);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        job_kind.processed_stacks_message(processed_stacks, total_stacks),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to persist update progress"
                        );
                    }

                    if final_status != "success" {
                        break;
                    }

                    if let Some(b) = backup_id_for_cleanup.take() {
                        backups_to_cleanup.push(b);
                    }
                }
                Err(e) => {
                    let stop_requested = crate::update_stop::is_requested(&e);
                    let restored = if services_kept_stopped_for_apply.is_empty() {
                        Ok(())
                    } else {
                        backup::restore_services_after_failed_apply(
                            &*state.runner,
                            &state.config.compose_bin,
                            state.config.docker_config_path.as_deref(),
                            &stack,
                            &services_kept_stopped_for_apply,
                        )
                        .await
                    };
                    let cleanup = recovery_store.clear().await;
                    let recovery_error = restored.err().or_else(|| cleanup.err());
                    final_status = if stop_requested && recovery_error.is_none() {
                        "cancelled".to_string()
                    } else {
                        "failed".to_string()
                    };
                    let mut update_summary = json!({"error": e.to_string(), "stopRequested": stop_requested});
                    if let Some(error) = recovery_error
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        obj.insert("recoveryError".to_string(), json!(error.to_string()));
                    }
                    if let Some(step_failure) = e.downcast_ref::<updater::UpdateStepFailure>()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        if let Some(partial) = step_failure.partial_summary.as_ref()
                            && let Some(partial_obj) = partial.as_object()
                        {
                            for (key, value) in partial_obj {
                                obj.entry(key.clone()).or_insert_with(|| value.clone());
                            }
                        }
                        obj.insert(
                            "failureStep".to_string(),
                            json!(step_failure.step.clone()),
                        );
                        obj.insert("retry".to_string(), json!(step_failure.retry.clone()));
                        obj.insert(
                            "lastError".to_string(),
                            json!(step_failure.last_error.clone()),
                        );
                    }
                    if !skipped_version_anomaly.is_empty()
                        && let Some(obj) = update_summary.as_object_mut()
                    {
                        obj.insert(
                            "skippedVersionAnomaly".to_string(),
                            serde_json::Value::Array(skipped_version_anomaly.clone()),
                        );
                    }
                    stack_summary.insert(job_kind.summary_key().to_string(), update_summary);
                    stack_summaries.push(serde_json::Value::Object(stack_summary));
                    processed_stacks = processed_stacks.saturating_add(1);
                    latest_progress = make_job_progress(
                        "apply",
                        job_kind.processed_stacks_message(processed_stacks, total_stacks),
                        processed_stacks,
                        total_stacks,
                        Some(stack_id.clone()),
                        now_rfc3339()
                            .unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
                    );
                    if let Err(err) = persist_job_progress(&state, &job_id, &latest_progress).await
                    {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %err,
                            "failed to persist update progress"
                        );
                    }
                    break;
                }
            }
        }

        let terminal_status = normalize_transition_outcome_status(job_kind, &final_status);
        latest_progress = make_job_progress(
            "done",
            transition_terminal_message(job_kind, &terminal_status, &stack_summaries),
            processed_stacks,
            total_stacks,
            None,
            now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()),
        );
        if let Err(e) = persist_job_progress(&state, &job_id, &latest_progress).await {
            tracing::warn!(job_id = %job_id, error = %e, "failed to persist update progress");
        }

        Ok((
            terminal_status,
            stack_summaries,
            backups_to_cleanup,
            latest_progress,
        ))
    }
    .await;

    let (final_status, stack_summaries, backups_to_cleanup, final_summary, finished_at) =
        match outcome {
            Ok((final_status, stack_summaries, backups_to_cleanup, progress)) => {
                let progress_json = serde_json::to_value(&progress)?;
                let final_summary = json!({
                    "mode": job_kind.summary_mode(&req.mode),
                    "targets": &req.targets,
                    "stacks": stack_summaries.clone(),
                    "progress": progress_json,
                });
                let finished_at = now_rfc3339()?;
                (
                    final_status,
                    stack_summaries,
                    backups_to_cleanup,
                    final_summary,
                    finished_at,
                )
            }
            Err(err) => {
                let finished_at = now_rfc3339()?;
                let stop_requested = crate::update_stop::is_requested(&err);
                let progress = make_job_progress(
                    "done",
                    if stop_requested {
                        "update cancelled".to_string()
                    } else {
                        job_kind.failed_message().to_string()
                    },
                    0,
                    0,
                    None,
                    finished_at.clone(),
                );
                let progress_json = serde_json::to_value(&progress)?;
                let _ = persist_job_progress(&state, &job_id, &progress).await;
                let _ = state
                    .db
                    .insert_job_log(
                        &job_id,
                        &JobLogLine {
                            ts: finished_at.clone(),
                            level: "error".to_string(),
                            msg: if stop_requested {
                                format!("update cancelled: {err}")
                            } else {
                                format!("{}: {err}", job_kind.failed_message())
                            },
                        },
                    )
                    .await;
                let final_summary = json!({
                    "mode": job_kind.summary_mode(&req.mode),
                    "targets": &req.targets,
                    "error": err.to_string(),
                    "stopRequested": stop_requested,
                    "progress": progress_json,
                });
                (
                    if stop_requested {
                        "cancelled".to_string()
                    } else {
                        "failed".to_string()
                    },
                    Vec::new(),
                    Vec::new(),
                    final_summary,
                    finished_at,
                )
            }
        };

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

    state
        .db
        .finish_job(&job_id, &final_status, &finished_at, &final_summary)
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

    if should_notify {
        let notify_state = state.clone();
        let notify_job_id = job_id.clone();
        let notify_status = final_status.clone();
        let notify_now = finished_at.clone();
        let notify_summary = notify_summary.clone();
        tokio::spawn(async move {
            let _ = notify::notify_job_updated(
                notify_state.as_ref(),
                &notify_job_id,
                &notify_status,
                &notify_now,
                &notify_summary,
            )
            .await;
        });
    }

    state.update_stop_hub.remove(&job_id);
    Ok(())
}

fn combined_backup_pull_percent(
    processed_stacks: u32,
    total_stacks: u32,
    pull_branch_percent: &AtomicU32,
    backup_branch_percent: &AtomicU32,
) -> u32 {
    if total_stacks == 0 {
        return 0;
    }
    let pull = pull_branch_percent.load(Ordering::Relaxed).min(10_000) as f64 / 10_000.0;
    let backup = backup_branch_percent.load(Ordering::Relaxed).min(10_000) as f64 / 10_000.0;
    (((processed_stacks as f64 + pull * 0.35 + backup * 0.40) / total_stacks as f64) * 100.0)
        .floor() as u32
}

async fn record_update_tag_history(state: &AppState, req: &TriggerUpdateRequest, now: &str) {
    let Ok(targets) = requested_update_targets(req) else {
        return;
    };
    if targets.is_empty() {
        return;
    }
    let targets_by_service = targets
        .into_iter()
        .map(|target| (target.service_id.clone(), target))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut stack_ids = std::collections::BTreeSet::new();
    for service_id in targets_by_service.keys() {
        match state.db.get_service_stack_id(service_id).await {
            Ok(Some(stack_id)) => {
                stack_ids.insert(stack_id);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    service_id = %service_id,
                    "failed to resolve service stack for tag history"
                );
            }
        }
    }
    for stack_id in stack_ids {
        let Ok(Some(stack)) = state.db.get_stack(&stack_id).await else {
            continue;
        };
        for service in stack.services {
            let Some(target) = targets_by_service.get(&service.id) else {
                continue;
            };
            let Some(image_repo) =
                crate::snapshot_worker::image_repo_from_image_ref(&service.image.reference)
            else {
                continue;
            };
            let tag = target.target_tag.trim();
            if tag.is_empty() {
                continue;
            }
            if let Err(err) = state
                .db
                .upsert_service_tag_history(&service.id, &image_repo, tag, "update", now)
                .await
            {
                tracing::warn!(
                    error = %err,
                    service_id = %service.id,
                    "failed to record update tag history"
                );
            }
        }
    }
}

fn should_record_update_tag_history(req: &TriggerUpdateRequest, final_status: &str) -> bool {
    final_status == "success" && matches!(&req.mode, UpdateMode::Apply)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_req(mode: UpdateMode) -> TriggerUpdateRequest {
        TriggerUpdateRequest {
            scope: JobScope::Service,
            stack_id: Some("stack_1".to_string()),
            service_id: Some("svc_1".to_string()),
            target_tag: Some("5.2".to_string()),
            target_digest: Some("sha256:abc".to_string()),
            pull_tags: Some(Vec::new()),
            targets: None,
            mode,
            allow_arch_mismatch: false,
            backup_mode: BackupMode::Inherit,
            reason: UpdateReason::Ui,
        }
    }

    #[test]
    fn tag_history_is_recorded_only_for_successful_apply_updates() {
        assert!(should_record_update_tag_history(
            &update_req(UpdateMode::Apply),
            "success"
        ));
        assert!(!should_record_update_tag_history(
            &update_req(UpdateMode::DryRun),
            "success"
        ));
        assert!(!should_record_update_tag_history(
            &update_req(UpdateMode::Apply),
            "failed"
        ));
    }

    #[test]
    fn backup_and_pull_progress_are_weighted_without_terminal_jump() {
        let pull = AtomicU32::new(5_000);
        let backup = AtomicU32::new(2_500);
        assert_eq!(combined_backup_pull_percent(0, 1, &pull, &backup), 27);

        pull.store(10_000, Ordering::Relaxed);
        backup.store(10_000, Ordering::Relaxed);
        assert_eq!(combined_backup_pull_percent(0, 1, &pull, &backup), 75);
        assert_eq!(combined_backup_pull_percent(1, 2, &pull, &backup), 87);
    }
}
