use std::{collections::HashMap, sync::Arc};

use crate::{
    api::types::{JobLogLine, ServiceDigestTagsScanSummary, ServiceDigestTagsSnapshotResponse},
    ignore, registry,
    state::AppState,
};

#[derive(Clone, Debug)]
pub(crate) struct ServiceCheckOutcome {
    pub current_digest: Option<String>,
    pub current_resolved_tag: Option<String>,
    pub current_resolved_tags_json: Option<String>,
    pub current_resolved_tags: Option<Vec<String>>,
    pub candidate_tag: Option<String>,
    pub candidate_resolved_tag: Option<String>,
    pub candidate_digest: Option<String>,
    pub candidate_arch_match: Option<String>,
    pub candidate_arch_json: Option<String>,
    pub ignore_rule_id: Option<String>,
    pub ignore_reason: Option<String>,
    pub candidate_present: bool,
}

const RESOLVED_TAG_INFER_SCAN_LIMIT: usize = 60;

pub(crate) type ManifestDigestCache =
    Arc<tokio::sync::RwLock<HashMap<String, (Option<String>, Option<String>)>>>;

pub(crate) fn new_manifest_digest_cache() -> ManifestDigestCache {
    Arc::new(tokio::sync::RwLock::new(HashMap::new()))
}

async fn infer_semver_tags_for_digests(
    state: &Arc<AppState>,
    img: &registry::ImageRef,
    tags: &[String],
    host_platform: &str,
    wanted_digests: &[String],
    manifest_digest_cache: &ManifestDigestCache,
) -> Vec<String> {
    let wanted: Vec<String> = wanted_digests
        .iter()
        .map(|d| d.trim().to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }

    let mut semver_tags: Vec<(semver::Version, String)> = tags
        .iter()
        .filter_map(|t| ignore::parse_version(t).map(|v| (v, t.clone())))
        .collect();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let mut matched: Vec<String> = Vec::new();
    for (_v, tag) in semver_tags.into_iter().take(RESOLVED_TAG_INFER_SCAN_LIMIT) {
        let cache_key = format!("{}/{}:{}", img.registry, img.name, tag);
        let cached = {
            let cache = manifest_digest_cache.read().await;
            cache.get(&cache_key).cloned()
        };
        let (digest, platform_digest) = if let Some(v) = cached {
            v
        } else {
            let mut manifest = state
                .registry
                .get_manifest(img, &tag, host_platform)
                .await
                .ok();
            if manifest.is_none() {
                // Best-effort retry: registry lookups can fail transiently and we still want
                // resolvedTag inference to be useful.
                manifest = state
                    .registry
                    .get_manifest(img, &tag, host_platform)
                    .await
                    .ok();
            }
            let (d, pd) = manifest
                .map(|m| (m.digest, m.platform_digest))
                .unwrap_or((None, None));
            let (digest, platform_digest) = {
                let mut cache = manifest_digest_cache.write().await;
                let entry = cache
                    .entry(cache_key)
                    .or_insert_with(|| (d.clone(), pd.clone()));
                (entry.0.clone(), entry.1.clone())
            };
            (digest, platform_digest)
        };

        let digest_matches = digest
            .as_deref()
            .is_some_and(|d| wanted.iter().any(|w| d.trim().eq_ignore_ascii_case(w)))
            || platform_digest
                .as_deref()
                .is_some_and(|d| wanted.iter().any(|w| d.trim().eq_ignore_ascii_case(w)));
        if digest_matches {
            matched.push(tag);
        }
    }

    matched
}

