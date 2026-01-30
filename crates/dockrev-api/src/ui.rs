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
use url::Url;

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

    if let Some(base_prefix) = self_upgrade_base_prefix(state.config.self_upgrade_url.as_str()) {
        if let Some(remaining) = strip_prefix_path(&path, &base_prefix) {
            if remaining.is_empty() {
                return supervisor_fallback_html(&state.config.self_upgrade_url);
            }
            return supervisor_api_misroute_json(&state.config.self_upgrade_url, &remaining);
        }
    }

    if path == "api" || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(resp) = serve_path(&path) {
        return resp;
    }

    serve_index(state.as_ref()).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
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
    <h1>部署问题：<code>{self_upgrade_url}</code> 未映射到 Dockrev Supervisor</h1>
    <p>你正在访问的是自我升级入口（Supervisor）。但当前响应来自 <strong>Dockrev 主服务</strong>，这通常意味着反向代理/路由配置漏配或误配。</p>
    <p class="muted">正确情况下：<code>{self_upgrade_url}</code> 应该由 <code>dockrev-supervisor</code> 提供（含 UI 与 API）。</p>

    <h2 style="font-size:16px; margin: 18px 0 8px;">如何验证</h2>
    <p>请在同域下验证以下接口应由 supervisor 返回：</p>
    <pre>curl -i {self_upgrade_url}health
curl -i {self_upgrade_url}version
curl -i {self_upgrade_url}self-upgrade</pre>

    <h2 style="font-size:16px; margin: 18px 0 8px;">如何修复（思路）</h2>
    <p>在你的反向代理中，把 <code>{self_upgrade_url}</code> 路由到 supervisor 的 HTTP 地址（并保持 base path 一致）。</p>
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
    *resp.status_mut() = StatusCode::BAD_GATEWAY;
    if let Some(v) = mime_value {
        resp.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    resp
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
