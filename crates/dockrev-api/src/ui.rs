use std::sync::{Arc, OnceLock};

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::state::AppState;

static WEB_DIST: Dir<'_> = include_dir!("$OUT_DIR/dockrev-ui-dist");
const NO_CACHE: &str = "no-cache";
const IMMUTABLE_INSTALL_ICON_CACHE: &str = "public, max-age=31536000, immutable";

#[derive(Debug, Deserialize)]
struct RouteContract {
    #[serde(rename = "version")]
    _version: u8,
    #[serde(rename = "basePath")]
    base_path: String,
    #[serde(rename = "staticPagePaths")]
    static_page_paths: Vec<String>,
    #[serde(rename = "dynamicSegmentPattern")]
    _dynamic_segment_pattern: String,
    #[serde(rename = "dynamicPageTemplates")]
    _dynamic_page_templates: Vec<String>,
    #[serde(rename = "reservedPrefixes")]
    _reserved_prefixes: Vec<String>,
}

static ROUTE_CONTRACT: OnceLock<RouteContract> = OnceLock::new();

fn route_contract() -> &'static RouteContract {
    ROUTE_CONTRACT.get_or_init(|| {
        serde_json::from_str(
            WEB_DIST
                .get_file(".dockrev-route-contract.json")
                .and_then(|file| std::str::from_utf8(file.contents()).ok())
                .unwrap_or(r#"{"version":1,"basePath":"/","staticPagePaths":["/"],"dynamicPageTemplates":[],"reservedPrefixes":["/api","/supervisor","/assets"]}"#),
        )
        .expect("validated route contract")
    })
}

