use anyhow::Context as _;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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
}

impl GitHubClient {
    pub fn new(pat: &str) -> anyhow::Result<Self> {
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
        let auth = format!("Bearer {}", pat.trim());
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth)?);

        Ok(Self {
            client: reqwest::Client::builder()
                .default_headers(headers.clone())
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .context("build reqwest client")?,
            base_url: Url::parse("https://api.github.com/")?,
            headers,
        })
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<T> {
        let url = self.base_url.join(path)?;
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
        serde_json::from_str(&text).context("decode github json")
    }

    async fn request_empty(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let url = self.base_url.join(path)?;
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
        Ok(())
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
                let user_path = format!("users/{owner}/repos");
                self.paginated_get::<GitHubRepo>(&user_path)
                    .await
                    .with_context(|| format!("list repos failed (org_err={org_err})"))
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
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubWebhook {
    pub id: i64,
    pub active: bool,
    pub events: Vec<String>,
    pub config: GitHubWebhookConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubWebhookConfig {
    pub url: Option<String>,
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
