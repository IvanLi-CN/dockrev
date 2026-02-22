use std::{
    collections::HashMap,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::header::{ACCEPT, AUTHORIZATION, LINK, RETRY_AFTER, WWW_AUTHENTICATE};
use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::api::types::ArchMatch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub registry: String,
    pub name: String,
    pub reference: String,
}

impl ImageRef {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        // Very small parser: registry host is the first segment if it contains '.' or ':'.
        // Otherwise default to docker.io. Name is the rest. Reference is required.
        let (without_digest, _) = input.split_once('@').unwrap_or((input, ""));

        let (name_with_registry, reference) = without_digest
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("image ref missing tag (expected repo/name:tag)"))?;

        if reference.is_empty() || reference.contains('/') {
            return Err(anyhow::anyhow!(
                "invalid tag in image ref (expected repo/name:tag)"
            ));
        }

        let mut parts = name_with_registry.split('/').collect::<Vec<_>>();
        if parts.is_empty() {
            return Err(anyhow::anyhow!("invalid image ref"));
        }

        let (registry, name) = if parts[0].contains('.') || parts[0].contains(':') {
            let reg = parts.remove(0).to_string();
            (reg, parts.join("/"))
        } else {
            ("docker.io".to_string(), name_with_registry.to_string())
        };

        let name = normalize_dockerhub_name(&registry, &name);

        Ok(Self {
            registry,
            name,
            reference: reference.to_string(),
        })
    }
}