pub fn router() -> Router<Arc<AppState>> {
    Router::<Arc<AppState>>::new()
        .route("/", get(index))
        .route("/assets/{*path}", get(asset))
        .route("/{*path}", get(fallback))
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    serve_index(state.as_ref()).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

async fn asset(Path(path): Path<String>) -> Response {
    if path.split('/').any(|seg| seg == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let path = format!("assets/{path}");
    serve_path(&path).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

async fn fallback(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    if path.split('/').any(|seg| seg == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if path.is_empty() {
        return index(State(state)).await;
    }

    if let Some(base_prefix) = self_upgrade_base_prefix(state.config.self_upgrade_url.as_str())
        && let Some(remaining) = strip_prefix_path(&path, &base_prefix)
    {
        if remaining.is_empty() {
            return supervisor_fallback_html(&state.config.self_upgrade_url);
        }
        return supervisor_api_misroute_json(&state.config.self_upgrade_url, remaining);
    }

    if path == "api" || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if path == "404.html" {
        return serve_not_found();
    }
    if path == ".dockrev-route-contract.json" || path.starts_with(".dockrev-route-contract/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(resp) = serve_path(&path) {
        return resp;
    }

    let canonical = canonical_contract_path(&path);
    if canonical != path && is_contract_page(&canonical) {
        return redirect_to(&canonical);
    }
    if is_contract_page(&canonical) {
        return serve_index(state.as_ref())
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
    }
    if path_has_extension(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_not_found()
}

fn canonical_contract_path(path: &str) -> String {
    if path.is_empty() || path == "/" || path.ends_with('.') || !path.ends_with('/') {
        return path.to_string();
    }
    path.trim_end_matches('/').to_string()
}

fn redirect_to(path: &str) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::PERMANENT_REDIRECT;
    if let Ok(value) = HeaderValue::from_str(&format!("/{path}")) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
}

fn path_has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

fn is_contract_page(path: &str) -> bool {
    let contract = route_contract();
    let path = format!("/{path}");
    let base = contract.base_path.trim_matches('/');
    let relative = if base.is_empty() {
        path.as_str().to_string()
    } else if path == format!("/{base}") {
        "/".to_string()
    } else if let Some(rest) = path.strip_prefix(&format!("/{base}/")) {
        format!("/{rest}")
    } else {
        return false;
    };
    if contract
        .static_page_paths
        .iter()
        .any(|candidate| candidate == &relative)
    {
        return true;
    }
    let relative = relative.trim_start_matches('/');
    contract
        ._dynamic_page_templates
        .iter()
        .any(|template| matches_template(template, relative))
}

fn matches_template(template: &str, relative: &str) -> bool {
    let template = template.trim_matches('/');
    let parts: Vec<&str> = relative
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let expected: Vec<&str> = template.split('/').collect();
    parts.len() == expected.len()
        && parts.iter().zip(expected.iter()).all(|(part, expected)| {
            if expected.starts_with(':') {
                is_safe_segment(part)
            } else {
                part == expected
            }
        })
}

fn is_safe_segment(segment: &str) -> bool {
    let len = segment.len();
    (1..=128).contains(&len)
        && segment
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn serve_not_found() -> Response {
    let Some(file) = WEB_DIST.get_file("404.html") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = Response::new(Body::from(file.contents()));
    *response.status_mut() = StatusCode::NOT_FOUND;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn self_upgrade_base_prefix(self_upgrade_url: &str) -> Option<String> {
    let s = self_upgrade_url.trim();
    if s.is_empty() {
        return None;
    }

    let base = Url::parse("http://example.invalid").ok()?;
    let joined = base.join(s).ok()?;
    let path = joined.path().trim();
    if path.is_empty() || path == "/" {
        return None;
    }

    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }

    Some(trimmed.trim_start_matches('/').to_string())
}

fn strip_prefix_path<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if path == prefix {
        return Some("");
    }
    let p = format!("{prefix}/");
    path.strip_prefix(p.as_str())
}

fn supervisor_api_misroute_json(self_upgrade_url: &str, path: &str) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        axum::Json(json!({
            "ok": false,
            "code": "supervisor_misrouted",
            "message": "This path should be served by dockrev-supervisor (self-upgrade console/API), but the request hit dockrev main service. Check your reverse proxy mapping.",
            "selfUpgradeUrl": self_upgrade_url,
            "path": path,
        })),
    )
        .into_response()
}

fn supervisor_fallback_html(self_upgrade_url: &str) -> Response {
    let display_url = escape_html(self_upgrade_url.trim());
    let curl_base = ensure_trailing_slash(self_upgrade_url.trim());
    let curl_base = escape_html(&curl_base);

    let body = format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Dockrev Supervisor 未正确映射</title>
  <style>
    :root {{ color-scheme: light dark; }}
    body {{ font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; margin: 0; padding: 24px; line-height: 1.45; }}
    .card {{ max-width: 860px; margin: 0 auto; padding: 20px 18px; border: 1px solid rgba(127,127,127,.35); border-radius: 12px; background: rgba(127,127,127,.06); }}
    h1 {{ margin: 0 0 12px; font-size: 20px; }}
    p {{ margin: 10px 0; }}
    code, pre {{ font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; }}
    pre {{ padding: 12px; border-radius: 10px; overflow: auto; background: rgba(127,127,127,.12); }}
    .muted {{ opacity: .85; }}
    .row {{ display: flex; gap: 12px; flex-wrap: wrap; margin-top: 14px; }}
    a.button {{ display: inline-block; padding: 8px 12px; border-radius: 10px; border: 1px solid rgba(127,127,127,.45); text-decoration: none; }}
  </style>
</head>
<body>
  <div class="card">
    <h1>部署问题：<code>{display_url}</code> 未映射到 Dockrev Supervisor</h1>
    <p>你正在访问的是自我升级入口（Supervisor）。但当前响应来自 <strong>Dockrev 主服务</strong>，这通常意味着反向代理/路由配置漏配或误配。</p>
    <p class="muted">正确情况下：<code>{display_url}</code> 应该由 <code>dockrev-supervisor</code> 提供（含 UI 与 API）。</p>

    <h2 style="font-size:16px; margin: 18px 0 8px;">如何验证</h2>
    <p>请在同域下验证以下接口应由 supervisor 返回：</p>
    <pre>curl -i {curl_base}health
curl -i {curl_base}version
curl -i {curl_base}self-upgrade</pre>

    <h2 style="font-size:16px; margin: 18px 0 8px;">如何修复（思路）</h2>
    <p>在你的反向代理中，把 <code>{display_url}</code> 路由到 supervisor 的 HTTP 地址（并保持 base path 一致）。</p>
    <p class="muted">常见相关配置：<code>DOCKREV_SELF_UPGRADE_URL</code>（Dockrev 主服务/前端使用）与 <code>DOCKREV_SUPERVISOR_BASE_PATH</code>（supervisor 使用）。</p>

    <div class="row">
      <a class="button" href="/">返回 Dockrev</a>
    </div>
  </div>
</body>
</html>"#,
    );

    let mime = mime_guess::from_path("index.html").first_or_octet_stream();
    let mime_value = HeaderValue::from_str(mime.as_ref()).ok();

    let mut resp = Response::new(Body::from(body.into_bytes()));
    // Use 200 to ensure browsers render the fallback page without treating it as a hard error,
    // while supervisor API paths still fail with non-2xx to avoid false "ok" probes.
    *resp.status_mut() = StatusCode::OK;
    if let Some(v) = mime_value {
        resp.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    resp
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn ensure_trailing_slash(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('/') {
        return trimmed.to_string();
    }
    format!("{trimmed}/")
}

fn serve_path(path: &str) -> Option<Response> {
    let file = WEB_DIST.get_file(path)?;

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mime_value = HeaderValue::from_str(mime.as_ref()).ok()?;

    let mut resp = Response::new(Body::from(file.contents()));
    resp.headers_mut().insert(header::CONTENT_TYPE, mime_value);
    if let Some(cache_control) = cache_control_for_ui_path(path) {
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(cache_control),
        );
    }
    Some(resp)
}

fn serve_index(state: &AppState) -> Option<Response> {
    let file = WEB_DIST.get_file("index.html")?;
    let raw = std::str::from_utf8(file.contents()).ok()?;

    let config_json = json!({
        "selfUpgradeUrl": &state.config.self_upgrade_url,
        "dockrevImageRepo": &state.config.dockrev_image_repo,
    })
    .to_string();

    let config_json = escape_json_for_inline_script(&config_json);

    let injected = format!(r#"<script>window.__DOCKREV_CONFIG__ = {config_json};</script>"#);

    let body = if raw.contains("<!-- DOCKREV_RUNTIME_CONFIG -->") {
        raw.replace("<!-- DOCKREV_RUNTIME_CONFIG -->", &injected)
    } else if let Some(idx) = raw.find("</head>") {
        let mut out = String::with_capacity(raw.len() + injected.len() + 32);
        out.push_str(&raw[..idx]);
        out.push_str(&injected);
        out.push_str(&raw[idx..]);
        out
    } else {
        raw.to_string()
    };

    let mime = mime_guess::from_path("index.html").first_or_octet_stream();
    let mime_value = HeaderValue::from_str(mime.as_ref()).ok()?;
    let mut resp = Response::new(Body::from(body.into_bytes()));
    resp.headers_mut().insert(header::CONTENT_TYPE, mime_value);
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_CACHE));
    Some(resp)
}

fn cache_control_for_ui_path(path: &str) -> Option<&'static str> {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if matches!(
        file_name,
        "index.html" | "manifest.webmanifest" | "sw.js" | "registerSW.js"
    ) {
        return Some(NO_CACHE);
    }
    if is_hashed_install_icon(file_name) {
        return Some(IMMUTABLE_INSTALL_ICON_CACHE);
    }
    if is_legacy_install_asset(file_name) {
        return Some(NO_CACHE);
    }
    None
}

