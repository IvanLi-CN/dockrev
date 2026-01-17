#![forbid(unsafe_code)]

use axum::{Router, routing::get};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dockrev=info,dockrev_api=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new().route("/api/health", get(health));

    let bind = std::env::var("DOCKREV_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:50883".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "dockrev api listening");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
