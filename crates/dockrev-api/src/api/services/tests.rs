use super::*;
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use url::Url;
use url::form_urlencoded;

#[derive(Debug, Default, Deserialize)]
struct OctoRillReleasesQuery {
    limit: Option<u32>,
    cursor: Option<String>,
    direction: Option<String>,
    #[serde(default)]
    highlight: Vec<String>,
    highlight_active: Option<String>,
}

async fn spawn_public_releases_server(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}/")
}

fn test_octo_rill_settings(api_base_url: String) -> OctoRillReleaseNotesSettings {
    OctoRillReleaseNotesSettings {
        enabled: true,
        api_base_url: Some(api_base_url),
        api_key: Some("orill_ak_test".to_string()),
        default_view: ReleaseNotesView::Smart,
    }
}

fn test_repo_ref() -> ServiceGitHubRepoRef {
    ServiceGitHubRepoRef {
        full_name: "acme/app".to_string(),
        html_url: "https://github.com/acme/app".to_string(),
    }
}

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
fn normalize_homepage_href_accepts_only_http_https_or_root_relative_paths() {
    assert_eq!(
        normalize_homepage_href(" https://api.example.com/path "),
        Some("https://api.example.com/path".to_string())
    );
    assert_eq!(
        normalize_homepage_href("/dashboard"),
        Some("/dashboard".to_string())
    );
    for value in [
        "javascript:alert(1)",
        "//other.example",
        "/\\other.example",
        "/\t\\evil.example",
        "relative",
    ] {
        assert_eq!(normalize_homepage_href(value), None, "{value}");
    }
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
async fn list_service_github_releases_falls_back_to_anonymous_when_pat_cannot_access_public_repo() {
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

    async fn releases(headers: HeaderMap, Query(query): Query<ReleasesQuery>) -> impl IntoResponse {
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

    assert_eq!(response.status, GitHubReleaseLocateStatus::Found);
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

    assert_eq!(response.status, GitHubReleaseLocateStatus::Found);
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

    assert_eq!(response.status, GitHubReleaseLocateStatus::OutsideWindow);
    assert_eq!(response.matched_tag.as_deref(), Some("1.39.5"));
    assert_eq!(response.searched_count, 20);
}

#[tokio::test]
async fn fetch_octo_rill_public_release_notes_uses_releases_endpoint_and_maps_items() {
    async fn releases(
        headers: HeaderMap,
        Query(query): Query<OctoRillReleasesQuery>,
    ) -> impl IntoResponse {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer orill_ak_test")
        );
        assert_eq!(query.limit, Some(20));
        assert!(query.cursor.is_none());
        assert!(query.direction.is_none());
        assert!(query.highlight.is_empty());
        assert!(query.highlight_active.is_none());

        Json(json!({
            "status": "ready",
            "next_cursor": "cursor-older",
            "items": [
                {
                    "release_id": "123",
                    "tag_name": "v1.2.3",
                    "name": "v1.2.3",
                    "body": "Original body",
                    "translated": { "summary_md": "翻译摘要", "body_md": "翻译正文" },
                    "smart": { "summaryMd": "润色摘要" },
                    "html_url": "https://github.com/acme/app/releases/tag/v1.2.3",
                    "published_at": "2026-07-19T00:00:00Z"
                }
            ]
        }))
    }

    let api_base_url = spawn_public_releases_server(
        Router::new().route("/api/public/repos/acme/app/releases", get(releases)),
    )
    .await;

    let response = release_notes::fetch_octo_rill_public_release_notes(
        &test_octo_rill_settings(api_base_url),
        &test_repo_ref(),
        None,
        release_notes::ServiceReleaseNotesDirection::Older,
        20,
        None,
        false,
    )
    .await
    .expect("public releases response should map cleanly");

    assert_eq!(response.next_cursor.as_deref(), Some("cursor-older"));
    assert!(response.previous_cursor.is_none());
    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].tag_name, "v1.2.3");
    assert_eq!(
        response.items[0].original_body.as_deref(),
        Some("Original body")
    );
    assert_eq!(
        response.items[0].translated_body.as_deref(),
        Some("翻译摘要\n\n翻译正文")
    );
    assert_eq!(response.items[0].smart_body.as_deref(), Some("润色摘要"));
}

