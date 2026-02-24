use std::sync::Arc;

use crate::{
    config::Config, db::Db, registry::RegistryClient, runner::CommandRunner,
    snapshot_worker::SnapshotWorker, version_inference_worker::VersionInferenceWorker,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub registry: Arc<dyn RegistryClient>,
    pub runner: Arc<dyn CommandRunner>,
    pub snapshot_worker: Arc<SnapshotWorker>,
    pub version_inference_worker: Arc<VersionInferenceWorker>,
}

impl AppState {
    pub fn new(
        config: Config,
        db: Db,
        registry: Arc<dyn RegistryClient>,
        runner: Arc<dyn CommandRunner>,
        snapshot_worker: Arc<SnapshotWorker>,
        version_inference_worker: Arc<VersionInferenceWorker>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            db,
            registry,
            runner,
            snapshot_worker,
            version_inference_worker,
        })
    }
}