fn is_hashed_install_icon(file_name: &str) -> bool {
    let Some((stem, extension)) = file_name.rsplit_once('.') else {
        return false;
    };
    if !matches!(extension, "svg" | "png" | "ico") {
        return false;
    }
    let Some((prefix, digest)) = stem.rsplit_once('-') else {
        return false;
    };
    matches!(
        prefix,
        "favicon" | "pwa-192" | "pwa-512" | "pwa-maskable-192" | "pwa-maskable-512"
    ) && digest.len() == 12
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn is_legacy_install_asset(file_name: &str) -> bool {
    matches!(
        file_name,
        "favicon.svg"
            | "favicon.png"
            | "favicon.ico"
            | "pwa-192.png"
            | "pwa-512.png"
            | "pwa-maskable-192.png"
            | "pwa-maskable-512.png"
    )
}

fn escape_json_for_inline_script(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::{
        IMMUTABLE_INSTALL_ICON_CACHE, NO_CACHE, cache_control_for_ui_path, ensure_trailing_slash,
        escape_html, escape_json_for_inline_script, is_contract_page, matches_template,
        path_has_extension,
    };

    #[test]
    fn escape_json_for_inline_script_prevents_script_breakout() {
        let json = r#"{"selfUpgradeUrl":"</script><img src=x onerror=alert(1)>"}"#;
        let out = escape_json_for_inline_script(json);
        assert!(!out.contains("</script>"));
        assert!(out.contains("\\u003c/script>"));
        assert!(out.contains("\\u003cimg"));
    }

    #[test]
    fn escape_json_for_inline_script_escapes_line_separators() {
        let json = "{\"x\":\"\u{2028}\u{2029}\"}";
        let out = escape_json_for_inline_script(json);
        assert!(out.contains("\\u2028"));
        assert!(out.contains("\\u2029"));
    }

    #[test]
    fn escape_html_escapes_special_chars() {
        let s = r#"<a href="x&y">O'Reilly</a>"#;
        let out = escape_html(s);
        assert_eq!(
            out,
            "&lt;a href=&quot;x&amp;y&quot;&gt;O&#39;Reilly&lt;/a&gt;"
        );
    }

    #[test]
    fn ensure_trailing_slash_adds_one_when_missing() {
        assert_eq!(ensure_trailing_slash("/supervisor"), "/supervisor/");
        assert_eq!(ensure_trailing_slash("/supervisor/"), "/supervisor/");
        assert_eq!(ensure_trailing_slash(""), "");
        assert_eq!(ensure_trailing_slash("   "), "");
    }

    #[test]
    fn hashed_install_icons_are_immutable() {
        assert_eq!(
            cache_control_for_ui_path("pwa-192-3d6999d34c2d.png"),
            Some(IMMUTABLE_INSTALL_ICON_CACHE)
        );
        assert_eq!(
            cache_control_for_ui_path("favicon-0a0e56c2e2df.svg"),
            Some(IMMUTABLE_INSTALL_ICON_CACHE)
        );
    }

    #[test]
    fn legacy_install_assets_are_revalidated() {
        assert_eq!(cache_control_for_ui_path("pwa-192.png"), Some(NO_CACHE));
        assert_eq!(cache_control_for_ui_path("favicon.ico"), Some(NO_CACHE));
        assert_eq!(cache_control_for_ui_path("apple-touch-icon.png"), None);
    }

    #[test]
    fn app_update_metadata_is_revalidated() {
        for path in [
            "index.html",
            "manifest.webmanifest",
            "sw.js",
            "registerSW.js",
        ] {
            assert_eq!(cache_control_for_ui_path(path), Some(NO_CACHE));
        }
        assert_eq!(cache_control_for_ui_path("assets/app.js"), None);
        assert_eq!(cache_control_for_ui_path("pwa-192-short.png"), None);
    }

    #[test]
    fn contract_classifies_pages_without_widening_to_unknown_paths() {
        assert!(is_contract_page("queue"));
        assert!(is_contract_page("queue/job_01-safe"));
        assert!(is_contract_page("services/stack-prod/svc-prod-api/logs"));
        assert!(!is_contract_page("apple-touch-icon.png"));
        assert!(!is_contract_page("made-up-deep-link"));
        assert!(!is_contract_page("services/stack prod/svc"));
    }

    #[test]
    fn dynamic_template_validation_uses_safe_segment_grammar() {
        assert!(matches_template("/queue/:jobId", "queue/job_01-safe"));
        assert!(!matches_template("/queue/:jobId", "queue/ümlaut"));
        assert!(!matches_template("/queue/:jobId", "queue/../etc"));
    }

    #[test]
    fn unknown_resources_are_detected_by_extension() {
        assert!(path_has_extension("missing.css"));
        assert!(!path_has_extension("missing-page"));
    }
}
