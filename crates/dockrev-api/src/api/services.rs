use super::*;

fn normalize_optional_value(input: Option<&str>) -> Option<String> {
    input
        .map(|value| crate::db::normalize_discovery_key(Some(value)))
        .filter(|value| !value.is_empty())
}

fn normalize_repo_full_name(full_name: &str) -> String {
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

fn normalize_github_source_repo_key(source: &str) -> Option<String> {
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

fn github_repo_url_from_key(repo_key: &str) -> String {
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

fn normalize_repo_url_input(input: Option<&str>) -> Result<Option<String>, ApiError> {
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
    fn into_response(self) -> ServiceRepoLinkInferenceResponse {
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

pub(super) async fn get_service_new_version_discovery_timeline(
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

const DEFAULT_GITHUB_RELEASES_PAGE: u32 = 1;
const DEFAULT_GITHUB_RELEASES_PER_PAGE: u32 = 20;
const MAX_GITHUB_RELEASES_PER_PAGE: u32 = 100;
const DEFAULT_GITHUB_RELEASE_LOCATE_LIMIT: u32 = 50;
const MAX_GITHUB_RELEASE_LOCATE_LIMIT: u32 = 50;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServiceGitHubReleasesQuery {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServiceGitHubReleaseLocateQuery {
    version: Option<String>,
    per_page: Option<u32>,
    limit: Option<u32>,
}

fn github_release_auth_mode(mode: github::GitHubAuthMode) -> GitHubReleaseAuthMode {
    match mode {
        github::GitHubAuthMode::Pat => GitHubReleaseAuthMode::Pat,
        github::GitHubAuthMode::Anonymous => GitHubReleaseAuthMode::Anonymous,
    }
}

fn normalize_service_github_repo_ref(repo_url: Option<&str>) -> Option<ServiceGitHubRepoRef> {
    let repo_key = normalize_github_source_repo_key(repo_url?)?;
    Some(ServiceGitHubRepoRef {
        full_name: repo_key.clone(),
        html_url: github_repo_url_from_key(&repo_key),
    })
}

pub(crate) async fn resolve_service_github_repo_ref(
    state: &Arc<AppState>,
    service_id: &str,
    repo_url: Option<&str>,
) -> anyhow::Result<Option<ServiceGitHubRepoRef>> {
    let saved_repo_url = repo_url.map(str::trim).filter(|value| !value.is_empty());
    if saved_repo_url.is_some() {
        return Ok(normalize_service_github_repo_ref(saved_repo_url));
    }

    let snapshot_target = match state.db.get_service_snapshot_target(service_id).await? {
        Some(snapshot_target) => snapshot_target,
        None => return Ok(None),
    };
    let context = build_repo_link_inference_context(state).await?;
    let inferred =
        infer_service_repo_link_for_snapshot_target(state, &snapshot_target, &context).await;
    Ok(normalize_service_github_repo_ref(
        inferred.repo_url.as_deref(),
    ))
}

fn split_github_repo_full_name(full_name: &str) -> Option<(String, String)> {
    let trimmed = normalize_repo_full_name(full_name);
    let (owner, repo) = trimmed.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn normalize_github_releases_page(value: Option<u32>) -> u32 {
    value.unwrap_or(DEFAULT_GITHUB_RELEASES_PAGE).max(1)
}

fn normalize_github_releases_per_page(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_GITHUB_RELEASES_PER_PAGE)
        .clamp(1, MAX_GITHUB_RELEASES_PER_PAGE)
}

fn normalize_github_release_locate_limit(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_GITHUB_RELEASE_LOCATE_LIMIT)
        .clamp(1, MAX_GITHUB_RELEASE_LOCATE_LIMIT)
}

fn github_release_item_from_api(release: github::GitHubRelease) -> ServiceGitHubReleaseItem {
    ServiceGitHubReleaseItem {
        id: release.id,
        tag_name: release.tag_name,
        name: release.name,
        body: release.body,
        html_url: release.html_url,
        draft: release.draft,
        prerelease: release.prerelease,
        published_at: release.published_at,
        created_at: release.created_at,
    }
}

fn github_release_tag_variants(version: &str) -> Vec<String> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut variants = Vec::new();
    let mut push_unique = |candidate: String| {
        if candidate.is_empty() {
            return;
        }
        if variants.iter().any(|existing| existing == &candidate) {
            return;
        }
        variants.push(candidate);
    };
    push_unique(trimmed.to_string());
    if let Some(stripped) = trimmed.strip_prefix('v') {
        push_unique(stripped.trim().to_string());
    } else {
        push_unique(format!("v{trimmed}"));
    }
    variants
}

fn github_release_matches_variants(tag_name: &str, variants: &[String]) -> bool {
    let trimmed = tag_name.trim();
    if trimmed.is_empty() {
        return false;
    }
    variants.iter().any(|candidate| candidate == trimmed)
}

fn github_release_error_is_rate_limited(err: &anyhow::Error) -> bool {
    if github_http_status_from_error(err).is_some_and(|status| status == 429) {
        return true;
    }
    let lower = err.to_string().to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("secondary rate limit")
}

fn github_release_error_message(
    status: ServiceGitHubReleasesStatus,
    auth_mode: GitHubReleaseAuthMode,
) -> String {
    match status {
        ServiceGitHubReleasesStatus::UnsupportedRepo => {
            "该服务未配置 GitHub 仓库链接，或 repoUrl 不是 github.com 仓库地址。".to_string()
        }
        ServiceGitHubReleasesStatus::PermissionDenied => match auth_mode {
            GitHubReleaseAuthMode::Pat => {
                "当前 GitHub PAT 无法访问该仓库的 Releases。请检查仓库可见性与 token 权限。".to_string()
            }
            GitHubReleaseAuthMode::Anonymous => {
                "匿名身份无法访问该仓库的 Releases。若这是私有仓库或受限仓库，请到“设置 -> GitHub Packages”配置 GitHub PAT。".to_string()
            }
        },
        ServiceGitHubReleasesStatus::RateLimited => match auth_mode {
            GitHubReleaseAuthMode::Pat => {
                "当前 GitHub PAT 访问 GitHub Releases 时触发了 rate limit，请稍后重试。".to_string()
            }
            GitHubReleaseAuthMode::Anonymous => {
                "GitHub 匿名访问已触发 rate limit。请稍后再试，或到“设置 -> GitHub Packages”配置 GitHub PAT。".to_string()
            }
        },
        ServiceGitHubReleasesStatus::UpstreamError => {
            "GitHub Releases 拉取失败，请稍后重试。".to_string()
        }
        ServiceGitHubReleasesStatus::Ready => String::new(),
    }
}

fn github_release_upstream_404_message(auth_mode: GitHubReleaseAuthMode) -> String {
    match auth_mode {
        GitHubReleaseAuthMode::Pat => {
            "GitHub 返回 404：仓库可能不存在，或当前 GitHub PAT 无法访问该仓库。请先检查 repoUrl，再检查仓库可见性与 token 权限。".to_string()
        }
        GitHubReleaseAuthMode::Anonymous => {
            "GitHub 返回 404：仓库可能不存在，或当前匿名身份无法访问该仓库。请先检查 repoUrl；若这是私有仓库，请到“设置 -> GitHub Packages”配置 GitHub PAT。".to_string()
        }
    }
}

fn github_release_failure_message(
    status: ServiceGitHubReleasesStatus,
    auth_mode: GitHubReleaseAuthMode,
    err: &anyhow::Error,
) -> String {
    if status == ServiceGitHubReleasesStatus::UpstreamError
        && github_http_status_from_error(err).is_some_and(|status| status == 404)
    {
        return github_release_upstream_404_message(auth_mode);
    }
    github_release_error_message(status, auth_mode)
}

fn github_release_locate_error_message(
    status: ServiceGitHubReleaseLocateStatus,
    auth_mode: GitHubReleaseAuthMode,
    version: &str,
    searched_count: u32,
) -> Option<String> {
    Some(match status {
        ServiceGitHubReleaseLocateStatus::Found => return None,
        ServiceGitHubReleaseLocateStatus::OutsideWindow => {
            format!("已定位到 {version}，但它不在前 {searched_count} 条发布记录内。")
        }
        ServiceGitHubReleaseLocateStatus::NotFound => {
            format!("在前 {searched_count} 条发布记录中未找到 {version}。")
        }
        ServiceGitHubReleaseLocateStatus::UnsupportedRepo => {
            github_release_error_message(ServiceGitHubReleasesStatus::UnsupportedRepo, auth_mode)
        }
        ServiceGitHubReleaseLocateStatus::PermissionDenied => {
            github_release_error_message(ServiceGitHubReleasesStatus::PermissionDenied, auth_mode)
        }
        ServiceGitHubReleaseLocateStatus::RateLimited => {
            github_release_error_message(ServiceGitHubReleasesStatus::RateLimited, auth_mode)
        }
        ServiceGitHubReleaseLocateStatus::UpstreamError => {
            github_release_error_message(ServiceGitHubReleasesStatus::UpstreamError, auth_mode)
        }
    })
}

fn github_release_locate_failure_message(
    status: ServiceGitHubReleaseLocateStatus,
    auth_mode: GitHubReleaseAuthMode,
    version: &str,
    searched_count: u32,
    err: &anyhow::Error,
) -> Option<String> {
    if status == ServiceGitHubReleaseLocateStatus::UpstreamError
        && github_http_status_from_error(err).is_some_and(|status| status == 404)
    {
        return Some(match auth_mode {
            GitHubReleaseAuthMode::Pat => {
                format!(
                    "GitHub 返回 404：仓库可能不存在，或当前 GitHub PAT 无法访问该仓库，因此暂时无法定位 {version}。请先检查 repoUrl，再检查仓库可见性与 token 权限。"
                )
            }
            GitHubReleaseAuthMode::Anonymous => {
                format!(
                    "GitHub 返回 404：仓库可能不存在，或当前匿名身份无法访问该仓库，因此暂时无法定位 {version}。请先检查 repoUrl；若这是私有仓库，请到“设置 -> GitHub Packages”配置 GitHub PAT。"
                )
            }
        });
    }
    github_release_locate_error_message(status, auth_mode, version, searched_count)
}

fn classify_github_releases_failure(
    _auth_mode: GitHubReleaseAuthMode,
    err: &anyhow::Error,
) -> ServiceGitHubReleasesStatus {
    if github_release_error_is_rate_limited(err) {
        return ServiceGitHubReleasesStatus::RateLimited;
    }
    if let Some(status) = github_http_status_from_error(err)
        && matches!(status, 401 | 403)
    {
        return ServiceGitHubReleasesStatus::PermissionDenied;
    }
    if github_error_is_timeout(err) {
        return ServiceGitHubReleasesStatus::UpstreamError;
    }
    ServiceGitHubReleasesStatus::UpstreamError
}

fn classify_github_release_locate_failure(
    auth_mode: GitHubReleaseAuthMode,
    err: &anyhow::Error,
) -> ServiceGitHubReleaseLocateStatus {
    match classify_github_releases_failure(auth_mode, err) {
        ServiceGitHubReleasesStatus::PermissionDenied => {
            ServiceGitHubReleaseLocateStatus::PermissionDenied
        }
        ServiceGitHubReleasesStatus::RateLimited => ServiceGitHubReleaseLocateStatus::RateLimited,
        ServiceGitHubReleasesStatus::UnsupportedRepo => {
            ServiceGitHubReleaseLocateStatus::UnsupportedRepo
        }
        ServiceGitHubReleasesStatus::UpstreamError | ServiceGitHubReleasesStatus::Ready => {
            ServiceGitHubReleaseLocateStatus::UpstreamError
        }
    }
}

fn build_service_github_releases_client(
    settings: &crate::models::GitHubPackagesSettingsDb,
) -> Result<github::GitHubClient, ApiError> {
    let pat = settings
        .pat
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(token) = pat {
        github::GitHubClient::new(token).map_err(map_internal)
    } else {
        github::GitHubClient::new_anonymous().map_err(map_internal)
    }
}

async fn list_service_github_releases_with_client(
    client: &github::GitHubClient,
    repo: ServiceGitHubRepoRef,
    page: u32,
    per_page: u32,
) -> ServiceGitHubReleasesResponse {
    let response =
        list_service_github_releases_with_client_once(client, repo.clone(), page, per_page).await;
    if response.auth_mode == GitHubReleaseAuthMode::Pat
        && matches!(
            response.status,
            ServiceGitHubReleasesStatus::PermissionDenied
                | ServiceGitHubReleasesStatus::UpstreamError
        )
    {
        if let Some(message) = response.message.as_deref()
            && (response.status == ServiceGitHubReleasesStatus::PermissionDenied
                || message.contains("GitHub 返回 404"))
            && let Ok(anonymous) = client.clone_as_anonymous()
        {
            let fallback =
                list_service_github_releases_with_client_once(&anonymous, repo, page, per_page)
                    .await;
            if fallback.status == ServiceGitHubReleasesStatus::Ready {
                return fallback;
            }
        }
    }
    response
}

async fn list_service_github_releases_with_client_once(
    client: &github::GitHubClient,
    repo: ServiceGitHubRepoRef,
    page: u32,
    per_page: u32,
) -> ServiceGitHubReleasesResponse {
    let auth_mode = github_release_auth_mode(client.auth_mode());
    let Some((owner, repo_name)) = split_github_repo_full_name(&repo.full_name) else {
        return ServiceGitHubReleasesResponse {
            status: ServiceGitHubReleasesStatus::UnsupportedRepo,
            auth_mode,
            repo: Some(repo),
            page,
            per_page,
            has_more: false,
            items: Vec::new(),
            message: Some(github_release_error_message(
                ServiceGitHubReleasesStatus::UnsupportedRepo,
                auth_mode,
            )),
        };
    };

    match client
        .list_releases_page(&owner, &repo_name, page, per_page)
        .await
    {
        Ok(result) => ServiceGitHubReleasesResponse {
            status: ServiceGitHubReleasesStatus::Ready,
            auth_mode,
            repo: Some(repo),
            page,
            per_page,
            has_more: result.has_next,
            items: result
                .items
                .into_iter()
                .map(github_release_item_from_api)
                .collect(),
            message: None,
        },
        Err(err) => {
            let status = classify_github_releases_failure(auth_mode, &err);
            ServiceGitHubReleasesResponse {
                status,
                auth_mode,
                repo: Some(repo),
                page,
                per_page,
                has_more: false,
                items: Vec::new(),
                message: Some(github_release_failure_message(status, auth_mode, &err)),
            }
        }
    }
}

async fn locate_service_github_release_with_client(
    client: &github::GitHubClient,
    repo: ServiceGitHubRepoRef,
    version: &str,
    per_page: u32,
    limit: u32,
) -> ServiceGitHubReleaseLocateResponse {
    let response = locate_service_github_release_with_client_once(
        client,
        repo.clone(),
        version,
        per_page,
        limit,
    )
    .await;
    if response.auth_mode == GitHubReleaseAuthMode::Pat
        && matches!(
            response.status,
            ServiceGitHubReleaseLocateStatus::PermissionDenied
                | ServiceGitHubReleaseLocateStatus::UpstreamError
        )
    {
        if let Some(message) = response.message.as_deref()
            && (response.status == ServiceGitHubReleaseLocateStatus::PermissionDenied
                || message.contains("GitHub 返回 404"))
            && let Ok(anonymous) = client.clone_as_anonymous()
        {
            let fallback = locate_service_github_release_with_client_once(
                &anonymous, repo, version, per_page, limit,
            )
            .await;
            if !matches!(
                fallback.status,
                ServiceGitHubReleaseLocateStatus::PermissionDenied
                    | ServiceGitHubReleaseLocateStatus::UpstreamError
            ) {
                return fallback;
            }
        }
    }
    response
}

async fn locate_service_github_release_with_client_once(
    client: &github::GitHubClient,
    repo: ServiceGitHubRepoRef,
    version: &str,
    per_page: u32,
    limit: u32,
) -> ServiceGitHubReleaseLocateResponse {
    let auth_mode = github_release_auth_mode(client.auth_mode());
    let trimmed_version = version.trim().to_string();
    let empty = || ServiceGitHubReleaseLocateResponse {
        status: ServiceGitHubReleaseLocateStatus::UnsupportedRepo,
        auth_mode,
        repo: Some(repo.clone()),
        version: trimmed_version.clone(),
        searched_count: 0,
        matched_tag: None,
        page: None,
        index_within_page: None,
        absolute_index: None,
        message: github_release_locate_error_message(
            ServiceGitHubReleaseLocateStatus::UnsupportedRepo,
            auth_mode,
            &trimmed_version,
            0,
        ),
    };
    let Some((owner, repo_name)) = split_github_repo_full_name(&repo.full_name) else {
        return empty();
    };

    let variants = github_release_tag_variants(&trimmed_version);
    if variants.is_empty() {
        return ServiceGitHubReleaseLocateResponse {
            status: ServiceGitHubReleaseLocateStatus::NotFound,
            auth_mode,
            repo: Some(repo),
            version: trimmed_version.clone(),
            searched_count: 0,
            matched_tag: None,
            page: None,
            index_within_page: None,
            absolute_index: None,
            message: github_release_locate_error_message(
                ServiceGitHubReleaseLocateStatus::NotFound,
                auth_mode,
                &trimmed_version,
                0,
            ),
        };
    }

    let mut matched_tag = None;
    for candidate in &variants {
        match client
            .get_release_by_tag(&owner, &repo_name, candidate)
            .await
        {
            Ok(release) => {
                matched_tag = Some(release.tag_name);
                break;
            }
            Err(err) => {
                if github_http_status_from_error(&err).is_some_and(|status| status == 404) {
                    continue;
                }
                let status = classify_github_release_locate_failure(auth_mode, &err);
                return ServiceGitHubReleaseLocateResponse {
                    status,
                    auth_mode,
                    repo: Some(repo),
                    version: trimmed_version.clone(),
                    searched_count: 0,
                    matched_tag: None,
                    page: None,
                    index_within_page: None,
                    absolute_index: None,
                    message: github_release_locate_failure_message(
                        status,
                        auth_mode,
                        &trimmed_version,
                        0,
                        &err,
                    ),
                };
            }
        }
    }

    let mut searched_count = 0u32;
    let mut page = 1u32;
    loop {
        if searched_count >= limit {
            break;
        }
        let remaining = (limit - searched_count) as usize;
        let result = match client
            .list_releases_page(&owner, &repo_name, page, per_page)
            .await
        {
            Ok(result) => result,
            Err(err) => {
                let status = classify_github_release_locate_failure(auth_mode, &err);
                return ServiceGitHubReleaseLocateResponse {
                    status,
                    auth_mode,
                    repo: Some(repo),
                    version: trimmed_version.clone(),
                    searched_count,
                    matched_tag,
                    page: None,
                    index_within_page: None,
                    absolute_index: None,
                    message: github_release_locate_failure_message(
                        status,
                        auth_mode,
                        &trimmed_version,
                        searched_count,
                        &err,
                    ),
                };
            }
        };

        let scanned_this_page = remaining.min(result.items.len());
        for (index_within_page, release) in result.items.iter().take(scanned_this_page).enumerate()
        {
            if github_release_matches_variants(&release.tag_name, &variants) {
                let absolute_index = searched_count + index_within_page as u32;
                let matched_tag = Some(release.tag_name.clone());
                return ServiceGitHubReleaseLocateResponse {
                    status: ServiceGitHubReleaseLocateStatus::Found,
                    auth_mode,
                    repo: Some(repo),
                    version: trimmed_version.clone(),
                    searched_count: searched_count + scanned_this_page as u32,
                    matched_tag,
                    page: Some(page),
                    index_within_page: Some(index_within_page as u32),
                    absolute_index: Some(absolute_index),
                    message: None,
                };
            }
        }

        searched_count += scanned_this_page as u32;
        if scanned_this_page < result.items.len() || !result.has_next || result.items.is_empty() {
            break;
        }
        page += 1;
    }

    let status = if matched_tag.is_some() {
        ServiceGitHubReleaseLocateStatus::OutsideWindow
    } else {
        ServiceGitHubReleaseLocateStatus::NotFound
    };
    ServiceGitHubReleaseLocateResponse {
        status,
        auth_mode,
        repo: Some(repo),
        version: trimmed_version.clone(),
        searched_count,
        matched_tag,
        page: None,
        index_within_page: None,
        absolute_index: None,
        message: github_release_locate_error_message(
            status,
            auth_mode,
            &trimmed_version,
            searched_count,
        ),
    }
}

pub(super) async fn list_service_github_releases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceGitHubReleasesQuery>,
) -> Result<Json<ServiceGitHubReleasesResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    let github_settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let client = build_service_github_releases_client(&github_settings)?;
    let auth_mode = github_release_auth_mode(client.auth_mode());
    let page = normalize_github_releases_page(query.page);
    let per_page = normalize_github_releases_per_page(query.per_page);
    let Some(repo) =
        resolve_service_github_repo_ref(&state, &service_id, settings.repo_url.as_deref())
            .await
            .map_err(map_internal)?
    else {
        return Ok(Json(ServiceGitHubReleasesResponse {
            status: ServiceGitHubReleasesStatus::UnsupportedRepo,
            auth_mode,
            repo: None,
            page,
            per_page,
            has_more: false,
            items: Vec::new(),
            message: Some(github_release_error_message(
                ServiceGitHubReleasesStatus::UnsupportedRepo,
                auth_mode,
            )),
        }));
    };

    Ok(Json(
        list_service_github_releases_with_client(&client, repo, page, per_page).await,
    ))
}

pub(super) async fn locate_service_github_release(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceGitHubReleaseLocateQuery>,
) -> Result<Json<ServiceGitHubReleaseLocateResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let version = query.version.unwrap_or_default();
    let trimmed_version = version.trim();
    if trimmed_version.is_empty() {
        return Err(ApiError::invalid_argument("version is required"));
    }

    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    let github_settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let client = build_service_github_releases_client(&github_settings)?;
    let auth_mode = github_release_auth_mode(client.auth_mode());
    let per_page = normalize_github_releases_per_page(query.per_page);
    let limit = normalize_github_release_locate_limit(query.limit);
    let Some(repo) =
        resolve_service_github_repo_ref(&state, &service_id, settings.repo_url.as_deref())
            .await
            .map_err(map_internal)?
    else {
        return Ok(Json(ServiceGitHubReleaseLocateResponse {
            status: ServiceGitHubReleaseLocateStatus::UnsupportedRepo,
            auth_mode,
            repo: None,
            version: trimmed_version.to_string(),
            searched_count: 0,
            matched_tag: None,
            page: None,
            index_within_page: None,
            absolute_index: None,
            message: github_release_locate_error_message(
                ServiceGitHubReleaseLocateStatus::UnsupportedRepo,
                auth_mode,
                trimmed_version,
                0,
            ),
        }));
    };

    Ok(Json(
        locate_service_github_release_with_client(&client, repo, trimmed_version, per_page, limit)
            .await,
    ))
}

pub(super) async fn get_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    Ok(Json(ServiceSettingsResponse {
        auto_rollback: settings.auto_rollback,
        backup_targets: settings.backup_targets,
        repo_url: settings.repo_url,
    }))
}

pub(super) async fn infer_service_repo_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceRepoLinkInferenceResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };
    let context = build_repo_link_inference_context(&state)
        .await
        .map_err(map_internal)?;
    Ok(Json(
        infer_service_repo_link_for_snapshot_target(&state, &snapshot_target, &context)
            .await
            .into_response(),
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TriggerVersionInferenceRefreshRequest {
    digest: Option<String>,
}

pub(super) async fn trigger_service_version_inference_refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let body: TriggerVersionInferenceRefreshRequest = if body.is_empty() {
        TriggerVersionInferenceRefreshRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::invalid_argument("invalid json"))?
    };
    let digest_input = body.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    if digest_trimmed.is_empty() {
        return Err(ApiError::invalid_argument("digest is required"));
    }
    let digest = snapshot_worker::normalize_digest(digest_trimmed)
        .ok_or_else(|| ApiError::invalid_argument("digest is required"))?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };

    let known_digest = snapshot_target
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .is_some_and(|d| d.eq_ignore_ascii_case(&digest))
        || snapshot_target
            .candidate_digest
            .as_deref()
            .and_then(snapshot_worker::normalize_digest)
            .is_some_and(|d| d.eq_ignore_ascii_case(&digest));
    if !known_digest {
        return Err(ApiError::not_found("digest snapshot not found"));
    }

    let image_repo = snapshot_worker::image_repo_from_image_ref(&snapshot_target.image_ref)
        .ok_or_else(|| ApiError::invalid_argument("invalid service image ref"))?;
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let inserted = state
        .snapshot_worker
        .enqueue(
            &image_repo,
            &digest,
            &host_platform,
            VERSION_INFERENCE_REASON_FORCE,
        )
        .await;
    let reason = if inserted {
        VERSION_INFERENCE_REASON_FORCE
    } else {
        VERSION_INFERENCE_REASON_RUNNING
    };
    let resp = TriggerVersionInferenceRefreshResponse {
        status: "pending".to_string(),
        service_id,
        image_repo,
        digest,
        reason: reason.to_string(),
    };
    Ok((StatusCode::ACCEPTED, Json(resp)).into_response())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VersionInferenceOverviewQuery {
    q: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VersionInferenceEventsQuery {
    #[serde(default)]
    after_id: i64,
}

#[derive(Debug, Clone)]
pub(super) struct VersionInferenceOverviewRowAccum {
    image_repo: String,
    host_platform: String,
    service_count: u32,
    has_snapshot: bool,
    has_stale: bool,
    all_failed_only: bool,
    checked_at: Option<String>,
    updated_at: Option<String>,
    task: Option<snapshot_worker::SnapshotTaskSnapshot>,
}

pub(super) fn normalize_version_inference_status_filter(
    input: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if raw == "all" {
        return Ok(None);
    }
    match raw {
        "queued" | "running" | "ready" | "stale" | "all_failed" => Ok(Some(raw.to_string())),
        other => Err(ApiError::invalid_argument(format!(
            "invalid status filter: {other}"
        ))),
    }
}

pub(super) fn map_task_progress_state(
    progress: Option<snapshot_worker::SnapshotTaskProgress>,
) -> Option<VersionInferenceTaskProgressState> {
    progress.map(|p| VersionInferenceTaskProgressState {
        phase: p.phase,
        message: p.message,
        current: p.current,
        total: p.total,
        percent: p.percent,
        assigned_current: p.assigned_current,
        assigned_total: p.assigned_total,
        assigned_percent: p.assigned_percent,
        result_current: p.result_current,
        result_total: p.result_total,
        result_percent: p.result_percent,
        updated_at: p.updated_at,
    })
}

pub(super) fn derive_overview_row_status(
    row: &VersionInferenceOverviewRowAccum,
) -> (
    String,
    Option<String>,
    Option<VersionInferenceTaskProgressState>,
) {
    if let Some(task) = row.task.as_ref() {
        if task.status == "running" {
            return (
                "running".to_string(),
                Some(task.reason.clone()),
                map_task_progress_state(task.progress.clone()),
            );
        }
        if task.status == "queued" {
            return ("queued".to_string(), Some(task.reason.clone()), None);
        }
    }

    if row.has_snapshot {
        if row.has_stale {
            return (
                "stale".to_string(),
                Some(VERSION_INFERENCE_REASON_CACHE_STALE.to_string()),
                None,
            );
        }
        if row.all_failed_only {
            return (
                "all_failed".to_string(),
                Some(VERSION_INFERENCE_REASON_ALL_FAILED.to_string()),
                None,
            );
        }
        return ("ready".to_string(), None, None);
    }

    // Rows are constructed from cached snapshots and in-flight tasks only.
    (
        "queued".to_string(),
        Some(VERSION_INFERENCE_REASON_CACHE_MISS.to_string()),
        None,
    )
}

pub(super) fn version_inference_status_rank(status: &str) -> u8 {
    match status {
        "running" => 0,
        "queued" => 1,
        "stale" => 2,
        "all_failed" => 3,
        "ready" => 4,
        _ => 9,
    }
}

pub(super) async fn get_version_inference_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceOverviewQuery>,
) -> Result<Json<VersionInferenceOverviewResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let status_filter = normalize_version_inference_status_filter(q.status.as_deref())?;
    let search =
        q.q.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());

    let worker_snapshot = state.snapshot_worker.worker_stats().await;
    let gc_snapshot = state.snapshot_worker.gc_status().await;
    let task_snapshots = state.snapshot_worker.snapshot_tasks().await;
    let snapshot_rows = state
        .db
        .list_image_digest_tags_snapshots()
        .await
        .map_err(map_internal)?;
    let service_targets = state
        .db
        .list_version_inference_service_targets()
        .await
        .map_err(map_internal)?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    let mut service_count_by_key = BTreeMap::<String, u32>::new();
    for service in service_targets {
        if !needs_version_inference_for_tags(&service.image_tag, service.candidate_tag.as_deref()) {
            continue;
        }
        let Some(image_repo) = snapshot_worker::image_repo_from_image_ref(&service.image_ref)
        else {
            continue;
        };
        let key = format!("{image_repo}@{host_platform}");
        let count = service_count_by_key.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    }

    let mut rows_by_key = BTreeMap::<String, VersionInferenceOverviewRowAccum>::new();

    for snapshot in snapshot_rows {
        let key = format!("{}@{}", snapshot.image_repo, snapshot.host_platform);
        let entry =
            rows_by_key
                .entry(key.clone())
                .or_insert_with(|| VersionInferenceOverviewRowAccum {
                    image_repo: snapshot.image_repo.clone(),
                    host_platform: snapshot.host_platform.clone(),
                    service_count: *service_count_by_key.get(&key).unwrap_or(&0),
                    has_snapshot: false,
                    has_stale: false,
                    all_failed_only: true,
                    checked_at: None,
                    updated_at: None,
                    task: None,
                });
        entry.has_snapshot = true;
        entry.checked_at =
            checked_at_latest(entry.checked_at.clone(), Some(snapshot.checked_at.as_str()));
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(snapshot.updated_at.as_str()));
        if checked_at_is_stale(&snapshot.checked_at) {
            entry.has_stale = true;
        }
        let all_failed = parse_digest_snapshot_row(&snapshot.snapshot_json, &snapshot.checked_at)
            .is_some_and(|parsed| snapshot_worker::snapshot_is_all_failed(&parsed.snapshot));
        entry.all_failed_only = entry.all_failed_only && all_failed;
    }

    for task in task_snapshots.iter() {
        let image_key = format!("{}@{}", task.image_repo, task.host_platform);
        let entry = rows_by_key.entry(image_key.clone()).or_insert_with(|| {
            VersionInferenceOverviewRowAccum {
                image_repo: task.image_repo.clone(),
                host_platform: task.host_platform.clone(),
                service_count: *service_count_by_key.get(&image_key).unwrap_or(&0),
                has_snapshot: false,
                has_stale: false,
                all_failed_only: false,
                checked_at: None,
                updated_at: Some(task.updated_at.clone()),
                task: None,
            }
        });
        let replace = entry.task.as_ref().is_none_or(|existing| {
            version_inference_status_rank(&task.status)
                < version_inference_status_rank(&existing.status)
                || (task.status == existing.status && task.updated_at > existing.updated_at)
        });
        if replace {
            entry.task = Some(task.clone());
        }
        entry.updated_at =
            checked_at_latest(entry.updated_at.clone(), Some(task.updated_at.as_str()));
    }

    let mut all_rows = rows_by_key
        .into_values()
        .map(|row| {
            let (status, reason, progress) = derive_overview_row_status(&row);
            VersionInferenceOverviewRow {
                key: format!("{}@{}", row.image_repo, row.host_platform),
                image_repo: row.image_repo,
                host_platform: row.host_platform,
                status,
                service_count: row.service_count,
                reason,
                checked_at: row.checked_at,
                updated_at: row.updated_at,
                progress,
            }
        })
        .collect::<Vec<_>>();

    all_rows.sort_by(|a, b| {
        version_inference_status_rank(&a.status)
            .cmp(&version_inference_status_rank(&b.status))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.image_repo.cmp(&b.image_repo))
            .then_with(|| a.host_platform.cmp(&b.host_platform))
    });

    let snapshots_total = all_rows
        .iter()
        .filter(|row| row.checked_at.is_some())
        .count() as u32;
    let mut summary = VersionInferenceOverviewSummary {
        snapshots_total,
        queued: 0,
        running: 0,
        ready: 0,
        stale: 0,
        all_failed: 0,
    };
    for row in &all_rows {
        match row.status.as_str() {
            "queued" => summary.queued = summary.queued.saturating_add(1),
            "running" => summary.running = summary.running.saturating_add(1),
            "ready" => summary.ready = summary.ready.saturating_add(1),
            "stale" => summary.stale = summary.stale.saturating_add(1),
            "all_failed" => summary.all_failed = summary.all_failed.saturating_add(1),
            _ => {}
        }
    }

    let mut filtered_rows = all_rows;
    if let Some(status_filter) = status_filter.as_deref() {
        filtered_rows.retain(|row| row.status == status_filter);
    }
    if let Some(search) = search.as_deref() {
        filtered_rows.retain(|row| {
            row.image_repo.to_ascii_lowercase().contains(search)
                || row.key.to_ascii_lowercase().contains(search)
        });
    }

    let total = filtered_rows.len() as u32;
    let start = page.saturating_sub(1).saturating_mul(per_page) as usize;
    let rows = filtered_rows
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect::<Vec<_>>();

    let tasks = task_snapshots
        .into_iter()
        .map(|task| VersionInferenceTaskState {
            key: task.key,
            image_repo: task.image_repo,
            host_platform: task.host_platform,
            status: task.status,
            reason: task.reason,
            enqueued_at: task.enqueued_at,
            started_at: task.started_at,
            updated_at: task.updated_at,
            progress: map_task_progress_state(task.progress),
        })
        .collect::<Vec<_>>();

    Ok(Json(VersionInferenceOverviewResponse {
        worker: VersionInferenceWorkerState {
            max_concurrency: worker_snapshot.max_concurrency,
            queued: worker_snapshot.queued,
            running: worker_snapshot.running,
            in_flight: worker_snapshot.in_flight,
        },
        gc: VersionInferenceGcState {
            retention_days: gc_snapshot.retention_days,
            interval_seconds: gc_snapshot.interval_seconds,
            last_run_at: gc_snapshot.last_run_at,
            last_deleted: gc_snapshot.last_deleted,
            last_duration_ms: gc_snapshot.last_duration_ms,
            last_error: gc_snapshot.last_error,
        },
        summary,
        tasks,
        rows,
        page,
        per_page,
        total,
    }))
}

