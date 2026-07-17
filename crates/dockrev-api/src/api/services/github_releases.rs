use super::repo_links::{
    github_repo_url_from_key, normalize_github_source_repo_key, normalize_repo_full_name,
};
use super::*;

const DEFAULT_GITHUB_RELEASES_PAGE: u32 = 1;
const DEFAULT_GITHUB_RELEASES_PER_PAGE: u32 = 20;
const MAX_GITHUB_RELEASES_PER_PAGE: u32 = 100;
pub(super) const GITHUB_RELEASE_LOCATE_SCAN_LIMIT: u32 = 50;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceGitHubReleasesQuery {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GitHubReleaseLocateStatus {
    Found,
    OutsideWindow,
    NotFound,
    UnsupportedRepo,
    PermissionDenied,
    RateLimited,
    UpstreamError,
}

#[derive(Clone, Debug)]
pub(super) struct GitHubReleaseLocateResult {
    pub status: GitHubReleaseLocateStatus,
    pub auth_mode: GitHubReleaseAuthMode,
    #[allow(dead_code)]
    pub searched_count: u32,
    pub matched_tag: Option<String>,
    pub page: Option<u32>,
    pub index_within_page: Option<u32>,
    pub absolute_index: Option<u32>,
    pub message: Option<String>,
}

pub(super) fn github_release_auth_mode(mode: github::GitHubAuthMode) -> GitHubReleaseAuthMode {
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
    let Some(stored_settings) = state.db.get_stored_service_settings(service_id).await? else {
        return Ok(None);
    };
    if stored_settings.repo_url_auto_disabled {
        return Ok(None);
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

pub(super) fn normalize_github_releases_page(value: Option<u32>) -> u32 {
    value.unwrap_or(DEFAULT_GITHUB_RELEASES_PAGE).max(1)
}

pub(super) fn normalize_github_releases_per_page(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_GITHUB_RELEASES_PER_PAGE)
        .clamp(1, MAX_GITHUB_RELEASES_PER_PAGE)
}

pub(super) fn github_release_item_from_api(
    release: github::GitHubRelease,
) -> ServiceGitHubReleaseItem {
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

pub(super) fn github_release_tag_variants(version: &str) -> Vec<String> {
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
    status: GitHubReleaseLocateStatus,
    auth_mode: GitHubReleaseAuthMode,
    version: &str,
    searched_count: u32,
) -> Option<String> {
    Some(match status {
        GitHubReleaseLocateStatus::Found => return None,
        GitHubReleaseLocateStatus::OutsideWindow => {
            format!("已定位到 {version}，但它不在前 {searched_count} 条发布记录内。")
        }
        GitHubReleaseLocateStatus::NotFound => {
            format!("在前 {searched_count} 条发布记录中未找到 {version}。")
        }
        GitHubReleaseLocateStatus::UnsupportedRepo => {
            github_release_error_message(ServiceGitHubReleasesStatus::UnsupportedRepo, auth_mode)
        }
        GitHubReleaseLocateStatus::PermissionDenied => {
            github_release_error_message(ServiceGitHubReleasesStatus::PermissionDenied, auth_mode)
        }
        GitHubReleaseLocateStatus::RateLimited => {
            github_release_error_message(ServiceGitHubReleasesStatus::RateLimited, auth_mode)
        }
        GitHubReleaseLocateStatus::UpstreamError => {
            github_release_error_message(ServiceGitHubReleasesStatus::UpstreamError, auth_mode)
        }
    })
}

fn github_release_locate_failure_message(
    status: GitHubReleaseLocateStatus,
    auth_mode: GitHubReleaseAuthMode,
    version: &str,
    searched_count: u32,
    err: &anyhow::Error,
) -> Option<String> {
    if status == GitHubReleaseLocateStatus::UpstreamError
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

pub(super) fn classify_github_releases_failure(
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
) -> GitHubReleaseLocateStatus {
    match classify_github_releases_failure(auth_mode, err) {
        ServiceGitHubReleasesStatus::PermissionDenied => {
            GitHubReleaseLocateStatus::PermissionDenied
        }
        ServiceGitHubReleasesStatus::RateLimited => GitHubReleaseLocateStatus::RateLimited,
        ServiceGitHubReleasesStatus::UnsupportedRepo => GitHubReleaseLocateStatus::UnsupportedRepo,
        ServiceGitHubReleasesStatus::UpstreamError | ServiceGitHubReleasesStatus::Ready => {
            GitHubReleaseLocateStatus::UpstreamError
        }
    }
}

pub(super) fn build_service_github_releases_client(
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

pub(super) async fn list_service_github_releases_with_client(
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
        && let Some(message) = response.message.as_deref()
        && (response.status == ServiceGitHubReleasesStatus::PermissionDenied
            || message.contains("GitHub 返回 404"))
        && let Ok(anonymous) = client.clone_as_anonymous()
    {
        let fallback =
            list_service_github_releases_with_client_once(&anonymous, repo, page, per_page).await;
        if fallback.status == ServiceGitHubReleasesStatus::Ready {
            return fallback;
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

pub(super) async fn locate_service_github_release_with_client(
    client: &github::GitHubClient,
    repo: ServiceGitHubRepoRef,
    version: &str,
    per_page: u32,
    limit: u32,
) -> GitHubReleaseLocateResult {
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
            GitHubReleaseLocateStatus::PermissionDenied | GitHubReleaseLocateStatus::UpstreamError
        )
        && let Some(message) = response.message.as_deref()
        && (response.status == GitHubReleaseLocateStatus::PermissionDenied
            || message.contains("GitHub 返回 404"))
        && let Ok(anonymous) = client.clone_as_anonymous()
    {
        let fallback = locate_service_github_release_with_client_once(
            &anonymous, repo, version, per_page, limit,
        )
        .await;
        if !matches!(
            fallback.status,
            GitHubReleaseLocateStatus::PermissionDenied | GitHubReleaseLocateStatus::UpstreamError
        ) {
            return fallback;
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
) -> GitHubReleaseLocateResult {
    let auth_mode = github_release_auth_mode(client.auth_mode());
    let trimmed_version = version.trim().to_string();
    let empty = || GitHubReleaseLocateResult {
        status: GitHubReleaseLocateStatus::UnsupportedRepo,
        auth_mode,
        searched_count: 0,
        matched_tag: None,
        page: None,
        index_within_page: None,
        absolute_index: None,
        message: github_release_locate_error_message(
            GitHubReleaseLocateStatus::UnsupportedRepo,
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
        return GitHubReleaseLocateResult {
            status: GitHubReleaseLocateStatus::NotFound,
            auth_mode,
            searched_count: 0,
            matched_tag: None,
            page: None,
            index_within_page: None,
            absolute_index: None,
            message: github_release_locate_error_message(
                GitHubReleaseLocateStatus::NotFound,
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
                return GitHubReleaseLocateResult {
                    status,
                    auth_mode,
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
                return GitHubReleaseLocateResult {
                    status,
                    auth_mode,
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
                return GitHubReleaseLocateResult {
                    status: GitHubReleaseLocateStatus::Found,
                    auth_mode,
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
        GitHubReleaseLocateStatus::OutsideWindow
    } else {
        GitHubReleaseLocateStatus::NotFound
    };
    GitHubReleaseLocateResult {
        status,
        auth_mode,
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

pub(crate) async fn list_service_github_releases(
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
