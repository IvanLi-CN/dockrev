use anyhow::Context as _;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Owner { owner: String },
    Repo { owner: String, repo: String },
}

pub fn parse_target_input(input: &str) -> anyhow::Result<TargetKind> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(anyhow::anyhow!("input is empty"));
    }

    if raw.starts_with("http://") || raw.starts_with("https://") {
        let url = Url::parse(raw).context("invalid url")?;
        let host = url.host_str().unwrap_or_default();
        if host != "github.com" && host != "www.github.com" {
            return Err(anyhow::anyhow!("unsupported host: {host}"));
        }
        let segments: Vec<&str> = url
            .path_segments()
            .map(|it| it.filter(|s| !s.is_empty()).collect())
            .unwrap_or_else(Vec::new);
        match segments.as_slice() {
            ["orgs", owner, ..] => Ok(TargetKind::Owner {
                owner: owner.to_string(),
            }),
            [owner] => Ok(TargetKind::Owner {
                owner: owner.to_string(),
            }),
            [owner, repo, ..] => Ok(TargetKind::Repo {
                owner: owner.to_string(),
                repo: repo.trim_end_matches(".git").to_string(),
            }),
            _ => Err(anyhow::anyhow!("unrecognized github url path")),
        }
    } else if raw.starts_with("git@github.com:") {
        let rest = raw.trim_start_matches("git@github.com:");
        let rest = rest.trim_end_matches(".git");
        let mut parts = rest.split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();
        if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
            return Err(anyhow::anyhow!("unrecognized git ssh url"));
        }
        Ok(TargetKind::Repo {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    } else {
        let raw = raw.trim_end_matches(".git");
        let mut parts = raw.split('/');
        let a = parts.next().unwrap_or_default();
        let b = parts.next();
        let c = parts.next();
        match (a, b, c) {
            (owner, None, None) if !owner.is_empty() => Ok(TargetKind::Owner {
                owner: owner.to_string(),
            }),
            (owner, Some(repo), None) if !owner.is_empty() && !repo.is_empty() => {
                Ok(TargetKind::Repo {
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                })
            }
            _ => Err(anyhow::anyhow!("unrecognized target input")),
        }
    }
}

