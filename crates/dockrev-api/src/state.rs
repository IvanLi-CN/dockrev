use std::sync::Arc;

use crate::{config::Config, db::Db, registry::RegistryClient};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub registry: Arc<dyn RegistryClient>,
}

impl AppState {
    pub fn new(config: Config, db: Db, registry: Arc<dyn RegistryClient>) -> Arc<Self> {
        Arc::new(Self {
            config,
            db,
            registry,
        })
    }
}
