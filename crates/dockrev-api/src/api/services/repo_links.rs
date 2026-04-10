use super::*;

fn normalize_optional_value(input: Option<&str>) -> Option<String> {
    input
        .map(|value| crate::db::normalize_discovery_key(Some(value)))
        .filter(|value| !value.is_empty())
}

pub(super) fn normalize_repo_full_name(full_name: &str) -> String {
    full_name.trim().to_ascii_lowercase()
}

fn is_reserved_github_path_root(segment: &str) -> bool {
    matches!(
        segment.trim().to_ascii_lowercase().as_str(),
        "account"
            | "apps"
            | "collections"
            | "contact"
            | "customer-stories"
            | "enterprise"
            | "events"
            | "explore"
            | "features"
            | "gist"
            | "git-guides"
            | "images"
            | "issues"
            | "login"
            | "marketplace"
            | "new"
            | "notifications"
            | "orgs"
            | "organizations"
            | "pricing"
            | "pulls"
            | "readme"
            | "search"
            | "security"
            | "session"
            | "settings"
            | "showcases"
            | "site"
            | "sponsors"
            | "stars"
            | "team"
            | "teams"
            | "topics"
            | "trending"
            | "users"
    )
}

pub(super) fn normalize_github_source_repo_key(source: &str) -> Option<String> {
    let trimmed = source.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed = Url::parse(trimmed).ok()?;
        let host = parsed.host_str()?.trim().to_ascii_lowercase();
        if host == "github.com" || host == "www.github.com" {
            let first = parsed
                .path_segments()
                .and_then(|mut segments| segments.find(|segment| !segment.trim().is_empty()))?;
            if is_reserved_github_path_root(first) {
                return None;
            }
        }
    }
    match github::parse_target_input(source).ok()? {
        github::TargetKind::Repo { owner, repo } => Some(format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        )),
        github::TargetKind::Owner { .. } => None,
    }
}

pub(super) fn github_repo_url_from_key(repo_key: &str) -> String {
    format!("https://github.com/{repo_key}")
}

fn ghcr_exact_repo_key(image: &registry::ImageRef) -> Option<String> {
    if !image.registry.eq_ignore_ascii_case("ghcr.io") {
        return None;
    }
    let mut parts = image.name.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!(
        "{}/{}",
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase()
    ))
}

pub(super) fn normalize_repo_url_input(input: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = input.map(str::trim) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }

    let parsed = Url::parse(value).map_err(|_| ApiError::invalid_argument("invalid repoUrl"))?;
    let scheme = parsed.scheme();
    if (scheme != "http" && scheme != "https") || !parsed.has_host() {
        return Err(ApiError::invalid_argument("invalid repoUrl"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::invalid_argument("invalid repoUrl"));
    }

    Ok(Some(value.to_string()))
}

fn normalize_repo_path_segments(segments: &[&str]) -> Option<Vec<String>> {
    let mut normalized = segments
        .iter()
        .map(|segment| segment.trim())
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let last = normalized.last_mut()?;
    if last.ends_with(".git") {
        let trimmed = last.trim_end_matches(".git").trim();
        if trimmed.is_empty() {
            return None;
        }
        *last = trimmed.to_string();
    }
    Some(normalized)
}

fn build_normalized_browse_url(mut parsed: Url, segments: &[String]) -> Option<String> {
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed
        .path_segments_mut()
        .ok()?
        .clear()
        .extend(segments.iter().map(String::as_str));
    Some(parsed.to_string())
}

fn collapse_gitlab_repo_segments(segments: &[String]) -> Option<Vec<String>> {
    let first = segments.first().map(|segment| segment.to_ascii_lowercase());
    if segments.len() < 2
        || matches!(
            first.as_deref(),
            Some("groups" | "users" | "explore" | "help" | "admin" | "dashboard" | "projects")
        )
    {
        return None;
    }
    let repo_end = segments
        .iter()
        .position(|segment| segment == "-")
        .unwrap_or(segments.len());
    if repo_end < 2 {
        return None;
    }
    Some(segments[..repo_end].to_vec())
}

fn is_repo_browse_marker(segment: &str) -> bool {
    matches!(
        segment.trim().to_ascii_lowercase().as_str(),
        "-" | "blob"
            | "branch"
            | "branches"
            | "commit"
            | "commits"
            | "compare"
            | "raw"
            | "releases"
            | "src"
            | "tree"
    )
}

fn collapse_generic_repo_segments(segments: &[String]) -> Option<Vec<String>> {
    if segments.len() < 2 {
        return None;
    }
    let repo_end = segments
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(idx, segment)| is_repo_browse_marker(segment).then_some(idx))
        .unwrap_or(segments.len());
    if repo_end < 2 {
        return None;
    }
    Some(segments[..repo_end].to_vec())
}

