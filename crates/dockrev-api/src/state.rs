use std::sync::Arc;

use crate::{config::Config, db::Db, registry::RegistryClient, runner::CommandRunner};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub registry: Arc<dyn RegistryClient>,
    pub runner: Arc<dyn CommandRunner>,
}

impl AppState {
    pub fn new(
        config: Config,
        db: Db,
        registry: Arc<dyn RegistryClient>,
        runner: Arc<dyn CommandRunner>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            db,
            registry,
            runner,
        })
    }
}
