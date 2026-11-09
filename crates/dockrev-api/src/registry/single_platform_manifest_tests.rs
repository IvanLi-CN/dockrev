use super::*;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

async fn spawn_server(app: Router) -> Option<(String, tokio::task::JoinHandle<()>)> {
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping network-bound test: {error}");
            return None;
        }
        Err(error) => panic!("failed to bind test listener: {error}"),
    };
    let addr = listener.local_addr().expect("failed to read test address");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test server failed");
    });
    Some((format!("http://{addr}"), handle))
}

fn manifest_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Docker-Content-Digest",
        HeaderValue::from_static("sha256:manifest"),
    );
    headers
}

async fn single_platform_manifest() -> impl IntoResponse {
    (
        manifest_headers(),
        Json(json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:config" },
            "layers": []
        })),
    )
}

async fn config_blob(
    State(requests): State<Arc<Mutex<Vec<String>>>>,
    Path(digest): Path<String>,
) -> impl IntoResponse {
    requests.lock().unwrap().push(digest);
    Json(json!({ "os": "linux", "architecture": "amd64" }))
}

#[tokio::test]
async fn get_manifest_reads_single_platform_config_blob() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(
            "/v2/testrepo/manifests/latest",
            get(single_platform_manifest),
        )
        .route("/v2/testrepo/blobs/{digest}", get(config_blob))
        .with_state(requests.clone());
    let Some((base, server)) = spawn_server(app).await else {
        return;
    };
    let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default())
        .unwrap()
        .with_test_registry_base_url(base);
    let image = ImageRef::parse("test.registry/testrepo:latest").unwrap();
    let info = RegistryClient::get_manifest(&client, &image, "latest", "linux/amd64")
        .await
        .unwrap();

    assert_eq!(info.digest.as_deref(), Some("sha256:manifest"));
    assert_eq!(info.arch, vec!["linux/amd64"]);
    assert_eq!(requests.lock().unwrap().as_slice(), ["sha256:config"]);
    assert!(matches!(
        compute_arch_match("linux/amd64", &info.arch),
        ArchMatch::Match
    ));
    server.abort();
}

async fn config_blob_failure(
    State(requests): State<Arc<Mutex<Vec<String>>>>,
    Path(digest): Path<String>,
) -> impl IntoResponse {
    requests.lock().unwrap().push(digest);
    (StatusCode::INTERNAL_SERVER_ERROR, "config unavailable")
}

#[tokio::test]
async fn get_manifest_preserves_unknown_when_config_blob_fails() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(
            "/v2/testrepo/manifests/latest",
            get(single_platform_manifest),
        )
        .route("/v2/testrepo/blobs/{digest}", get(config_blob_failure))
        .with_state(requests.clone());
    let Some((base, server)) = spawn_server(app).await else {
        return;
    };
    let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default())
        .unwrap()
        .with_test_registry_base_url(base);
    let image = ImageRef::parse("test.registry/testrepo:latest").unwrap();
    let info = RegistryClient::get_manifest(&client, &image, "latest", "linux/amd64")
        .await
        .unwrap();

    assert_eq!(info.digest.as_deref(), Some("sha256:manifest"));
    assert!(info.arch.is_empty());
    assert_eq!(requests.lock().unwrap().as_slice(), ["sha256:config"]);
    assert!(matches!(
        compute_arch_match("linux/amd64", &info.arch),
        ArchMatch::Unknown
    ));
    server.abort();
}

async fn slow_config_blob(
    State(requests): State<Arc<Mutex<Vec<String>>>>,
    Path(digest): Path<String>,
) -> impl IntoResponse {
    requests.lock().unwrap().push(digest);
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    Json(json!({ "os": "linux", "architecture": "amd64" }))
}

#[tokio::test]
async fn get_manifest_preserves_manifest_result_when_config_blob_times_out() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(
            "/v2/testrepo/manifests/latest",
            get(single_platform_manifest),
        )
        .route("/v2/testrepo/blobs/{digest}", get(slow_config_blob))
        .with_state(requests.clone());
    let Some((base, server)) = spawn_server(app).await else {
        return;
    };
    let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default())
        .unwrap()
        .with_test_registry_base_url(base);
    let image = ImageRef::parse("test.registry/testrepo:latest").unwrap();
    let info = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        RegistryClient::get_manifest(&client, &image, "latest", "linux/amd64"),
    )
    .await
    .expect("config fallback exceeded test deadline")
    .unwrap();

    assert_eq!(info.digest.as_deref(), Some("sha256:manifest"));
    assert!(info.arch.is_empty());
    assert_eq!(requests.lock().unwrap().as_slice(), ["sha256:config"]);
    server.abort();
}

async fn oversized_config_blob() -> impl IntoResponse {
    (
        StatusCode::OK,
        "x".repeat(MAX_CONFIG_BLOB_BYTES.saturating_add(1)),
    )
}

