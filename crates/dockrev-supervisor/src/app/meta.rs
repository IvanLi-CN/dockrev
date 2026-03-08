pub(crate) const DEFAULT_REPOSITORY_URL: &str = "https://github.com/IvanLi-CN/dockrev";
pub(crate) const DEFAULT_DEVELOPER_NAME: &str = "Ivan Li";
pub(crate) const DEFAULT_DEVELOPER_URL: &str = "https://github.com/IvanLi-CN";

#[derive(Clone, Debug)]
pub(crate) struct SupervisorMeta {
    pub(crate) version: String,
    pub(crate) repository: String,
    pub(crate) developer_name: String,
    pub(crate) developer_url: String,
    pub(crate) release_url: Option<String>,
}

pub(crate) fn supervisor_meta() -> SupervisorMeta {
    let app_effective_version = std::env::var("APP_EFFECTIVE_VERSION").ok();
    build_supervisor_meta(
        app_effective_version.as_deref(),
        env!("CARGO_PKG_VERSION"),
        option_env!("CARGO_PKG_REPOSITORY"),
        option_env!("CARGO_PKG_AUTHORS"),
        option_env!("CARGO_PKG_HOMEPAGE"),
    )
}

pub(crate) fn build_supervisor_meta(
    app_effective_version: Option<&str>,
    package_version: &str,
    package_repository: Option<&str>,
    package_authors: Option<&str>,
    package_homepage: Option<&str>,
) -> SupervisorMeta {
    let version = trimmed_non_empty(app_effective_version)
        .unwrap_or(package_version)
        .to_string();

    let package_repository = trimmed_non_empty(package_repository);
    let repository = package_repository
        .unwrap_or(DEFAULT_REPOSITORY_URL)
        .to_string();

    let developer_name = parse_first_author(package_authors)
        .or_else(|| {
            if package_repository.is_some() {
                github_owner_from_repo(&repository)
            } else {
                None
            }
        })
        .unwrap_or(DEFAULT_DEVELOPER_NAME.to_string());

    let developer_url = trimmed_non_empty(package_homepage)
        .map(ToString::to_string)
        .or_else(|| {
            if package_repository.is_some() {
                github_owner_profile_url(&repository)
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_DEVELOPER_URL.to_string());

    let release_url = github_release_url(&repository, &version);

    SupervisorMeta {
        version,
        repository,
        developer_name,
        developer_url,
        release_url,
    }
}

pub(crate) fn trimmed_non_empty(input: Option<&str>) -> Option<&str> {
    input.map(str::trim).filter(|s| !s.is_empty())
}

fn parse_first_author(authors: Option<&str>) -> Option<String> {
    let raw = trimmed_non_empty(authors)?;
    for item in raw.split(':') {
        let candidate = item.trim();
        if candidate.is_empty() {
            continue;
        }
        let normalized = candidate
            .split_once('<')
            .map(|(name, _)| name.trim())
            .unwrap_or(candidate);
        if !normalized.is_empty() {
            return Some(normalized.to_string());
        }
    }
    None
}

fn github_owner_from_repo(repo: &str) -> Option<String> {
    let normalized = normalize_github_repo_url(repo)?;
    let without_host = normalized.strip_prefix("https://github.com/")?;
    let owner = without_host.split('/').next()?.trim();
    if owner.is_empty() {
        None
    } else {
        Some(owner.to_string())
    }
}

fn github_owner_profile_url(repo: &str) -> Option<String> {
    let owner = github_owner_from_repo(repo)?;
    Some(format!("https://github.com/{owner}"))
}

fn github_release_url(repo: &str, version: &str) -> Option<String> {
    let normalized_repo = normalize_github_repo_url(repo)?;
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(format!("{normalized_repo}/releases/tag/{version}"))
}

fn normalize_github_repo_url(repo: &str) -> Option<String> {
    let trimmed = repo.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let without_host = without_scheme.strip_prefix("github.com/")?;
    let without_git = without_host.strip_suffix(".git").unwrap_or(without_host);
    let mut parts = without_git.split('/').filter(|s| !s.trim().is_empty());
    let owner = parts.next()?.trim();
    let name = parts.next()?.trim();
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{name}"))
}
