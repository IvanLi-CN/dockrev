use std::path::PathBuf;

use axum::http::HeaderName;

#[derive(Clone)]
pub struct Config {
    pub app_effective_version: String,
    pub http_addr: String,
    pub db_path: PathBuf,
    pub docker_config_path: Option<PathBuf>,
    pub compose_bin: String,
    pub auth_forward_header_name: HeaderName,
    pub auth_allow_anonymous_in_dev: bool,
    pub self_upgrade_url: String,
    pub dockrev_image_repo: String,
    pub webhook_secret: Option<String>,
    pub host_platform: Option<String>,
    pub discovery_interval_seconds: u64,
    pub discovery_max_actions: u32,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let app_effective_version = match std::env::var("APP_EFFECTIVE_VERSION") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => env!("CARGO_PKG_VERSION").to_string(),
        };

        let http_addr =
            std::env::var("DOCKREV_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:50883".to_string());

        let db_path = std::env::var("DOCKREV_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/dockrev.sqlite3"));

        let docker_config_path = std::env::var("DOCKREV_DOCKER_CONFIG")
            .ok()
            .map(PathBuf::from);

        let compose_bin =
            std::env::var("DOCKREV_COMPOSE_BIN").unwrap_or_else(|_| "docker-compose".to_string());

        let auth_forward_header_name = std::env::var("DOCKREV_AUTH_FORWARD_HEADER_NAME")
            .unwrap_or_else(|_| "X-Forwarded-User".to_string())
            .parse::<HeaderName>()?;

        let auth_allow_anonymous_in_dev = std::env::var("DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV")
            .ok()
            .and_then(|v| parse_bool(&v))
            .unwrap_or(true);

        let self_upgrade_url = match std::env::var("DOCKREV_SELF_UPGRADE_URL") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => "/supervisor/".to_string(),
        };

        let dockrev_image_repo = match std::env::var("DOCKREV_IMAGE_REPO") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => "ghcr.io/ivanli-cn/dockrev".to_string(),
        };

        let webhook_secret = std::env::var("DOCKREV_WEBHOOK_SECRET").ok();
        let host_platform = std::env::var("DOCKREV_HOST_PLATFORM").ok();

        let discovery_interval_seconds = std::env::var("DOCKREV_DISCOVERY_INTERVAL_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(60);
        if discovery_interval_seconds < 10 {
            return Err(anyhow::anyhow!(
                "DOCKREV_DISCOVERY_INTERVAL_SECONDS must be >= 10"
            ));
        }

        let discovery_max_actions = std::env::var("DOCKREV_DISCOVERY_MAX_ACTIONS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(200);

        Ok(Self {
            app_effective_version,
            http_addr,
            db_path,
            docker_config_path,
            compose_bin,
            auth_forward_header_name,
            auth_allow_anonymous_in_dev,
            self_upgrade_url,
            dockrev_image_repo,
            webhook_secret,
            host_platform,
            discovery_interval_seconds,
            discovery_max_actions,
        })
    }
}

fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}
