use std::path::PathBuf;

use axum::http::HeaderName;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_addr: String,
    pub base_path: String,

    pub auth_forward_header_name: HeaderName,
    pub auth_group_header_name: HeaderName,
    pub auth_allowed_user: Option<String>,
    pub auth_allowed_group: Option<String>,
    pub auth_allow_anonymous_in_dev: bool,

    pub target_image_repo: String,
    pub target_container_id: Option<String>,

    pub target_compose_project: Option<String>,
    pub target_compose_service: Option<String>,
    pub target_compose_files: Vec<String>,

    pub docker_bin: String,
    pub docker_host: Option<String>,
    pub compose_bin: String,

    pub state_path: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let http_addr = std::env::var("DOCKREV_SUPERVISOR_HTTP_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:50884".to_string());

        let base_path = std::env::var("DOCKREV_SUPERVISOR_BASE_PATH")
            .unwrap_or_else(|_| "/supervisor".to_string());
        let base_path = normalize_base_path(&base_path)?;

        let auth_forward_header_name = std::env::var("DOCKREV_AUTH_FORWARD_HEADER_NAME")
            .unwrap_or_else(|_| "X-Forwarded-User".to_string())
            .parse::<HeaderName>()?;

        let auth_group_header_name = std::env::var("DOCKREV_AUTH_GROUP_HEADER_NAME")
            .unwrap_or_else(|_| "Remote-Groups".to_string())
            .parse::<HeaderName>()?;

        let auth_allowed_user = std::env::var("DOCKREV_AUTH_ALLOWED_USER")
            .ok()
            .and_then(non_empty);
        let auth_allowed_group = std::env::var("DOCKREV_AUTH_ALLOWED_GROUP")
            .ok()
            .and_then(non_empty);
        let auth_allow_anonymous_in_dev = std::env::var("DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV")
            .ok()
            .and_then(|v| parse_bool(&v))
            .unwrap_or(true);

        let target_image_repo = std::env::var("DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO")
            .unwrap_or_else(|_| "ghcr.io/ivanli-cn/dockrev".to_string());
        let target_container_id = std::env::var("DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID")
            .ok()
            .and_then(non_empty);

        let target_compose_project = std::env::var("DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT")
            .ok()
            .and_then(non_empty);
        let target_compose_service = std::env::var("DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE")
            .ok()
            .and_then(non_empty);
        let target_compose_files_raw = std::env::var("DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES")
            .ok()
            .and_then(non_empty);
        let target_compose_files = target_compose_files_raw
            .as_deref()
            .map(parse_csv_paths)
            .unwrap_or_default();

        let docker_bin =
            std::env::var("DOCKREV_SUPERVISOR_DOCKER_BIN").unwrap_or_else(|_| "docker".to_string());
        let docker_host = std::env::var("DOCKREV_SUPERVISOR_DOCKER_HOST")
            .ok()
            .and_then(non_empty);
        let compose_bin = std::env::var("DOCKREV_SUPERVISOR_COMPOSE_BIN")
            .unwrap_or_else(|_| "docker-compose".to_string());

        let state_path = match std::env::var("DOCKREV_SUPERVISOR_STATE_PATH") {
            Ok(value) => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err(anyhow::anyhow!(
                        "DOCKREV_SUPERVISOR_STATE_PATH must be absolute"
                    ));
                }
                let parent = path.parent().ok_or_else(|| {
                    anyhow::anyhow!("DOCKREV_SUPERVISOR_STATE_PATH must have a parent")
                })?;
                if !parent.is_dir() {
                    return Err(anyhow::anyhow!(
                        "DOCKREV_SUPERVISOR_STATE_PATH parent must be a directory: {}",
                        parent.display()
                    ));
                }
                std::fs::read_dir(parent).map_err(|error| {
                    anyhow::anyhow!(
                        "DOCKREV_SUPERVISOR_STATE_PATH parent is not readable: {} ({error})",
                        parent.display()
                    )
                })?;
                path
            }
            Err(_) => PathBuf::from("./data/supervisor/self-upgrade.json"),
        };

        Ok(Self {
            http_addr,
            base_path,
            auth_forward_header_name,
            auth_group_header_name,
            auth_allowed_user,
            auth_allowed_group,
            auth_allow_anonymous_in_dev,
            target_image_repo,
            target_container_id,
            target_compose_project,
            target_compose_service,
            target_compose_files,
            docker_bin,
            docker_host,
            compose_bin,
            state_path,
        })
    }
}

fn non_empty(v: String) -> Option<String> {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_csv_paths(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn normalize_base_path(input: &str) -> anyhow::Result<String> {
    let t = input.trim();
    if t.is_empty() {
        return Err(anyhow::anyhow!(
            "DOCKREV_SUPERVISOR_BASE_PATH must not be empty"
        ));
    }
    if !t.starts_with('/') {
        return Err(anyhow::anyhow!(
            "DOCKREV_SUPERVISOR_BASE_PATH must start with '/'"
        ));
    }
    let out = t.trim_end_matches('/');
    if out.is_empty() {
        return Err(anyhow::anyhow!(
            "DOCKREV_SUPERVISOR_BASE_PATH must not be '/'"
        ));
    }
    Ok(out.to_string())
}

#[cfg(test)]
mod tests {
    use super::non_empty;

    #[test]
    fn non_empty_trims_whitespace() {
        assert_eq!(non_empty("  alice  ".to_string()).as_deref(), Some("alice"));
        assert_eq!(non_empty("   ".to_string()), None);
    }
}
