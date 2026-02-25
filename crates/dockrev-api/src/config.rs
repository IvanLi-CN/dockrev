use std::path::PathBuf;

use axum::http::HeaderName;

pub const FIXED_CHECK_PARALLELISM: usize = 5;
pub const FIXED_REGISTRY_PER_HOST_CONCURRENCY: usize = 5;

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
    pub runtime_scan_interval_seconds: u64,
    pub deploy_check_local_command_timeout_seconds: u64,
    pub registry_per_host_concurrency: usize,
    pub registry_retry_max_attempts: usize,
    pub registry_retry_base_ms: u64,
    pub registry_retry_max_ms: u64,
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

        let runtime_scan_interval_seconds = std::env::var("DOCKREV_RUNTIME_SCAN_INTERVAL_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(600);
        if runtime_scan_interval_seconds < 30 {
            return Err(anyhow::anyhow!(
                "DOCKREV_RUNTIME_SCAN_INTERVAL_SECONDS must be >= 30"
            ));
        }

        let deploy_check_local_command_timeout_seconds =
            std::env::var("DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(12);
        if deploy_check_local_command_timeout_seconds == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS must be >= 1"
            ));
        }

        enforce_fixed_concurrency_env("DOCKREV_CHECK_CONCURRENCY", FIXED_CHECK_PARALLELISM)?;
        enforce_fixed_concurrency_env(
            "DOCKREV_REGISTRY_PER_HOST_CONCURRENCY",
            FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        )?;

        let registry_per_host_concurrency = FIXED_REGISTRY_PER_HOST_CONCURRENCY;

        let registry_retry_max_attempts = std::env::var("DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(3);

        let registry_retry_base_ms = std::env::var("DOCKREV_REGISTRY_RETRY_BASE_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(250);
        if registry_retry_base_ms == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_REGISTRY_RETRY_BASE_MS must be >= 1"
            ));
        }

        let registry_retry_max_ms = std::env::var("DOCKREV_REGISTRY_RETRY_MAX_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(2000);
        if registry_retry_max_ms == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_REGISTRY_RETRY_MAX_MS must be >= 1"
            ));
        }
        if registry_retry_max_ms < registry_retry_base_ms {
            return Err(anyhow::anyhow!(
                "DOCKREV_REGISTRY_RETRY_MAX_MS must be >= DOCKREV_REGISTRY_RETRY_BASE_MS"
            ));
        }

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
            runtime_scan_interval_seconds,
            deploy_check_local_command_timeout_seconds,
            registry_per_host_concurrency,
            registry_retry_max_attempts,
            registry_retry_base_ms,
            registry_retry_max_ms,
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

fn enforce_fixed_concurrency_env(name: &str, fixed_value: usize) -> anyhow::Result<()> {
    let Some(raw_value) = std::env::var_os(name) else {
        return Ok(());
    };

    let value = raw_value.to_string_lossy();
    match value.trim().parse::<usize>() {
        Ok(parsed) if parsed == fixed_value => {
            tracing::warn!(
                env = name,
                value = parsed,
                fixed_value,
                "legacy concurrency env is deprecated and ignored because concurrency is fixed"
            );
            Ok(())
        }
        Ok(parsed) => Err(anyhow::anyhow!(
            "{name}={parsed} is no longer supported; concurrency is fixed at {fixed_value}. Remove this env var."
        )),
        Err(_) => Err(anyhow::anyhow!(
            "{name} has invalid value '{value}' and is no longer supported; concurrency is fixed at {fixed_value}. Remove this env var."
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn enforce_fixed_concurrency_accepts_matching_value() {
        assert!(check_legacy_fixed_value("DOCKREV_CHECK_CONCURRENCY", "5", 5).is_ok());
    }

    #[test]
    fn enforce_fixed_concurrency_rejects_mismatch() {
        let err = check_legacy_fixed_value("DOCKREV_CHECK_CONCURRENCY", "8", 5)
            .expect_err("mismatched legacy value should fail");
        assert!(err.to_string().contains("fixed at 5"));
    }

    #[test]
    fn enforce_fixed_concurrency_rejects_invalid_value() {
        let err = check_legacy_fixed_value("DOCKREV_CHECK_CONCURRENCY", "abc", 5)
            .expect_err("invalid legacy value should fail");
        assert!(err.to_string().contains("invalid value"));
    }

    fn check_legacy_fixed_value(
        name: &str,
        raw_value: &str,
        fixed_value: usize,
    ) -> anyhow::Result<()> {
        match raw_value.trim().parse::<usize>() {
            Ok(parsed) if parsed == fixed_value => Ok(()),
            Ok(parsed) => Err(anyhow::anyhow!(
                "{name}={parsed} is no longer supported; concurrency is fixed at {fixed_value}. Remove this env var."
            )),
            Err(_) => Err(anyhow::anyhow!(
                "{name} has invalid value '{raw_value}' and is no longer supported; concurrency is fixed at {fixed_value}. Remove this env var."
            )),
        }
    }
}
