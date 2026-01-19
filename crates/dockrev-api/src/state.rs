use std::sync::Arc;

use crate::{config::Config, db::Db};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Db,
}

impl AppState {
    pub fn new(config: Config, db: Db) -> Arc<Self> {
        Arc::new(Self { config, db })
    }
}
