use super::github_releases::{
    GITHUB_RELEASE_LOCATE_SCAN_LIMIT, GitHubReleaseLocateStatus,
    build_service_github_releases_client, github_release_tag_variants,
    list_service_github_releases_with_client, locate_service_github_release_with_client,
    normalize_github_releases_per_page,
};
use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ServiceReleaseNotesDirection {
    #[default]
    Older,
    Newer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceReleaseNotesQuery {
    cursor: Option<String>,
    direction: Option<ServiceReleaseNotesDirection>,
    limit: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceReleaseNotesLocateQuery {
    version: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug)]
struct OctoRillPublicReleaseNotesSuccess {
    items: Vec<ServiceReleaseNoteItem>,
    next_cursor: Option<String>,
    previous_cursor: Option<String>,
    matched_tag: Option<String>,
    index_within_window: Option<u32>,
}

#[derive(Debug)]
struct OctoRillPublicReleaseNotesFailure {
    reason: ServiceReleaseNotesFailureReason,
    message: String,
}

#[derive(Debug, Deserialize)]
struct OctoRillPublicHighlightResolved {
    #[serde(default)]
    tag_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OctoRillPublicHighlight {
    #[serde(default)]
    resolved: Vec<OctoRillPublicHighlightResolved>,
    #[serde(default)]
    active_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OctoRillPublicReleaseContentResponse {
    status: String,
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    previous_cursor: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    highlight: Option<OctoRillPublicHighlight>,
}

const DEFAULT_RELEASE_NOTES_LIMIT: u32 = 20;
const MAX_RELEASE_NOTES_LIMIT: u32 = 100;
const DEFAULT_RELEASE_NOTES_LOCATE_LIMIT: u32 = 20;
const MAX_RELEASE_NOTES_LOCATE_LIMIT: u32 = 30;

fn normalize_release_notes_limit(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_RELEASE_NOTES_LIMIT)
        .clamp(1, MAX_RELEASE_NOTES_LIMIT)
}

fn normalize_release_notes_locate_limit(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_RELEASE_NOTES_LOCATE_LIMIT)
        .clamp(1, MAX_RELEASE_NOTES_LOCATE_LIMIT)
}

fn normalize_release_notes_direction(
    value: Option<ServiceReleaseNotesDirection>,
) -> ServiceReleaseNotesDirection {
    value.unwrap_or_default()
}

fn failure_message(reason: ServiceReleaseNotesFailureReason) -> String {
    match reason {
        ServiceReleaseNotesFailureReason::Disabled => "OctoRill 更新日志未启用。".to_string(),
        ServiceReleaseNotesFailureReason::NotConfigured => {
            "OctoRill API Base URL 或 API Key 未配置完整。".to_string()
        }
        ServiceReleaseNotesFailureReason::UnsupportedRepo => {
            "未能解析该服务的 GitHub 仓库，无法读取 OctoRill Release Notes。".to_string()
        }
        ServiceReleaseNotesFailureReason::Unauthorized => {
            "OctoRill API Key 无效或权限不足。".to_string()
        }
        ServiceReleaseNotesFailureReason::EmptyFeed => {
            "OctoRill 没有返回可展示的发布记录。".to_string()
        }
        ServiceReleaseNotesFailureReason::UpstreamError => {
            "OctoRill 公开 Release 暂不可用。".to_string()
        }
    }
}

fn page_failure_message(reason: ServiceReleaseNotesFailureReason) -> String {
    match reason {
        ServiceReleaseNotesFailureReason::UnsupportedRepo => {
            "未能解析该服务的 GitHub 仓库，无法继续读取 OctoRill Release Notes。".to_string()
        }
        ServiceReleaseNotesFailureReason::Unauthorized => {
            "OctoRill API Key 无效或权限不足，无法继续读取 OctoRill 发布记录。".to_string()
        }
        ServiceReleaseNotesFailureReason::EmptyFeed => {
            "OctoRill 没有返回可继续展示的发布记录。".to_string()
        }
        ServiceReleaseNotesFailureReason::NotConfigured => {
            "OctoRill API Base URL 或 API Key 未配置完整，无法继续读取 OctoRill 发布记录。"
                .to_string()
        }
        ServiceReleaseNotesFailureReason::Disabled => {
            "OctoRill 更新日志未启用，无法继续读取 OctoRill 发布记录。".to_string()
        }
        ServiceReleaseNotesFailureReason::UpstreamError => {
            "OctoRill 公开 Release 请求失败，无法继续读取后续发布记录。".to_string()
        }
    }
}

fn octo_rill_page_failure_response(
    repo: Option<ServiceGitHubRepoRef>,
    cursor: Option<String>,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    reason: ServiceReleaseNotesFailureReason,
) -> ServiceReleaseNotesResponse {
    ServiceReleaseNotesResponse {
        status: match reason {
            ServiceReleaseNotesFailureReason::UnsupportedRepo => {
                ServiceReleaseNotesStatus::UnsupportedRepo
            }
            _ => ServiceReleaseNotesStatus::UpstreamError,
        },
        source: ServiceReleaseNotesSource::OctoRill,
        repo,
        cursor,
        limit,
        next_cursor: None,
        previous_cursor: None,
        has_more: false,
        default_view,
        external_links,
        items: Vec::new(),
        message: Some(page_failure_message(reason)),
        stale: None,
        anchor: None,
    }
}

fn octo_rill_locate_failure_response(
    repo: Option<ServiceGitHubRepoRef>,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    version: &str,
    reason: ServiceReleaseNotesFailureReason,
    message: String,
) -> ServiceReleaseNotesResponse {
    let mut response =
        octo_rill_page_failure_response(repo, None, limit, default_view, external_links, reason);
    response.message = Some(message.clone());
    response.anchor = Some(ServiceReleaseNotesAnchor {
        status: ServiceReleaseNotesAnchorStatus::Unavailable,
        version: version.to_string(),
        matched_tag: None,
        index_within_window: None,
        absolute_index: None,
        message: Some(message),
    });
    response
}

fn parse_cursor_as_page(cursor: Option<&str>) -> u32 {
    cursor
        .map(|value| value.strip_prefix("github:").unwrap_or(value))
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

fn github_cursor_for_page(page: u32) -> String {
    format!("github:{page}")
}

fn github_previous_cursor(page: u32) -> Option<String> {
    (page > 1).then(|| github_cursor_for_page(page - 1))
}

fn github_current_cursor(page: u32) -> Option<String> {
    (page > 1).then(|| github_cursor_for_page(page))
}

fn octo_rill_cursor_for_upstream(cursor: &str) -> String {
    format!("octo:{}", URL_SAFE_NO_PAD.encode(cursor.as_bytes()))
}

fn upstream_cursor_from_octo_rill_cursor(cursor: &str) -> Option<String> {
    let encoded = cursor.trim().strip_prefix("octo:")?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    String::from_utf8(bytes)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_github_cursor(cursor: Option<&str>) -> bool {
    cursor
        .map(str::trim)
        .is_some_and(|value| value.starts_with("github:") || value.parse::<u32>().is_ok())
}

fn build_github_releases_url(repo: &ServiceGitHubRepoRef) -> Option<String> {
    let mut url = url::Url::parse(&repo.html_url).ok()?;
    {
        let mut segments = url.path_segments_mut().ok()?;
        segments.pop_if_empty();
        segments.push("releases");
    }
    Some(url.to_string())
}

fn split_repo_owner_and_name(full_name: &str) -> Option<(&str, &str)> {
    let trimmed = full_name.trim();
    let (owner, repo) = trimmed.split_once('/')?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

fn build_octo_rill_releases_url(
    base_url: Option<&str>,
    repo: &ServiceGitHubRepoRef,
) -> Option<String> {
    let base_url = base_url?.trim();
    if base_url.is_empty() {
        return None;
    }
    let (owner, repo_name) = split_repo_owner_and_name(&repo.full_name)?;
    let mut url = url::Url::parse(base_url).ok()?;
    {
        let mut segments = url.path_segments_mut().ok()?;
        segments.pop_if_empty();
        segments.extend([owner, repo_name, "releases"]);
    }
    Some(url.to_string())
}

fn build_external_links(
    repo: Option<&ServiceGitHubRepoRef>,
    octo_rill_base_url: Option<&str>,
) -> Option<ServiceReleaseNotesExternalLinks> {
    let repo = repo?;
    Some(ServiceReleaseNotesExternalLinks {
        github_releases_url: build_github_releases_url(repo)?,
        octo_rill_releases_url: build_octo_rill_releases_url(octo_rill_base_url, repo),
    })
}

fn github_note_item_from_release(item: ServiceGitHubReleaseItem) -> ServiceReleaseNoteItem {
    ServiceReleaseNoteItem {
        id: format!("github:{}", item.id),
        tag_name: item.tag_name,
        name: item.name,
        original_body: item.body,
        translated_body: None,
        smart_body: None,
        html_url: item.html_url,
        draft: item.draft,
        prerelease: item.prerelease,
        published_at: item.published_at,
        created_at: item.created_at,
    }
}

fn release_notes_status_from_github(
    status: ServiceGitHubReleasesStatus,
) -> ServiceReleaseNotesStatus {
    match status {
        ServiceGitHubReleasesStatus::Ready => ServiceReleaseNotesStatus::Ready,
        ServiceGitHubReleasesStatus::UnsupportedRepo => ServiceReleaseNotesStatus::UnsupportedRepo,
        ServiceGitHubReleasesStatus::PermissionDenied
        | ServiceGitHubReleasesStatus::RateLimited
        | ServiceGitHubReleasesStatus::UpstreamError => ServiceReleaseNotesStatus::UpstreamError,
    }
}

#[derive(Clone)]
struct GitHubReleaseNotesResponseOptions {
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    anchor: Option<ServiceReleaseNotesAnchor>,
}

fn github_release_notes_response_from_list(
    response: ServiceGitHubReleasesResponse,
    limit: u32,
    options: GitHubReleaseNotesResponseOptions,
) -> ServiceReleaseNotesResponse {
    let page = response.page.max(1);
    let next_cursor = response.has_more.then(|| github_cursor_for_page(page + 1));
    ServiceReleaseNotesResponse {
        status: release_notes_status_from_github(response.status),
        source: ServiceReleaseNotesSource::GitHub,
        repo: response.repo,
        cursor: github_current_cursor(page),
        limit,
        next_cursor,
        previous_cursor: github_previous_cursor(page),
        has_more: response.has_more,
        default_view: options.default_view,
        external_links: options.external_links,
        items: response
            .items
            .into_iter()
            .map(github_note_item_from_release)
            .collect(),
        message: response.message,
        stale: None,
        anchor: options.anchor,
    }
}

fn unsupported_github_release_notes_response(
    limit: u32,
    options: GitHubReleaseNotesResponseOptions,
) -> ServiceReleaseNotesResponse {
    ServiceReleaseNotesResponse {
        status: ServiceReleaseNotesStatus::UnsupportedRepo,
        source: ServiceReleaseNotesSource::GitHub,
        repo: None,
        cursor: None,
        limit,
        next_cursor: None,
        previous_cursor: None,
        has_more: false,
        default_view: options.default_view,
        external_links: options.external_links,
        items: Vec::new(),
        message: Some("未能解析该服务的 GitHub 仓库。".to_string()),
        stale: None,
        anchor: options.anchor,
    }
}

async fn github_release_notes_response_from_page(
    client: &github::GitHubClient,
    repo: Option<ServiceGitHubRepoRef>,
    page: u32,
    limit: u32,
    options: GitHubReleaseNotesResponseOptions,
) -> ServiceReleaseNotesResponse {
    let per_page = normalize_github_releases_per_page(Some(limit));
    let Some(repo_ref) = repo else {
        return unsupported_github_release_notes_response(per_page, options);
    };
    let response = list_service_github_releases_with_client(client, repo_ref, page, per_page).await;
    github_release_notes_response_from_list(response, per_page, options)
}

fn value_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn rich_text_from_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    if !value.is_object() {
        return None;
    }
    let mut parts = Vec::new();
    for key in [
        "titleZh",
        "title_zh",
        "summaryMd",
        "summary_md",
        "bodyMd",
        "body_md",
        "contentMarkdown",
        "content_markdown",
        "body",
        "summary",
        "content",
        "text",
    ] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && !parts.iter().any(|existing| existing == trimmed) {
                parts.push(trimmed.to_string());
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn release_tag_from_url(input: &str) -> Option<String> {
    let url = url::Url::parse(input).ok()?;
    let segments: Vec<&str> = url.path_segments()?.collect();
    for window in segments.windows(3) {
        if window[0] == "releases" && window[1] == "tag" {
            let tag = window[2].trim().to_string();
            if !tag.is_empty() {
                return Some(tag);
            }
        }
    }
    None
}

fn octo_rill_item_to_release_note(
    item: &Value,
    index: usize,
    cursor: Option<&str>,
) -> Option<ServiceReleaseNoteItem> {
    if !item.is_object() {
        return None;
    }
    let html_url = value_string(item, &["htmlUrl", "html_url", "url"]);
    let name = value_string(item, &["title", "name"]);
    let explicit_tag = value_string(item, &["tagName", "tag_name", "tag"]);
    let original_body = value_string(item, &["body", "summary", "excerpt", "description"]);
    let explicit_id = value_string(item, &["id", "releaseId", "release_id"]);
    if explicit_tag.is_none()
        && html_url.is_none()
        && name.is_none()
        && original_body.is_none()
        && explicit_id.is_none()
    {
        return None;
    }
    let tag_name = explicit_tag
        .or_else(|| html_url.as_deref().and_then(release_tag_from_url))
        .or_else(|| name.clone())
        .or_else(|| explicit_id.clone())
        .unwrap_or_else(|| format!("item-{index}"));
    let fallback_scope = cursor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("first");
    let id = explicit_id
        .or_else(|| html_url.clone())
        .unwrap_or_else(|| format!("{fallback_scope}:{tag_name}:{index}"));
    let translated_body = rich_text_from_value(item.get("translated"));
    let smart_body = rich_text_from_value(item.get("smart"));
    Some(ServiceReleaseNoteItem {
        id: format!("octorill:{id}"),
        tag_name,
        name,
        original_body,
        translated_body,
        smart_body,
        html_url: html_url.unwrap_or_default(),
        draft: item
            .get("isDraft")
            .or_else(|| item.get("is_draft"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        prerelease: item
            .get("isPrerelease")
            .or_else(|| item.get("is_prerelease"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        published_at: value_string(
            item,
            &[
                "publishedAt",
                "published_at",
                "ts",
                "createdAt",
                "created_at",
            ],
        ),
        created_at: value_string(item, &["createdAt", "created_at"]),
    })
}

fn release_note_matches_version(item: &ServiceReleaseNoteItem, version: &str) -> bool {
    let normalized_tag = item.tag_name.trim().to_ascii_lowercase();
    github_release_tag_variants(version)
        .into_iter()
        .map(|candidate| candidate.trim().to_ascii_lowercase())
        .any(|candidate| candidate == normalized_tag)
}

fn split_repo_full_name(full_name: &str) -> Option<(&str, &str)> {
    let (owner, repo) = full_name.trim().split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

async fn fetch_octo_rill_public_release_notes(
    settings: &OctoRillReleaseNotesSettings,
    repo: &ServiceGitHubRepoRef,
    cursor: Option<&str>,
    direction: ServiceReleaseNotesDirection,
    limit: u32,
    highlight_version: Option<&str>,
) -> Result<OctoRillPublicReleaseNotesSuccess, OctoRillPublicReleaseNotesFailure> {
    let base_url = settings
        .api_base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::NotConfigured,
            message: failure_message(ServiceReleaseNotesFailureReason::NotConfigured),
        })?;
    settings
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::NotConfigured,
            message: failure_message(ServiceReleaseNotesFailureReason::NotConfigured),
        })?;
    let (owner, repo_name) =
        split_repo_full_name(&repo.full_name).ok_or_else(|| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UnsupportedRepo,
            message: failure_message(ServiceReleaseNotesFailureReason::UnsupportedRepo),
        })?;

    let mut url = url::Url::parse(base_url).map_err(|_| OctoRillPublicReleaseNotesFailure {
        reason: ServiceReleaseNotesFailureReason::UpstreamError,
        message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
    })?;
    {
        let mut segments =
            url.path_segments_mut()
                .map_err(|_| OctoRillPublicReleaseNotesFailure {
                    reason: ServiceReleaseNotesFailureReason::UpstreamError,
                    message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
                })?;
        segments.pop_if_empty();
        segments.extend([
            "api", "public", "repos", owner, repo_name, "releases", "content",
        ]);
    }
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
            qp.append_pair("cursor", cursor);
            if direction == ServiceReleaseNotesDirection::Newer {
                qp.append_pair("direction", "newer");
            }
        }
        if let Some(version) = highlight_version
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let selectors = github_release_tag_variants(version);
            for selector in selectors {
                qp.append_pair("highlight", &format!("tag:{selector}"));
            }
            let active_selector = github_release_tag_variants(version)
                .into_iter()
                .next()
                .unwrap_or_else(|| version.to_string());
            qp.append_pair("highlight_active", &format!("tag:{active_selector}"));
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("dockrev octorill public releases"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .default_headers(headers)
        .build()
        .map_err(|_| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UpstreamError,
            message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
        })?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UpstreamError,
            message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
        })?;
    if !resp.status().is_success() {
        return Err(OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UpstreamError,
            message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
        });
    }
    let parsed = resp
        .json::<OctoRillPublicReleaseContentResponse>()
        .await
        .map_err(|_| OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UpstreamError,
            message: failure_message(ServiceReleaseNotesFailureReason::UpstreamError),
        })?;
    if parsed.status != "ready" {
        return Err(OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::UpstreamError,
            message: parsed
                .message
                .unwrap_or_else(|| "OctoRill 公开 Release 数据暂未就绪。".to_string()),
        });
    }
    let items = parsed
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| octo_rill_item_to_release_note(item, index, cursor))
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Err(OctoRillPublicReleaseNotesFailure {
            reason: ServiceReleaseNotesFailureReason::EmptyFeed,
            message: failure_message(ServiceReleaseNotesFailureReason::EmptyFeed),
        });
    }

    let fallback_match = highlight_version.and_then(|version| {
        items
            .iter()
            .position(|item| release_note_matches_version(item, version))
            .map(|index| (index as u32, items[index].tag_name.clone()))
    });
    let active_index = parsed
        .highlight
        .as_ref()
        .and_then(|highlight| highlight.active_index)
        .and_then(|index| index.checked_sub(1));
    let active_match = active_index.and_then(|index| {
        items
            .get(index as usize)
            .map(|item| (index, item.tag_name.clone()))
    });
    let resolved_tag = parsed
        .highlight
        .as_ref()
        .and_then(|highlight| highlight.resolved.first())
        .and_then(|resolved| resolved.tag_name.clone());
    let (index_within_window, matched_tag) = active_match
        .or(fallback_match)
        .map(|(index, tag)| (Some(index), Some(tag)))
        .unwrap_or((None, resolved_tag));

    Ok(OctoRillPublicReleaseNotesSuccess {
        items,
        next_cursor: parsed.next_cursor,
        previous_cursor: parsed.previous_cursor,
        matched_tag,
        index_within_window,
    })
}

