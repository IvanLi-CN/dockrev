#![forbid(unsafe_code)]

mod api;
mod authz;
mod auto_update;
mod backup;
mod backup_helper;
mod backup_storage;
mod cleanup;
mod cleanup_scan_runs;
mod cleanup_snapshot_worker;
mod compose;
mod compose_capability;
mod compose_runner;
mod config;
mod cron_expr;
mod db;
mod deploy_check_refresh_worker;
mod discovery;
mod docker_engine;
mod docker_runner;
mod error;
mod ghcr_webhook_jobs;
mod github;
mod ids;
mod ignore;
mod job_live_logs;
mod managed_override;
mod management_events;
mod metrics_store;
mod models;
mod notify;
mod operational_read_model;
mod preflight;
mod registry;
mod repo_link_backfill;
mod resource_usage;
mod runner;
mod runtime_scan;
mod schedules;
mod service_check;
mod service_logs;
mod snapshot_worker;
mod state;
mod ui;
mod update_stop;
mod updater;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

async fn shutdown_signal() {
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
    // Keep running jobs intact. The next process must see their checkpoints so startup recovery
    // can cancel stop-mode helpers, remove partial artifacts, and restore prior service state.
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if backup_helper::maybe_run_from_args().await? {
        return Ok(());
    }
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
    let metrics = metrics_store::MetricsStore::open(&config.metrics_db_path).await?;
    let active_service_ids = db.list_active_service_ids_for_metrics().await?;
    metrics
        .migrate_from_legacy_with_active_services(&db, &active_service_ids)
        .await?;
    let operational_reads =
        operational_read_model::OperationalReadModel::open(&config.db_path).await?;
    let registry = std::sync::Arc::new(registry::HttpRegistryClient::new(
        config.docker_config_path.as_deref(),
        registry::HttpRegistryClientOptions {
            per_host_concurrency: config.registry_per_host_concurrency,
            retry_max_attempts: config.registry_retry_max_attempts,
            retry_base_ms: config.registry_retry_base_ms,
            retry_max_ms: config.registry_retry_max_ms,
            rate_limit_cooldown_seconds: config.registry_rate_limit_cooldown_seconds,
        },
    )?);
    let runner = std::sync::Arc::new(runner::TokioCommandRunner);
    let docker_engine = docker_engine::DockerEngineClient::from_env()?;
    let resource_sampling =
        resource_usage::ResourceSamplingCoordinator::with_docker_engine(docker_engine);
    let resource_hub = std::sync::Arc::new(resource_usage::RealtimeSamplerHub::with_coordinator(
        db.clone(),
        resource_sampling.clone(),
    ));
    let service_log_hub =
        std::sync::Arc::new(service_logs::ServiceLogHub::new(db.clone(), runner.clone()));
    let snapshot_worker = std::sync::Arc::new(snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let cleanup_snapshot_worker = std::sync::Arc::new(
        cleanup_snapshot_worker::CleanupSnapshotWorker::new(db.clone(), runner.clone()),
    );
    let cleanup_scan_runs = std::sync::Arc::new(cleanup_scan_runs::CleanupScanRunHub::new());
    let deploy_check_refresh_worker =
        std::sync::Arc::new(deploy_check_refresh_worker::DeployCheckRefreshWorker::new(
            db.clone(),
            runner.clone(),
            config.clone(),
        ));
    let state = state::AppState::new(
        config,
        db,
        metrics,
        operational_reads,
        registry,
        runner,
        snapshot_worker,
        cleanup_snapshot_worker,
        cleanup_scan_runs,
        deploy_check_refresh_worker,
        resource_hub,
        service_log_hub,
    );

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
        backup::recover_interrupted_backups(state.as_ref(), &recovered).await?;
    }
    let host_platform = registry::host_platform_override(state.config.host_platform.as_deref())
        .unwrap_or_else(|| "linux/amd64".to_string());
    state.snapshot_worker.spawn_startup_warmup(&host_platform);
    state.snapshot_worker.spawn_periodic_refresh(&host_platform);
    state.snapshot_worker.spawn_gc_task();

    backup::spawn_cleanup_task(state.clone());
    if let Err(err) = discovery::run_scan(state.as_ref()).await {
        // A failed Docker enumeration must leave discovery state untouched. The
        // periodic task below will retry without turning a transient failure into
        // an archive operation.
        tracing::warn!(error = %err, "startup discovery scan failed");
    }
    discovery::spawn_task(state.clone());
    runtime_scan::spawn_task(state.clone());
    ghcr_webhook_jobs::spawn_tasks(state.clone());
    repo_link_backfill::spawn_tasks(state.clone());
    schedules::spawn_tasks(state.clone());
    auto_update::spawn_tasks(state.clone());
    resource_usage::spawn_history_sampler(
        state.db.clone(),
        state.metrics.clone(),
        resource_sampling,
    );
    if let Err(err) = repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref()).await {
        tracing::warn!(error = %err, "failed to enqueue startup repo link backfill");
    }
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(bind = %bind, "dockrev api listening");

    // Recovery owns only the services recorded before a pre-apply backup stopped them. It is
    // deliberately detached from startup so a failed restore cannot prevent Dockrev serving.
    tokio::spawn(api::recover_interrupted_update_backups(state.clone()));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
