use super::github_releases::{
    build_service_github_releases_client, list_service_github_releases_with_client,
    normalize_github_releases_per_page,
};
use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceReleaseNotesQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

const DEFAULT_RELEASE_NOTES_LIMIT: u32 = 20;
const MAX_RELEASE_NOTES_LIMIT: u32 = 100;

fn normalize_release_notes_limit(value: Option<u32>) -> u32 {
    value
        .unwrap_or(DEFAULT_RELEASE_NOTES_LIMIT)
        .clamp(1, MAX_RELEASE_NOTES_LIMIT)
}

fn fallback_message(reason: ServiceReleaseNotesFallbackReason) -> String {
    match reason {
        ServiceReleaseNotesFallbackReason::Disabled => {
            "OctoRill 更新日志未启用，已使用 GitHub Releases。".to_string()
        }
        ServiceReleaseNotesFallbackReason::NotConfigured => {
            "OctoRill API Base URL 或 API Key 未配置完整，已使用 GitHub Releases。".to_string()
        }
        ServiceReleaseNotesFallbackReason::UnsupportedRepo => {
            "未能解析该服务的 GitHub 仓库，无法读取 OctoRill 仓库 feed。".to_string()
        }
        ServiceReleaseNotesFallbackReason::Unauthorized => {
            "OctoRill API Key 无效或权限不足，已回退到 GitHub Releases。".to_string()
        }
        ServiceReleaseNotesFallbackReason::EmptyFeed => {
            "OctoRill 没有返回可展示的发布记录，已回退到 GitHub Releases。".to_string()
        }
        ServiceReleaseNotesFallbackReason::UpstreamError => {
            "OctoRill 请求失败，已回退到 GitHub Releases。".to_string()
        }
    }
}

fn build_fallback(reason: ServiceReleaseNotesFallbackReason) -> ServiceReleaseNotesFallback {
    ServiceReleaseNotesFallback {
        from: ServiceReleaseNotesSource::OctoRill,
        reason,
        message: fallback_message(reason),
    }
}

fn page_failure_message(reason: ServiceReleaseNotesFallbackReason) -> String {
    match reason {
        ServiceReleaseNotesFallbackReason::UnsupportedRepo => {
            "未能解析该服务的 GitHub 仓库，无法继续读取 OctoRill 仓库 feed。".to_string()
        }
        ServiceReleaseNotesFallbackReason::Unauthorized => {
            "OctoRill API Key 无效或权限不足，无法继续读取后续发布记录。".to_string()
        }
        ServiceReleaseNotesFallbackReason::EmptyFeed => {
            "OctoRill 没有返回可继续展示的发布记录。".to_string()
        }
        ServiceReleaseNotesFallbackReason::NotConfigured => {
            "OctoRill API Base URL 或 API Key 未配置完整，无法继续读取后续发布记录。".to_string()
        }
        ServiceReleaseNotesFallbackReason::Disabled => {
            "OctoRill 更新日志未启用，无法继续读取 OctoRill 发布记录。".to_string()
        }
        ServiceReleaseNotesFallbackReason::UpstreamError => {
            "OctoRill 请求失败，无法继续读取后续发布记录。".to_string()
        }
    }
}