#[tokio::test]
async fn get_manifest_preserves_unknown_when_config_blob_is_too_large() {
    let app = Router::new()
        .route(
            "/v2/testrepo/manifests/latest",
            get(single_platform_manifest),
        )
        .route("/v2/testrepo/blobs/{digest}", get(oversized_config_blob));
    let Some((base, server)) = spawn_server(app).await else {
        return;
    };
    let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default())
        .unwrap()
        .with_test_registry_base_url(base);
    let image = ImageRef::parse("test.registry/testrepo:latest").unwrap();
    let info = RegistryClient::get_manifest(&client, &image, "latest", "linux/amd64")
        .await
        .unwrap();

    assert_eq!(info.digest.as_deref(), Some("sha256:manifest"));
    assert!(info.arch.is_empty());
    server.abort();
}

async fn multi_platform_manifest() -> impl IntoResponse {
    (
        manifest_headers(),
        Json(json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                { "digest": "sha256:amd64", "platform": { "os": "linux", "architecture": "amd64" } },
                { "digest": "sha256:arm64", "platform": { "os": "linux", "architecture": "arm64" } }
            ]
        })),
    )
}

async fn unexpected_config_blob(
    State(requests): State<Arc<Mutex<Vec<String>>>>,
    Path(digest): Path<String>,
) -> impl IntoResponse {
    requests.lock().unwrap().push(digest);
    StatusCode::INTERNAL_SERVER_ERROR
}

#[tokio::test]
async fn get_manifest_does_not_fetch_config_blob_for_manifest_lists() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route(
            "/v2/testrepo/manifests/latest",
            get(multi_platform_manifest),
        )
        .route("/v2/testrepo/blobs/{digest}", get(unexpected_config_blob))
        .with_state(requests.clone());
    let Some((base, server)) = spawn_server(app).await else {
        return;
    };
    let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default())
        .unwrap()
        .with_test_registry_base_url(base);
    let image = ImageRef::parse("test.registry/testrepo:latest").unwrap();
    let info = RegistryClient::get_manifest(&client, &image, "latest", "linux/amd64")
        .await
        .unwrap();

    assert_eq!(info.arch, vec!["linux/amd64", "linux/arm64"]);
    assert_eq!(info.platform_digest.as_deref(), Some("sha256:amd64"));
    assert!(requests.lock().unwrap().is_empty());
    server.abort();
}

#[test]
fn merge_config_blob_platform_reads_single_platform_manifest_config() {
    let manifest = json!({ "config": { "digest": "sha256:config" } });
    let config_blob = json!({ "os": "linux", "architecture": "amd64" });
    let mut info = parse_manifest_json(
        &manifest.to_string(),
        Some("sha256:manifest".to_string()),
        "linux/amd64",
    )
    .unwrap();

    merge_config_blob_platform(&mut info, &manifest, &config_blob, "linux/amd64");

    assert_eq!(info.digest.as_deref(), Some("sha256:manifest"));
    assert_eq!(info.arch, vec!["linux/amd64"]);
}

#[test]
fn merge_config_blob_platform_preserves_arm64_variant_for_default_host() {
    let manifest = json!({ "config": { "digest": "sha256:config" } });
    let config_blob = json!({
        "os": "linux",
        "architecture": "arm64",
        "variant": "v8"
    });
    let mut info = parse_manifest_json(&manifest.to_string(), None, "linux/arm64").unwrap();

    merge_config_blob_platform(&mut info, &manifest, &config_blob, "linux/arm64");

    assert_eq!(info.arch, vec!["linux/arm64", "linux/arm64/v8"]);
    assert!(matches!(
        compute_arch_match("linux/arm64", &info.arch),
        ArchMatch::Match
    ));
}

#[test]
fn merge_config_blob_platform_preserves_unknown_for_incomplete_config() {
    let manifest = json!({ "config": { "digest": "sha256:config" } });
    let config_blob = json!({ "os": "linux" });
    let mut info = parse_manifest_json(&manifest.to_string(), None, "linux/amd64").unwrap();

    merge_config_blob_platform(&mut info, &manifest, &config_blob, "linux/amd64");

    assert!(info.arch.is_empty());
    assert!(matches!(
        compute_arch_match("linux/amd64", &info.arch),
        ArchMatch::Unknown
    ));
}

#[test]
fn merge_config_blob_platform_ignores_manifest_lists() {
    let manifest = json!({
        "manifests": [{
            "digest": "sha256:amd64",
            "platform": { "os": "linux", "architecture": "amd64" }
        }],
        "config": { "digest": "sha256:index-config" }
    });
    let config_blob = json!({ "os": "linux", "architecture": "arm64" });
    let mut info = parse_manifest_json(&manifest.to_string(), None, "linux/amd64").unwrap();

    merge_config_blob_platform(&mut info, &manifest, &config_blob, "linux/amd64");

    assert_eq!(info.arch, vec!["linux/amd64"]);
    assert_eq!(info.platform_digest.as_deref(), Some("sha256:amd64"));
}