fn octorill_ready_response(
    repo: Option<ServiceGitHubRepoRef>,
    cursor: Option<String>,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    response: OctoRillPublicReleaseNotesSuccess,
    anchor: Option<ServiceReleaseNotesAnchor>,
) -> ServiceReleaseNotesResponse {
    ServiceReleaseNotesResponse {
        status: ServiceReleaseNotesStatus::Ready,
        source: ServiceReleaseNotesSource::OctoRill,
        repo,
        cursor,
        limit,
        next_cursor: response
            .next_cursor
            .as_deref()
            .map(octo_rill_cursor_for_upstream),
        previous_cursor: response
            .previous_cursor
            .as_deref()
            .map(octo_rill_cursor_for_upstream),
        has_more: response.next_cursor.is_some(),
        default_view,
        external_links,
        items: response.items,
        message: None,
        stale: None,
        anchor,
    }
}

async fn github_locate_release_notes_response(
    client: &github::GitHubClient,
    repo: Option<ServiceGitHubRepoRef>,
    version: &str,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
) -> ServiceReleaseNotesResponse {
    let per_page = normalize_github_releases_per_page(Some(limit));
    let Some(repo_ref) = repo.clone() else {
        return unsupported_github_release_notes_response(
            per_page,
            GitHubReleaseNotesResponseOptions {
                default_view,
                external_links,
                anchor: Some(ServiceReleaseNotesAnchor {
                    status: ServiceReleaseNotesAnchorStatus::Unavailable,
                    version: version.to_string(),
                    matched_tag: None,
                    index_within_window: None,
                    absolute_index: None,
                    message: Some("未能解析该服务的 GitHub 仓库，无法定位当前版本。".to_string()),
                }),
            },
        );
    };

    let locate = locate_service_github_release_with_client(
        client,
        repo_ref.clone(),
        version,
        per_page,
        GITHUB_RELEASE_LOCATE_SCAN_LIMIT,
    )
    .await;

    let response_options = |anchor: ServiceReleaseNotesAnchor| GitHubReleaseNotesResponseOptions {
        default_view,
        external_links: external_links.clone(),
        anchor: Some(anchor),
    };

    match locate.status {
        GitHubReleaseLocateStatus::Found => {
            let page = locate.page.unwrap_or(1);
            let anchor = ServiceReleaseNotesAnchor {
                status: ServiceReleaseNotesAnchorStatus::Found,
                version: version.to_string(),
                matched_tag: locate.matched_tag,
                index_within_window: locate.index_within_page,
                absolute_index: locate.absolute_index,
                message: None,
            };
            github_release_notes_response_from_page(
                client,
                Some(repo_ref),
                page,
                per_page,
                response_options(anchor),
            )
            .await
        }
        GitHubReleaseLocateStatus::OutsideWindow | GitHubReleaseLocateStatus::NotFound => {
            let anchor = ServiceReleaseNotesAnchor {
                status: if locate.status == GitHubReleaseLocateStatus::OutsideWindow {
                    ServiceReleaseNotesAnchorStatus::OutsideWindow
                } else {
                    ServiceReleaseNotesAnchorStatus::NotFound
                },
                version: version.to_string(),
                matched_tag: locate.matched_tag,
                index_within_window: None,
                absolute_index: None,
                message: locate.message,
            };
            github_release_notes_response_from_page(
                client,
                Some(repo_ref),
                1,
                per_page,
                response_options(anchor),
            )
            .await
        }
        GitHubReleaseLocateStatus::UnsupportedRepo
        | GitHubReleaseLocateStatus::PermissionDenied
        | GitHubReleaseLocateStatus::RateLimited
        | GitHubReleaseLocateStatus::UpstreamError => ServiceReleaseNotesResponse {
            status: if locate.status == GitHubReleaseLocateStatus::UnsupportedRepo {
                ServiceReleaseNotesStatus::UnsupportedRepo
            } else {
                ServiceReleaseNotesStatus::UpstreamError
            },
            source: ServiceReleaseNotesSource::GitHub,
            repo,
            cursor: None,
            limit: per_page,
            next_cursor: None,
            previous_cursor: None,
            has_more: false,
            default_view,
            external_links,
            items: Vec::new(),
            message: locate.message.clone(),
            stale: None,
            anchor: Some(ServiceReleaseNotesAnchor {
                status: ServiceReleaseNotesAnchorStatus::Unavailable,
                version: version.to_string(),
                matched_tag: locate.matched_tag,
                index_within_window: None,
                absolute_index: None,
                message: locate.message,
            }),
        },
    }
}

