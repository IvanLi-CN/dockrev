use std::path::PathBuf;

use axum::http::HeaderName;

pub const FIXED_CHECK_PARALLELISM: usize = 7;
pub const FIXED_REGISTRY_PER_HOST_CONCURRENCY: usize = 7;

#[derive(Clone)]
pub struct Config {
    pub app_effective_version: String,
    pub http_addr: String,
    pub db_path: PathBuf,
    pub metrics_db_path: PathBuf,
    pub docker_config_path: Option<PathBuf>,
    pub compose_bin: String,
    pub auth_forward_header_name: HeaderName,
    pub auth_group_header_name: HeaderName,
    pub auth_allowed_user: Option<String>,
    pub auth_allowed_group: Option<String>,
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
    pub registry_rate_limit_cooldown_seconds: u64,
    pub update_idempotent_retry_max_attempts: usize,
    pub update_idempotent_retry_base_ms: u64,
    pub update_idempotent_retry_max_ms: u64,
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
        let metrics_db_path = std::env::var("DOCKREV_METRICS_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                db_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join("metrics.sqlite3")
            });
        if database_paths_match(&db_path, &metrics_db_path) {
            return Err(anyhow::anyhow!(
                "DOCKREV_METRICS_DB_PATH must not point to the same file as DOCKREV_DB_PATH"
            ));
        }

        let docker_config_path = std::env::var("DOCKREV_DOCKER_CONFIG")
            .ok()
            .map(PathBuf::from);

        let compose_bin =
            std::env::var("DOCKREV_COMPOSE_BIN").unwrap_or_else(|_| "docker-compose".to_string());

        let auth_forward_header_name = std::env::var("DOCKREV_AUTH_FORWARD_HEADER_NAME")
            .unwrap_or_else(|_| "X-Forwarded-User".to_string())
            .parse::<HeaderName>()?;

        let auth_group_header_name = std::env::var("DOCKREV_AUTH_GROUP_HEADER_NAME")
            .unwrap_or_else(|_| "Remote-Groups".to_string())
            .parse::<HeaderName>()?;

        let auth_allowed_user = std::env::var("DOCKREV_AUTH_ALLOWED_USER")
            .ok()
            .and_then(parse_non_empty);

        let auth_allowed_group = std::env::var("DOCKREV_AUTH_ALLOWED_GROUP")
            .ok()
            .and_then(parse_non_empty);

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

        warn_legacy_ghcr_webhook_interval_env("DOCKREV_GHCR_WEBHOOK_AUDIT_INTERVAL_SECONDS");

        let deploy_check_local_command_timeout_seconds =
            std::env::var("DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(8);
        if deploy_check_local_command_timeout_seconds == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS must be >= 1"
            ));
        }

        warn_legacy_fixed_concurrency_env("DOCKREV_CHECK_CONCURRENCY", FIXED_CHECK_PARALLELISM);
        warn_legacy_fixed_concurrency_env(
            "DOCKREV_REGISTRY_PER_HOST_CONCURRENCY",
            FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        );

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

        let registry_rate_limit_cooldown_seconds =
            std::env::var("DOCKREV_REGISTRY_RATE_LIMIT_COOLDOWN_SECONDS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(6 * 60 * 60);
        if registry_rate_limit_cooldown_seconds == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_REGISTRY_RATE_LIMIT_COOLDOWN_SECONDS must be >= 1"
            ));
        }

        let update_idempotent_retry_max_attempts =
            std::env::var("DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_ATTEMPTS")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(3);
        if update_idempotent_retry_max_attempts == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_ATTEMPTS must be >= 1"
            ));
        }

        let update_idempotent_retry_base_ms =
            std::env::var("DOCKREV_UPDATE_IDEMPOTENT_RETRY_BASE_MS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(300);
        if update_idempotent_retry_base_ms == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_UPDATE_IDEMPOTENT_RETRY_BASE_MS must be >= 1"
            ));
        }

        let update_idempotent_retry_max_ms =
            std::env::var("DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_MS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(3000);
        if update_idempotent_retry_max_ms == 0 {
            return Err(anyhow::anyhow!(
                "DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_MS must be >= 1"
            ));
        }
        if update_idempotent_retry_max_ms < update_idempotent_retry_base_ms {
            return Err(anyhow::anyhow!(
                "DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_MS must be >= DOCKREV_UPDATE_IDEMPOTENT_RETRY_BASE_MS"
            ));
        }

        Ok(Self {
            app_effective_version,
            http_addr,
            db_path,
            metrics_db_path,
            docker_config_path,
            compose_bin,
            auth_forward_header_name,
            auth_group_header_name,
            auth_allowed_user,
            auth_allowed_group,
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
            registry_rate_limit_cooldown_seconds,
            update_idempotent_retry_max_attempts,
            update_idempotent_retry_base_ms,
            update_idempotent_retry_max_ms,
        })
    }
}