pub(crate) async fn check_service_and_persist(
    state: &Arc<AppState>,
    job_id: &str,
    svc: &crate::db::ServiceForCheck,
    runtime_digest: Option<String>,
    host_platform: &str,
    now: &str,
    manifest_digest_cache: &ManifestDigestCache,
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
                candidate_resolved_tag: None,
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
                candidate_resolved_tag: None,
                candidate_digest: None,
                candidate_arch_match: None,
                candidate_arch_json: None,
                ignore_rule_id: None,
                ignore_reason: None,
                candidate_present: false,
            });
        }
    };

    let mut current_manifest = state
        .registry
        .get_manifest(&img, &svc.image_tag, host_platform)
        .await
        .ok();
    if current_manifest.is_none() {
        // Best-effort retry: the configured tag is the most important lookup for candidate
        // digest-only updates, so transient registry failures should not immediately erase it.
        current_manifest = state
            .registry
            .get_manifest(&img, &svc.image_tag, host_platform)
            .await
            .ok();
    }
    let current_digest_registry = current_manifest
        .as_ref()
        .and_then(|m| m.digest.clone().or(m.platform_digest.clone()));
    let effective_current_digest = runtime_digest.clone().or(current_digest_registry.clone());
    // Persist the best-known digest so that pinned tags and offline/missing compose projects
    // don't lose observability just because the runtime digest is unavailable.
    let current_digest = effective_current_digest.clone();

    // Candidate policy: only consider digest changes for the service's *current* tag (no cross-tag
    // upgrades). We only emit a candidate when the runtime digest is known and differs.
    let mut candidate_tag: Option<String> = None;
    let mut candidate_digest: Option<String> = None;
    let mut candidate_platform_digest: Option<String> = None;
    let mut candidate_arch_match: Option<String> = None;
    let mut candidate_arch_json: Option<String> = None;

    if runtime_digest.is_some()
        && let Some(m) = current_manifest.as_ref()
    {
        // Prefer the registry-provided digest (index/manifest digest) when available; fall back
        // to platform digest when that's the only option.
        let digest = m.digest.clone().or(m.platform_digest.clone());
        if let Some(digest) = digest {
            candidate_tag = Some(svc.image_tag.clone());
            candidate_digest = Some(digest);
            candidate_platform_digest = m.platform_digest.clone();
            let arch_match = registry::compute_arch_match(host_platform, &m.arch);
            candidate_arch_match = Some(arch_match.as_str().to_string());
            candidate_arch_json = Some(serde_json::to_string(&m.arch).unwrap_or_default());
        }
    }

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
            let mut resolved_tags = infer_semver_tags_for_digests(
                state,
                &img,
                &tags,
                host_platform,
                &[runtime_digest.to_string()],
                manifest_digest_cache,
            )
            .await;

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

    let candidate_resolved_tag = if let (Some(tag), Some(digest)) =
        (candidate_tag.as_deref(), candidate_digest.as_deref())
        && !ignore::is_strict_semver(tag)
    {
        let mut wanted_digests = vec![digest.to_string()];
        if let Some(platform_digest) = candidate_platform_digest.as_deref()
            && !platform_digest.trim().is_empty()
            && !platform_digest.eq_ignore_ascii_case(digest)
        {
            wanted_digests.push(platform_digest.to_string());
        }
        let mut resolved_tags = infer_semver_tags_for_digests(
            state,
            &img,
            &tags,
            host_platform,
            &wanted_digests,
            manifest_digest_cache,
        )
        .await;
        resolved_tags.retain(|t| t != tag);
        resolved_tags.first().cloned()
    } else {
        None
    };

    state
        .db
        .update_service_check_result(
            &svc.id,
            current_digest.clone(),
            current_resolved_tag.clone(),
            current_resolved_tags_json.clone(),
            candidate_tag.clone(),
            candidate_resolved_tag.clone(),
            candidate_digest.clone(),
            candidate_arch_match.clone(),
            candidate_arch_json.clone(),
            ignore_match.as_ref().map(|(id, _)| id.clone()),
            ignore_match.as_ref().map(|(_, r)| r.clone()),
            now,
            now,
        )
        .await?;

    // Persist best-effort digest->tags snapshot at scan-time so UI can remain deterministic and
    // avoid live registry fan-out (which may drift away from the last scan).
    if let Err(e) = persist_digest_tags_snapshots_best_effort(
        state,
        &svc.id,
        &img,
        &tags,
        host_platform,
        &svc.image_tag,
        current_digest.as_deref(),
        candidate_tag.as_deref(),
        candidate_digest.as_deref(),
        now,
    )
    .await
    {
        tracing::debug!(
            service_id = %svc.id,
            error = %e,
            "digest tags snapshot persistence failed (ignored)"
        );
    }

    Ok(ServiceCheckOutcome {
        current_digest,
        current_resolved_tag,
        current_resolved_tags_json,
        current_resolved_tags,
        candidate_tag,
        candidate_resolved_tag,
        candidate_digest,
        candidate_arch_match,
        candidate_arch_json,
        ignore_rule_id: ignore_match.as_ref().map(|(id, _)| id.clone()),
        ignore_reason: ignore_match.as_ref().map(|(_, r)| r.clone()),
        candidate_present,
    })
}

