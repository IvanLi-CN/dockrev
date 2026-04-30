use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use url::Url;

use crate::error::ApiError;
use crate::state::AppState;

use super::{map_internal, require_user};

const MAX_ICON_BYTES: usize = 512 * 1024;
const ICON_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Deserialize)]
pub(crate) struct HomepageIconQuery {
    color: Option<String>,
}

struct UpstreamIcon {
    url: Url,
    content_type: &'static str,
}

pub(crate) async fn proxy_homepage_icon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((provider, path)): Path<(String, String)>,
    Query(query): Query<HomepageIconQuery>,
) -> Result<Response, ApiError> {
    let _user = require_user(&state, &headers).await?;
    let upstream = resolve_homepage_icon_upstream(&provider, &path, query.color.as_deref())?;
    let client = reqwest::Client::builder()
        .timeout(ICON_TIMEOUT)
        .build()
        .context("build homepage icon http client")
        .map_err(map_internal)?;

    let mut response = client
        .get(upstream.url)
        .header(header::USER_AGENT, "dockrev-homepage-icon-proxy")
        .send()
        .await
        .context("fetch homepage icon")
        .map_err(map_internal)?;

    if !response.status().is_success() {
        return Ok((StatusCode::NOT_FOUND, Bytes::new()).into_response());
    }

    if response
        .content_length()
        .is_some_and(|length| length > MAX_ICON_BYTES as u64)
    {
        return Ok((StatusCode::BAD_GATEWAY, Bytes::new()).into_response());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("read homepage icon body")
        .map_err(map_internal)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_ICON_BYTES {
            return Ok((StatusCode::BAD_GATEWAY, Bytes::new()).into_response());
        }
        body.extend_from_slice(&chunk);
    }
    let bytes = Bytes::from(body);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(upstream.content_type),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, stale-while-revalidate=604800"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));

    Ok((headers, bytes).into_response())
}

fn resolve_homepage_icon_upstream(
    provider: &str,
    path: &str,
    color: Option<&str>,
) -> Result<UpstreamIcon, ApiError> {
    match provider {
        "iconify" => resolve_iconify(path, color),
        "selfhst" => resolve_static_icon("https://cdn.jsdelivr.net/gh/selfhst/icons", path),
        "dashboard" => resolve_static_icon(
            "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons",
            path,
        ),
        _ => Err(ApiError::not_found("homepage icon provider is not allowed")),
    }
}

fn resolve_iconify(path: &str, color: Option<&str>) -> Result<UpstreamIcon, ApiError> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 {
        return Err(ApiError::invalid_argument("invalid iconify path"));
    }
    let collection = parts[0];
    let file = parts[1];
    if collection != "mdi" && collection != "simple-icons" {
        return Err(ApiError::not_found("iconify collection is not allowed"));
    }
    if !file.ends_with(".svg") || !is_safe_file_name(file) {
        return Err(ApiError::invalid_argument("invalid iconify icon name"));
    }

    let mut url = Url::parse(&format!("https://api.iconify.design/{collection}/{file}"))
        .context("build iconify url")
        .map_err(map_internal)?;
    if let Some(color) = color {
        let normalized = validate_icon_color(color)?;
        url.query_pairs_mut().append_pair("color", normalized);
    }

    Ok(UpstreamIcon {
        url,
        content_type: "image/svg+xml",
    })
}

fn resolve_static_icon(base: &str, path: &str) -> Result<UpstreamIcon, ApiError> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 {
        return Err(ApiError::invalid_argument("invalid homepage icon path"));
    }
    let ext = parts[0];
    let file = parts[1];
    if !matches!(ext, "svg" | "png" | "webp") {
        return Err(ApiError::invalid_argument(
            "homepage icon extension is not allowed",
        ));
    }
    if !file.ends_with(&format!(".{ext}")) || !is_safe_file_name(file) {
        return Err(ApiError::invalid_argument("invalid homepage icon filename"));
    }

    let url = Url::parse(&format!("{base}/{ext}/{file}"))
        .context("build homepage icon url")
        .map_err(map_internal)?;
    Ok(UpstreamIcon {
        url,
        content_type: content_type_for_ext(ext),
    })
}

fn content_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/svg+xml",
    }
}

fn is_safe_file_name(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|value| value.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
        && !value.contains("..")
}

fn validate_icon_color(value: &str) -> Result<&str, ApiError> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| ApiError::invalid_argument("homepage icon color must be a hex color"))?;
    if matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|value| value.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(ApiError::invalid_argument(
            "homepage icon color must be a hex color",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_iconify_with_color() {
        let icon =
            resolve_homepage_icon_upstream("iconify", "simple-icons/github.svg", Some("#dbeafe"))
                .unwrap();

        assert_eq!(
            icon.url.as_str(),
            "https://api.iconify.design/simple-icons/github.svg?color=%23dbeafe"
        );
        assert_eq!(icon.content_type, "image/svg+xml");
    }

    #[test]
    fn resolves_dashboard_icon_path() {
        let icon = resolve_homepage_icon_upstream("dashboard", "svg/prometheus.svg", None).unwrap();

        assert_eq!(
            icon.url.as_str(),
            "https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/prometheus.svg"
        );
        assert_eq!(icon.content_type, "image/svg+xml");
    }

    #[test]
    fn rejects_unknown_provider_and_traversal() {
        assert!(resolve_homepage_icon_upstream("other", "svg/prometheus.svg", None).is_err());
        assert!(resolve_homepage_icon_upstream("dashboard", "svg/../secret.svg", None).is_err());
    }
}
