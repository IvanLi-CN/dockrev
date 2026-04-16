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
    pub candidate_digest_changed: bool,
}

pub(crate) type ManifestDigestCache =
    Arc<tokio::sync::RwLock<HashMap<String, (Option<String>, Option<String>)>>>;
pub(crate) type RepoTagsCache = Arc<RepoTagsCacheInner>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeServiceObservation {
    pub digest: String,
    pub started_at: Option<String>,
    pub started_at_inferred: bool,
}

impl RuntimeServiceObservation {
    #[allow(dead_code)]
    pub(crate) fn digest_only(digest: impl Into<String>) -> Self {
        Self {
            digest: digest.into(),
            started_at: None,
            started_at_inferred: false,
        }
    }
}

pub(crate) struct RepoTagsCacheInner;

pub(crate) fn new_manifest_digest_cache() -> ManifestDigestCache {
    Arc::new(tokio::sync::RwLock::new(HashMap::new()))
}

pub(crate) fn new_repo_tags_cache() -> RepoTagsCache {
    Arc::new(RepoTagsCacheInner)
}

pub(crate) fn normalize_runtime_started_at(input: Option<&str>) -> Option<String> {
    let trimmed = input.map(str::trim).filter(|value| !value.is_empty())?;
    if trimmed.starts_with("0001-01-01T00:00:00") {
        return None;
    }
    Some(trimmed.to_string())
}