fn normalize_dockerhub_name(registry: &str, name: &str) -> String {
    if registry == "docker.io" && !name.contains('/') {
        format!("library/{name}")
    } else {
        name.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct ManifestInfo {
    // Best-effort digest for the requested reference (tag or digest). Prefer the registry header
    // digest when available (index/manifest-list digest for multi-arch), otherwise fall back to the
    // host platform digest when it can be selected unambiguously.
    pub digest: Option<String>,
    // For multi-arch images, the selected host platform's child manifest digest (when available).
    //
    // Note: Docker runtime `.RepoDigests` may report either the index digest or the platform digest
    // depending on environment; callers that compare against runtime digests should consider both.
    pub platform_digest: Option<String>,
    pub arch: Vec<String>,
}

#[async_trait]
pub trait RegistryClient: Send + Sync {
    async fn list_tags(&self, image: &ImageRef) -> anyhow::Result<Vec<String>>;
    async fn get_manifest(
        &self,
        image: &ImageRef,
        reference: &str,
        host_platform: &str,
    ) -> anyhow::Result<ManifestInfo>;
}

#[derive(Clone)]
pub struct HttpRegistryClient {
    http: reqwest::Client,
    docker: Option<DockerConfig>,
    token_cache: Arc<Mutex<HashMap<String, String>>>,
    host_limiters: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
    options: HttpRegistryClientOptions,
}

#[derive(Clone, Debug)]
pub struct HttpRegistryClientOptions {
    pub per_host_concurrency: usize,
    pub retry_max_attempts: usize,
    pub retry_base_ms: u64,
    pub retry_max_ms: u64,
}

impl Default for HttpRegistryClientOptions {
    fn default() -> Self {
        Self {
            per_host_concurrency: 3,
            retry_max_attempts: 3,
            retry_base_ms: 250,
            retry_max_ms: 2000,
        }
    }
}

impl HttpRegistryClient {
    pub fn new(
        docker_config_path: Option<&Path>,
        options: HttpRegistryClientOptions,
    ) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build()?;
        let docker = docker_config_path.and_then(|p| DockerConfig::load(p).ok());
        let per_host_concurrency = options.per_host_concurrency.max(1);
        Ok(Self {
            http,
            docker,
            token_cache: Arc::new(Mutex::new(HashMap::new())),
            host_limiters: Arc::new(Mutex::new(HashMap::new())),
            options: HttpRegistryClientOptions {
                per_host_concurrency,
                ..options
            },
        })
    }
}

#[async_trait]
impl RegistryClient for HttpRegistryClient {
    async fn list_tags(&self, image: &ImageRef) -> anyhow::Result<Vec<String>> {
        use std::collections::HashSet;

        fn parse_next_link(link_header: &str, base_host: &str) -> Option<String> {
            // Example:
            //   Link: </v2/library/alpine/tags/list?last=20190508&n=5>; rel="next"
            //
            // We only care about rel="next". Some registries include multiple links in one header.
            for part in link_header.split(',') {
                let part = part.trim();
                if !part.contains("rel=\"next\"") && !part.contains("rel=next") {
                    continue;
                }

                let start = part.find('<')?;
                let end = part[start + 1..].find('>')? + start + 1;
                let raw = part[start + 1..end].trim();
                if raw.is_empty() {
                    continue;
                }

                if raw.starts_with("http://") || raw.starts_with("https://") {
                    return Some(raw.to_string());
                }
                if raw.starts_with('/') {
                    return Some(format!("https://{base_host}{raw}"));
                }
                // Best-effort fallback for weird relative URLs.
                return Some(format!("https://{base_host}/{raw}"));
            }

            None
        }

        let scope = format!("repository:{}:pull", image.name);
        let base_host = registry_api_host(&image.registry);
        let base_url = format!("https://{base_host}/v2/{}/tags/list", image.name);
        // Large page size to reduce round-trips. We'll still follow `Link: rel="next"` if the
        // registry paginates.
        let mut url = format!("{base_url}?n=1000");

        #[derive(Deserialize)]
        struct TagsResponse {
            tags: Option<Vec<String>>,
        }

        let mut out: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        // Some registries don't support `n` pagination params; in that case, retry once with the
        // bare endpoint.
        let mut pagination_params_ok = true;

        loop {
            if !visited.insert(url.clone()) {
                break;
            }

            let resp = match self
                .get_with_auth(&image.registry, &scope, url.clone(), None)
                .await
            {
                Ok(resp) => resp,
                Err(_e) if pagination_params_ok => {
                    pagination_params_ok = false;
                    url = base_url.clone();
                    continue;
                }
                Err(e) => return Err(e),
            };
            pagination_params_ok = false;

            let link_header = resp
                .headers()
                .get(LINK)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let body: TagsResponse = resp.json().await?;
            out.extend(body.tags.unwrap_or_default());

            let Some(link_header) = link_header else {
                break;
            };
            let Some(next) = parse_next_link(&link_header, base_host) else {
                break;
            };
            url = next;
        }

        out.sort();
        out.dedup();
        Ok(out)
    }

    async fn get_manifest(
        &self,
        image: &ImageRef,
        reference: &str,
        host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let scope = format!("repository:{}:pull", image.name);
        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            registry_api_host(&image.registry),
            image.name,
            reference
        );

        let accept = Some(
            "application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json",
        );
        let resp = self
            .get_with_auth(&image.registry, &scope, url, accept)
            .await?;

        let digest = resp
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = resp.text().await?;
        parse_manifest_json(&body, digest, host_platform)
    }
}

