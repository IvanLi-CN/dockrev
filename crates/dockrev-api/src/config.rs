use std::path::PathBuf;

use axum::http::HeaderName;

#[derive(Clone)]
pub struct Config {
    pub http_addr: String,
    pub db_path: PathBuf,
    pub auth_forward_header_name: HeaderName,
    pub auth_allow_anonymous_in_dev: bool,
    pub webhook_secret: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let http_addr =
            std::env::var("DOCKREV_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:50883".to_string());

        let db_path = std::env::var("DOCKREV_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/dockrev.sqlite3"));

        let auth_forward_header_name = std::env::var("DOCKREV_AUTH_FORWARD_HEADER_NAME")
            .unwrap_or_else(|_| "X-Forwarded-User".to_string())
            .parse::<HeaderName>()?;

        let auth_allow_anonymous_in_dev = std::env::var("DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV")
            .ok()
            .and_then(|v| parse_bool(&v))
            .unwrap_or(true);

        let webhook_secret = std::env::var("DOCKREV_WEBHOOK_SECRET").ok();

        Ok(Self {
            http_addr,
            db_path,
            auth_forward_header_name,
            auth_allow_anonymous_in_dev,
            webhook_secret,
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