pub(crate) fn aggregate_runtime_started_at(
    values: &std::collections::BTreeSet<String>,
) -> (Option<String>, bool) {
    match values.len() {
        0 => (None, false),
        1 => (values.iter().next().cloned(), true),
        _ => (None, true),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn check_service_and_persist(
    state: &Arc<AppState>,
    job_id: &str,
    svc: &crate::db::ServiceForCheck,
    runtime: Option<RuntimeServiceObservation>,
    host_platform: &str,
    now: &str,
    manifest_digest_cache: &ManifestDigestCache,
    repo_tags_cache: &RepoTagsCache,
) -> anyhow::Result<ServiceCheckOutcome> {
    let _ = manifest_digest_cache;
    let _ = repo_tags_cache;
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
                candidate_digest_changed: false,
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

    let runtime_digest = runtime
        .as_ref()
        .map(|observation| observation.digest.clone());
    let runtime_started_at = normalize_runtime_started_at(
        runtime
            .as_ref()
            .and_then(|observation| observation.started_at.as_deref()),
    );

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
    let current_manifest_digest = current_manifest
        .as_ref()
        .and_then(|m| m.digest.clone().or(m.platform_digest.clone()));
    let current_digest_registry = current_manifest_digest.clone();
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
        candidate_arch_match = None;
        candidate_arch_json = None;
    }

    let candidate_present = candidate_tag.is_some();
    let candidate_digest_changed = candidate_digest.as_deref() != svc.candidate_digest.as_deref();
    // Only let an in-memory check supersede active notification records when the candidate state
    // is authoritative. Missing runtime state or transient registry failures can clear the service
    // row temporarily, but they should not reopen the same digest for re-notification.
    let candidate_state_authoritative =
        runtime_digest.is_some() && current_manifest_digest.is_some();

    let mut ignore_match: Option<(String, String)> = None;
    if let Some(ref tag) = candidate_tag
        && let Some((rule_id, _)) = matchers.iter().find(|(_, m)| m.matches(tag))
    {
        ignore_match = Some((
            rule_id.clone(),
            format!("matched ignore rule for tag {tag}"),
        ));
    }

    let current_digest_changed = current_digest.as_deref() != svc.current_digest.as_deref();
    let (current_resolved_tag, current_resolved_tags_json, current_resolved_tags) =
        if current_digest_changed {
            (None, None, None)
        } else {
            let resolved_tags_api = svc
                .current_resolved_tags_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                .and_then(|v| if v.is_empty() { None } else { Some(v) });
            (
                svc.current_resolved_tag.clone(),
                svc.current_resolved_tags_json.clone(),
                resolved_tags_api,
            )
        };

    let candidate_resolved_tag = if candidate_digest_changed {
        None
    } else {
        svc.candidate_resolved_tag.clone()
    };

    let existing_runtime_started_at =
        normalize_runtime_started_at(svc.current_runtime_started_at.as_deref());
    let observed_runtime_started_at = runtime
        .as_ref()
        .filter(|_| runtime_digest.as_deref() == current_digest.as_deref())
        .and_then(|observation| {
            observation
                .started_at_inferred
                .then_some(runtime_started_at)
        });
    let current_runtime_started_at = if current_digest_changed {
        observed_runtime_started_at.flatten()
    } else {
        observed_runtime_started_at.unwrap_or(existing_runtime_started_at)
    };

    state
        .db
        .update_service_check_result_with_runtime_started_at(
            &svc.id,
            current_digest.clone(),
            current_runtime_started_at,
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
    if candidate_state_authoritative {
        state
            .db
            .reconcile_service_new_version_notifications(
                &svc.id,
                &svc.image_ref,
                &svc.image_tag,
                candidate_digest.as_deref(),
                now,
            )
            .await?;
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
        candidate_digest_changed,
    })
}

pub(crate) async fn persist_runtime_fallback_result(
    db: &crate::db::Db,
    service_id: &str,
    _image_ref: &str,
    _image_tag: &str,
    runtime: &RuntimeServiceObservation,
    now: &str,
) -> anyhow::Result<()> {
    let current_runtime_started_at = if runtime.started_at_inferred {
        normalize_runtime_started_at(runtime.started_at.as_deref())
    } else {
        db.get_service_new_version_timeline_context(service_id)
            .await?
            .and_then(|context| {
                normalize_runtime_started_at(context.current_runtime_started_at.as_deref())
            })
    };
    db.update_service_check_result_with_runtime_started_at(
        service_id,
        Some(runtime.digest.clone()),
        current_runtime_started_at,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        now,
        now,
    )
    .await?;
    // This fallback means registry inference was inconclusive for the current runtime digest, so
    // keep any active notification record until a later authoritative check confirms the candidate
    // changed or truly disappeared.
    Ok(())
}

#[allow(dead_code)]
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

const SNAPSHOT_CONSIDERED_MAX: usize = 40;
const SNAPSHOT_NON_PARSEABLE_FALLBACK_TOPK: usize = 20;

fn pick_considered_tags_for_snapshot(repo_tags: &[String], anchors: &[String]) -> Vec<String> {
    use std::collections::HashSet;

    if repo_tags.is_empty() {
        return Vec::new();
    }

    let repo_set: HashSet<&str> = repo_tags.iter().map(|t| t.as_str()).collect();

    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Always include anchor tags first (when present in repo tags) so digest snapshots remain
    // stable for current/candidate references even if they are non-semver labels.
    for a in anchors {
        if out.len() >= SNAPSHOT_CONSIDERED_MAX {
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

    if out.len() >= SNAPSHOT_CONSIDERED_MAX {
        return out;
    }

    let mut parseable_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut non_parseable_tags: Vec<String> = Vec::new();
    for tag in repo_tags {
        if seen.contains(tag) {
            continue;
        }
        if let Some(v) = ignore::parse_version(tag) {
            parseable_tags.push((v, tag.clone()));
        } else {
            non_parseable_tags.push(tag.clone());
        }
    }

    parseable_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_v, tag) in parseable_tags {
        if out.len() >= SNAPSHOT_CONSIDERED_MAX {
            return out;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }

    non_parseable_tags.sort_by(|a, b| b.cmp(a));
    let max_non_parseable =
        SNAPSHOT_NON_PARSEABLE_FALLBACK_TOPK.min(SNAPSHOT_CONSIDERED_MAX.saturating_sub(out.len()));
    for tag in non_parseable_tags.into_iter().take(max_non_parseable) {
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }

    out
}

pub(crate) async fn scan_digest_tags_snapshot_best_effort(
    registry: Arc<dyn registry::RegistryClient>,
    img: registry::ImageRef,
    host_platform: &str,
    repo_tags: &[String],
    wanted_digest: &str,
    anchors: &[String],
) -> (Vec<String>, ServiceDigestTagsScanSummary) {
    scan_digest_tags_snapshot_best_effort_with_progress(
        registry,
        img,
        host_platform,
        repo_tags,
        wanted_digest,
        anchors,
        |_| {},
    )
    .await
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SnapshotScanProgress {
    pub processed: usize,
    pub task_total: usize,
    pub repo_total: usize,
    pub success: usize,
    pub timeout: usize,
    pub error: usize,
}

pub(crate) async fn scan_digest_tags_snapshot_best_effort_with_progress<F>(
    registry: Arc<dyn registry::RegistryClient>,
    img: registry::ImageRef,
    host_platform: &str,
    repo_tags: &[String],
    wanted_digest: &str,
    anchors: &[String],
    mut on_progress: F,
) -> (Vec<String>, ServiceDigestTagsScanSummary)
where
    F: FnMut(SnapshotScanProgress),
{
    use std::time::Duration;

    use tokio::{
        task::JoinSet,
        time::{Instant, timeout, timeout_at},
    };

    // The registry client already limits per-host concurrency. Keep the scan fan-out lower than
    // that limiter so tasks do not spend most of the timeout waiting for a permit.
    const MANIFEST_CONCURRENCY: usize = 4;
    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(12);
    const MANIFEST_BUDGET_MIN_SECS: u64 = 20;
    const MANIFEST_BUDGET_MAX_SECS: u64 = 90;

    let wanted = wanted_digest.trim().to_string();
    let repo_tags_total = repo_tags.len();

    let considered = pick_considered_tags_for_snapshot(repo_tags, anchors);
    let repo_tags_considered = considered.len();

    let mut report_progress = |processed: usize,
                               manifests_ok: usize,
                               manifests_timeout: usize,
                               manifests_error: usize| {
        on_progress(SnapshotScanProgress {
            processed,
            task_total: repo_tags_considered,
            repo_total: repo_tags_total,
            success: manifests_ok,
            timeout: manifests_timeout,
            error: manifests_error,
        });
    };

    if wanted.is_empty() || repo_tags_considered == 0 {
        report_progress(0, 0, 0, 0);
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
    report_progress(0, manifests_ok, manifests_timeout, manifests_error);

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

    // Give larger snapshots more wall-clock budget while keeping an upper bound so a single
    // digest does not monopolize the worker forever.
    let manifest_budget_secs = ((repo_tags_considered as u64) * 2 / MANIFEST_CONCURRENCY as u64)
        .clamp(MANIFEST_BUDGET_MIN_SECS, MANIFEST_BUDGET_MAX_SECS);
    let deadline = Instant::now() + Duration::from_secs(manifest_budget_secs);
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
        let processed = manifests_ok + manifests_timeout + manifests_error;
        report_progress(processed, manifests_ok, manifests_timeout, manifests_error);

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
    let processed = manifests_ok + manifests_timeout + manifests_error;
    report_progress(processed, manifests_ok, manifests_timeout, manifests_error);

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
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::Path};

    use super::{
        RuntimeServiceObservation, persist_runtime_fallback_result,
        pick_considered_tags_for_snapshot,
    };
    use crate::{
        api::types::{BackupRetention, ComposeConfig, StackBackupConfig},
        db::{Db, NewVersionNotificationPending, NewVersionNotificationReserveResult},
        models::{ServiceSeed, StackRecord},
    };

    async fn seed_service(db: &Db) -> (String, String) {
        let stack_id = "stack_1".to_string();
        let service_id = "svc_1".to_string();
        let now = "2026-03-09T00:00:00Z";
        let stack = StackRecord {
            id: stack_id.clone(),
            name: "demo".to_string(),
            archived: false,
            compose: ComposeConfig {
                kind: "compose".to_string(),
                compose_files: vec!["/tmp/demo.yml".to_string()],
                env_file: None,
            },
            backup: StackBackupConfig {
                targets: Vec::new(),
                retention: BackupRetention::default(),
            },
            services: Vec::new(),
        };
        let seeds = vec![ServiceSeed {
            id: service_id.clone(),
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web:latest".to_string(),
            image_tag: "latest".to_string(),
            homepage: None,
            auto_rollback: false,
            backup_bind_paths: BTreeMap::new(),
            backup_volume_names: BTreeMap::new(),
        }];
        db.insert_stack(&stack, &seeds, now).await.unwrap();
        (stack_id, service_id)
    }

    fn pending_notification(
        id: &str,
        service_id: &str,
        candidate_digest: &str,
        created_at: &str,
    ) -> NewVersionNotificationPending {
        NewVersionNotificationPending {
            id: id.to_string(),
            service_id: service_id.to_string(),
            job_id: "job_1".to_string(),
            reason: "schedule".to_string(),
            image_ref: "ghcr.io/acme/web:latest".to_string(),
            image_tag: "latest".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.1.0".to_string(),
            candidate_digest: candidate_digest.to_string(),
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn anchors_are_kept_even_if_non_parseable() {
        let repo_tags = vec![
            "1.0.2".to_string(),
            "latest".to_string(),
            "legacy-1".to_string(),
            "1.0.1".to_string(),
        ];
        let anchors = vec!["legacy-1".to_string()];

        let considered = pick_considered_tags_for_snapshot(&repo_tags, &anchors);

        assert_eq!(considered.first().map(String::as_str), Some("legacy-1"));
        assert!(considered.contains(&"legacy-1".to_string()));
    }

    #[test]
    fn considered_tags_are_capped_to_40() {
        let repo_tags = (0..100).map(|i| format!("1.0.{i}")).collect::<Vec<_>>();

        let considered = pick_considered_tags_for_snapshot(&repo_tags, &[]);

        assert_eq!(considered.len(), 40);
        assert_eq!(considered.first().map(String::as_str), Some("1.0.99"));
        assert_eq!(considered.last().map(String::as_str), Some("1.0.60"));
    }

    #[test]
    fn fallback_non_parseable_topk_applies_when_parseable_insufficient() {
        let mut repo_tags = vec![
            "1.0.0".to_string(),
            "1.0.1".to_string(),
            "1.0.2".to_string(),
        ];
        repo_tags.extend((0..25).map(|i| format!("n{i:02}")));

        let considered = pick_considered_tags_for_snapshot(&repo_tags, &[]);

        assert_eq!(considered.len(), 23);
        assert_eq!(&considered[..3], ["1.0.2", "1.0.1", "1.0.0"]);
        assert_eq!(considered[3], "n24");
        assert_eq!(
            considered.last().map(String::as_str),
            Some("n05"),
            "fallback should include only top 20 non-parseable tags"
        );
    }

    #[tokio::test]
    async fn runtime_fallback_keeps_sent_notification_active_until_authoritative_clear() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let (_stack_id, service_id) = seed_service(&db).await;
        let now = "2026-03-09T00:00:00Z";
        db.update_service_check_result(
            &service_id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
        let reserved = db
            .reserve_new_version_notification(&pending_notification(
                "nvn_1",
                &service_id,
                "sha256:new",
                now,
            ))
            .await
            .unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        db.finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:00:30Z",
        )
        .await
        .unwrap();

        persist_runtime_fallback_result(
            &db,
            &service_id,
            "ghcr.io/acme/web:latest",
            "latest",
            &RuntimeServiceObservation::digest_only("sha256:runtime"),
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();

        let rows = db
            .list_new_version_notifications_for_service(&service_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "sent");
        assert_eq!(rows[0].superseded_at.as_deref(), None);
    }

    #[tokio::test]
    async fn settle_fallback_keeps_same_digest_deduped_until_authoritative_clear() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let (stack_id, service_id) = seed_service(&db).await;
        let now = "2026-03-09T00:00:00Z";
        db.update_service_check_result(
            &service_id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
        let reserved = db
            .reserve_new_version_notification(&pending_notification(
                "nvn_1",
                &service_id,
                "sha256:new",
                now,
            ))
            .await
            .unwrap();
        assert_eq!(
            reserved,
            NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
        );
        db.finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:00:30Z",
        )
        .await
        .unwrap();

        persist_runtime_fallback_result(
            &db,
            &service_id,
            "ghcr.io/acme/web:latest",
            "latest",
            &RuntimeServiceObservation::digest_only("sha256:runtime"),
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();

        let stack = db.get_stack(&stack_id).await.unwrap().unwrap();
        let service = stack
            .services
            .iter()
            .find(|svc| svc.id == service_id)
            .unwrap();
        assert_eq!(service.image.digest.as_deref(), Some("sha256:runtime"));
        assert_eq!(service.image.resolved_tag, None);
        assert_eq!(service.image.resolved_tags, None);
        assert!(service.candidate.is_none());

        let retried = db
            .reserve_new_version_notification(&pending_notification(
                "nvn_2",
                &service_id,
                "sha256:new",
                "2026-03-09T00:02:00Z",
            ))
            .await
            .unwrap();
        assert_eq!(
            retried,
            NewVersionNotificationReserveResult::SkippedDuplicate
        );
    }
}
