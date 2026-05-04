use std::{collections::HashMap, path::Path};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub(super) struct BearerAuth {
    pub(super) realm: String,
    pub(super) service: Option<String>,
}

pub(super) fn parse_www_authenticate_bearer(header_value: &str) -> Option<BearerAuth> {
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
pub(super) struct DockerConfig {
    auths: HashMap<String, DockerAuthEntry>,
}

#[derive(Clone, Debug, Deserialize)]
struct DockerAuthEntry {
    auth: Option<String>,
    #[serde(rename = "identitytoken")]
    identity_token: Option<String>,
}

impl DockerConfig {
    pub(super) fn load(path: &Path) -> anyhow::Result<Self> {
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

    pub(super) fn basic_auth(&self, registry_host: &str) -> Option<(String, String)> {
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

pub(super) fn normalize_auth_key(input: &str) -> String {
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
