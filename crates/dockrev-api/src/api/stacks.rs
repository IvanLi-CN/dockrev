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

pub(super) async fn get_stack_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
) -> Result<Json<StackSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    if state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .is_none()
    {
        return Err(ApiError::not_found("stack not found"));
    }
    let auto_update_policy = state
        .db
        .get_auto_update_policy(
            "stack",
            &stack_id,
            crate::api::types::AutoUpdatePolicyMode::Override,
        )
        .await
        .map_err(map_internal)?;
    Ok(Json(StackSettingsResponse { auto_update_policy }))
}

pub(super) async fn put_stack_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(stack_id): Path<String>,
    Json(req): Json<StackSettingsRequest>,
) -> Result<Json<PutStackSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    if state
        .db
        .get_stack(&stack_id)
        .await
        .map_err(map_internal)?
        .is_none()
    {
        return Err(ApiError::not_found("stack not found"));
    }
    crate::auto_update::validate_policy_for_scope(&req.auto_update_policy, "stack")?;
    let now = now_rfc3339().map_err(map_internal)?;
    state
        .db
        .put_auto_update_policy("stack", &stack_id, &req.auto_update_policy, &now)
        .await
        .map_err(map_internal)?;
    Ok(Json(PutStackSettingsResponse { ok: true }))
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

