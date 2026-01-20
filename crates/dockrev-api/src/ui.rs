use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use include_dir::{Dir, include_dir};

static WEB_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/{*path}", get(asset))
        .route("/{*path}", get(fallback))
}

async fn index() -> Response {
    serve_path("index.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

async fn asset(Path(path): Path<String>) -> Response {
    if path.split('/').any(|seg| seg == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let path = format!("assets/{path}");
    serve_path(&path).unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

async fn fallback(Path(path): Path<String>) -> Response {
    if path.split('/').any(|seg| seg == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if path.is_empty() {
        return index().await;
    }

    if let Some(resp) = serve_path(&path) {
        return resp;
    }

    serve_path("index.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn serve_path(path: &str) -> Option<Response> {
    let file = WEB_DIST.get_file(path)?;

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mime_value = HeaderValue::from_str(mime.as_ref()).ok()?;

    let mut resp = Response::new(Body::from(file.contents()));
    resp.headers_mut().insert(header::CONTENT_TYPE, mime_value);
    Some(resp)
}
