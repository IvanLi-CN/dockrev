#![forbid(unsafe_code)]

mod app;
mod config;
mod docker_exec;
mod state_store;

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dockrev_supervisor=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::Config::from_env()?;
    let bind = cfg.http_addr.clone();
    let app_state = Arc::new(app::App::new(cfg).await?);
    let router = app_state.clone().router();

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "dockrev supervisor listening");

    axum::serve(listener, router).await?;
    Ok(())
}