async fn ensure_low_priority_snapshot_scheduled(
    state: &Arc<AppState>,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    reason: &str,
) {
    let Some(normalized) = snapshot_worker::normalize_digest(digest) else {
        return;
    };
    state
        .snapshot_worker
        .ensure_low_priority_snapshot_scheduled(image_repo, &normalized, host_platform, reason)
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

pub(crate) fn needs_version_inference_for_tags(
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

fn trim_nonempty(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn image_repo_key_from_image_ref(image_ref: &str) -> Option<String> {
    let trimmed = image_ref.trim();
    if trimmed.is_empty() {
        return None;
    }
    crate::snapshot_worker::image_repo_from_image_ref(trimmed).or_else(|| Some(trimmed.to_string()))
}

fn resolved_tag_for_digest_from_snapshot(
    snapshot_entry: &DigestSnapshotCacheValue,
    raw_tag: &str,
) -> Option<String> {
    let ready = crate::notify::notification_snapshot_is_ready(
        &snapshot_entry.snapshot,
        snapshot_entry.snapshot.checked_at.as_str(),
    );
    ready
        .then(|| {
            infer_semver_tags_from_snapshot(&snapshot_entry.snapshot, raw_tag)
                .into_iter()
                .next()
        })
        .flatten()
}

pub(super) fn merge_inferred_resolved_tag(
    persisted_resolved_tag: Option<&str>,
    inferred_first: Option<String>,
    scan_has_failures: bool,
    scan_is_complete: bool,
) -> Option<String> {
    if inferred_first.is_some() || (!scan_has_failures && scan_is_complete) {
        return inferred_first;
    }
    trim_nonempty(persisted_resolved_tag)
}

pub(super) async fn resolve_resolved_tag_for_digest(
    state: &Arc<AppState>,
    image_ref: &str,
    raw_tag: Option<&str>,
    digest: Option<&str>,
    persisted_resolved_tag: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(raw_tag) = raw_tag.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(trim_nonempty(persisted_resolved_tag));
    };

    if ignore::is_strict_semver(raw_tag) {
        return Ok(trim_nonempty(persisted_resolved_tag));
    }

    let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(image_ref) else {
        return Ok(trim_nonempty(persisted_resolved_tag));
    };
    let Some(digest) = digest.and_then(snapshot_worker::normalize_digest) else {
        return Ok(trim_nonempty(persisted_resolved_tag));
    };
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let snapshot_entry = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?
        .as_ref()
        .and_then(|(snapshot_json, checked_at, _updated_at)| {
            parse_digest_snapshot_row(snapshot_json, checked_at)
        });

    let Some(snapshot_entry) = snapshot_entry.as_ref() else {
        return Ok(trim_nonempty(persisted_resolved_tag));
    };

    let inferred_first = resolved_tag_for_digest_from_snapshot(snapshot_entry, raw_tag);
    let scan_has_failures = snapshot_entry.snapshot.scan.manifests_timeout > 0
        || snapshot_entry.snapshot.scan.manifests_error > 0;
    let scan_is_complete = snapshot_entry.snapshot.scan.repo_tags_considered
        >= snapshot_entry.snapshot.scan.repo_tags_total;

    Ok(merge_inferred_resolved_tag(
        persisted_resolved_tag,
        inferred_first,
        scan_has_failures,
        scan_is_complete,
    ))
}

pub(super) async fn resolve_current_running_resolved_tag(
    state: &Arc<AppState>,
    image_ref: &str,
    image_tag: &str,
    current_digest: Option<&str>,
    current_resolved_tag: Option<&str>,
) -> Result<Option<String>, ApiError> {
    resolve_resolved_tag_for_digest(
        state,
        image_ref,
        Some(image_tag),
        current_digest,
        current_resolved_tag,
    )
    .await
}

pub(super) async fn resolve_candidate_resolved_tag(
    state: &Arc<AppState>,
    service_id: &str,
    image_ref: &str,
    current_tag: &str,
    candidate_tag: Option<&str>,
    candidate_digest: Option<&str>,
    candidate_resolved_tag: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let resolved = resolve_resolved_tag_for_digest(
        state,
        image_ref,
        candidate_tag,
        candidate_digest,
        candidate_resolved_tag,
    )
    .await?;
    let raw_candidate_tag = candidate_tag.unwrap_or_default();
    if crate::db::stable_candidate_display_tag(raw_candidate_tag, resolved.as_deref().unwrap_or(""))
        .is_some()
        || !crate::db::candidate_tag_allows_settled_fallback(raw_candidate_tag)
    {
        return Ok(resolved);
    }

    let Some(candidate_digest) = candidate_digest.and_then(snapshot_worker::normalize_digest)
    else {
        return Ok(resolved);
    };
    let Some(notification_image_ref) = trim_nonempty(Some(image_ref)) else {
        return Ok(resolved);
    };
    let notification_tags = state
        .db
        .list_stable_candidate_display_tags_for_notification_targets(&[(
            service_id.to_string(),
            notification_image_ref,
            current_tag.to_string(),
            candidate_digest,
        )])
        .await
        .map_err(map_internal)?;
    let fallback = notification_tags.values().next().and_then(|tags| {
        crate::db::stable_candidate_display_tag_from_tags(raw_candidate_tag, tags)
    });
    Ok(fallback.or(resolved))
}

pub(super) async fn resolve_discovery_stable_tags_by_provenance(
    state: &Arc<AppState>,
    rows: &[crate::db::NewVersionDiscoveryRow],
) -> Result<
    std::collections::HashMap<(String, String, String, String), std::collections::BTreeSet<String>>,
    ApiError,
> {
    use std::collections::{BTreeSet, HashMap};

    let notification_targets = crate::db::new_version_discovery_notification_targets(rows);
    let notification_tags = state
        .db
        .list_stable_candidate_display_tags_for_notification_targets(&notification_targets)
        .await
        .map_err(map_internal)?;
    if rows.is_empty() {
        return Ok(notification_tags);
    }

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let snapshot_targets = rows
        .iter()
        .filter(|row| {
            crate::db::stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
                .is_none()
        })
        .filter_map(|row| {
            let image_repo = image_repo_key_from_image_ref(&row.image_ref)?;
            let digest = snapshot_worker::normalize_digest(&row.candidate_digest)?;
            Some((image_repo, digest))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let snapshot_rows = state
        .db
        .list_image_digest_tags_snapshots_for_targets(&host_platform, &snapshot_targets)
        .await
        .map_err(map_internal)?;
    let snapshot_cache = snapshot_rows
        .into_iter()
        .filter_map(|row| {
            parse_digest_snapshot_row(&row.snapshot_json, &row.checked_at)
                .map(|entry| ((row.image_repo, row.digest), entry))
        })
        .collect::<HashMap<_, _>>();

    let mut combined =
        HashMap::<(String, String, String, String), std::collections::BTreeSet<String>>::new();
    for row in rows {
        let Some(digest) = snapshot_worker::normalize_digest(&row.candidate_digest) else {
            continue;
        };
        let key = (
            row.service_id.clone(),
            row.image_ref.clone(),
            row.current_tag.clone(),
            digest.clone(),
        );
        if crate::db::stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
            .is_some()
        {
            continue;
        }
        if !crate::db::candidate_tag_allows_settled_fallback(&row.candidate_tag) {
            continue;
        }

        let snapshot_tag = image_repo_key_from_image_ref(&row.image_ref)
            .and_then(|image_repo| snapshot_cache.get(&(image_repo, digest.clone())))
            .and_then(|entry| resolved_tag_for_digest_from_snapshot(entry, &row.candidate_tag))
            .map(|tag| crate::db::canonical_visible_version_tag(&tag));
        if let Some(snapshot_tag) = snapshot_tag {
            combined.entry(key).or_default().insert(snapshot_tag);
            continue;
        }

        if let Some(tags) = notification_tags.get(&key) {
            combined.entry(key).or_insert_with(|| tags.clone());
        }
    }

    Ok(combined)
}

async fn apply_candidate_notification_fallbacks_to_services(
    state: &Arc<AppState>,
    services: &mut [Service],
) -> Result<(), ApiError> {
    use std::collections::HashMap;

    let targets = services
        .iter()
        .filter_map(|service| {
            let candidate = service.candidate.as_ref()?;
            if crate::db::stable_candidate_display_tag(
                &candidate.tag,
                candidate.resolved_tag.as_deref().unwrap_or(""),
            )
            .is_some()
            {
                return None;
            }
            if !crate::db::candidate_tag_allows_settled_fallback(&candidate.tag) {
                return None;
            }
            let digest = snapshot_worker::normalize_digest(&candidate.digest)?;
            let image_ref = trim_nonempty(Some(service.image.reference.as_str()))?;
            Some((
                service.id.clone(),
                image_ref,
                service.image.tag.clone(),
                digest,
            ))
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(());
    }

    let notification_tags = state
        .db
        .list_stable_candidate_display_tags_for_notification_targets(&targets)
        .await
        .map_err(map_internal)?;
    let notification_tags = notification_tags.into_iter().collect::<HashMap<_, _>>();

    for service in services {
        let Some(candidate) = service.candidate.as_mut() else {
            continue;
        };
        if crate::db::stable_candidate_display_tag(
            &candidate.tag,
            candidate.resolved_tag.as_deref().unwrap_or(""),
        )
        .is_some()
        {
            continue;
        }
        let Some(candidate_digest) = snapshot_worker::normalize_digest(&candidate.digest) else {
            continue;
        };
        let Some(image_ref) = trim_nonempty(Some(service.image.reference.as_str())) else {
            continue;
        };
        let key = (
            service.id.clone(),
            image_ref,
            service.image.tag.clone(),
            candidate_digest,
        );
        let Some(tags) = notification_tags.get(&key) else {
            continue;
        };
        let Some(resolved_tag) =
            crate::db::stable_candidate_display_tag_from_tags(&candidate.tag, tags)
        else {
            continue;
        };
        candidate.resolved_tag = Some(resolved_tag);
    }

    Ok(())
}

pub(super) async fn enrich_services_with_version_inference(
    state: &Arc<AppState>,
    services: &mut [Service],
) -> Result<(), ApiError> {
    use std::collections::HashMap;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut snapshot_cache: HashMap<String, Option<DigestSnapshotCacheValue>> = HashMap::new();
    let mut inflight_cache: HashMap<String, Option<String>> = HashMap::new();

    for svc in services.iter_mut() {
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
                ensure_low_priority_snapshot_scheduled(
                    state,
                    &image_repo,
                    &digest,
                    &host_platform,
                    reason,
                )
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
                let scan_has_failures = snapshot_entry.snapshot.scan.manifests_timeout > 0
                    || snapshot_entry.snapshot.scan.manifests_error > 0;
                let scan_is_complete = snapshot_entry.snapshot.scan.repo_tags_considered
                    >= snapshot_entry.snapshot.scan.repo_tags_total;

                // Only clear inferred tags when the snapshot scan completed without failures.
                // Error/all_failed snapshots are persisted best-effort and must not wipe the last
                // known good inference values. Additionally, snapshot scans can be deliberately
                // truncated; only treat an "empty inferred tag set" as authoritative when the
                // scan is complete.
                if inferred_first.is_some() || (!scan_has_failures && scan_is_complete) {
                    if for_candidate {
                        if let Some(candidate) = svc.candidate.as_mut() {
                            candidate.resolved_tag = merge_inferred_resolved_tag(
                                candidate.resolved_tag.as_deref(),
                                inferred_first,
                                scan_has_failures,
                                scan_is_complete,
                            );
                        }
                    } else {
                        svc.image.resolved_tag = merge_inferred_resolved_tag(
                            svc.image.resolved_tag.as_deref(),
                            inferred_first,
                            scan_has_failures,
                            scan_is_complete,
                        );
                        svc.image.resolved_tags = if tags.len() > 1 { Some(tags) } else { None };
                    }
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

    apply_candidate_notification_fallbacks_to_services(state, services).await?;

    Ok(())
}

#[derive(Clone)]
struct DiscoveryCountServiceContext {
    image_ref: String,
    current_digest: String,
    current_display_tag: String,
    current_tag: String,
}

pub(super) async fn enrich_services_with_new_version_discovery_counts(
    state: &Arc<AppState>,
    services: &mut [Service],
) -> Result<(), ApiError> {
    use std::collections::HashMap;

    let contexts = services
        .iter()
        .filter(|service| service.candidate.is_some())
        .map(|service| {
            (
                service.id.clone(),
                DiscoveryCountServiceContext {
                    image_ref: crate::db::normalize_discovery_key(Some(
                        service.image.reference.as_str(),
                    )),
                    current_digest: crate::db::normalize_discovery_key(
                        service.image.digest.as_deref(),
                    ),
                    current_display_tag: crate::db::normalize_discovery_key(
                        service
                            .image
                            .resolved_tag
                            .as_deref()
                            .or(Some(service.image.tag.as_str())),
                    ),
                    current_tag: crate::db::normalize_discovery_key(Some(
                        service.image.tag.as_str(),
                    )),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for service in services.iter_mut() {
        service.new_version_discovery_count = None;
    }
    if contexts.is_empty() {
        return Ok(());
    }

    let discovery_rows = state
        .db
        .list_new_version_discoveries_for_services(&contexts.keys().cloned().collect::<Vec<_>>())
        .await
        .map_err(map_internal)?;
    let effective_stable_tags_by_provenance =
        resolve_discovery_stable_tags_by_provenance(state, &discovery_rows).await?;
    let rows_by_service = discovery_rows.into_iter().fold(
        HashMap::<String, Vec<crate::db::NewVersionDiscoveryRow>>::new(),
        |mut acc, row| {
            acc.entry(row.service_id.clone()).or_default().push(row);
            acc
        },
    );

    for service in services.iter_mut() {
        let Some(context) = contexts.get(&service.id) else {
            continue;
        };
        let Some(rows) = rows_by_service.get(&service.id) else {
            continue;
        };

        let count = crate::db::count_new_version_discoveries_from_rows(
            rows.iter(),
            &context.image_ref,
            &context.current_digest,
            &context.current_display_tag,
            &context.current_tag,
            &effective_stable_tags_by_provenance,
        );
        service.new_version_discovery_count = (count > 0).then_some(count);
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
    enrich_services_with_version_inference(&state, &mut stack.services).await?;
    enrich_services_with_new_version_discovery_counts(&state, &mut stack.services).await?;

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
