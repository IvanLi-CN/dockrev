use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use include_dir::{Dir, include_dir};
use serde_json::json;

use crate::state::AppState;

static WEB_DIST: Dir<'_> = include_dir!("$OUT_DIR/dockrev-ui-dist");

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

    if path == "api" || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(resp) = serve_path(&path) {
        return resp;
    }

    serve_index(state.as_ref()).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn serve_path(path: &str) -> Option<Response> {
    let file = WEB_DIST.get_file(path)?;

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mime_value = HeaderValue::from_str(mime.as_ref()).ok()?;

    let mut resp = Response::new(Body::from(file.contents()));
    resp.headers_mut().insert(header::CONTENT_TYPE, mime_value);
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
    Some(resp)
}

fn escape_json_for_inline_script(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::escape_json_for_inline_script;

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
}
