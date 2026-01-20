#![forbid(unsafe_code)]

mod api;
mod backup;
mod candidates;
mod compose;
mod compose_runner;
mod config;
mod db;
mod docker_runner;
mod error;
mod ids;
mod ignore;
mod notify;
mod registry;
mod runner;
mod state;
mod ui;
mod updater;

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

    let config = config::Config::from_env()?;
    let bind = config.http_addr.clone();
    let db = db::Db::open(&config.db_path).await?;
    let registry = std::sync::Arc::new(registry::HttpRegistryClient::new(
        config.docker_config_path.as_deref(),
    )?);
    let runner = std::sync::Arc::new(runner::TokioCommandRunner);
    let state = state::AppState::new(config, db, registry, runner);
    backup::spawn_cleanup_task(state.clone());
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "dockrev api listening");

    axum::serve(listener, app).await?;
    Ok(())
}