fn octo_rill_page_failure_response(
    repo: Option<ServiceGitHubRepoRef>,
    cursor: Option<String>,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    reason: ServiceReleaseNotesFallbackReason,
) -> ServiceReleaseNotesResponse {
    ServiceReleaseNotesResponse {
        status: match reason {
            ServiceReleaseNotesFallbackReason::UnsupportedRepo => {
                ServiceReleaseNotesStatus::UnsupportedRepo
            }
            _ => ServiceReleaseNotesStatus::UpstreamError,
        },
        source: ServiceReleaseNotesSource::OctoRill,
        repo,
        cursor,
        limit,
        next_cursor: None,
        has_more: false,
        default_view,
        external_links,
        items: Vec::new(),
        message: Some(page_failure_message(reason)),
        fallback: None,
    }
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
        .is_some_and(|value| value.starts_with("github:"))
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

struct GithubReleaseNotesRequest {
    repo: Option<ServiceGitHubRepoRef>,
    cursor: Option<String>,
    limit: u32,
    default_view: ReleaseNotesView,
    external_links: Option<ServiceReleaseNotesExternalLinks>,
    fallback: Option<ServiceReleaseNotesFallback>,
}

async fn github_release_notes_response(
    state: &Arc<AppState>,
    request: GithubReleaseNotesRequest,
) -> Result<ServiceReleaseNotesResponse, ApiError> {
    let GithubReleaseNotesRequest {
        repo,
        cursor,
        limit,
        default_view,
        external_links,
        fallback,
    } = request;
    let github_settings = state
        .db
        .get_github_packages_settings()
        .await
        .map_err(map_internal)?;
    let client = build_service_github_releases_client(&github_settings)?;
    let page = parse_cursor_as_page(cursor.as_deref());
    let per_page = normalize_github_releases_per_page(Some(limit));
    let Some(repo) = repo else {
        return Ok(ServiceReleaseNotesResponse {
            status: ServiceReleaseNotesStatus::UnsupportedRepo,
            source: ServiceReleaseNotesSource::GitHub,
            repo: None,
            cursor,
            limit: per_page,
            next_cursor: None,
            has_more: false,
            default_view,
            external_links,
            items: Vec::new(),
            message: Some("未能解析该服务的 GitHub 仓库。".to_string()),
            fallback,
        });
    };

    let response =
        list_service_github_releases_with_client(&client, repo.clone(), page, per_page).await;
    let next_cursor = if response.has_more {
        Some(github_cursor_for_page(response.page + 1))
    } else {
        None
    };
    let status = if response.status == ServiceGitHubReleasesStatus::Ready {
        ServiceReleaseNotesStatus::Ready
    } else if response.status == ServiceGitHubReleasesStatus::UnsupportedRepo {
        ServiceReleaseNotesStatus::UnsupportedRepo
    } else {
        ServiceReleaseNotesStatus::UpstreamError
    };
    Ok(ServiceReleaseNotesResponse {
        status,
        source: ServiceReleaseNotesSource::GitHub,
        repo: response.repo.or(Some(repo)),
        cursor,
        limit: per_page,
        next_cursor,
        has_more: response.has_more,
        default_view,
        external_links,
        items: response
            .items
            .into_iter()
            .map(github_note_item_from_release)
            .collect(),
        message: response.message,
        fallback,
    })
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

fn next_cursor_from_feed(value: &Value) -> Option<String> {
    value_string(value, &["nextCursor", "next_cursor"])
}

fn octo_rill_items_from_feed(value: &Value, cursor: Option<&str>) -> Vec<ServiceReleaseNoteItem> {
    value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| octo_rill_item_to_release_note(item, index, cursor))
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_octo_rill_release_notes(
    settings: &OctoRillReleaseNotesSettings,
    repo: &ServiceGitHubRepoRef,
    cursor: Option<&str>,
    limit: u32,
) -> Result<(Vec<ServiceReleaseNoteItem>, Option<String>), ServiceReleaseNotesFallbackReason> {
    let base_url = settings
        .api_base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ServiceReleaseNotesFallbackReason::NotConfigured)?;
    let api_key = settings
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ServiceReleaseNotesFallbackReason::NotConfigured)?;
    let mut url =
        url::Url::parse(base_url).map_err(|_| ServiceReleaseNotesFallbackReason::UpstreamError)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ServiceReleaseNotesFallbackReason::UpstreamError)?;
        segments.pop_if_empty();
        segments.extend(["api", "feed"]);
    }
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("scope", "repo");
        qp.append_pair("items", &repo.full_name);
        qp.append_pair("types", "releases");
        qp.append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
            qp.append_pair("cursor", cursor);
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("dockrev octorill release notes"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|_| ServiceReleaseNotesFallbackReason::NotConfigured)?,
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .default_headers(headers)
        .build()
        .map_err(|_| ServiceReleaseNotesFallbackReason::UpstreamError)?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|_| ServiceReleaseNotesFallbackReason::UpstreamError)?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(ServiceReleaseNotesFallbackReason::Unauthorized);
    }
    if !status.is_success() {
        return Err(ServiceReleaseNotesFallbackReason::UpstreamError);
    }
    let value = resp
        .json::<Value>()
        .await
        .map_err(|_| ServiceReleaseNotesFallbackReason::UpstreamError)?;
    let items = octo_rill_items_from_feed(&value, cursor);
    if items.is_empty() {
        return Err(ServiceReleaseNotesFallbackReason::EmptyFeed);
    }
    Ok((items, next_cursor_from_feed(&value)))
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
    let default_view = release_notes_settings.octo_rill.default_view;
    let limit = normalize_release_notes_limit(query.limit);
    let requested_cursor = query
        .cursor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let octo_rill_upstream_cursor = requested_cursor.map(|cursor| {
        upstream_cursor_from_octo_rill_cursor(cursor).unwrap_or_else(|| cursor.to_string())
    });
    let repo = resolve_service_github_repo_ref(&state, &service_id, settings.repo_url.as_deref())
        .await
        .map_err(map_internal)?;
    let external_links = build_external_links(
        repo.as_ref(),
        release_notes_settings.octo_rill.api_base_url.as_deref(),
    );

    let mut fallback = None;
    if is_github_cursor(query.cursor.as_deref()) {
        // Continue the already-selected GitHub fallback source instead of retrying OctoRill mid-list.
    } else if !release_notes_settings.octo_rill.enabled {
        // Intentionally disabled uses GitHub Releases as the primary source, without warning.
    } else if repo.is_none() {
        fallback = Some(build_fallback(
            ServiceReleaseNotesFallbackReason::UnsupportedRepo,
        ));
    } else if release_notes_settings
        .octo_rill
        .api_base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .is_none()
        || release_notes_settings
            .octo_rill
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .is_none()
    {
        fallback = Some(build_fallback(
            ServiceReleaseNotesFallbackReason::NotConfigured,
        ));
    } else if let Some(repo_ref) = repo.as_ref() {
        match fetch_octo_rill_release_notes(
            &release_notes_settings.octo_rill,
            repo_ref,
            octo_rill_upstream_cursor.as_deref(),
            limit,
        )
        .await
        {
            Ok((items, next_cursor)) => {
                return Ok(Json(ServiceReleaseNotesResponse {
                    status: ServiceReleaseNotesStatus::Ready,
                    source: ServiceReleaseNotesSource::OctoRill,
                    repo,
                    cursor: query.cursor,
                    limit,
                    has_more: next_cursor.is_some(),
                    next_cursor: next_cursor.map(|cursor| octo_rill_cursor_for_upstream(&cursor)),
                    default_view,
                    external_links: external_links.clone(),
                    items,
                    message: None,
                    fallback: None,
                }));
            }
            Err(reason) => {
                if query
                    .cursor
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Ok(Json(octo_rill_page_failure_response(
                        repo,
                        query.cursor,
                        limit,
                        default_view,
                        external_links.clone(),
                        reason,
                    )));
                }
                fallback = Some(build_fallback(reason));
            }
        }
    }

    Ok(Json(
        github_release_notes_response(
            &state,
            GithubReleaseNotesRequest {
                repo,
                cursor: query.cursor,
                limit,
                default_view,
                external_links,
                fallback,
            },
        )
        .await?,
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
            ServiceReleaseNotesFallbackReason::UpstreamError,
        );

        assert_eq!(response.status, ServiceReleaseNotesStatus::UpstreamError);
        assert_eq!(response.source, ServiceReleaseNotesSource::OctoRill);
        assert_eq!(response.cursor.as_deref(), Some("opaque-next-cursor"));
        assert!(!response.has_more);
        assert!(response.items.is_empty());
        assert!(response.fallback.is_none());
        assert!(!response.message.unwrap().contains("回退"));
    }

    #[test]
    fn github_fallback_cursor_carries_source_prefix() {
        assert_eq!(github_cursor_for_page(3), "github:3");
        assert_eq!(parse_cursor_as_page(Some("github:3")), 3);
        assert!(is_github_cursor(Some("github:3")));
        assert!(!is_github_cursor(Some("opaque-octorill-cursor")));
    }

    #[test]
    fn octo_rill_cursor_is_wrapped_before_returning_to_clients() {
        let upstream = "github:opaque-upstream-cursor";
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
}