fn normalize_digest(input: &str) -> Option<String> {
    let t = input.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains(':') {
        return Some(t.to_string());
    }
    Some(format!("sha256:{t}"))
}

fn sort_tags_semver_then_lex_desc(tags: Vec<String>) -> Vec<String> {
    let mut semver_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut other_tags: Vec<String> = Vec::new();

    for tag in tags {
        if let Some(v) = ignore::parse_version(&tag) {
            semver_tags.push((v, tag));
        } else {
            other_tags.push(tag);
        }
    }

    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    other_tags.sort_by(|a, b| b.cmp(a));

    let mut out: Vec<String> = Vec::new();
    out.extend(semver_tags.into_iter().map(|(_, t)| t));
    out.extend(other_tags);
    out
}

fn pick_considered_tags_for_snapshot(
    repo_tags: &[String],
    anchors: &[String],
    depth: usize,
) -> Vec<String> {
    use std::collections::HashSet;

    let repo_tags_total = repo_tags.len();
    if repo_tags_total == 0 || depth == 0 {
        return Vec::new();
    }

    let repo_set: HashSet<&str> = repo_tags.iter().map(|t| t.as_str()).collect();

    let sorted = sort_tags_semver_then_lex_desc(repo_tags.to_vec());

    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Always try to include anchors if they exist in the repo tag set.
    for a in anchors {
        if out.len() >= depth {
            break;
        }
        let t = a.trim();
        if t.is_empty() {
            continue;
        }
        if !repo_set.contains(t) {
            continue;
        }
        if seen.insert(t.to_string()) {
            out.push(t.to_string());
        }
    }

    for t in sorted {
        if out.len() >= depth {
            break;
        }
        if seen.insert(t.clone()) {
            out.push(t);
        }
    }

    out
}

async fn scan_digest_tags_snapshot_best_effort(
    registry: Arc<dyn registry::RegistryClient>,
    img: registry::ImageRef,
    host_platform: &str,
    repo_tags: &[String],
    wanted_digest: &str,
    anchors: &[String],
) -> (Vec<String>, ServiceDigestTagsScanSummary) {
    use std::time::Duration;

    use tokio::{
        task::JoinSet,
        time::{Instant, timeout, timeout_at},
    };

    const SNAPSHOT_DEPTH: usize = 100;
    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(4);
    const MANIFEST_BUDGET: Duration = Duration::from_secs(12);
    const MANIFEST_CONCURRENCY: usize = 10;

    let wanted = wanted_digest.trim().to_string();
    let repo_tags_total = repo_tags.len();

    let considered = pick_considered_tags_for_snapshot(repo_tags, anchors, SNAPSHOT_DEPTH);
    let repo_tags_considered = considered.len();

    if wanted.is_empty() || repo_tags_considered == 0 {
        return (
            Vec::new(),
            ServiceDigestTagsScanSummary {
                repo_tags_total,
                repo_tags_considered,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        );
    }

    enum ScanOutcome {
        OkMatch(String),
        OkNoMatch,
        Timeout,
        Error,
    }

    let mut out: Vec<String> = Vec::new();
    let mut manifests_ok: usize = 0;
    let mut manifests_timeout: usize = 0;
    let mut manifests_error: usize = 0;

    let host_platform = host_platform.to_string();

    let mut join_set: JoinSet<ScanOutcome> = JoinSet::new();
    let mut queue = considered.into_iter();

    let spawn_one = |join_set: &mut JoinSet<ScanOutcome>,
                     tag: String,
                     registry: Arc<dyn registry::RegistryClient>,
                     img: registry::ImageRef,
                     host_platform: String,
                     wanted: String| {
        join_set.spawn(async move {
            match timeout(
                MANIFEST_TIMEOUT,
                registry.get_manifest(&img, &tag, &host_platform),
            )
            .await
            {
                Ok(Ok(m)) => {
                    let ok = m
                        .digest
                        .as_deref()
                        .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted))
                        || m.platform_digest
                            .as_deref()
                            .is_some_and(|v| v.trim().eq_ignore_ascii_case(&wanted));
                    if ok {
                        ScanOutcome::OkMatch(tag)
                    } else {
                        ScanOutcome::OkNoMatch
                    }
                }
                Ok(Err(_)) => ScanOutcome::Error,
                Err(_) => ScanOutcome::Timeout,
            }
        });
    };

    for _ in 0..MANIFEST_CONCURRENCY {
        let Some(tag) = queue.next() else { break };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    let deadline = Instant::now() + MANIFEST_BUDGET;
    while !join_set.is_empty() {
        let next = match timeout_at(deadline, join_set.join_next()).await {
            Ok(next) => next,
            Err(_) => {
                join_set.abort_all();
                break;
            }
        };

        let Some(joined) = next else { break };
        match joined {
            Ok(ScanOutcome::OkMatch(tag)) => {
                manifests_ok += 1;
                out.push(tag);
            }
            Ok(ScanOutcome::OkNoMatch) => {
                manifests_ok += 1;
            }
            Ok(ScanOutcome::Timeout) => {
                manifests_timeout += 1;
            }
            Ok(ScanOutcome::Error) => {
                manifests_error += 1;
            }
            Err(_) => {
                manifests_error += 1;
            }
        };

        let Some(tag) = queue.next() else {
            continue;
        };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
            wanted.clone(),
        );
    }

    // If the budget was exhausted (or tasks were aborted), treat remaining *considered* tags as
    // timeouts so the UI can warn that the result may be incomplete.
    let processed = manifests_ok + manifests_timeout + manifests_error;
    if processed < repo_tags_considered {
        manifests_timeout += repo_tags_considered - processed;
    }

    let sorted = sort_tags_semver_then_lex_desc(out);
    (
        sorted,
        ServiceDigestTagsScanSummary {
            repo_tags_total,
            repo_tags_considered,
            manifests_ok,
            manifests_timeout,
            manifests_error,
        },
    )
}