pub(crate) async fn list_service_release_notes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceReleaseNotesQuery>,
) -> Result<Json<ServiceReleaseNotesResponse>, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .map_err(map_internal)?;
    let Some(settings) = settings else {
        return Err(ApiError::not_found("service not found"));
    };

    let release_notes_settings = state
        .db
        .get_release_notes_settings()
        .await
        .map_err(map_internal)?;
    let default_view = match release_notes_settings.provider {
        ReleaseNotesProvider::GitHub => ReleaseNotesView::Original,
        ReleaseNotesProvider::OctoRill => release_notes_settings.octo_rill.default_view,
    };
    let direction = normalize_release_notes_direction(query.direction);
    let limit = normalize_release_notes_limit(query.limit);
    let repo = resolve_service_github_repo_ref(&state, &service_id, settings.repo_url.as_deref())
        .await
        .map_err(map_internal)?;
    let external_links = build_external_links(
        repo.as_ref(),
        release_notes_settings.octo_rill.api_base_url.as_deref(),
    );

    if release_notes_settings.provider == ReleaseNotesProvider::OctoRill {
        let Some(repo_ref) = repo.as_ref() else {
            return Ok(Json(octo_rill_page_failure_response(
                repo,
                query.cursor,
                limit,
                default_view,
                external_links,
                ServiceReleaseNotesFailureReason::UnsupportedRepo,
            )));
        };
        let cursor = query
            .cursor
            .clone()
            .filter(|value| value.starts_with("octo:"));
        let upstream_cursor = cursor
            .as_deref()
            .and_then(upstream_cursor_from_octo_rill_cursor);
        return Ok(Json(
            match fetch_octo_rill_public_release_notes(
                &release_notes_settings.octo_rill,
                repo_ref,
                upstream_cursor.as_deref(),
                direction,
                limit,
                None,
            )
            .await
            {
                Ok(response) => octorill_ready_response(
                    repo,
                    cursor,
                    limit,
                    default_view,
                    external_links,
                    response,
                    None,
                ),
                Err(failure) => octo_rill_page_failure_response(
                    repo,
                    cursor,
                    limit,
                    default_view,
                    external_links,
                    failure.reason,
                ),
            },
        ));
    }

    let github_settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let client = build_service_github_releases_client(&github_settings)?;
    let page = if is_github_cursor(query.cursor.as_deref()) {
        parse_cursor_as_page(query.cursor.as_deref())
    } else {
        1
    };
    Ok(Json(
        github_release_notes_response_from_page(
            &client,
            repo,
            page,
            limit,
            GitHubReleaseNotesResponseOptions {
                default_view,
                external_links,
                anchor: None,
            },
        )
        .await,
    ))
}