impl HttpRegistryClient {
    async fn get_with_auth(
        &self,
        registry_host: &str,
        scope: &str,
        url: String,
        accept: Option<&str>,
    ) -> anyhow::Result<reqwest::Response> {
        let basic_auth = self
            .docker
            .as_ref()
            .and_then(|d| d.basic_auth(registry_host))
            .map(|(user, pass)| format!("Basic {}", BASE64.encode(format!("{user}:{pass}"))));
        let accept_header = accept.map(|s| s.to_string());
        let limit_host = request_limit_host(&url, registry_host);
        let resp = self
            .send_with_429_retry(&limit_host, || {
                let mut builder = self.http.get(url.clone());
                if let Some(accept) = accept_header.as_deref() {
                    builder = builder.header(ACCEPT, accept);
                }
                if let Some(auth) = basic_auth.as_deref() {
                    builder = builder.header(AUTHORIZATION, auth.to_string());
                }
                builder
            })
            .await?;
        if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!(
                    "registry request failed: {}",
                    resp.status()
                ));
            }
            return Ok(resp);
        }

        let www = resp
            .headers()
            .get(WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();

        let Some(bearer) = parse_www_authenticate_bearer(&www) else {
            return Err(anyhow::anyhow!("unauthorized"));
        };

        let token = self
            .get_bearer_token(registry_host, &bearer, scope, false)
            .await?;

        let token_auth = format!("Bearer {token}");
        let resp2 = self
            .send_with_429_retry(&limit_host, || {
                let mut builder = self.http.get(url.clone());
                if let Some(accept) = accept_header.as_deref() {
                    builder = builder.header(ACCEPT, accept);
                }
                builder.header(AUTHORIZATION, token_auth.clone())
            })
            .await?;
        if resp2.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Registries frequently return short-lived bearer tokens. We cache them for performance,
            // but if a cached token is expired (or otherwise invalid), a retry with a refreshed
            // token restores correctness while keeping the common path fast.
            tracing::debug!(
                registry_host,
                scope,
                "registry bearer token rejected, refreshing and retrying"
            );

            let token = self
                .get_bearer_token(registry_host, &bearer, scope, true)
                .await?;
            let token_auth = format!("Bearer {token}");
            let resp3 = self
                .send_with_429_retry(&limit_host, || {
                    let mut builder = self.http.get(url.clone());
                    if let Some(accept) = accept_header.as_deref() {
                        builder = builder.header(ACCEPT, accept);
                    }
                    builder.header(AUTHORIZATION, token_auth.clone())
                })
                .await?;
            if !resp3.status().is_success() {
                return Err(anyhow::anyhow!(
                    "registry request failed: {}",
                    resp3.status()
                ));
            }
            return Ok(resp3);
        }
        if !resp2.status().is_success() {
            return Err(anyhow::anyhow!(
                "registry request failed: {}",
                resp2.status()
            ));
        }
        Ok(resp2)
    }

    fn limiter_for_host(&self, host: &str) -> Arc<Semaphore> {
        if let Ok(mut guard) = self.host_limiters.lock() {
            return guard
                .entry(host.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(self.options.per_host_concurrency)))
                .clone();
        }
        Arc::new(Semaphore::new(self.options.per_host_concurrency))
    }

    async fn send_with_429_retry<F>(
        &self,
        host: &str,
        mut make_request: F,
    ) -> anyhow::Result<reqwest::Response>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempts_done: usize = 0;

        loop {
            let limiter = self.limiter_for_host(host);
            let _permit = limiter
                .acquire()
                .await
                .map_err(|_| anyhow::anyhow!("registry limiter closed for host {host}"))?;

            let resp = make_request().send().await?;
            if resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Ok(resp);
            }

            if attempts_done >= self.options.retry_max_attempts {
                return Ok(resp);
            }

            let delay = parse_retry_after_delay(resp.headers(), self.options.retry_max_ms)
                .unwrap_or_else(|| {
                    retry_backoff_with_jitter(
                        self.options.retry_base_ms,
                        self.options.retry_max_ms,
                        attempts_done,
                        host,
                    )
                });
            attempts_done = attempts_done.saturating_add(1);

            tracing::warn!(
                registry_host = host,
                retry_attempt = attempts_done,
                backoff_ms = delay.as_millis() as u64,
                "registry rate limited (429); backing off"
            );

            tokio::time::sleep(delay).await;
        }
    }

    async fn get_bearer_token(
        &self,
        registry_host: &str,
        bearer: &BearerAuth,
        scope: &str,
        force_refresh: bool,
    ) -> anyhow::Result<String> {
        let cache_key = format!(
            "{}|{}|{}",
            bearer.realm,
            bearer.service.as_deref().unwrap_or_default(),
            scope
        );
        if let Ok(mut m) = self.token_cache.lock() {
            if force_refresh {
                m.remove(&cache_key);
            } else if let Some(t) = m.get(&cache_key) {
                return Ok(t.clone());
            }
        }

        let mut url = reqwest::Url::parse(&bearer.realm)?;
        {
            let mut qp = url.query_pairs_mut();
            if let Some(service) = bearer.service.as_deref() {
                qp.append_pair("service", service);
            }
            qp.append_pair("scope", scope);
        }

        let mut req = self.http.get(url);
        if let Some((user, pass)) = self
            .docker
            .as_ref()
            .and_then(|d| d.basic_auth(registry_host))
        {
            req = req.basic_auth(user, Some(pass));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            token: Option<String>,
            access_token: Option<String>,
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("token request failed: {}", resp.status()));
        }
        let tr: TokenResponse = resp.json().await?;
        let token = tr
            .token
            .or(tr.access_token)
            .ok_or_else(|| anyhow::anyhow!("token response missing token"))?;

        if let Ok(mut m) = self.token_cache.lock() {
            m.insert(cache_key, token.clone());
        }

        Ok(token)
    }
}

