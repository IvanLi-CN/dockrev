use super::*;
use axum::{Json, Router, extract::Query, http::HeaderMap, response::IntoResponse, routing::get};
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
