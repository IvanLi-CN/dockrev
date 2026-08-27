use std::sync::Arc;

use crate::{
    cleanup_scan_runs::CleanupScanRunHub, cleanup_snapshot_worker::CleanupSnapshotWorker,
    config::Config, db::Db, deploy_check_refresh_worker::DeployCheckRefreshWorker,
    job_live_logs::JobLiveLogHub, lifecycle_observer::OperationScopedLifecycleObserver,
    management_events::ManagementEventHub, metrics_store::MetricsStore,
    operational_read_model::OperationalReadModel, registry::RegistryClient,
    resource_usage::RealtimeSamplerHub, runner::CommandRunner, service_logs::ServiceLogHub,
    snapshot_worker::SnapshotWorker,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub metrics: MetricsStore,
    pub operational_reads: OperationalReadModel,
    pub registry: Arc<dyn RegistryClient>,
    pub runner: Arc<dyn CommandRunner>,
    pub snapshot_worker: Arc<SnapshotWorker>,
    pub cleanup_snapshot_worker: Arc<CleanupSnapshotWorker>,
    pub cleanup_scan_runs: Arc<CleanupScanRunHub>,
    pub deploy_check_refresh_worker: Arc<DeployCheckRefreshWorker>,
    pub resource_hub: Arc<RealtimeSamplerHub>,
    pub service_log_hub: Arc<ServiceLogHub>,
    pub job_live_log_hub: Arc<JobLiveLogHub>,
    pub management_events: Arc<ManagementEventHub>,
    pub update_stop_hub: Arc<crate::update_stop::UpdateStopHub>,
    pub lifecycle_observer: Arc<OperationScopedLifecycleObserver>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        db: Db,
        metrics: MetricsStore,
        operational_reads: OperationalReadModel,
        registry: Arc<dyn RegistryClient>,
        runner: Arc<dyn CommandRunner>,
        snapshot_worker: Arc<SnapshotWorker>,
        cleanup_snapshot_worker: Arc<CleanupSnapshotWorker>,
        cleanup_scan_runs: Arc<CleanupScanRunHub>,
        deploy_check_refresh_worker: Arc<DeployCheckRefreshWorker>,
        resource_hub: Arc<RealtimeSamplerHub>,
        service_log_hub: Arc<ServiceLogHub>,
    ) -> Arc<Self> {
        let management_events = db.management_events();
        let lifecycle_observer = Arc::new(
            OperationScopedLifecycleObserver::from_env(db.clone())
                .unwrap_or_else(|_| OperationScopedLifecycleObserver::unavailable(db.clone())),
        );
        Arc::new(Self {
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
            job_live_log_hub: Arc::new(JobLiveLogHub::new()),
            management_events,
            update_stop_hub: Arc::new(crate::update_stop::UpdateStopHub::default()),
            lifecycle_observer,
        })
    }
}