fn registry_api_host(registry: &str) -> &str {
    if registry == "docker.io" {
        "registry-1.docker.io"
    } else {
        registry
    }
}

fn request_limit_host(url: &str, fallback_host: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| fallback_host.to_string())
}

fn parse_retry_after_delay(headers: &reqwest::header::HeaderMap, max_ms: u64) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }

    let cap_ms = max_ms.max(1);

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_millis(
            seconds.saturating_mul(1000).min(cap_ms),
        ));
    }

    if let Ok(at) = time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc2822)
    {
        let now = time::OffsetDateTime::now_utc();
        let delta = at - now;
        let millis = if delta.is_negative() {
            0
        } else {
            delta.whole_milliseconds().try_into().unwrap_or(u64::MAX)
        };
        return Some(Duration::from_millis(millis.min(cap_ms)));
    }

    None
}

fn retry_backoff_with_jitter(base_ms: u64, max_ms: u64, attempt: usize, host: &str) -> Duration {
    let cap_ms = max_ms.max(1);
    let factor = 1u64
        .checked_shl((attempt as u32).min(16))
        .unwrap_or(u64::MAX);
    let raw_ms = base_ms.saturating_mul(factor).min(cap_ms);

    let jitter_cap = (base_ms / 2).max(1);
    let mut hasher = DefaultHasher::new();
    host.hash(&mut hasher);
    attempt.hash(&mut hasher);
    let jitter_ms = hasher.finish() % (jitter_cap + 1);

    Duration::from_millis(raw_ms.saturating_add(jitter_ms).min(cap_ms))
}

#[derive(Clone, Debug)]
struct BearerAuth {
    realm: String,
    service: Option<String>,
}

fn parse_www_authenticate_bearer(header_value: &str) -> Option<BearerAuth> {
    let mut parts = header_value.splitn(2, ' ');
    let scheme = parts.next()?.trim().to_ascii_lowercase();
    let params = parts.next().unwrap_or("").trim();
    if scheme != "bearer" {
        return None;
    }

    let mut realm: Option<String> = None;
    let mut service: Option<String> = None;
    for item in params.split(',') {
        let item = item.trim();
        let (k, v) = item.split_once('=')?;
        let v = v.trim().trim_matches('"');
        match k.trim() {
            "realm" => realm = Some(v.to_string()),
            "service" => service = Some(v.to_string()),
            _ => {}
        }
    }

    Some(BearerAuth {
        realm: realm?,
        service,
    })
}

#[derive(Clone, Debug)]
struct DockerConfig {
    auths: HashMap<String, DockerAuthEntry>,
}

#[derive(Clone, Debug, Deserialize)]
struct DockerAuthEntry {
    auth: Option<String>,
    #[serde(rename = "identitytoken")]
    identity_token: Option<String>,
}