pub(super) async fn version_inference_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<VersionInferenceEventsQuery>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let mut after_id = resolve_sse_after_id(&headers, q.after_id);
    if after_id <= 0 {
        after_id = state.snapshot_worker.latest_event_id().await;
    }
    let sse_state = state.clone();

    let stream = async_stream::stream! {
        loop {
            let batch = sse_state
                .snapshot_worker
                .events_since(after_id, 200)
                .await;

            if let Some(oldest_id) = batch.oldest_id
                && after_id > 0
                && after_id < oldest_id.saturating_sub(1)
            {
                let evt = sse_state
                    .snapshot_worker
                    .emit_resync_required(after_id, oldest_id, batch.latest_id)
                    .await;
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
                continue;
            }

            if batch.events.is_empty() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            for evt in batch.events {
                after_id = evt.id;
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(evt.id.to_string())
                        .event("version_inference_event")
                        .data(evt.data.to_string()),
                );
            }
        }
    };
    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((resp_headers, sse))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ServiceResourceHistoryQuery {
    window: Option<String>,
}

pub(super) async fn get_service_resource_usage_history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ServiceResourceHistoryQuery>,
) -> Result<Json<ServiceResourceHistoryResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    if !settings.enabled {
        return Err(
            ApiError::conflict("resource monitor disabled").with_details(json!({
                "reason": "resource_monitor_disabled",
            })),
        );
    }

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if stack_id.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let window = q.window.unwrap_or_else(|| "1h".to_string());
    let Some(window_seconds) = resource_usage::parse_window_to_seconds(&window) else {
        return Err(ApiError::invalid_argument(
            "window must be one of 15m/1h/6h",
        ));
    };

    let since = (time::OffsetDateTime::now_utc() - time::Duration::seconds(window_seconds as i64))
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| map_internal(err.into()))?;
    let samples = state
        .db
        .list_service_resource_samples_since(&service_id, &since)
        .await
        .map_err(map_internal)?;

    Ok(Json(ServiceResourceHistoryResponse {
        service_id,
        window,
        samples,
    }))
}