pub(crate) async fn locate_service_release_notes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(service_id): Path<String>,
    Query(query): Query<ServiceReleaseNotesLocateQuery>,
) -> Result<Json<ServiceReleaseNotesResponse>, ApiError> {
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

    let release_notes_settings = state
        .db
        .get_release_notes_settings()
        .await
        .map_err(map_internal)?;
    let default_view = match release_notes_settings.provider {
        ReleaseNotesProvider::GitHub => ReleaseNotesView::Original,
        ReleaseNotesProvider::OctoRill => release_notes_settings.octo_rill.default_view,
    };
    let limit = normalize_release_notes_locate_limit(query.limit);
    let repo = resolve_service_github_repo_ref(&state, &service_id, settings.repo_url.as_deref())
        .await
        .map_err(map_internal)?;
    let external_links = build_external_links(
        repo.as_ref(),
        release_notes_settings.octo_rill.api_base_url.as_deref(),
    );

    if release_notes_settings.provider == ReleaseNotesProvider::OctoRill {
        let Some(repo_ref) = repo.as_ref() else {
            return Ok(Json(octo_rill_locate_failure_response(
                repo,
                limit,
                default_view,
                external_links,
                trimmed_version,
                ServiceReleaseNotesFailureReason::UnsupportedRepo,
                "未能解析该服务的 GitHub 仓库，无法定位当前版本。".to_string(),
            )));
        };
        return Ok(Json(
            match fetch_octo_rill_public_release_notes(
                &release_notes_settings.octo_rill,
                repo_ref,
                None,
                ServiceReleaseNotesDirection::Older,
                limit,
                Some(trimmed_version),
            )
            .await
            {
                Ok(response) if response.index_within_window.is_some() => {
                    let matched_tag = response.matched_tag.clone();
                    let index_within_window = response.index_within_window;
                    octorill_ready_response(
                        repo,
                        None,
                        limit,
                        default_view,
                        external_links,
                        response,
                        Some(ServiceReleaseNotesAnchor {
                            status: ServiceReleaseNotesAnchorStatus::Found,
                            version: trimmed_version.to_string(),
                            matched_tag,
                            index_within_window,
                            absolute_index: None,
                            message: None,
                        }),
                    )
                }
                Ok(_) => octo_rill_locate_failure_response(
                    repo,
                    limit,
                    default_view,
                    external_links,
                    trimmed_version,
                    ServiceReleaseNotesFailureReason::UpstreamError,
                    format!("OctoRill 未能直接定位 {trimmed_version}。"),
                ),
                Err(failure) => octo_rill_locate_failure_response(
                    repo,
                    limit,
                    default_view,
                    external_links,
                    trimmed_version,
                    failure.reason,
                    failure.message,
                ),
            },
        ));
    }

    let github_settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let client = build_service_github_releases_client(&github_settings)?;

    Ok(Json(
        github_locate_release_notes_response(
            &client,
            repo,
            trimmed_version,
            limit,
            default_view,
            external_links,
        )
        .await,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_octo_rill_release_note_variants() {
        let item = serde_json::json!({
            "id": "123",
            "title": "v1.2.3",
            "body": "Original body",
            "html_url": "https://github.com/acme/app/releases/tag/v1.2.3",
            "ts": "2026-07-01T00:00:00Z",
            "translated": { "summary_md": "翻译摘要", "body_md": "翻译正文" },
            "smart": { "summaryMd": "润色摘要" }
        });
        let parsed = octo_rill_item_to_release_note(&item, 0, None).unwrap();
        assert_eq!(parsed.tag_name, "v1.2.3");
        assert_eq!(parsed.original_body.as_deref(), Some("Original body"));
        assert_eq!(
            parsed.translated_body.as_deref(),
            Some("翻译摘要\n\n翻译正文")
        );
        assert_eq!(parsed.smart_body.as_deref(), Some("润色摘要"));
    }

    #[test]
    fn parses_octo_rill_release_note_without_html_url() {
        let item = serde_json::json!({
            "tag": "v2.0.0",
            "title": "Release 2.0.0",
            "body": "Original body",
            "smart": "润色正文"
        });
        let parsed = octo_rill_item_to_release_note(&item, 0, Some("opaque-page-2")).unwrap();
        assert_eq!(parsed.id, "octorill:opaque-page-2:v2.0.0:0");
        assert_eq!(parsed.tag_name, "v2.0.0");
        assert_eq!(parsed.name.as_deref(), Some("Release 2.0.0"));
        assert_eq!(parsed.html_url, "");
        assert_eq!(parsed.smart_body.as_deref(), Some("润色正文"));
    }

    #[test]
    fn ignores_octo_rill_release_note_without_displayable_fields() {
        assert!(octo_rill_item_to_release_note(&serde_json::json!(null), 0, None).is_none());
        assert!(octo_rill_item_to_release_note(&serde_json::json!({}), 0, None).is_none());
    }

    #[test]
    fn octo_rill_cursor_page_failure_does_not_fallback_to_github() {
        let response = octo_rill_page_failure_response(
            Some(ServiceGitHubRepoRef {
                full_name: "acme/app".to_string(),
                html_url: "https://github.com/acme/app".to_string(),
            }),
            Some("opaque-next-cursor".to_string()),
            20,
            ReleaseNotesView::Smart,
            None,
            ServiceReleaseNotesFailureReason::UpstreamError,
        );

        assert_eq!(response.status, ServiceReleaseNotesStatus::UpstreamError);
        assert_eq!(response.source, ServiceReleaseNotesSource::OctoRill);
        assert_eq!(response.cursor.as_deref(), Some("opaque-next-cursor"));
        assert!(response.previous_cursor.is_none());
        assert!(!response.has_more);
        assert!(response.items.is_empty());
        assert!(response.stale.is_none());
        assert!(!response.message.unwrap().contains("回退"));
    }

    #[test]
    fn github_fallback_cursor_carries_source_prefix() {
        assert_eq!(github_cursor_for_page(3), "github:3");
        assert_eq!(parse_cursor_as_page(Some("github:3")), 3);
        assert_eq!(parse_cursor_as_page(Some("3")), 3);
        assert!(is_github_cursor(Some("github:3")));
        assert!(is_github_cursor(Some("3")));
        assert!(!is_github_cursor(Some("opaque-octorill-cursor")));
    }

    #[test]
    fn octo_rill_cursor_is_wrapped_before_returning_to_clients() {
        let upstream = "opaque-upstream-cursor";
        let client_cursor = octo_rill_cursor_for_upstream(upstream);

        assert!(client_cursor.starts_with("octo:"));
        assert!(!is_github_cursor(Some(&client_cursor)));
        assert_eq!(
            upstream_cursor_from_octo_rill_cursor(&client_cursor).as_deref(),
            Some(upstream)
        );
    }

    #[test]
    fn extracts_release_tag_from_url() {
        assert_eq!(
            release_tag_from_url("https://github.com/acme/app/releases/tag/v1.2.3").as_deref(),
            Some("v1.2.3")
        );
    }

    #[test]
    fn builds_release_notes_external_links() {
        let repo = ServiceGitHubRepoRef {
            full_name: "acme/app".to_string(),
            html_url: "https://github.com/acme/app".to_string(),
        };
        let external_links =
            build_external_links(Some(&repo), Some("https://octo.example.com/octo-rill"))
                .expect("external links");

        assert_eq!(
            external_links.github_releases_url,
            "https://github.com/acme/app/releases"
        );
        assert_eq!(
            external_links.octo_rill_releases_url.as_deref(),
            Some("https://octo.example.com/octo-rill/acme/app/releases")
        );
    }

    #[test]
    fn hides_octo_rill_release_link_when_repo_full_name_is_not_owner_repo() {
        let repo = ServiceGitHubRepoRef {
            full_name: "invalid".to_string(),
            html_url: "https://github.com/acme/app".to_string(),
        };
        let external_links =
            build_external_links(Some(&repo), Some("https://octo.example.com/octo-rill"))
                .expect("external links");

        assert_eq!(
            external_links.github_releases_url,
            "https://github.com/acme/app/releases"
        );
        assert!(external_links.octo_rill_releases_url.is_none());
    }

    #[test]
    fn release_note_match_supports_plain_and_v_prefixed_versions() {
        let item = ServiceReleaseNoteItem {
            id: "1".to_string(),
            tag_name: "v1.2.3".to_string(),
            name: None,
            original_body: None,
            translated_body: None,
            smart_body: None,
            html_url: String::new(),
            draft: false,
            prerelease: false,
            published_at: None,
            created_at: None,
        };

        assert!(release_note_matches_version(&item, "1.2.3"));
        assert!(release_note_matches_version(&item, "v1.2.3"));
        assert!(!release_note_matches_version(&item, "1.2.4"));
    }
}