fn normalize_external_repo_url(input: &str) -> Option<String> {
    let value = normalize_repo_url_input(Some(input)).ok().flatten()?;
    let parsed = Url::parse(&value).ok()?;
    let host = parsed.host_str()?.trim().to_ascii_lowercase();
    if host == "github.com" || host == "www.github.com" {
        return None;
    }
    let segments = normalize_repo_path_segments(
        &parsed
            .path_segments()
            .map(|parts| parts.collect::<Vec<_>>())
            .unwrap_or_default(),
    )?;
    let is_gitlab_host = host == "gitlab.com"
        || host == "www.gitlab.com"
        || host.starts_with("gitlab.")
        || host.ends_with(".gitlab.com")
        || host.contains(".gitlab.");
    if is_gitlab_host {
        let repo_segments = collapse_gitlab_repo_segments(&segments)?;
        return build_normalized_browse_url(parsed, &repo_segments);
    }

    let repo_segments = collapse_generic_repo_segments(&segments)?;
    build_normalized_browse_url(parsed, &repo_segments)
}

fn image_ref_pinned_digest(image_ref: &str) -> Option<String> {
    let (_, digest) = image_ref.trim().split_once('@')?;
    snapshot_worker::normalize_digest(digest)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RepoLinkInferenceOutcomeKind {
    Match,
    NoMatch,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) struct RepoLinkInferenceContext {
    pub host_platform: String,
    pub tracked_ghcr_repo_keys: std::collections::BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RepoLinkInferenceResult {
    pub repo_url: Option<String>,
    pub strategy: ServiceRepoLinkInferenceStrategy,
    pub reason: Option<String>,
    pub outcome: RepoLinkInferenceOutcomeKind,
}

impl RepoLinkInferenceResult {
    pub(super) fn into_response(self) -> ServiceRepoLinkInferenceResponse {
        ServiceRepoLinkInferenceResponse {
            repo_url: self.repo_url,
            strategy: self.strategy,
            reason: self.reason,
        }
    }
}

pub(crate) async fn build_repo_link_inference_context(
    state: &Arc<AppState>,
) -> anyhow::Result<RepoLinkInferenceContext> {
    let tracked_ghcr_repo_keys = state
        .db
        .list_github_packages_repos()
        .await?
        .into_iter()
        .filter(|repo| repo.selected)
        .map(|repo| normalize_repo_full_name(&format!("{}/{}", repo.owner, repo.repo)))
        .collect::<std::collections::BTreeSet<_>>();
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    Ok(RepoLinkInferenceContext {
        host_platform,
        tracked_ghcr_repo_keys,
    })
}

fn service_repo_link_inspection_reference(
    snapshot_target: &crate::db::ServiceSnapshotTarget,
    image: &registry::ImageRef,
) -> String {
    let parsed_reference = image.reference.trim().to_string();
    let parsed_reference_is_digest = parsed_reference.starts_with("sha256:");
    snapshot_target
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .or_else(|| image_ref_pinned_digest(&snapshot_target.image_ref))
        .or_else(|| {
            if parsed_reference_is_digest {
                Some(parsed_reference.clone())
            } else {
                None
            }
        })
        .or_else(|| {
            let current_tag = snapshot_target.current_tag.trim();
            if current_tag.is_empty() {
                None
            } else {
                Some(current_tag.to_string())
            }
        })
        .unwrap_or(parsed_reference)
}

pub(crate) async fn infer_service_repo_link_for_snapshot_target(
    state: &Arc<AppState>,
    snapshot_target: &crate::db::ServiceSnapshotTarget,
    context: &RepoLinkInferenceContext,
) -> RepoLinkInferenceResult {
    let image = match registry::ImageRef::parse(&snapshot_target.image_ref) {
        Ok(image) => image,
        Err(err) => {
            return RepoLinkInferenceResult {
                repo_url: None,
                strategy: ServiceRepoLinkInferenceStrategy::None,
                reason: Some(format!("invalid service image ref: {err}")),
                outcome: RepoLinkInferenceOutcomeKind::Error,
            };
        }
    };
    let inspection_reference = service_repo_link_inspection_reference(snapshot_target, &image);

    let mut miss_reasons = Vec::new();
    let mut had_error = false;

    match state
        .registry
        .get_oci_source(&image, &inspection_reference, &context.host_platform)
        .await
    {
        Ok(source) => {
            if let Some(repo_key) = source.as_deref().and_then(normalize_github_source_repo_key) {
                return RepoLinkInferenceResult {
                    repo_url: Some(github_repo_url_from_key(&repo_key)),
                    strategy: ServiceRepoLinkInferenceStrategy::OciSource,
                    reason: None,
                    outcome: RepoLinkInferenceOutcomeKind::Match,
                };
            }
            if let Some(repo_url) = source.as_deref().and_then(normalize_external_repo_url) {
                return RepoLinkInferenceResult {
                    repo_url: Some(repo_url),
                    strategy: ServiceRepoLinkInferenceStrategy::OciSource,
                    reason: None,
                    outcome: RepoLinkInferenceOutcomeKind::Match,
                };
            }
            if source.as_deref().is_some() {
                miss_reasons
                    .push("oci source not recognized as a valid repository URL".to_string());
            } else {
                miss_reasons.push("oci source missing".to_string());
            }
        }
        Err(err) => {
            had_error = true;
            miss_reasons.push(format!("read oci source failed: {err}"));
        }
    }

    if let Some(repo_key) = ghcr_exact_repo_key(&image) {
        if context.tracked_ghcr_repo_keys.contains(&repo_key) {
            return RepoLinkInferenceResult {
                repo_url: Some(github_repo_url_from_key(&repo_key)),
                strategy: ServiceRepoLinkInferenceStrategy::GhcrExact,
                reason: None,
                outcome: RepoLinkInferenceOutcomeKind::Match,
            };
        }
        miss_reasons.push("ghcr exact fallback skipped because repo is not tracked".to_string());
    } else {
        miss_reasons.push("ghcr exact fallback not applicable".to_string());
    }

    RepoLinkInferenceResult {
        repo_url: None,
        strategy: ServiceRepoLinkInferenceStrategy::None,
        reason: if miss_reasons.is_empty() {
            None
        } else {
            Some(miss_reasons.join("; "))
        },
        outcome: if had_error {
            RepoLinkInferenceOutcomeKind::Error
        } else {
            RepoLinkInferenceOutcomeKind::NoMatch
        },
    }
}

fn timeline_candidate_identity(
    context: &crate::db::ServiceNewVersionTimelineContext,
    candidate_display_tag_hint: Option<&str>,
) -> Option<String> {
    let candidate_tag = crate::db::normalize_discovery_key(context.candidate_tag.as_deref());
    let candidate_display_tag = crate::db::normalize_discovery_key(
        candidate_display_tag_hint
            .or(context.candidate_resolved_tag.as_deref())
            .or(context.candidate_tag.as_deref()),
    );
    if let Some(tag) =
        crate::db::stable_candidate_display_tag(&candidate_tag, &candidate_display_tag)
    {
        return Some(format!(
            "tag:{}",
            crate::db::canonical_candidate_identity_tag(&candidate_tag, tag)
        ));
    }

    if !candidate_display_tag.is_empty()
        && !candidate_display_tag
            .to_ascii_lowercase()
            .starts_with("sha256:")
    {
        return Some(format!(
            "alias:{}",
            crate::db::canonical_visible_version_tag(&candidate_display_tag)
        ));
    }

    if !candidate_tag.is_empty() && !candidate_tag.to_ascii_lowercase().starts_with("sha256:") {
        return Some(format!(
            "alias:{}",
            crate::db::canonical_visible_version_tag(&candidate_tag)
        ));
    }

    normalize_optional_value(context.candidate_digest.as_deref())
        .map(|digest| format!("digest:{digest}"))
}

fn timeline_candidate_version(
    context: &crate::db::ServiceNewVersionTimelineContext,
    stable_candidate_display_tag: Option<&str>,
) -> Option<String> {
    if let Some(tag) = stable_candidate_display_tag {
        return Some(crate::db::canonical_visible_version_tag(tag));
    }

    normalize_optional_value(
        stable_candidate_display_tag.or(context.candidate_resolved_tag.as_deref()),
    )
    .map(|value| crate::db::canonical_visible_version_tag(&value))
    .or_else(|| {
        normalize_optional_value(context.candidate_tag.as_deref())
            .map(|value| crate::db::canonical_visible_version_tag(&value))
    })
    .or_else(|| normalize_optional_value(context.candidate_digest.as_deref()))
}

fn timeline_running_version(
    context: &crate::db::ServiceNewVersionTimelineContext,
    effective_current_display_tag: Option<&str>,
) -> Option<String> {
    normalize_optional_value(effective_current_display_tag)
        .map(|value| crate::db::canonical_visible_version_tag(&value))
        .or_else(|| {
            normalize_optional_value(context.current_resolved_tag.as_deref())
                .map(|value| crate::db::canonical_visible_version_tag(&value))
        })
        .or_else(|| {
            normalize_optional_value(Some(context.current_tag.as_str()))
                .map(|value| crate::db::canonical_visible_version_tag(&value))
        })
        .or_else(|| normalize_optional_value(context.current_digest.as_deref()))
}

pub(crate) async fn get_service_new_version_discovery_timeline(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<NewVersionDiscoveryTimelineResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let context = state
        .db
        .get_service_new_version_timeline_context(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(context) = context else {
        return Err(ApiError::not_found("service not found"));
    };

    let effective_current_resolved_tag = super::stacks::resolve_current_running_resolved_tag(
        &state,
        &context.image_ref,
        &context.current_tag,
        context.current_digest.as_deref(),
        context.current_resolved_tag.as_deref(),
    )
    .await?;
    let effective_candidate_resolved_tag = super::stacks::resolve_candidate_resolved_tag(
        &state,
        &service_id,
        &context.image_ref,
        &context.current_tag,
        context.candidate_tag.as_deref(),
        context.candidate_digest.as_deref(),
        context.candidate_resolved_tag.as_deref(),
    )
    .await?;

    let discovery_rows = state
        .db
        .list_new_version_discoveries_for_services(std::slice::from_ref(&service_id))
        .await
        .map_err(map_internal)?;
    let effective_stable_tags_by_provenance =
        super::stacks::resolve_discovery_stable_tags_by_provenance(&state, &discovery_rows).await?;

    let current_digest = crate::db::normalize_discovery_key(context.current_digest.as_deref());
    let current_display_tag = crate::db::normalize_discovery_key(
        effective_current_resolved_tag
            .as_deref()
            .or(context.current_resolved_tag.as_deref())
            .or(Some(context.current_tag.as_str())),
    );
    let current_tag = crate::db::normalize_discovery_key(Some(context.current_tag.as_str()));
    let current_candidate_stable_tag = crate::db::infer_stable_candidate_display_tag_from_rows(
        discovery_rows.iter(),
        &crate::db::normalize_discovery_key(Some(context.image_ref.as_str())),
        &current_digest,
        &current_display_tag,
        &current_tag,
        &crate::db::normalize_discovery_key(context.candidate_digest.as_deref()),
        &effective_stable_tags_by_provenance,
    );

    let mut historical_candidates = crate::db::collect_new_version_discovery_candidates_from_rows(
        discovery_rows.iter(),
        &crate::db::normalize_discovery_key(Some(context.image_ref.as_str())),
        &current_digest,
        &current_display_tag,
        &current_tag,
        &effective_stable_tags_by_provenance,
    );
    historical_candidates.sort_by(|left, right| {
        right
            .first_discovered_at
            .cmp(&left.first_discovered_at)
            .then_with(|| left.version.cmp(&right.version))
    });

    let current_candidate_identity = timeline_candidate_identity(
        &context,
        effective_candidate_resolved_tag
            .as_deref()
            .or(current_candidate_stable_tag.as_deref()),
    );
    let current_candidate_version = timeline_candidate_version(
        &context,
        effective_candidate_resolved_tag
            .as_deref()
            .or(current_candidate_stable_tag.as_deref()),
    );

    let current_candidate_item = current_candidate_identity.as_ref().and_then(|identity| {
        historical_candidates
            .iter()
            .position(|candidate| candidate.identity_key == *identity)
            .map(|index| {
                let candidate = historical_candidates.remove(index);
                NewVersionDiscoveryTimelineItem {
                    kind: NewVersionDiscoveryTimelineItemKind::CurrentCandidate,
                    version: current_candidate_version
                        .clone()
                        .unwrap_or(candidate.version),
                    occurred_at: candidate.first_discovered_at,
                }
            })
            .or_else(|| {
                current_candidate_version
                    .clone()
                    .map(|version| NewVersionDiscoveryTimelineItem {
                        kind: NewVersionDiscoveryTimelineItemKind::CurrentCandidate,
                        version,
                        occurred_at: None,
                    })
            })
    });

    let mut items = Vec::with_capacity(historical_candidates.len() + 2);
    if let Some(current_candidate_item) = current_candidate_item {
        items.push(current_candidate_item);
    }
    items.extend(historical_candidates.into_iter().map(|candidate| {
        NewVersionDiscoveryTimelineItem {
            kind: NewVersionDiscoveryTimelineItemKind::HistoricalCandidate,
            version: candidate.version,
            occurred_at: candidate.first_discovered_at,
        }
    }));
    items.push(NewVersionDiscoveryTimelineItem {
        kind: NewVersionDiscoveryTimelineItemKind::CurrentRunning,
        version: timeline_running_version(&context, effective_current_resolved_tag.as_deref())
            .unwrap_or_else(|| "-".to_string()),
        occurred_at: normalize_optional_value(context.current_runtime_started_at.as_deref()),
    });

    Ok(Json(NewVersionDiscoveryTimelineResponse { items }))
}