pub(super) async fn service_resource_usage_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_resource_monitor_settings()
        .await
        .map_err(map_internal)?;
    if !settings.enabled {
        return Err(
            ApiError::conflict("resource monitor disabled").with_details(json!({
                "reason": "resource_monitor_disabled",
            })),
        );
    }

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    if stack_id.is_none() {
        return Err(ApiError::not_found("service not found"));
    }

    let mut subscription = state.resource_hub.subscribe(&service_id).await;
    let (initial, initial_error) = match state.resource_hub.sample_once(&service_id).await {
        Ok(sample) => (sample, None),
        Err(err) => {
            tracing::warn!(
                service_id = %service_id,
                error = %err,
                "resource monitor initial snapshot failed"
            );
            (None, Some(err.to_string()))
        }
    };
    let stream_service_id = service_id.clone();

    let stream = async_stream::stream! {
        let mut event_id: u64 = 0;
        if let Some(error) = initial_error {
            event_id = event_id.saturating_add(1);
            let data = json!({
                "serviceId": stream_service_id.clone(),
                "error": error,
            });
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(event_id.to_string())
                    .event("resource_usage_error")
                    .data(data.to_string()),
            );
        } else if let Some(sample) = initial {
                event_id = event_id.saturating_add(1);
                let data = json!({
                    "serviceId": stream_service_id.clone(),
                    "sample": sample,
                });
                yield Ok::<Event, Infallible>(
                    Event::default()
                        .id(event_id.to_string())
                        .event("resource_usage_snapshot")
                        .data(data.to_string()),
                );
        } else {
            event_id = event_id.saturating_add(1);
            let data = json!({
                "serviceId": stream_service_id.clone(),
                "error": "runtime_stats_unavailable",
            });
            yield Ok::<Event, Infallible>(
                Event::default()
                    .id(event_id.to_string())
                    .event("resource_usage_error")
                    .data(data.to_string()),
            );
        }

        loop {
            match subscription.recv().await {
                Ok(resource_usage::RealtimeMessage::Tick(sample)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "sample": sample,
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_tick")
                            .data(data.to_string()),
                    );
                }
                Ok(resource_usage::RealtimeMessage::Error(error)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "error": error,
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_error")
                            .data(data.to_string()),
                    );
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    event_id = event_id.saturating_add(1);
                    let data = json!({
                        "serviceId": stream_service_id.clone(),
                        "error": "resource_usage_lagged",
                    });
                    yield Ok::<Event, Infallible>(
                        Event::default()
                            .id(event_id.to_string())
                            .event("resource_usage_error")
                            .data(data.to_string()),
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((resp_headers, sse))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListServiceDigestTagsQuery {
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetServiceDigestTagsSnapshotQuery {
    digest: Option<String>,
}

pub(super) async fn get_service_digest_tags_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<GetServiceDigestTagsSnapshotQuery>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    if digest_trimmed.is_empty() {
        return Err(ApiError::invalid_argument("digest is required"));
    }

    let digest = snapshot_worker::normalize_digest(digest_trimmed)
        .ok_or_else(|| ApiError::invalid_argument("digest is required"))?;

    let snapshot_target = state
        .db
        .get_service_snapshot_target(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(snapshot_target) = snapshot_target else {
        return Err(ApiError::not_found("service not found"));
    };

    let known_digest = snapshot_target
        .current_digest
        .as_deref()
        .and_then(snapshot_worker::normalize_digest)
        .is_some_and(|d| d.eq_ignore_ascii_case(&digest))
        || snapshot_target
            .candidate_digest
            .as_deref()
            .and_then(snapshot_worker::normalize_digest)
            .is_some_and(|d| d.eq_ignore_ascii_case(&digest));
    if !known_digest {
        return Err(ApiError::not_found("digest snapshot not found"));
    }

    let image_repo = snapshot_worker::image_repo_from_image_ref(&snapshot_target.image_ref)
        .ok_or_else(|| ApiError::invalid_argument("invalid service image ref"))?;
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let in_flight_reason = state
        .snapshot_worker
        .in_flight_reason(&image_repo, &digest, &host_platform)
        .await;

    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, &digest, &host_platform)
        .await
        .map_err(map_internal)?;
    let Some((snapshot_json, _checked_at, _updated_at)) = snapshot else {
        // If an inference task is already in flight (cache refresh / new version, etc.),
        // just surface `pending` so callers can poll, but avoid enqueuing duplicate work.
        if in_flight_reason.is_none() {
            state
                .snapshot_worker
                .enqueue(
                    &image_repo,
                    &digest,
                    &host_platform,
                    "api_snapshot_read_miss",
                )
                .await;
        }
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    };

    // When a user explicitly triggers a digest refresh, we want callers to wait for the new
    // snapshot even if an older one is available.
    if in_flight_reason.as_deref() == Some(VERSION_INFERENCE_REASON_FORCE) {
        let pending = ServiceDigestTagsSnapshotPendingResponse {
            status: "pending".to_string(),
            digest: digest.clone(),
            retry_after_ms: snapshot_worker::SNAPSHOT_PENDING_RETRY_AFTER_MS,
        };
        return Ok((StatusCode::ACCEPTED, Json(pending)).into_response());
    }

    let parsed: ServiceDigestTagsSnapshotResponse =
        serde_json::from_str(&snapshot_json).map_err(|e| {
            ApiError::internal("invalid digest tags snapshot").with_details(json!({
                "error": e.to_string(),
            }))
        })?;

    Ok(Json(parsed).into_response())
}

pub(super) async fn list_service_digest_tags(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(q): Query<ListServiceDigestTagsQuery>,
) -> Result<Json<ServiceDigestTagsResponse>, ApiError> {
    use std::time::Duration;

    use tokio::{
        task::JoinSet,
        time::{Instant, timeout, timeout_at},
    };

    let _user = require_user(&state, &headers).await?;

    let digest_input = q.digest.unwrap_or_default();
    let digest_trimmed = digest_input.trim();
    // This endpoint is primarily used for UI observability. When digest is missing, we still want
    // to return the full `repo_tags` list so the UI can show something actionable (and avoid
    // "empty bubbles").
    let (digest, wanted) = if digest_trimmed.is_empty() {
        (String::new(), None)
    } else if digest_trimmed.contains(':') {
        (digest_trimmed.to_string(), Some(digest_trimmed.to_string()))
    } else {
        let normalized = format!("sha256:{digest_trimmed}");
        (normalized.clone(), Some(normalized))
    };

    let stack_id = state
        .db
        .get_service_stack_id(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(stack_id) = stack_id else {
        return Err(ApiError::not_found("service not found"));
    };

    let stack = state.db.get_stack(&stack_id).await.map_err(map_internal)?;
    let Some(stack) = stack else {
        return Err(ApiError::not_found("stack not found"));
    };

    let svc = stack
        .services
        .iter()
        .find(|s| s.id == service_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("service not found"))?;

    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());

    let img = registry::ImageRef::parse(&svc.image.reference).map_err(|_| {
        ApiError::invalid_argument("invalid image ref (expected repo/name[:tag][@sha256:digest])")
    })?;

    // Digest tag listing is used for UI debugging / observability, not as part of the "update
    // candidates" hot path. Still, we bound latency to avoid hanging requests forever.
    const LIST_TAGS_TIMEOUT: Duration = Duration::from_secs(8);
    const MANIFEST_TIMEOUT: Duration = Duration::from_secs(6);
    const MANIFEST_BUDGET: Duration = Duration::from_secs(40);
    const MANIFEST_CONCURRENCY: usize = 10;

    let repo_tags = match timeout(LIST_TAGS_TIMEOUT, state.registry.list_tags(&img)).await {
        Ok(Ok(tags)) => tags,
        Ok(Err(e)) => return Err(map_internal(e)),
        Err(_) => {
            return Err(ApiError::internal("registry timeout").with_details(json!({
                "op": "list_tags"
            })));
        }
    };

    let repo_tags_total = repo_tags.len();
    let Some(wanted) = wanted else {
        return Ok(Json(ServiceDigestTagsResponse {
            digest,
            tags: Vec::new(),
            repo_tags,
            scan: ServiceDigestTagsScanSummary {
                repo_tags_total,
                repo_tags_considered: 0,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        }));
    };

    let registry = state.registry.clone();
    let img = img.clone();
    let host_platform = host_platform.clone();

    let mut out: Vec<String> = Vec::new();
    let mut manifests_ok: usize = 0;
    let mut manifests_timeout: usize = 0;
    let mut manifests_error: usize = 0;

    enum ScanOutcome {
        OkMatch(String),
        OkNoMatch,
        Timeout,
        Error,
    }

    let mut join_set: JoinSet<ScanOutcome> = JoinSet::new();
    let mut queue = repo_tags.iter().cloned();

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
                // Degrade gracefully: keep best-effort matches and surface incompleteness via the
                // scan summary instead of failing the whole request.
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

    // If the budget was exhausted (or tasks were aborted), treat the remaining tags as timeouts so
    // the UI can warn that the result may be incomplete.
    let processed = manifests_ok + manifests_timeout + manifests_error;
    if processed < repo_tags_total {
        manifests_timeout += repo_tags_total - processed;
    }

    let mut semver_tags: Vec<(semver::Version, String)> = Vec::new();
    let mut other_tags: Vec<String> = Vec::new();
    for tag in out {
        if let Some(v) = ignore::parse_version(&tag) {
            semver_tags.push((v, tag));
        } else {
            other_tags.push(tag);
        }
    }

    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    other_tags.sort_by(|a, b| b.cmp(a));

    let mut sorted: Vec<String> = Vec::new();
    for (_, tag) in semver_tags {
        sorted.push(tag);
    }
    for tag in other_tags {
        sorted.push(tag);
    }

    Ok(Json(ServiceDigestTagsResponse {
        digest,
        tags: sorted,
        repo_tags,
        scan: ServiceDigestTagsScanSummary {
            repo_tags_total,
            repo_tags_considered: repo_tags_total,
            manifests_ok,
            manifests_timeout,
            manifests_error,
        },
    }))
}

pub(super) async fn put_service_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Json(req): Json<ServiceSettingsRequest>,
) -> Result<Json<PutServiceSettingsResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let now = now_rfc3339().map_err(map_internal)?;
    let current_settings = state
        .db
        .get_stored_service_settings(&service_id)
        .await
        .map_err(map_internal)?
        .ok_or_else(|| ApiError::not_found("service not found"))?;
    let (repo_url, repo_url_auto_disabled) = match req.repo_url {
        Some(repo_url) => {
            let repo_url = normalize_repo_url_input(repo_url.as_deref())?;
            let repo_url_auto_disabled = repo_url.is_none();
            (repo_url, repo_url_auto_disabled)
        }
        None => (
            current_settings.settings.repo_url.clone(),
            current_settings.repo_url_auto_disabled,
        ),
    };

    let settings = ServiceSettings {
        auto_rollback: req.auto_rollback,
        backup_targets: req.backup_targets,
        repo_url,
    };

    let updated = state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &settings,
            repo_url_auto_disabled,
            &now,
        )
        .await
        .map_err(map_internal)?;

    if !updated {
        return Err(ApiError::not_found("service not found"));
    }

    Ok(Json(PutServiceSettingsResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router, extract::Query, http::HeaderMap, response::IntoResponse, routing::get,
    };
    use serde_json::json;
    use url::Url;

    #[test]
    fn github_release_tag_variants_supports_plain_and_v_prefixed_tags() {
        assert_eq!(
            github_release_tag_variants("1.40.0"),
            vec!["1.40.0".to_string(), "v1.40.0".to_string()]
        );
        assert_eq!(
            github_release_tag_variants("v1.39.5"),
            vec!["v1.39.5".to_string(), "1.39.5".to_string()]
        );
    }

    #[test]
    fn classify_github_releases_failure_prefers_rate_limit() {
        let err = anyhow::anyhow!("github http 403 Forbidden: API rate limit exceeded");
        assert_eq!(
            classify_github_releases_failure(GitHubReleaseAuthMode::Anonymous, &err),
            ServiceGitHubReleasesStatus::RateLimited
        );
    }

    #[test]
    fn classify_github_releases_failure_keeps_repo_not_found_as_upstream_error() {
        let err = anyhow::anyhow!("github http 404 Not Found: repository not found");
        assert_eq!(
            classify_github_releases_failure(GitHubReleaseAuthMode::Anonymous, &err),
            ServiceGitHubReleasesStatus::UpstreamError
        );
    }

    #[tokio::test]
    async fn list_service_github_releases_falls_back_to_anonymous_when_pat_cannot_access_public_repo()
     {
        async fn releases(headers: HeaderMap) -> impl IntoResponse {
            if headers.contains_key("authorization") {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(json!({ "message": "Not Found" })),
                )
                    .into_response();
            }
            Json(json!([
                {
                    "id": 101,
                    "tag_name": "v1.40.0",
                    "name": "v1.40.0",
                    "body": "release notes",
                    "html_url": "https://github.com/acme/repo/releases/tag/v1.40.0",
                    "draft": false,
                    "prerelease": false,
                    "published_at": "2026-04-07T00:22:00Z",
                    "created_at": "2026-04-07T00:20:00Z"
                }
            ]))
            .into_response()
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/repos/acme/repo/releases", get(releases)),
            )
            .await
            .unwrap();
        });

        let client = github::GitHubClient::new_with_base_url(
            Some("test-token"),
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let response = list_service_github_releases_with_client(
            &client,
            ServiceGitHubRepoRef {
                full_name: "acme/repo".to_string(),
                html_url: "https://github.com/acme/repo".to_string(),
            },
            1,
            20,
        )
        .await;

        assert_eq!(response.status, ServiceGitHubReleasesStatus::Ready);
        assert_eq!(response.auth_mode, GitHubReleaseAuthMode::Anonymous);
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].tag_name, "v1.40.0");
    }

    #[tokio::test]
    async fn locate_service_github_release_falls_back_to_anonymous_when_pat_cannot_access_public_repo()
     {
        #[derive(Deserialize)]
        struct ReleasesQuery {
            page: Option<u32>,
        }

        async fn releases(
            headers: HeaderMap,
            Query(query): Query<ReleasesQuery>,
        ) -> impl IntoResponse {
            if headers.contains_key("authorization") {
                return (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(json!({ "message": "Not Found" })),
                )
                    .into_response();
            }
            match query.page.unwrap_or(1) {
                1 => Json(json!([
                    {
                        "id": 101,
                        "tag_name": "1.39.5",
                        "name": "1.39.5",
                        "body": "release notes",
                        "html_url": "https://github.com/acme/repo/releases/tag/1.39.5",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-04-07T00:22:00Z",
                        "created_at": "2026-04-07T00:20:00Z"
                    }
                ]))
                .into_response(),
                _ => Json(json!([])).into_response(),
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/repos/acme/repo/releases", get(releases)),
            )
            .await
            .unwrap();
        });

        let client = github::GitHubClient::new_with_base_url(
            Some("test-token"),
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let response = locate_service_github_release_with_client(
            &client,
            ServiceGitHubRepoRef {
                full_name: "acme/repo".to_string(),
                html_url: "https://github.com/acme/repo".to_string(),
            },
            "1.39.5",
            20,
            50,
        )
        .await;

        assert_eq!(response.status, ServiceGitHubReleaseLocateStatus::Found);
        assert_eq!(response.auth_mode, GitHubReleaseAuthMode::Anonymous);
        assert_eq!(response.matched_tag.as_deref(), Some("1.39.5"));
    }

    #[tokio::test]
    async fn locate_service_github_release_finds_release_within_window() {
        #[derive(Deserialize)]
        struct ReleasesQuery {
            page: Option<u32>,
        }

        async fn releases(Query(query): Query<ReleasesQuery>) -> impl IntoResponse {
            match query.page.unwrap_or(1) {
                1 => (
                    [("link", "</repos/acme/repo/releases?page=2&per_page=20>; rel=\"next\"")],
                    Json(json!((0..20)
                        .map(|idx| json!({
                            "id": idx + 1,
                            "tag_name": format!("v0.0.{idx}"),
                            "name": format!("v0.0.{idx}"),
                            "body": null,
                            "html_url": format!("https://github.com/acme/repo/releases/tag/v0.0.{idx}"),
                            "draft": false,
                            "prerelease": false,
                            "published_at": "2026-04-07T00:00:00Z",
                            "created_at": "2026-04-07T00:00:00Z"
                        }))
                        .collect::<Vec<_>>())),
                )
                    .into_response(),
                2 => Json(json!([
                    {
                        "id": 101,
                        "tag_name": "v1.40.0",
                        "name": "v1.40.0",
                        "body": "release notes",
                        "html_url": "https://github.com/acme/repo/releases/tag/v1.40.0",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-04-07T00:22:00Z",
                        "created_at": "2026-04-07T00:20:00Z"
                    }
                ]))
                .into_response(),
                _ => Json(json!([])).into_response(),
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/repos/acme/repo/releases", get(releases)),
            )
            .await
            .unwrap();
        });

        let client = github::GitHubClient::new_with_base_url(
            None,
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let response = locate_service_github_release_with_client(
            &client,
            ServiceGitHubRepoRef {
                full_name: "acme/repo".to_string(),
                html_url: "https://github.com/acme/repo".to_string(),
            },
            "1.40.0",
            20,
            50,
        )
        .await;

        assert_eq!(response.status, ServiceGitHubReleaseLocateStatus::Found);
        assert_eq!(response.page, Some(2));
        assert_eq!(response.index_within_page, Some(0));
        assert_eq!(response.absolute_index, Some(20));
        assert_eq!(response.matched_tag.as_deref(), Some("v1.40.0"));
    }

    #[tokio::test]
    async fn locate_service_github_release_reports_outside_window_when_direct_hit_is_older() {
        async fn release_by_tag() -> Json<serde_json::Value> {
            Json(json!({
                "id": 501,
                "tag_name": "1.39.5",
                "name": "1.39.5",
                "body": null,
                "html_url": "https://github.com/acme/repo/releases/tag/1.39.5",
                "draft": false,
                "prerelease": false,
                "published_at": "2026-04-07T00:37:00Z",
                "created_at": "2026-04-07T00:30:00Z"
            }))
        }

        async fn releases_page() -> Json<serde_json::Value> {
            Json(json!(
                (0..20)
                    .map(|idx| json!({
                        "id": idx + 1,
                        "tag_name": format!("v9.9.{idx}"),
                        "name": format!("v9.9.{idx}"),
                        "body": null,
                        "html_url": format!("https://github.com/acme/repo/releases/tag/v9.9.{idx}"),
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-04-07T00:00:00Z",
                        "created_at": "2026-04-07T00:00:00Z"
                    }))
                    .collect::<Vec<_>>()
            ))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/repos/acme/repo/releases", get(releases_page))
                    .route("/repos/acme/repo/releases/tags/1.39.5", get(release_by_tag)),
            )
            .await
            .unwrap();
        });

        let client = github::GitHubClient::new_with_base_url(
            None,
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let response = locate_service_github_release_with_client(
            &client,
            ServiceGitHubRepoRef {
                full_name: "acme/repo".to_string(),
                html_url: "https://github.com/acme/repo".to_string(),
            },
            "1.39.5",
            20,
            50,
        )
        .await;

        assert_eq!(
            response.status,
            ServiceGitHubReleaseLocateStatus::OutsideWindow
        );
        assert_eq!(response.matched_tag.as_deref(), Some("1.39.5"));
        assert_eq!(response.searched_count, 20);
    }
}