#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    base_url: Url,
    headers: HeaderMap,
    auth_mode: GitHubAuthMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitHubAuthMode {
    Pat,
    Anonymous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubPage<T> {
    pub items: Vec<T>,
    pub has_next: bool,
}

impl GitHubClient {
    pub fn new(pat: &str) -> anyhow::Result<Self> {
        Self::new_with_optional_pat(Some(pat), Url::parse("https://api.github.com/")?)
    }

    pub fn new_anonymous() -> anyhow::Result<Self> {
        Self::new_with_optional_pat(None, Url::parse("https://api.github.com/")?)
    }

    fn new_with_optional_pat(pat: Option<&str>, base_url: Url) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("dockrev (github packages webhook)"),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        let auth_mode = if let Some(token) = pat.map(str::trim).filter(|value| !value.is_empty()) {
            let auth = format!("Bearer {token}");
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth)?);
            GitHubAuthMode::Pat
        } else {
            GitHubAuthMode::Anonymous
        };

        Ok(Self {
            client: reqwest::Client::builder()
                .default_headers(headers.clone())
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .context("build reqwest client")?,
            base_url,
            headers,
            auth_mode,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_with_base_url(pat: Option<&str>, base_url: Url) -> anyhow::Result<Self> {
        Self::new_with_optional_pat(pat, base_url)
    }

    pub fn auth_mode(&self) -> GitHubAuthMode {
        self.auth_mode
    }

    pub fn clone_as_anonymous(&self) -> anyhow::Result<Self> {
        Self::new_with_optional_pat(None, self.base_url.clone())
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<T> {
        let url = self.base_url.join(path)?;
        self.request_json_url(method, url, body).await
    }

    async fn request_empty(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let url = self.base_url.join(path)?;
        let text = self.request_text_url(method, url, body).await?;
        if !text.is_empty() {
            return Ok(());
        }
        Ok(())
    }

    async fn request_json_url<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<T> {
        let text = self.request_text_url(method, url, body).await?;
        serde_json::from_str(&text).context("decode github json")
    }

    async fn request_text_url(
        &self,
        method: reqwest::Method,
        url: Url,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<String> {
        let mut req = self.client.request(method, url);
        req = req.headers(self.headers.clone());
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow::anyhow!("github http {}: {}", status, text));
        }
        Ok(text)
    }

    pub async fn list_owner_repos(&self, owner: &str) -> anyhow::Result<Vec<GitHubRepo>> {
        let owner = owner.trim();
        if owner.is_empty() {
            return Err(anyhow::anyhow!("owner is empty"));
        }

        let org_path = format!("orgs/{owner}/repos");
        match self.paginated_get::<GitHubRepo>(&org_path).await {
            Ok(v) => Ok(v),
            Err(org_err) => {
                let mut out = Vec::<GitHubRepo>::new();
                let mut seen = HashSet::<String>::new();

                // `GET /users/{owner}/repos` never includes private repositories, even when using
                // a PAT with `repo` scope. For "user" targets, also try `GET /user/repos` and
                // filter to repos owned by the requested login.
                //
                // Notes:
                // - `GET /user/repos` only returns private repos for the authenticated user, so
                //   this is best-effort for arbitrary `owner` values.
                // - We keep the public listing as a baseline to cover non-self owners.
                let self_path = "user/repos?visibility=all&affiliation=owner";
                let self_repos = self
                    .paginated_get::<GitHubRepoWithOwner>(self_path)
                    .await
                    .ok()
                    .unwrap_or_default();
                for r in self_repos {
                    if r.owner.login.eq_ignore_ascii_case(owner) {
                        let key = r.full_name.to_ascii_lowercase();
                        if seen.insert(key) {
                            out.push(GitHubRepo {
                                full_name: r.full_name,
                                is_private: r.is_private,
                                pushed_at: r.pushed_at,
                                updated_at: r.updated_at,
                            });
                        }
                    }
                }

                let user_path = format!("users/{owner}/repos");
                let public = self
                    .paginated_get::<GitHubRepo>(&user_path)
                    .await
                    .with_context(|| format!("list repos failed (org_err={org_err})"))?;
                for r in public {
                    let key = r.full_name.to_ascii_lowercase();
                    if seen.insert(key) {
                        out.push(r);
                    }
                }

                out.sort_by_key(|r| r.full_name.to_ascii_lowercase());
                Ok(out)
            }
        }
    }

    pub async fn get_authenticated_user_login(&self) -> anyhow::Result<String> {
        let user = self
            .request_json::<GitHubAuthenticatedUser>(reqwest::Method::GET, "user", None)
            .await?;
        Ok(user.login)
    }

    pub async fn list_owner_container_package_names(
        &self,
        owner: &str,
        authenticated_login: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let owner = owner.trim();
        if owner.is_empty() {
            return Err(anyhow::anyhow!("owner is empty"));
        }

        let is_self_owner =
            authenticated_login.is_some_and(|login| login.eq_ignore_ascii_case(owner));
        let mut out = Vec::<String>::new();
        let mut seen = HashSet::<String>::new();

        if is_self_owner {
            let self_path = "user/packages?package_type=container&visibility=all";
            for pkg in self
                .paginated_get::<GitHubPackageSummary>(self_path)
                .await?
            {
                let key = pkg.name.to_ascii_lowercase();
                if seen.insert(key) {
                    out.push(pkg.name);
                }
            }
            out.sort_by_key(|name| name.to_ascii_lowercase());
            return Ok(out);
        }

        let org_path = format!("orgs/{owner}/packages?package_type=container");
        match self.paginated_get::<GitHubPackageSummary>(&org_path).await {
            Ok(packages) => {
                for pkg in packages {
                    let key = pkg.name.to_ascii_lowercase();
                    if seen.insert(key) {
                        out.push(pkg.name);
                    }
                }
                out.sort_by_key(|name| name.to_ascii_lowercase());
                Ok(out)
            }
            Err(org_err) => {
                let user_path = format!("users/{owner}/packages?package_type=container");
                let packages = self
                    .paginated_get::<GitHubPackageSummary>(&user_path)
                    .await
                    .with_context(|| {
                        format!("list container packages failed (org_err={org_err})")
                    })?;
                for pkg in packages {
                    let key = pkg.name.to_ascii_lowercase();
                    if seen.insert(key) {
                        out.push(pkg.name);
                    }
                }
                out.sort_by_key(|name| name.to_ascii_lowercase());
                Ok(out)
            }
        }
    }

    pub async fn list_repo_hooks(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Vec<GitHubWebhook>> {
        let path = format!("repos/{owner}/{repo}/hooks");
        self.paginated_get::<GitHubWebhook>(&path).await
    }

    pub async fn create_repo_hook(
        &self,
        owner: &str,
        repo: &str,
        req: &CreateWebhookRequest<'_>,
    ) -> anyhow::Result<GitHubWebhook> {
        let path = format!("repos/{owner}/{repo}/hooks");
        let body = serde_json::to_value(req)?;
        self.request_json(reqwest::Method::POST, &path, Some(body))
            .await
    }

    pub async fn update_repo_hook(
        &self,
        owner: &str,
        repo: &str,
        hook_id: i64,
        req: &UpdateWebhookRequest<'_>,
    ) -> anyhow::Result<GitHubWebhook> {
        let path = format!("repos/{owner}/{repo}/hooks/{hook_id}");
        let body = serde_json::to_value(req)?;
        self.request_json(reqwest::Method::PATCH, &path, Some(body))
            .await
    }

    pub async fn delete_repo_hook(
        &self,
        owner: &str,
        repo: &str,
        hook_id: i64,
    ) -> anyhow::Result<()> {
        let path = format!("repos/{owner}/{repo}/hooks/{hook_id}");
        self.request_empty(reqwest::Method::DELETE, &path, None)
            .await?;
        Ok(())
    }

    pub async fn list_releases_page(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
        per_page: u32,
    ) -> anyhow::Result<GitHubPage<GitHubRelease>> {
        let owner = owner.trim();
        let repo = repo.trim();
        if owner.is_empty() || repo.is_empty() {
            return Err(anyhow::anyhow!("owner/repo is empty"));
        }
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let url = self.base_url.join(&format!(
            "repos/{owner}/{repo}/releases?per_page={per_page}&page={page}"
        ))?;
        let resp = self
            .client
            .request(reqwest::Method::GET, url)
            .headers(self.headers.clone())
            .send()
            .await?;
        let status = resp.status();
        let has_next = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_next_link)
            .is_some();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow::anyhow!("github http {}: {}", status, text));
        }
        Ok(GitHubPage {
            items: serde_json::from_str(&text).context("decode github json")?,
            has_next,
        })
    }

    pub async fn get_release_by_tag(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
    ) -> anyhow::Result<GitHubRelease> {
        let owner = owner.trim();
        let repo = repo.trim();
        let tag = tag.trim();
        if owner.is_empty() || repo.is_empty() || tag.is_empty() {
            return Err(anyhow::anyhow!("owner/repo/tag is empty"));
        }
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid github base url"))?;
            segments.pop_if_empty();
            segments.extend(["repos", owner, repo, "releases", "tags", tag]);
        }
        self.request_json_url(reqwest::Method::GET, url, None).await
    }

    async fn paginated_get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<Vec<T>> {
        let mut out = Vec::new();
        let mut next: Option<Url> = Some({
            let mut url = self.base_url.join(path)?;
            {
                let mut qp = url.query_pairs_mut();
                qp.append_pair("per_page", "100");
                qp.append_pair("page", "1");
            }
            url
        });

        while let Some(url) = next.take() {
            let resp = self
                .client
                .request(reqwest::Method::GET, url.clone())
                .headers(self.headers.clone())
                .send()
                .await?;
            let status = resp.status();
            let link = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(anyhow::anyhow!("github http {}: {}", status, text));
            }
            let mut page: Vec<T> = serde_json::from_str(&text).context("decode github json")?;
            out.append(&mut page);
            next = link
                .and_then(|l| parse_next_link(&l))
                .and_then(|u| Url::parse(&u).ok());
        }

        Ok(out)
    }
}