impl DockerConfig {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read docker config {path:?}"))?;
        #[derive(Deserialize)]
        struct Root {
            auths: Option<HashMap<String, DockerAuthEntry>>,
        }
        let root: Root = serde_json::from_str(&text).context("parse docker config json")?;
        let mut auths = HashMap::new();
        for (k, v) in root.auths.unwrap_or_default() {
            auths.insert(normalize_auth_key(&k), v);
        }
        Ok(Self { auths })
    }

    fn basic_auth(&self, registry_host: &str) -> Option<(String, String)> {
        let key = normalize_auth_key(registry_host);
        let entry = self.auths.get(&key)?;

        if let Some(token) = entry.identity_token.as_deref() {
            return Some(("oauth2".to_string(), token.to_string()));
        }

        let auth = entry.auth.as_deref()?;
        let decoded = BASE64.decode(auth).ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (user, pass) = decoded.split_once(':')?;
        Some((user.to_string(), pass.to_string()))
    }
}

fn normalize_auth_key(input: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(input)
        && let Some(host) = url.host_str()
    {
        return normalize_auth_key(host);
    }

    let host = input
        .trim()
        .trim_end_matches('/')
        .trim_end_matches("/v1/")
        .trim_end_matches("/v2/")
        .trim_end_matches("/v1")
        .trim_end_matches("/v2")
        .to_string();

    match host.as_str() {
        "index.docker.io" | "registry-1.docker.io" => "docker.io".to_string(),
        _ => host,
    }
}

pub fn parse_manifest_json(
    body: &str,
    digest: Option<String>,
    host_platform: &str,
) -> anyhow::Result<ManifestInfo> {
    let value: serde_json::Value = serde_json::from_str(body).context("parse manifest json")?;

    let mut arch = Vec::new();
    let mut host_platform_digest_exact: Option<String> = None;
    let mut host_platform_digest_base_matches: Vec<String> = Vec::new();
    let host_base = host_platform
        .split('/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    if let Some(manifests) = value.get("manifests").and_then(|v| v.as_array()) {
        for m in manifests {
            let os = m
                .get("platform")
                .and_then(|p| p.get("os"))
                .and_then(|v| v.as_str());
            let architecture = m
                .get("platform")
                .and_then(|p| p.get("architecture"))
                .and_then(|v| v.as_str());
            let variant = m
                .get("platform")
                .and_then(|p| p.get("variant"))
                .and_then(|v| v.as_str());

            if let (Some(os), Some(architecture)) = (os, architecture) {
                let plat = if let Some(variant) = variant {
                    format!("{os}/{architecture}/{variant}")
                } else {
                    format!("{os}/{architecture}")
                };
                arch.push(plat);
            }

            let digest = m.get("digest").and_then(|v| v.as_str());
            if digest.is_none() {
                continue;
            }
            let digest = digest.unwrap();

            if host_platform_digest_exact.is_none()
                && platform_matches(host_platform, os, architecture, variant)
            {
                host_platform_digest_exact = Some(digest.to_string());
                continue;
            }

            // Best-effort fallback: if the host platform doesn't match exactly (e.g. missing/unknown
            // variant), allow os/arch match ONLY when it is unambiguous. This avoids picking the
            // wrong digest for multi-variant lists like linux/arm/v6 + linux/arm/v7.
            if let (Some(os), Some(architecture)) = (os, architecture) {
                let base = format!("{os}/{architecture}");
                if base == host_base {
                    host_platform_digest_base_matches.push(digest.to_string());
                }
            }
        }
    } else if let (Some(os), Some(architecture)) = (
        value.get("os").and_then(|v| v.as_str()),
        value.get("architecture").and_then(|v| v.as_str()),
    ) {
        arch.push(format!("{os}/{architecture}"));
    }

    arch.sort();
    arch.dedup();

    // Prefer the digest returned by the registry for the requested reference (tag or digest).
    //
    // For multi-arch images, Docker commonly reports the *index/manifest-list* digest in
    // `.RepoDigests`, so aligning on the header digest makes runtime comparisons work.
    let platform_digest = if host_platform_digest_exact.is_some() {
        host_platform_digest_exact
    } else {
        host_platform_digest_base_matches.sort();
        host_platform_digest_base_matches.dedup();
        if host_platform_digest_base_matches.len() == 1 {
            host_platform_digest_base_matches.into_iter().next()
        } else {
            None
        }
    };
    let digest = digest.or(platform_digest.clone());
    Ok(ManifestInfo {
        digest,
        platform_digest,
        arch,
    })
}