#[tokio::test]
async fn fetch_octo_rill_public_release_notes_passes_cursor_and_newer_direction() {
    async fn releases(Query(query): Query<OctoRillReleasesQuery>) -> impl IntoResponse {
        assert_eq!(query.limit, Some(5));
        assert_eq!(query.cursor.as_deref(), Some("opaque-cursor"));
        assert_eq!(query.direction.as_deref(), Some("newer"));
        assert!(query.highlight.is_empty());
        assert!(query.highlight_active.is_none());

        Json(json!({
            "status": "ready",
            "previous_cursor": "cursor-newer",
            "items": [
                {
                    "release_id": "124",
                    "tag_name": "v1.2.4",
                    "name": "v1.2.4",
                    "body": "Body",
                    "html_url": "https://github.com/acme/app/releases/tag/v1.2.4"
                }
            ]
        }))
    }

    let api_base_url = spawn_public_releases_server(
        Router::new().route("/api/public/repos/acme/app/releases", get(releases)),
    )
    .await;

    let response = release_notes::fetch_octo_rill_public_release_notes(
        &test_octo_rill_settings(api_base_url),
        &test_repo_ref(),
        Some("opaque-cursor"),
        release_notes::ServiceReleaseNotesDirection::Newer,
        5,
        None,
        false,
    )
    .await
    .expect("cursor paging should work");

    assert!(response.next_cursor.is_none());
    assert_eq!(response.previous_cursor.as_deref(), Some("cursor-newer"));
    assert_eq!(response.items[0].tag_name, "v1.2.4");
}

#[tokio::test]
async fn fetch_octo_rill_public_release_notes_uses_highlight_window_for_locate() {
    async fn releases(uri: Uri) -> impl IntoResponse {
        let params = form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        let highlights = params
            .iter()
            .filter_map(|(key, value)| (key == "highlight").then_some(value.clone()))
            .collect::<Vec<_>>();
        let highlight_active = params
            .iter()
            .find_map(|(key, value)| (key == "highlight_active").then_some(value.clone()));

        assert!(
            params
                .iter()
                .any(|(key, value)| key == "limit" && value == "5")
        );
        assert_eq!(
            highlights,
            vec!["tag:v1.2.3".to_string(), "tag:1.2.3".to_string()]
        );
        assert_eq!(highlight_active.as_deref(), Some("tag:v1.2.3"));

        Json(json!({
            "status": "ready",
            "items": [
                {
                    "release_id": "123",
                    "tag_name": "v1.2.3",
                    "name": "v1.2.3",
                    "body": "Body",
                    "html_url": "https://github.com/acme/app/releases/tag/v1.2.3"
                }
            ],
            "highlight": {
                "resolved": [{ "tag_name": "v1.2.3" }],
                "active_index": 1
            }
        }))
    }

    let api_base_url = spawn_public_releases_server(
        Router::new().route("/api/public/repos/acme/app/releases", get(releases)),
    )
    .await;

    let response = release_notes::fetch_octo_rill_public_release_notes(
        &test_octo_rill_settings(api_base_url),
        &test_repo_ref(),
        None,
        release_notes::ServiceReleaseNotesDirection::Older,
        5,
        Some("v1.2.3"),
        false,
    )
    .await
    .expect("highlight locate window should map");

    assert_eq!(response.matched_tag.as_deref(), Some("v1.2.3"));
    assert_eq!(response.index_within_window, Some(0));
}

#[tokio::test]
async fn fetch_octo_rill_public_release_notes_maps_unauthorized_status() {
    async fn releases() -> impl IntoResponse {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "message": "unauthorized" })),
        )
    }

    let api_base_url = spawn_public_releases_server(
        Router::new().route("/api/public/repos/acme/app/releases", get(releases)),
    )
    .await;

    let failure = release_notes::fetch_octo_rill_public_release_notes(
        &test_octo_rill_settings(api_base_url),
        &test_repo_ref(),
        None,
        release_notes::ServiceReleaseNotesDirection::Older,
        5,
        None,
        false,
    )
    .await
    .expect_err("401 should map to unauthorized");

    assert_eq!(
        failure.reason,
        ServiceReleaseNotesFailureReason::Unauthorized
    );
    assert_eq!(failure.message, "OctoRill API Key 无效或权限不足。");
}
