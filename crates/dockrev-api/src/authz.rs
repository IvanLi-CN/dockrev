use axum::http::{HeaderMap, HeaderName};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthzMatchKind {
    User,
    Group,
    AnonymousDev,
}

#[derive(Clone, Debug)]
pub struct RequestAuth {
    pub principal: String,
    pub user: Option<String>,
    pub groups: Vec<String>,
    pub avatar_url: Option<String>,
    pub matched_by: AuthzMatchKind,
}

#[derive(Clone, Debug)]
pub struct AuthzFailure {
    pub reason: &'static str,
    pub message: &'static str,
    pub current_user: Option<String>,
    pub current_groups: Vec<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AuthzConfigView {
    pub forward_header_name: String,
    pub group_header_name: String,
    pub allow_anonymous_in_dev: bool,
    pub allowed_user_masked: Option<String>,
    pub allowed_group_masked: Option<String>,
    pub authorization_mode: &'static str,
}

pub fn authorize_request(
    config: &Config,
    headers: &HeaderMap,
) -> Result<RequestAuth, AuthzFailure> {
    let user = read_header_value(headers, &config.auth_forward_header_name);
    let groups = read_groups(headers, &config.auth_group_header_name);
    let avatar_url = read_avatar_url(headers);
    let allowlist_configured =
        config.auth_allowed_user.is_some() || config.auth_allowed_group.is_some();

    if let Some(current_user) = user.clone()
        && config.auth_allowed_user.as_deref() == Some(current_user.as_str())
    {
        return Ok(RequestAuth {
            principal: current_user.clone(),
            user: Some(current_user),
            groups,
            avatar_url,
            matched_by: AuthzMatchKind::User,
        });
    }

    if let Some(allowed_group) = config.auth_allowed_group.as_deref()
        && groups.iter().any(|group| group == allowed_group)
    {
        return Ok(RequestAuth {
            principal: user
                .clone()
                .unwrap_or_else(|| format!("group:{allowed_group}")),
            user,
            groups,
            avatar_url,
            matched_by: AuthzMatchKind::Group,
        });
    }

    if config.auth_allow_anonymous_in_dev && !allowlist_configured {
        let principal = user.clone().unwrap_or_else(|| "anonymous".to_string());
        return Ok(RequestAuth {
            principal,
            user,
            groups,
            avatar_url,
            matched_by: AuthzMatchKind::AnonymousDev,
        });
    }

    if !allowlist_configured {
        return Err(AuthzFailure {
            reason: "authz_config_missing",
            message: "authorization target is not configured",
            current_user: user,
            current_groups: groups,
            avatar_url,
        });
    }

    if user.is_none() && groups.is_empty() {
        return Err(AuthzFailure {
            reason: "identity_missing",
            message: "forward auth identity is missing",
            current_user: None,
            current_groups: Vec::new(),
            avatar_url,
        });
    }

    Err(AuthzFailure {
        reason: "authz_no_match",
        message: "forward auth identity is not allowed",
        current_user: user,
        current_groups: groups,
        avatar_url,
    })
}

pub fn config_view(config: &Config) -> AuthzConfigView {
    AuthzConfigView {
        forward_header_name: config.auth_forward_header_name.to_string(),
        group_header_name: config.auth_group_header_name.to_string(),
        allow_anonymous_in_dev: config.auth_allow_anonymous_in_dev,
        allowed_user_masked: mask_value(config.auth_allowed_user.as_deref()),
        allowed_group_masked: mask_value(config.auth_allowed_group.as_deref()),
        authorization_mode: authorization_mode(config),
    }
}

fn authorization_mode(config: &Config) -> &'static str {
    match (
        config.auth_allowed_user.is_some(),
        config.auth_allowed_group.is_some(),
    ) {
        (true, true) => "user_or_group",
        (true, false) => "user_only",
        (false, true) => "group_only",
        (false, false) => "unconfigured",
    }
}

pub fn mask_value(input: Option<&str>) -> Option<String> {
    let value = input?.trim();
    if value.is_empty() {
        return None;
    }
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 2 {
        return Some("**".to_string());
    }
    if chars.len() <= 4 {
        let first = chars.first().copied().unwrap_or('*');
        let last = chars.last().copied().unwrap_or('*');
        return Some(format!("{first}**{last}"));
    }
    let prefix: String = chars.iter().take(2).collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(format!("{prefix}***{suffix}"))
}

pub fn mask_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| mask_value(Some(value)))
        .collect()
}

