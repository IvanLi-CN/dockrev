#![forbid(unsafe_code)]

mod api;
mod compose;
mod config;
mod db;
mod error;
mod ids;
mod state;

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
    let state = state::AppState::new(config, db);
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "dockrev api listening");

    axum::serve(listener, app).await?;
    Ok(())
}