fn parse_next_link(link_header: &str) -> Option<String> {
    for part in link_header.split(',') {
        let part = part.trim();
        if !part.contains("rel=\"next\"") {
            continue;
        }
        let start = part.find('<')?;
        let end = part.find('>')?;
        if end <= start + 1 {
            continue;
        }
        return Some(part[start + 1..end].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::OriginalUri, response::IntoResponse, routing::get};
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn parse_next_link_extracts_next() {
        let link = "<https://api.github.com/organizations/1/repos?per_page=100&page=2>; rel=\"next\", <https://api.github.com/organizations/1/repos?per_page=100&page=4>; rel=\"last\"";
        assert_eq!(
            parse_next_link(link).as_deref(),
            Some("https://api.github.com/organizations/1/repos?per_page=100&page=2")
        );
    }

    #[test]
    fn parse_next_link_returns_none_when_no_next() {
        let link =
            "<https://api.github.com/organizations/1/repos?per_page=100&page=4>; rel=\"last\"";
        assert_eq!(parse_next_link(link), None);
    }

    #[test]
    fn parse_target_input_orgs_profile_url_is_owner() {
        assert_eq!(
            parse_target_input("https://github.com/orgs/acme").unwrap(),
            TargetKind::Owner {
                owner: "acme".to_string()
            }
        );
    }

    #[test]
    fn parse_target_input_orgs_profile_url_with_suffix_is_owner() {
        assert_eq!(
            parse_target_input("https://github.com/orgs/acme/people").unwrap(),
            TargetKind::Owner {
                owner: "acme".to_string()
            }
        );
    }

    #[tokio::test]
    async fn list_owner_container_package_names_uses_user_packages_for_self_owner() {
        async fn user() -> Json<serde_json::Value> {
            Json(json!({ "login": "ivanli-cn" }))
        }

        async fn user_packages() -> Json<serde_json::Value> {
            Json(json!([
                { "name": "dockrev" },
                { "name": "dockrev-supervisor" }
            ]))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/user", get(user))
                    .route("/user/packages", get(user_packages)),
            )
            .await
            .unwrap();
        });

        let client = GitHubClient::new_with_base_url(
            Some("test-token"),
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let login = client.get_authenticated_user_login().await.unwrap();
        let packages = client
            .list_owner_container_package_names("IvanLi-CN", Some(&login))
            .await
            .unwrap();

        assert_eq!(
            packages,
            vec!["dockrev".to_string(), "dockrev-supervisor".to_string()]
        );
    }

    #[tokio::test]
    async fn list_releases_page_reports_has_next() {
        async fn releases() -> impl IntoResponse {
            (
                [(
                    "link",
                    "<http://127.0.0.1/releases?page=2&per_page=2>; rel=\"next\"",
                )],
                Json(json!([
                    {
                        "id": 101,
                        "tag_name": "v1.2.0",
                        "name": "v1.2.0",
                        "body": "release notes",
                        "html_url": "https://github.com/acme/repo/releases/tag/v1.2.0",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-04-07T00:22:00Z",
                        "created_at": "2026-04-07T00:20:00Z"
                    }
                ])),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/repos/acme/repo/releases", get(releases)),
            )
            .await
            .unwrap();
        });

        let client =
            GitHubClient::new_with_base_url(None, Url::parse(&format!("http://{addr}/")).unwrap())
                .unwrap();
        let page = client
            .list_releases_page("acme", "repo", 1, 2)
            .await
            .unwrap();
        assert!(page.has_next);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].tag_name, "v1.2.0");
        assert_eq!(client.auth_mode(), GitHubAuthMode::Anonymous);
    }

    #[tokio::test]
    async fn get_release_by_tag_fetches_exact_tag() {
        async fn release() -> Json<serde_json::Value> {
            Json(json!({
                "id": 102,
                "tag_name": "1.39.5",
                "name": "1.39.5",
                "body": null,
                "html_url": "https://github.com/acme/repo/releases/tag/1.39.5",
                "draft": false,
                "prerelease": false,
                "published_at": "2026-04-07T00:37:00Z",
                "created_at": "2026-04-07T00:30:00Z"
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/repos/acme/repo/releases/tags/1.39.5", get(release)),
            )
            .await
            .unwrap();
        });

        let client = GitHubClient::new_with_base_url(
            Some("test-token"),
            Url::parse(&format!("http://{addr}/")).unwrap(),
        )
        .unwrap();
        let release = client
            .get_release_by_tag("acme", "repo", "1.39.5")
            .await
            .unwrap();
        assert_eq!(release.id, 102);
        assert_eq!(client.auth_mode(), GitHubAuthMode::Pat);
    }

    #[tokio::test]
    async fn get_release_by_tag_encodes_slash_in_tag() {
        let seen_paths = Arc::new(Mutex::new(Vec::<String>::new()));

        async fn release(
            OriginalUri(uri): OriginalUri,
            axum::extract::State(seen_paths): axum::extract::State<Arc<Mutex<Vec<String>>>>,
        ) -> Json<serde_json::Value> {
            seen_paths.lock().unwrap().push(uri.path().to_string());
            Json(json!({
                "id": 103,
                "tag_name": "release/2026.04",
                "name": "release/2026.04",
                "body": null,
                "html_url": "https://github.com/acme/repo/releases/tag/release/2026.04",
                "draft": false,
                "prerelease": false,
                "published_at": "2026-04-07T00:37:00Z",
                "created_at": "2026-04-07T00:30:00Z"
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/repos/acme/repo/releases/tags/{*tag}", get(release))
            .with_state(seen_paths.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client =
            GitHubClient::new_with_base_url(None, Url::parse(&format!("http://{addr}/")).unwrap())
                .unwrap();
        let release = client
            .get_release_by_tag("acme", "repo", "release/2026.04")
            .await
            .unwrap();

        assert_eq!(release.id, 103);
        assert_eq!(
            seen_paths.lock().unwrap().as_slice(),
            ["/repos/acme/repo/releases/tags/release%2F2026.04"]
        );
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
    #[serde(default, rename = "private")]
    pub is_private: bool,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubRepoWithOwner {
    pub full_name: String,
    #[serde(default, rename = "private")]
    pub is_private: bool,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub owner: GitHubRepoOwner,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubRepoOwner {
    pub login: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubAuthenticatedUser {
    pub login: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubPackageSummary {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubWebhook {
    pub id: i64,
    #[allow(dead_code)]
    pub active: bool,
    pub events: Vec<String>,
    pub config: GitHubWebhookConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubWebhookConfig {
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GitHubRelease {
    pub id: i64,
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateWebhookRequest<'a> {
    pub name: &'a str,
    pub active: bool,
    pub events: Vec<&'a str>,
    pub config: CreateWebhookConfig<'a>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateWebhookConfig<'a> {
    pub url: &'a str,
    pub content_type: &'a str,
    pub secret: &'a str,
    pub insecure_ssl: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateWebhookRequest<'a> {
    pub active: bool,
    pub events: Vec<&'a str>,
    pub config: UpdateWebhookConfig<'a>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateWebhookConfig<'a> {
    pub url: &'a str,
    pub content_type: &'a str,
    pub secret: &'a str,
    pub insecure_ssl: &'a str,
}