#[cfg(test)]
mod http_registry_tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Query, State},
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
        routing::get,
    };
    use serde_json::json;
    use std::collections::HashMap as StdHashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default)]
    struct TestState {
        base: String,
        token_calls: usize,
    }

    #[tokio::test]
    async fn refresh_bearer_token_on_unauthorized() {
        async fn token(
            State(state): State<Arc<Mutex<TestState>>>,
            Query(_q): Query<StdHashMap<String, String>>,
        ) -> impl IntoResponse {
            let mut state = state.lock().unwrap();
            state.token_calls += 1;
            let token = if state.token_calls == 1 {
                // First token is intentionally invalid to exercise the refresh path.
                "bad"
            } else {
                "good"
            };
            (StatusCode::OK, Json(json!({ "token": token })))
        }

        async fn manifest(
            State(state): State<Arc<Mutex<TestState>>>,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            // Only accept the refreshed token.
            let ok = headers
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v == "Bearer good");
            if ok {
                return (StatusCode::OK, "ok").into_response();
            }

            let base = state.lock().unwrap().base.clone();
            let www = format!("Bearer realm=\"{base}/token\",service=\"test\"");
            let mut h = HeaderMap::new();
            h.insert(WWW_AUTHENTICATE, HeaderValue::from_str(&www).unwrap());
            (StatusCode::UNAUTHORIZED, h, "").into_response()
        }

        // Some sandboxed test environments disallow binding sockets. In that case, skip this test
        // rather than failing the suite.
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping network-bound test: {e}");
                return;
            }
            Err(e) => panic!("failed to bind test listener: {e}"),
        };
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");

        let state = Arc::new(Mutex::new(TestState {
            base: base.clone(),
            token_calls: 0,
        }));

        let app = Router::new()
            .route("/token", get(token))
            .route("/v2/testrepo/manifests/latest", get(manifest))
            .with_state(state.clone());

        // Run the server in background.
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = HttpRegistryClient::new(None, HttpRegistryClientOptions::default()).unwrap();
        let scope = "repository:testrepo:pull";
        let url = format!("{base}/v2/testrepo/manifests/latest");

        // First attempt: token endpoint returns a bad token, registry rejects it with 401,
        // client should refresh and succeed.
        let resp = client
            .get_with_auth("example.com", scope, url.clone(), None)
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        // Second attempt: token should be cached (good) and should not hit token endpoint again.
        let resp = client
            .get_with_auth("example.com", scope, url, None)
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        assert_eq!(state.lock().unwrap().token_calls, 2);

        // Stop the server task to avoid it outliving the test runtime.
        server_handle.abort();
    }
}

fn platform_matches(
    host_platform: &str,
    os: Option<&str>,
    architecture: Option<&str>,
    variant: Option<&str>,
) -> bool {
    let (Some(os), Some(architecture)) = (os, architecture) else {
        return false;
    };
    let candidate = if let Some(variant) = variant {
        format!("{os}/{architecture}/{variant}")
    } else {
        format!("{os}/{architecture}")
    };
    candidate == host_platform
}

pub fn host_platform_override(config_value: Option<&str>) -> Option<String> {
    if let Some(v) = config_value {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };

    Some(format!("linux/{arch}"))
}

