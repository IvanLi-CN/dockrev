use std::path::PathBuf;

use axum::http::HeaderName;

#[derive(Clone, Debug)]
pub struct Config {
    pub http_addr: String,
    pub base_path: String,

    pub auth_forward_header_name: HeaderName,

    pub target_image_repo: String,
    pub target_container_id: Option<String>,

    pub target_compose_project: Option<String>,
    pub target_compose_service: Option<String>,
    pub target_compose_files: Vec<String>,

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

        let docker_host = std::env::var("DOCKREV_SUPERVISOR_DOCKER_HOST")
            .ok()
            .and_then(non_empty);
        let compose_bin = std::env::var("DOCKREV_SUPERVISOR_COMPOSE_BIN")
            .unwrap_or_else(|_| "docker-compose".to_string());

        let state_path = std::env::var("DOCKREV_SUPERVISOR_STATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/supervisor/self-upgrade.json"));

        Ok(Self {
            http_addr,
            base_path,
            auth_forward_header_name,
            target_image_repo,
            target_container_id,
            target_compose_project,
            target_compose_service,
            target_compose_files,
            docker_host,
            compose_bin,
            state_path,
        })
    }
}

fn non_empty(v: String) -> Option<String> {
    if v.trim().is_empty() { None } else { Some(v) }
}

fn parse_csv_paths(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
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
