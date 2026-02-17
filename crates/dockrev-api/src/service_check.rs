use std::{collections::HashMap, sync::Arc};

use crate::{api::types::JobLogLine, candidates, ignore, registry, state::AppState};

#[derive(Clone, Debug)]
pub(crate) struct ServiceCheckOutcome {
    pub current_digest: Option<String>,
    pub current_resolved_tag: Option<String>,
    pub current_resolved_tags_json: Option<String>,
    pub current_resolved_tags: Option<Vec<String>>,
    pub candidate_tag: Option<String>,
    pub candidate_digest: Option<String>,
    pub candidate_arch_match: Option<String>,
    pub candidate_arch_json: Option<String>,
    pub ignore_rule_id: Option<String>,
    pub ignore_reason: Option<String>,
    pub candidate_present: bool,
}

pub(crate) async fn check_service_and_persist(
    state: &Arc<AppState>,
    job_id: &str,
    svc: &crate::db::ServiceForCheck,
    runtime_digest: Option<String>,
    host_platform: &str,
    now: &str,
    manifest_digest_cache: &mut HashMap<String, (Option<String>, Option<String>)>,
) -> anyhow::Result<ServiceCheckOutcome> {
    let img = match registry::ImageRef::parse(&svc.image_ref) {
        Ok(img) => img,
        Err(_) => {
            // Preserve existing behavior: don't mutate DB on invalid refs; keep an audit trail.
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: now.to_string(),
                        level: "warn".to_string(),
                        msg: format!("skip service {}: invalid image ref", svc.id),
                    },
                )
                .await?;
            return Ok(ServiceCheckOutcome {
                current_digest: None,
                current_resolved_tag: None,
                current_resolved_tags_json: None,
                current_resolved_tags: None,
                candidate_tag: None,
                candidate_digest: None,
                candidate_arch_match: None,
                candidate_arch_json: None,
                ignore_rule_id: None,
                ignore_reason: None,
                candidate_present: false,
            });
        }
    };

    let ignore_rules = state.db.list_ignore_rules_for_service(&svc.id).await?;
    let matchers = ignore_rules
        .iter()
        .map(|r| {
            let kind = ignore::IgnoreKind::parse(&r.matcher.kind);
            (
                r.id.clone(),
                ignore::IgnoreRuleMatcher {
                    kind,
                    value: r.matcher.value.clone(),
                },
            )
        })
        .collect::<Vec<_>>();

    let tags = match state.registry.list_tags(&img).await {
        Ok(t) => t,
        Err(e) => {
            // Preserve existing behavior: don't mutate DB if we can't read registry tags.
            state
                .db
                .insert_job_log(
                    job_id,
                    &JobLogLine {
                        ts: now.to_string(),
                        level: "warn".to_string(),
                        msg: format!("list tags failed for {}: {}", img.name, e),
                    },
                )
                .await?;
            return Ok(ServiceCheckOutcome {
                current_digest: None,
                current_resolved_tag: None,
                current_resolved_tags_json: None,
                current_resolved_tags: None,
                candidate_tag: None,
                candidate_digest: None,
                candidate_arch_match: None,
                candidate_arch_json: None,
                ignore_rule_id: None,
                ignore_reason: None,
                candidate_present: false,
            });
        }
    };

    let is_ignored = |tag: &str| matchers.iter().any(|(_, m)| m.matches(tag));
    let candidate_non_ignored = candidates::select_candidate_tag(&svc.image_tag, &tags, is_ignored);
    let candidate_any = candidates::select_candidate_tag(&svc.image_tag, &tags, |_| false);
    let mut candidate_tag = candidate_non_ignored.or(candidate_any);

    let current_digest_registry = state
        .registry
        .get_manifest(&img, &svc.image_tag, host_platform)
        .await
        .ok()
        .and_then(|m| m.digest);
    let effective_current_digest = runtime_digest.clone().or(current_digest_registry.clone());
    // Persist the best-known digest so that pinned tags and offline/missing compose projects
    // don't lose observability just because the runtime digest is unavailable.
    let current_digest = effective_current_digest.clone();

    let (
        candidate_digest_for_infer,
        candidate_platform_digest_for_infer,
        candidate_arch_match_for_infer,
        candidate_arch_json_for_infer,
    ) = if let Some(tag) = candidate_tag.as_deref() {
        match state.registry.get_manifest(&img, tag, host_platform).await {
            Ok(m) => {
                let arch_match = registry::compute_arch_match(host_platform, &m.arch);
                (
                    m.digest,
                    m.platform_digest,
                    Some(arch_match.as_str().to_string()),
                    Some(serde_json::to_string(&m.arch).unwrap_or_default()),
                )
            }
            Err(_) => (None, None, None, None),
        }
    } else {
        (None, None, None, None)
    };

    let mut candidate_digest = candidate_digest_for_infer;
    let mut candidate_platform_digest = candidate_platform_digest_for_infer;
    let mut candidate_arch_match = candidate_arch_match_for_infer;
    let mut candidate_arch_json = candidate_arch_json_for_infer;

    // If the candidate resolves to the same digest as current, there's no actionable update.
    //
    // Note: for floating tags (e.g. `latest`) and missing runtime digest, comparing against the
    // registry digest could be misleading (the tag may have already moved), so we only do the
    // "no update" fast-path when runtime digest is known OR the current tag is semver/pinned.
    let can_compare_current = runtime_digest.is_some() || ignore::is_strict_semver(&svc.image_tag);
    let current_matches_candidate = matches!(
        (effective_current_digest.as_deref(), candidate_digest.as_deref()),
        (Some(cur), Some(cand)) if cur == cand
    ) || matches!(
        (
            effective_current_digest.as_deref(),
            candidate_platform_digest.as_deref()
        ),
        (Some(cur), Some(cand)) if cur == cand
    );
    if can_compare_current && current_matches_candidate {
        candidate_tag = None;
        candidate_digest = None;
        candidate_platform_digest = None;
        candidate_arch_match = None;
        candidate_arch_json = None;
    }

    let candidate_present = candidate_tag.is_some();

    let mut ignore_match: Option<(String, String)> = None;
    if let Some(ref tag) = candidate_tag
        && let Some((rule_id, _)) = matchers.iter().find(|(_, m)| m.matches(tag))
    {
        ignore_match = Some((
            rule_id.clone(),
            format!("matched ignore rule for tag {tag}"),
        ));
    }

    let (current_resolved_tag, current_resolved_tags_json, current_resolved_tags) =
        if let Some(runtime_digest) = runtime_digest.as_deref()
            && !ignore::is_strict_semver(&svc.image_tag)
        {
            let mut semver_tags: Vec<(semver::Version, String)> = tags
                .iter()
                .filter_map(|t| ignore::parse_version(t).map(|v| (v, t.clone())))
                .collect();
            semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

            let mut resolved_tags: Vec<String> = Vec::new();
            for (_v, tag) in semver_tags.into_iter().take(60) {
                let (digest, platform_digest) =
                    if candidate_tag.as_deref().is_some_and(|c| c == tag.as_str())
                        && candidate_digest.is_some()
                    {
                        (candidate_digest.clone(), candidate_platform_digest.clone())
                    } else {
                        let cache_key = format!("{}/{}:{}", img.registry, img.name, tag);
                        if let Some(v) = manifest_digest_cache.get(&cache_key) {
                            v.clone()
                        } else {
                            let (d, pd) = state
                                .registry
                                .get_manifest(&img, &tag, host_platform)
                                .await
                                .ok()
                                .map(|m| (m.digest, m.platform_digest))
                                .unwrap_or((None, None));
                            manifest_digest_cache.insert(cache_key, (d.clone(), pd.clone()));
                            (d, pd)
                        }
                    };

                let digest_matches_runtime = digest.as_deref().is_some_and(|d| d == runtime_digest)
                    || platform_digest
                        .as_deref()
                        .is_some_and(|d| d == runtime_digest);
                if digest_matches_runtime {
                    resolved_tags.push(tag);
                }
            }

            resolved_tags.retain(|t| t != &svc.image_tag);
            let resolved_tag = resolved_tags.first().cloned();
            let resolved_tags_json = if resolved_tags.len() > 1 {
                serde_json::to_string(&resolved_tags).ok()
            } else {
                None
            };

            let resolved_tags_api = if resolved_tags.is_empty() {
                None
            } else {
                Some(resolved_tags)
            };

            (resolved_tag, resolved_tags_json, resolved_tags_api)
        } else {
            (None, None, None)
        };

    state
        .db
        .update_service_check_result(
            &svc.id,
            current_digest.clone(),
            current_resolved_tag.clone(),
            current_resolved_tags_json.clone(),
            candidate_tag.clone(),
            candidate_digest.clone(),
            candidate_arch_match.clone(),
            candidate_arch_json.clone(),
            ignore_match.as_ref().map(|(id, _)| id.clone()),
            ignore_match.as_ref().map(|(_, r)| r.clone()),
            now,
            now,
        )
        .await?;

    Ok(ServiceCheckOutcome {
        current_digest,
        current_resolved_tag,
        current_resolved_tags_json,
        current_resolved_tags,
        candidate_tag,
        candidate_digest,
        candidate_arch_match,
        candidate_arch_json,
        ignore_rule_id: ignore_match.as_ref().map(|(id, _)| id.clone()),
        ignore_reason: ignore_match.as_ref().map(|(_, r)| r.clone()),
        candidate_present,
    })
}