pub fn compute_arch_match(host_platform: &str, arch: &[String]) -> ArchMatch {
    if arch.is_empty() {
        return ArchMatch::Unknown;
    }
    if arch.iter().any(|p| p == host_platform) {
        return ArchMatch::Match;
    }
    // Best-effort: tolerate missing variant for arm64.
    let host_no_variant = host_platform
        .split('/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    if host_no_variant != host_platform && arch.iter().any(|p| p == &host_no_variant) {
        return ArchMatch::Match;
    }
    ArchMatch::Mismatch
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn parse_image_ref_with_registry() {
        let img = ImageRef::parse("ghcr.io/org/app:5.2").unwrap();
        assert_eq!(img.registry, "ghcr.io");
        assert_eq!(img.name, "org/app");
        assert_eq!(img.reference, "5.2");
    }

    #[test]
    fn parse_image_ref_dockerhub() {
        let img = ImageRef::parse("postgres:16").unwrap();
        assert_eq!(img.registry, "docker.io");
        assert_eq!(img.name, "library/postgres");
        assert_eq!(img.reference, "16");
    }

    #[test]
    fn parse_retry_after_seconds_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));

        let delay = parse_retry_after_delay(&headers, 5_000).unwrap();
        assert_eq!(delay.as_millis(), 2_000);
    }

    #[test]
    fn parse_retry_after_http_date_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        let future = time::OffsetDateTime::now_utc() + time::Duration::seconds(3);
        let value = future
            .format(&time::format_description::well_known::Rfc2822)
            .unwrap();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(&value).unwrap());

        let delay = parse_retry_after_delay(&headers, 10_000).unwrap();
        assert!(delay.as_millis() <= 3_000);
        assert!(delay.as_millis() > 0);
    }

    #[test]
    fn retry_backoff_with_jitter_stays_within_bounds() {
        let delay = retry_backoff_with_jitter(250, 2_000, 2, "registry-1.docker.io");
        assert!(delay.as_millis() >= 1_000);
        assert!(delay.as_millis() <= 2_000);
    }

    #[test]
    fn parse_manifest_list_arch() {
        let json = r#"{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.list.v2+json",
  "manifests": [
    { "digest": "sha256:amd64", "platform": { "architecture": "amd64", "os": "linux" } },
    { "digest": "sha256:arm64", "platform": { "architecture": "arm64", "os": "linux" } }
  ]
}"#;
        let info =
            parse_manifest_json(json, Some("sha256:deadbeef".to_string()), "linux/amd64").unwrap();
        assert_eq!(info.digest.as_deref(), Some("sha256:deadbeef"));
        assert_eq!(info.arch, vec!["linux/amd64", "linux/arm64"]);
    }

    #[test]
    fn parse_manifest_json_selects_exact_variant_digest() {
        let json = r#"{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.list.v2+json",
  "manifests": [
    { "digest": "sha256:armv6", "platform": { "architecture": "arm", "os": "linux", "variant": "v6" } },
    { "digest": "sha256:armv7", "platform": { "architecture": "arm", "os": "linux", "variant": "v7" } }
  ]
}"#;
        let info = parse_manifest_json(json, None, "linux/arm/v7").unwrap();
        assert_eq!(info.digest.as_deref(), Some("sha256:armv7"));
    }

    #[test]
    fn parse_manifest_json_avoids_ambiguous_os_arch_fallback() {
        let json = r#"{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.list.v2+json",
  "manifests": [
    { "digest": "sha256:armv6", "platform": { "architecture": "arm", "os": "linux", "variant": "v6" } },
    { "digest": "sha256:armv7", "platform": { "architecture": "arm", "os": "linux", "variant": "v7" } }
  ]
}"#;
        let info = parse_manifest_json(json, None, "linux/arm").unwrap();
        assert_eq!(info.digest, None);
    }

    #[test]
    fn parse_manifest_json_allows_unambiguous_os_arch_fallback() {
        let json = r#"{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.list.v2+json",
  "manifests": [
    { "digest": "sha256:amd64", "platform": { "architecture": "amd64", "os": "linux" } }
  ]
}"#;
        let info = parse_manifest_json(json, None, "linux/amd64/v3").unwrap();
        assert_eq!(info.digest.as_deref(), Some("sha256:amd64"));
    }

    #[test]
    fn arch_match() {
        let arch = vec!["linux/amd64".to_string(), "linux/arm64".to_string()];
        assert!(matches!(
            compute_arch_match("linux/amd64", &arch),
            ArchMatch::Match
        ));
        assert!(matches!(
            compute_arch_match("linux/ppc64le", &arch),
            ArchMatch::Mismatch
        ));
    }
}
