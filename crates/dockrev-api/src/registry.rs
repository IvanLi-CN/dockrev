use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::header::{ACCEPT, AUTHORIZATION, WWW_AUTHENTICATE};
use serde::Deserialize;

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
    pub digest: Option<String>,
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
}

impl HttpRegistryClient {
    pub fn new(docker_config_path: Option<&Path>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().build()?;
        let docker = docker_config_path.and_then(|p| DockerConfig::load(p).ok());
        Ok(Self {
            http,
            docker,
            token_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl RegistryClient for HttpRegistryClient {
    async fn list_tags(&self, image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let scope = format!("repository:{}:pull", image.name);
        let url = format!(
            "https://{}/v2/{}/tags/list",
            registry_api_host(&image.registry),
            image.name
        );
        let resp = self
            .get_with_auth(&image.registry, &scope, url, None)
            .await?;

        #[derive(Deserialize)]
        struct TagsResponse {
            tags: Option<Vec<String>>,
        }

        let body: TagsResponse = resp.json().await?;
        Ok(body.tags.unwrap_or_default())
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
        let mut builder = self.http.get(url.clone());
        if let Some(accept) = accept {
            builder = builder.header(ACCEPT, accept);
        }
        if let Some((user, pass)) = self
            .docker
            .as_ref()
            .and_then(|d| d.basic_auth(registry_host))
        {
            builder = builder.header(
                AUTHORIZATION,
                format!("Basic {}", BASE64.encode(format!("{user}:{pass}"))),
            );
        }

        let resp = builder.send().await?;
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

        let token = self.get_bearer_token(registry_host, &bearer, scope).await?;

        let mut builder2 = self.http.get(url);
        if let Some(accept) = accept {
            builder2 = builder2.header(ACCEPT, accept);
        }
        builder2 = builder2.header(AUTHORIZATION, format!("Bearer {token}"));

        let resp2 = builder2.send().await?;
        if !resp2.status().is_success() {
            return Err(anyhow::anyhow!(
                "registry request failed: {}",
                resp2.status()
            ));
        }
        Ok(resp2)
    }

    async fn get_bearer_token(
        &self,
        registry_host: &str,
        bearer: &BearerAuth,
        scope: &str,
    ) -> anyhow::Result<String> {
        let cache_key = format!(
            "{}|{}|{}",
            bearer.realm,
            bearer.service.as_deref().unwrap_or_default(),
            scope
        );
        if let Ok(m) = self.token_cache.lock()
            && let Some(t) = m.get(&cache_key)
        {
            return Ok(t.clone());
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

    let digest = if host_platform_digest_exact.is_some() {
        host_platform_digest_exact
    } else {
        host_platform_digest_base_matches.sort();
        host_platform_digest_base_matches.dedup();
        if host_platform_digest_base_matches.len() == 1 {
            host_platform_digest_base_matches.into_iter().next()
        } else {
            None
        }
    }
    .or(digest);
    Ok(ManifestInfo { digest, arch })
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
        assert_eq!(info.digest.as_deref(), Some("sha256:amd64"));
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
