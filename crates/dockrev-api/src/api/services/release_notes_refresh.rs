use crate::api::{ServiceReleaseNotesRefresh, ServiceReleaseNotesRefreshState};
use serde::Deserialize;
use serde_json::Value;

use super::OctoRillPublicHighlight;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceReleaseNotesRefreshRequest {
    IfStale,
}

#[derive(Debug, Deserialize)]
pub(super) struct OctoRillPublicReleaseContentResponse {
    pub(super) status: String,
    #[serde(default)]
    pub(super) next_cursor: Option<String>,
    #[serde(default)]
    pub(super) previous_cursor: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
    #[serde(default)]
    pub(super) items: Vec<Value>,
    #[serde(default)]
    pub(super) highlight: Option<OctoRillPublicHighlight>,
    #[serde(default)]
    pub(super) refresh: Option<OctoRillPublicReleaseRefresh>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) struct OctoRillPublicReleaseRefresh {
    state: OctoRillPublicReleaseRefreshState,
    #[serde(default)]
    last_success_at: Option<String>,
    #[serde(default)]
    retry_after_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OctoRillPublicReleaseRefreshState {
    Fresh,
    Queued,
    Running,
    Backoff,
}

pub(super) fn release_notes_refresh_from_octo_rill(
    refresh: OctoRillPublicReleaseRefresh,
) -> ServiceReleaseNotesRefresh {
    let state = match refresh.state {
        OctoRillPublicReleaseRefreshState::Fresh => ServiceReleaseNotesRefreshState::Fresh,
        OctoRillPublicReleaseRefreshState::Queued => ServiceReleaseNotesRefreshState::Queued,
        OctoRillPublicReleaseRefreshState::Running => ServiceReleaseNotesRefreshState::Running,
        OctoRillPublicReleaseRefreshState::Backoff => ServiceReleaseNotesRefreshState::Backoff,
    };
    ServiceReleaseNotesRefresh {
        state,
        last_success_at: refresh.last_success_at,
        retry_after_seconds: refresh.retry_after_seconds,
    }
}

pub(super) fn should_refresh_octo_rill_first_window(
    refresh: Option<ServiceReleaseNotesRefreshRequest>,
    requested_cursor: Option<&str>,
    upstream_cursor: Option<&str>,
) -> bool {
    refresh == Some(ServiceReleaseNotesRefreshRequest::IfStale)
        && requested_cursor.is_none_or(|value| value.trim().is_empty())
        && upstream_cursor.is_none()
}

pub(super) fn split_repo_full_name(full_name: &str) -> Option<(&str, &str)> {
    let (owner, repo) = full_name.trim().split_once('/')?;
    (!owner.is_empty() && !repo.is_empty()).then_some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use axum::{Json, Router, extract::Query, response::IntoResponse, routing::get};
    use serde_json::json;

    #[test]
    fn refresh_requires_a_cursorless_first_window() {
        assert!(should_refresh_octo_rill_first_window(
            Some(ServiceReleaseNotesRefreshRequest::IfStale),
            None,
            None,
        ));
        assert!(!should_refresh_octo_rill_first_window(
            Some(ServiceReleaseNotesRefreshRequest::IfStale),
            Some("octo:opaque-cursor"),
            Some("opaque-cursor"),
        ));
        assert!(!should_refresh_octo_rill_first_window(
            Some(ServiceReleaseNotesRefreshRequest::IfStale),
            Some("invalid-cursor"),
            None,
        ));
    }

    #[derive(Debug, Default, serde::Deserialize)]
    struct OctoRillReleasesQuery {
        limit: Option<u32>,
        cursor: Option<String>,
        refresh: Option<String>,
    }

    #[tokio::test]
    async fn refresh_is_forwarded_only_for_the_first_window() {
        async fn releases(Query(query): Query<OctoRillReleasesQuery>) -> impl IntoResponse {
            assert_eq!(query.limit, Some(2));
            match query.cursor.as_deref() {
                None => {
                    assert_eq!(query.refresh.as_deref(), Some("if_stale"));
                    Json(json!({
                        "status": "ready",
                        "next_cursor": "cursor-2",
                        "refresh": {
                            "state": "queued",
                            "last_success_at": "2026-08-21T00:00:00Z",
                            "retry_after_seconds": 2
                        },
                        "items": [{
                            "release_id": "125",
                            "tag_name": "v1.2.5",
                            "body": "Body",
                            "html_url": "https://github.com/acme/app/releases/tag/v1.2.5"
                        }]
                    }))
                }
                Some("cursor-2") => {
                    assert!(query.refresh.is_none());
                    Json(json!({
                        "status": "ready",
                        "items": [{
                            "release_id": "124",
                            "tag_name": "v1.2.4",
                            "body": "Body",
                            "html_url": "https://github.com/acme/app/releases/tag/v1.2.4"
                        }]
                    }))
                }
                other => panic!("unexpected cursor: {other:?}"),
            }
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/api/public/repos/acme/app/releases", get(releases)),
            )
            .await
            .unwrap();
        });
        let settings = OctoRillReleaseNotesSettings {
            enabled: true,
            api_base_url: Some(format!("http://{addr}/")),
            api_key: Some("orill_ak_test".to_string()),
            default_view: ReleaseNotesView::Smart,
        };
        let repo = ServiceGitHubRepoRef {
            full_name: "acme/app".to_string(),
            html_url: "https://github.com/acme/app".to_string(),
        };

        let first = fetch_octo_rill_public_release_notes(
            &settings,
            &repo,
            None,
            ServiceReleaseNotesDirection::Older,
            2,
            None,
            true,
        )
        .await
        .expect("first window should be readable");
        assert_eq!(
            first.refresh.as_ref().map(|refresh| refresh.state),
            Some(ServiceReleaseNotesRefreshState::Queued)
        );
        assert_eq!(
            first
                .refresh
                .as_ref()
                .and_then(|refresh| refresh.retry_after_seconds),
            Some(2)
        );

        let page = fetch_octo_rill_public_release_notes(
            &settings,
            &repo,
            Some("cursor-2"),
            ServiceReleaseNotesDirection::Older,
            2,
            None,
            true,
        )
        .await
        .expect("page should be readable");
        assert!(page.refresh.is_none());
    }
}