fn read_header_value(headers: &HeaderMap, name: &HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn read_avatar_url(headers: &HeaderMap) -> Option<String> {
    [
        "x-forwarded-user-avatar",
        "x-forwarded-user-picture",
        "x-auth-request-user-avatar",
        "x-auth-request-user-picture",
        "x-forwarded-avatar",
    ]
    .iter()
    .find_map(|name| {
        let header_name = HeaderName::from_static(name);
        read_header_value(headers, &header_name).and_then(normalize_avatar_url)
    })
}

fn normalize_avatar_url(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 || trimmed.chars().any(char::is_control) {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://")
        || lower.starts_with("http://")
        || (trimmed.starts_with('/') && !trimmed.starts_with("//"))
    {
        return Some(trimmed.to_string());
    }
    None
}

fn read_groups(headers: &HeaderMap, name: &HeaderName) -> Vec<String> {
    let Some(raw) = read_header_value(headers, name) else {
        return Vec::new();
    };
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config() -> Config {
        Config {
            app_effective_version: "0.1.0".to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            db_path: PathBuf::from(":memory:"),
            metrics_db_path: PathBuf::from("/tmp/dockrev-authz-test-metrics.sqlite3"),
            docker_config_path: None,
            compose_bin: "docker-compose".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: None,
            auth_allowed_group: None,
            auth_allow_anonymous_in_dev: false,
            self_upgrade_url: "/supervisor/".to_string(),
            dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            webhook_secret: None,
            host_platform: None,
            discovery_interval_seconds: 60,
            discovery_max_actions: 200,
            runtime_scan_interval_seconds: 600,
            deploy_check_local_command_timeout_seconds: 12,
            registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
            registry_retry_max_attempts: 3,
            registry_retry_base_ms: 250,
            registry_retry_max_ms: 2000,
            registry_rate_limit_cooldown_seconds: 21600,
            update_idempotent_retry_max_attempts: 3,
            update_idempotent_retry_base_ms: 300,
            update_idempotent_retry_max_ms: 3000,
        }
    }

    #[test]
    fn authorizes_matching_user() {
        let mut config = config();
        config.auth_allowed_user = Some("alice".to_string());
        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-User", "alice".parse().unwrap());
        headers.insert(
            "X-Auth-Request-User-Avatar",
            "https://example.test/avatar/alice.png".parse().unwrap(),
        );

        let auth = authorize_request(&config, &headers).unwrap();
        assert_eq!(auth.principal, "alice");
        assert_eq!(auth.user.as_deref(), Some("alice"));
        assert_eq!(
            auth.avatar_url.as_deref(),
            Some("https://example.test/avatar/alice.png")
        );
        assert_eq!(auth.matched_by, AuthzMatchKind::User);
    }

    #[test]
    fn ignores_unsafe_avatar_header_values() {
        let mut config = config();
        config.auth_allowed_user = Some("alice".to_string());
        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-User", "alice".parse().unwrap());
        headers.insert(
            "X-Forwarded-User-Avatar",
            "javascript:alert(1)".parse().unwrap(),
        );

        let auth = authorize_request(&config, &headers).unwrap();
        assert_eq!(auth.avatar_url, None);
    }

    #[test]
    fn authorizes_matching_group() {
        let mut config = config();
        config.auth_allowed_group = Some("ops".to_string());
        let mut headers = HeaderMap::new();
        headers.insert("Remote-Groups", "dev, ops".parse().unwrap());

        let auth = authorize_request(&config, &headers).unwrap();
        assert_eq!(auth.principal, "group:ops");
        assert_eq!(auth.groups, vec!["dev".to_string(), "ops".to_string()]);
        assert_eq!(auth.matched_by, AuthzMatchKind::Group);
    }

    #[test]
    fn authorizes_when_user_or_group_matches() {
        let mut config = config();
        config.auth_allowed_user = Some("alice".to_string());
        config.auth_allowed_group = Some("ops".to_string());
        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-User", "bob".parse().unwrap());
        headers.insert("Remote-Groups", "ops, qa".parse().unwrap());

        let auth = authorize_request(&config, &headers).unwrap();
        assert_eq!(auth.principal, "bob");
        assert_eq!(auth.user.as_deref(), Some("bob"));
        assert_eq!(auth.matched_by, AuthzMatchKind::Group);
    }

    #[test]
    fn rejects_missing_targets_when_dev_bypass_disabled() {
        let config = config();
        let headers = HeaderMap::new();

        let err = authorize_request(&config, &headers).unwrap_err();
        assert_eq!(err.reason, "authz_config_missing");
    }

    #[test]
    fn allows_anonymous_in_dev() {
        let mut config = config();
        config.auth_allow_anonymous_in_dev = true;
        let headers = HeaderMap::new();

        let auth = authorize_request(&config, &headers).unwrap();
        assert_eq!(auth.principal, "anonymous");
        assert_eq!(auth.user, None);
        assert_eq!(auth.matched_by, AuthzMatchKind::AnonymousDev);
    }

    #[test]
    fn allowlist_disables_anonymous_dev_bypass() {
        let mut config = config();
        config.auth_allowed_user = Some("alice".to_string());
        config.auth_allow_anonymous_in_dev = true;
        let headers = HeaderMap::new();

        let err = authorize_request(&config, &headers).unwrap_err();
        assert_eq!(err.reason, "identity_missing");
    }

    #[test]
    fn config_view_reports_specific_authorization_mode() {
        let mut config = config();
        assert_eq!(config_view(&config).authorization_mode, "unconfigured");

        config.auth_allowed_user = Some("alice".to_string());
        assert_eq!(config_view(&config).authorization_mode, "user_only");

        config.auth_allowed_group = Some("ops".to_string());
        assert_eq!(config_view(&config).authorization_mode, "user_or_group");

        config.auth_allowed_user = None;
        assert_eq!(config_view(&config).authorization_mode, "group_only");
    }
}