fn database_paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    fn normalized(path: &std::path::Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| {
            let candidate = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            };
            candidate
                .components()
                .fold(PathBuf::new(), |mut normalized, component| {
                    use std::path::Component;

                    match component {
                        Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                        Component::RootDir => {
                            normalized.push(std::path::MAIN_SEPARATOR.to_string())
                        }
                        Component::CurDir => {}
                        Component::ParentDir => {
                            normalized.pop();
                        }
                        Component::Normal(part) => normalized.push(part),
                    }
                    normalized
                })
        })
    }

    let left = normalized(left);
    let right = normalized(right);
    if left == right {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        matches!(
            (std::fs::metadata(&left), std::fs::metadata(&right)),
            (Ok(left), Ok(right)) if left.dev() == right.dev() && left.ino() == right.ino()
        )
    }

    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(test)]
mod database_path_tests {
    use super::*;

    #[test]
    fn database_path_comparison_handles_relative_paths() {
        assert!(database_paths_match(
            std::path::Path::new("./data/dockrev.sqlite3"),
            std::path::Path::new("data/dockrev.sqlite3"),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn database_path_comparison_rejects_hard_link_aliases() {
        let root = std::env::temp_dir().join(format!("dockrev-db-path-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).unwrap();
        let primary = root.join("dockrev.sqlite3");
        let metrics = root.join("metrics.sqlite3");
        std::fs::write(&primary, b"sqlite placeholder").unwrap();
        std::fs::hard_link(&primary, &metrics).unwrap();

        assert!(database_paths_match(&primary, &metrics));

        std::fs::remove_file(metrics).unwrap();
        std::fs::remove_file(primary).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

fn warn_legacy_ghcr_webhook_interval_env(name: &str) {
    let Some(raw_value) = std::env::var_os(name) else {
        return;
    };

    let raw_value = raw_value.to_string_lossy();
    match raw_value.trim().parse::<u64>() {
        Ok(parsed) => {
            tracing::warn!(
                env = name,
                value = parsed,
                "legacy ghcr webhook interval env is deprecated and ignored; use settings.schedules.ghcrWebhookAudit.cron"
            );
        }
        Err(_) => {
            tracing::warn!(
                env = name,
                value = raw_value.as_ref(),
                "legacy ghcr webhook interval env has invalid value and is ignored; use settings.schedules.ghcrWebhookAudit.cron"
            );
        }
    }
}

fn parse_non_empty(input: String) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn warn_legacy_fixed_concurrency_env(name: &str, fixed_value: usize) {
    let Some(raw_value) = std::env::var_os(name) else {
        return;
    };

    let raw_value = raw_value.to_string_lossy();
    match parse_legacy_fixed_value(&raw_value) {
        Ok(parsed) if parsed == fixed_value => {
            tracing::warn!(
                env = name,
                value = parsed,
                fixed_value,
                "legacy concurrency env is deprecated and ignored because concurrency is fixed"
            );
        }
        Ok(parsed) => {
            tracing::warn!(
                env = name,
                value = parsed,
                fixed_value,
                "legacy concurrency env override is ignored because concurrency is fixed; remove this env var"
            );
        }
        Err(_) => {
            tracing::warn!(
                env = name,
                value = raw_value.as_ref(),
                fixed_value,
                "legacy concurrency env has invalid value and is ignored because concurrency is fixed; remove this env var"
            );
        }
    }
}

fn parse_legacy_fixed_value(raw_value: &str) -> Result<usize, ()> {
    raw_value.trim().parse::<usize>().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_legacy_fixed_value_accepts_matching_value() {
        assert_eq!(super::parse_legacy_fixed_value("5"), Ok(5));
    }

    #[test]
    fn parse_legacy_fixed_value_accepts_mismatch_value_for_warning_only_path() {
        assert_eq!(super::parse_legacy_fixed_value("8"), Ok(8));
    }

    #[test]
    fn parse_legacy_fixed_value_rejects_invalid_value() {
        assert_eq!(super::parse_legacy_fixed_value("abc"), Err(()));
    }
}
