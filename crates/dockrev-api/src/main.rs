#![forbid(unsafe_code)]

mod api;
mod backup;
mod compose;
mod compose_runner;
mod config;
mod db;
mod discovery;
mod docker_runner;
mod error;
mod ghcr_webhook_jobs;
mod github;
mod ids;
mod ignore;
mod notify;
mod preflight;
mod registry;
mod resource_usage;
mod runner;
mod runtime_scan;
mod service_check;
mod snapshot_worker;
mod state;
mod ui;
mod updater;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

async fn shutdown_signal(state: std::sync::Arc<state::AppState>) {
    #[cfg(unix)]
    async fn wait_signal() {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }

    #[cfg(not(unix))]
    async fn wait_signal() {
        let _ = tokio::signal::ctrl_c().await;
    }

    wait_signal().await;

    // Best-effort: on container shutdown we may not have much time, but we still want to try
    // to avoid leaving orphaned running jobs behind. Startup recovery is the hard guarantee.
    let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let _ = state
            .db
            .recover_incomplete_jobs(&now, "server_shutdown")
            .await;
    })
    .await;
}

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
        registry::HttpRegistryClientOptions {
            per_host_concurrency: config.registry_per_host_concurrency,
            retry_max_attempts: config.registry_retry_max_attempts,
            retry_base_ms: config.registry_retry_base_ms,
            retry_max_ms: config.registry_retry_max_ms,
        },
    )?);
    let runner = std::sync::Arc::new(runner::TokioCommandRunner);
    let resource_hub = std::sync::Arc::new(resource_usage::RealtimeSamplerHub::new(
        db.clone(),
        runner.clone(),
    ));
    let snapshot_worker = std::sync::Arc::new(snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let state = state::AppState::new(config, db, registry, runner, snapshot_worker, resource_hub);

    // Recover orphaned/incomplete jobs created by a previous process instance.
    // This covers cases where the container was killed or the process panicked mid-job.
    let now = now_rfc3339()?;
    let recovered = state
        .db
        .recover_incomplete_jobs(&now, "server_restart")
        .await?;
    if !recovered.is_empty() {
        tracing::warn!(
            count = recovered.len(),
            "recovered incomplete jobs on startup"
        );
    }
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    state.snapshot_worker.spawn_startup_warmup(&host_platform);
    state.snapshot_worker.spawn_gc_task();

    backup::spawn_cleanup_task(state.clone());
    discovery::spawn_task(state.clone());
    runtime_scan::spawn_task(state.clone());
    ghcr_webhook_jobs::spawn_tasks(state.clone());
    resource_usage::spawn_history_sampler(state.db.clone(), state.runner.clone());
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "dockrev api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await?;
    Ok(())
}
