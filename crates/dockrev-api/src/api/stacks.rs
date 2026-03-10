use super::*;

pub(super) async fn list_stacks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListStacksQuery>,
) -> Result<Json<ListStacksResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let stacks = state
        .db
        .list_stacks(parse_archived_filter(q.archived.as_deref())?)
        .await
        .map_err(map_internal)?;
    Ok(Json(ListStacksResponse { stacks }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListStacksQuery {
    archived: Option<String>,
}

pub(super) fn parse_archived_filter(
    input: Option<&str>,
) -> Result<crate::db::ArchivedFilter, ApiError> {
    match input.unwrap_or("exclude") {
        "exclude" => Ok(crate::db::ArchivedFilter::Exclude),
        "include" => Ok(crate::db::ArchivedFilter::Include),
        "only" => Ok(crate::db::ArchivedFilter::Only),
        other => Err(ApiError::invalid_argument(format!(
            "invalid archived filter: {other}"
        ))),
    }
}

pub(super) async fn enqueue_snapshot_for_image_ref(
    state: &Arc<AppState>,
    image_ref: &str,
    digest: &str,
    host_platform: &str,
    reason: &str,
) {
    let Some(repo) = snapshot_worker::image_repo_from_image_ref(image_ref) else {
        return;
    };
    let Some(normalized) = snapshot_worker::normalize_digest(digest) else {
        return;
    };
    state
        .snapshot_worker
        .enqueue(&repo, &normalized, host_platform, reason)
        .await;
}

pub(super) fn needs_version_inference(service: &Service) -> bool {
    if !ignore::is_strict_semver(&service.image.tag) {
        return true;
    }
    service
        .candidate
        .as_ref()
        .is_some_and(|c| !ignore::is_strict_semver(&c.tag))
}

pub(super) const VERSION_INFERENCE_REASON_CACHE_MISS: &str = "cache_miss";
pub(super) const VERSION_INFERENCE_REASON_CACHE_STALE: &str = "cache_stale";
pub(super) const VERSION_INFERENCE_REASON_ALL_FAILED: &str = "all_failed";
pub(super) const VERSION_INFERENCE_REASON_NEW_VERSION: &str = "new_version";
pub(super) const VERSION_INFERENCE_REASON_FORCE: &str = "force";
pub(super) const VERSION_INFERENCE_REASON_RUNNING: &str = "running";
pub(super) const VERSION_INFERENCE_REASON_NOT_REQUIRED: &str = "not_required";

pub(super) fn checked_at_is_older_than(checked_at: &str, min_age: time::Duration) -> bool {
    let parsed =
        time::OffsetDateTime::parse(checked_at, &time::format_description::well_known::Rfc3339);
    let now = time::OffsetDateTime::now_utc();
    match parsed {
        Ok(ts) => now - ts > min_age,
        Err(_) => true,
    }
}

pub(super) fn checked_at_is_stale(checked_at: &str) -> bool {
    checked_at_is_older_than(
        checked_at,
        time::Duration::days(snapshot_worker::SNAPSHOT_CACHE_TTL_DAYS),
    )
}

pub(super) fn checked_at_is_retryable_all_failed(checked_at: Option<&str>) -> bool {
    checked_at.is_none_or(|ts| {
        checked_at_is_older_than(
            ts,
            time::Duration::minutes(snapshot_worker::SNAPSHOT_ALL_FAILED_RETRY_MINUTES),
        )
    })
}

pub(super) fn needs_version_inference_for_tags(
    current_tag: &str,
    candidate_tag: Option<&str>,
) -> bool {
    if !ignore::is_strict_semver(current_tag) {
        return true;
    }
    candidate_tag.is_some_and(|tag| !ignore::is_strict_semver(tag))
}

#[derive(Clone)]
pub(super) struct DigestSnapshotCacheValue {
    pub(super) snapshot: ServiceDigestTagsSnapshotResponse,
    pub(super) checked_at: String,
}

pub(super) fn infer_semver_tags_from_snapshot(
    snapshot: &ServiceDigestTagsSnapshotResponse,
    raw_tag: &str,
) -> Vec<String> {
    let mut semver_tags = snapshot
        .tags
        .iter()
        .filter_map(|tag| ignore::parse_version(tag).map(|v| (v, tag.clone())))
        .collect::<Vec<_>>();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let raw_trim = raw_tag.trim();
    semver_tags
        .into_iter()
        .map(|(_, tag)| tag)
        .filter(|tag| tag != raw_trim)
        .collect()
}

pub(super) fn checked_at_latest(
    mut existing: Option<String>,
    candidate: Option<&str>,
) -> Option<String> {
    let Some(candidate) = candidate else {
        return existing;
    };
    if existing.as_deref().is_none_or(|cur| candidate > cur) {
        existing = Some(candidate.to_string());
    }
    existing
}

pub(super) fn parse_digest_snapshot_row(
    snapshot_json: &str,
    checked_at: &str,
) -> Option<DigestSnapshotCacheValue> {
    let mut parsed =
        serde_json::from_str::<ServiceDigestTagsSnapshotResponse>(snapshot_json).ok()?;
    if parsed.checked_at.trim().is_empty() {
        parsed.checked_at = checked_at.to_string();
    }
    Some(DigestSnapshotCacheValue {
        snapshot: parsed,
        checked_at: checked_at.to_string(),
    })
}

pub(super) async fn enrich_stack_with_version_inference(
    state: &Arc<AppState>,
    stack: &mut StackRecord,
) -> Result<(), ApiError> {
    use std::collections::HashMap;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut snapshot_cache: HashMap<String, Option<DigestSnapshotCacheValue>> = HashMap::new();
    let mut inflight_cache: HashMap<String, Option<String>> = HashMap::new();

    for svc in stack.services.iter_mut() {
        if !needs_version_inference(svc) {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        }

        let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&svc.image.reference)
        else {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        };

        let mut digest_targets: Vec<(String, bool)> = Vec::new();
        if !ignore::is_strict_semver(&svc.image.tag)
            && let Some(current_digest) = svc
                .image
                .digest
                .as_deref()
                .and_then(snapshot_worker::normalize_digest)
        {
            digest_targets.push((current_digest, false));
        }
        if let Some(candidate) = svc.candidate.as_ref()
            && !ignore::is_strict_semver(&candidate.tag)
            && let Some(candidate_digest) = snapshot_worker::normalize_digest(&candidate.digest)
            && !digest_targets
                .iter()
                .any(|(digest, _)| digest == &candidate_digest)
        {
            digest_targets.push((candidate_digest, true));
        }

        if digest_targets.is_empty() {
            svc.version_inference = Some(VersionInferenceState {
                status: "ready".to_string(),
                reason: Some(VERSION_INFERENCE_REASON_NOT_REQUIRED.to_string()),
                checked_at: None,
            });
            continue;
        }

        let mut pending = false;
        let mut pending_reason: Option<String> = None;
        let mut ready_reason: Option<String> = None;
        let mut latest_checked_at: Option<String> = None;

        for (digest, for_candidate) in digest_targets {
            let digest_key = format!("{image_repo}@{digest}@{host_platform}");

            let snapshot_entry = if let Some(cached) = snapshot_cache.get(&digest_key) {
                cached.clone()
            } else {
                let row = state
                    .db
                    .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
                    .await
                    .map_err(map_internal)?;
                let parsed = row
                    .as_ref()
                    .and_then(|(snapshot_json, checked_at, _updated_at)| {
                        parse_digest_snapshot_row(snapshot_json, checked_at)
                    });
                snapshot_cache.insert(digest_key.clone(), parsed.clone());
                parsed
            };

            let mut enqueue_reason: Option<&str> = None;
            if let Some(snapshot_entry) = snapshot_entry.as_ref() {
                latest_checked_at =
                    checked_at_latest(latest_checked_at, Some(snapshot_entry.checked_at.as_str()));
                let snapshot_all_failed =
                    snapshot_worker::snapshot_is_all_failed(&snapshot_entry.snapshot);
                if checked_at_is_stale(&snapshot_entry.checked_at) {
                    enqueue_reason = Some(VERSION_INFERENCE_REASON_CACHE_STALE);
                } else if snapshot_all_failed {
                    ready_reason
                        .get_or_insert_with(|| VERSION_INFERENCE_REASON_ALL_FAILED.to_string());
                    if checked_at_is_retryable_all_failed(Some(snapshot_entry.checked_at.as_str()))
                    {
                        enqueue_reason = Some(VERSION_INFERENCE_REASON_ALL_FAILED);
                    }
                }
            } else {
                enqueue_reason = Some(VERSION_INFERENCE_REASON_CACHE_MISS);
            }

            if let Some(reason) = enqueue_reason {
                pending = true;
                let _ = state
                    .snapshot_worker
                    .enqueue(&image_repo, &digest, &host_platform, reason)
                    .await;
                pending_reason.get_or_insert_with(|| reason.to_string());
            }

            let in_flight_reason = if let Some(reason) = inflight_cache.get(&digest_key) {
                reason.clone()
            } else {
                let reason = state
                    .snapshot_worker
                    .in_flight_reason(&image_repo, &digest, &host_platform)
                    .await;
                inflight_cache.insert(digest_key.clone(), reason.clone());
                reason
            };
            if let Some(in_flight_reason) = in_flight_reason {
                // `force` tasks are user-triggered best-effort refreshes for a single digest.
                // They must not leak into the stack/service-level pending/loading UX.
                if in_flight_reason != VERSION_INFERENCE_REASON_FORCE {
                    pending = true;
                    pending_reason.get_or_insert(in_flight_reason);
                }
            }

            if let Some(snapshot_entry) = snapshot_entry.as_ref() {
                let tags = if for_candidate {
                    let raw = svc
                        .candidate
                        .as_ref()
                        .map(|candidate| candidate.tag.as_str())
                        .unwrap_or_default();
                    infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, raw)
                } else {
                    infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, &svc.image.tag)
                };
                let inferred_first = tags.first().cloned();
                if for_candidate {
                    if let Some(candidate) = svc.candidate.as_mut() {
                        candidate.resolved_tag = inferred_first;
                    }
                } else {
                    svc.image.resolved_tag = inferred_first;
                    svc.image.resolved_tags = if tags.len() > 1 { Some(tags) } else { None };
                }
            }
        }

        let status = if pending { "pending" } else { "ready" };
        let reason = if pending {
            Some(pending_reason.unwrap_or_else(|| VERSION_INFERENCE_REASON_RUNNING.to_string()))
        } else {
            ready_reason
        };

        svc.version_inference = Some(VersionInferenceState {
            status: status.to_string(),
            reason,
            checked_at: latest_checked_at,
        });
    }

    Ok(())
}

pub(super) async fn get_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<GetStackResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(mut stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };
    enrich_stack_with_version_inference(&state, &mut stack).await?;

    Ok(Json(GetStackResponse {
        stack: StackResponse {
            id: stack.id,
            name: stack.name,
            compose: stack.compose,
            services: stack.services,
            archived: Some(stack.archived),
        },
    }))
}

pub(super) async fn register_stack_disabled(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let _user = require_user(&state, &headers).await?;
    Ok((
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({
            "error": "manual stack registration is disabled; use auto-discovery instead"
        })),
    ))
}

pub(super) async fn archive_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_stack_archived(&stack_id, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("stack not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn restore_stack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_stack_archived(&stack_id, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("stack not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn archive_service(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_service_archived(&service_id, true, Some("user_archive"), &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("service not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn restore_service(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let changed = state
        .db
        .set_service_archived(&service_id, false, None, &now)
        .await
        .map_err(map_internal)?;
    if !changed {
        return Err(ApiError::not_found("service not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