#[allow(clippy::too_many_arguments)]
async fn persist_digest_tags_snapshots_best_effort(
    state: &Arc<AppState>,
    service_id: &str,
    img: &registry::ImageRef,
    repo_tags: &[String],
    host_platform: &str,
    current_tag: &str,
    current_digest: Option<&str>,
    candidate_tag: Option<&str>,
    candidate_digest: Option<&str>,
    now: &str,
) -> anyhow::Result<()> {
    let mut digest_to_anchors: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    if let Some(d) = current_digest.and_then(normalize_digest) {
        digest_to_anchors
            .entry(d)
            .or_default()
            .push(current_tag.to_string());
    }

    if let (Some(tag), Some(digest)) = (candidate_tag, candidate_digest)
        && let Some(d) = normalize_digest(digest)
    {
        digest_to_anchors
            .entry(d)
            .or_default()
            .extend([tag.to_string(), current_tag.to_string()]);
    }

    for anchors in digest_to_anchors.values_mut() {
        anchors.retain(|t| !t.trim().is_empty());
        anchors.sort();
        anchors.dedup();
    }

    // If we couldn't determine any digests to snapshot, keep any existing snapshots instead of
    // pruning them away on a transient failure.
    if digest_to_anchors.is_empty() {
        return Ok(());
    }

    for (digest, anchors) in &digest_to_anchors {
        let (tags, scan) = scan_digest_tags_snapshot_best_effort(
            state.registry.clone(),
            img.clone(),
            host_platform,
            repo_tags,
            digest,
            anchors,
        )
        .await;

        let snapshot = ServiceDigestTagsSnapshotResponse {
            digest: digest.clone(),
            tags,
            checked_at: now.to_string(),
            scan,
        };
        let snapshot_json = serde_json::to_string(&snapshot)?;
        state
            .db
            .upsert_service_digest_tags_snapshot(service_id, digest, &snapshot_json, now, now)
            .await?;
    }

    let allowed_digests = digest_to_anchors.keys().cloned().collect::<Vec<_>>();
    let _deleted = state
        .db
        .delete_service_digest_tags_snapshots_except(service_id, &allowed_digests)
        .await?;
    Ok(())
}
