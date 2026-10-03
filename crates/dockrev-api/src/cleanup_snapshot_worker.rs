use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::Mutex;

use crate::{
    cleanup, db::Db, docker_engine::DockerEngineClient, now_rfc3339, runner::CommandRunner,
};

pub const CLEANUP_SNAPSHOT_KEY: &str = "aggressive_all";
pub const CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS: u64 = 800;
pub const CLEANUP_CONFIRM_MAX_AGE_SECONDS: i64 = 300;

#[derive(Clone)]
pub struct CleanupSnapshotWorker {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    docker_engine: Option<DockerEngineClient>,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    snapshot_gate: Arc<Mutex<()>>,
}

impl CleanupSnapshotWorker {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            db,
            runner,
            docker_engine: None,
            running: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
            snapshot_gate: Arc::new(Mutex::new(())),
        }
    }

    pub fn with_docker_engine(mut self, docker_engine: DockerEngineClient) -> Self {
        self.docker_engine = Some(docker_engine);
        self
    }

    pub async fn build_inventory_snapshot_with_progress(
        &self,
        on_partial: impl FnMut(crate::api::types::CleanupInventorySnapshot) + Send,
    ) -> anyhow::Result<crate::api::types::CleanupInventorySnapshot> {
        let image_unique_sizes = match &self.docker_engine {
            Some(docker_engine) => match docker_engine.image_disk_usage().await {
                Ok(usage) => Some(cleanup::image_unique_sizes_from_system_df_json(&usage)),
                Err(error) => {
                    tracing::warn!(%error, "Docker image disk usage unavailable; image estimates will be unknown");
                    None
                }
            },
            None => None,
        };
        cleanup::build_inventory_snapshot_with_image_unique_sizes(
            self.db.clone(),
            self.runner.clone(),
            image_unique_sizes,
            on_partial,
        )
        .await
    }

    pub async fn enqueue(&self) -> bool {
        self.pending.store(true, Ordering::SeqCst);
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run_loop().await;
        });
        true
    }

    pub async fn enqueue_if_snapshot_unchanged(
        &self,
        observed_snapshot_json: &str,
    ) -> anyhow::Result<bool> {
        let _snapshot_guard = self.snapshot_gate.lock().await;
        if let Some(current) = self
            .db
            .get_cleanup_inventory_snapshot(CLEANUP_SNAPSHOT_KEY)
            .await?
            && current.snapshot_json != observed_snapshot_json
        {
            return Ok(false);
        }
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(false);
        }
        self.pending.store(true, Ordering::SeqCst);
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run_loop().await;
        });
        Ok(true)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub async fn last_error(&self) -> Option<String> {
        self.last_error.lock().await.clone()
    }

    #[cfg(test)]
    pub async fn set_last_error_for_test(&self, value: Option<String>) {
        *self.last_error.lock().await = value;
    }

    async fn run_loop(self) {
        loop {
            self.pending.store(false, Ordering::SeqCst);
            let result = self.refresh_once().await;
            let mut last_error = self.last_error.lock().await;
            *last_error = result.err().map(|err| err.to_string());
            drop(last_error);

            let snapshot_guard = self.snapshot_gate.lock().await;
            if self.pending.swap(false, Ordering::SeqCst) {
                drop(snapshot_guard);
                continue;
            }

            self.running.store(false, Ordering::SeqCst);
            if self.pending.load(Ordering::SeqCst)
                && self
                    .running
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                drop(snapshot_guard);
                continue;
            }
            drop(snapshot_guard);
            break;
        }
    }

    async fn refresh_once(&self) -> anyhow::Result<()> {
        let snapshot = self.build_inventory_snapshot_with_progress(|_| {}).await?;
        let now = now_rfc3339()?;
        let checked_at = snapshot.scanned_at.clone();
        let snapshot_json = serde_json::to_string(&snapshot)?;
        self.db
            .upsert_cleanup_inventory_snapshot(
                CLEANUP_SNAPSHOT_KEY,
                &snapshot_json,
                &checked_at,
                &now,
            )
            .await?;
        Ok(())
    }
}

pub fn cleanup_snapshot_is_fresh(checked_at: &str, now: time::OffsetDateTime) -> bool {
    let Ok(checked_at) =
        time::OffsetDateTime::parse(checked_at, &time::format_description::well_known::Rfc3339)
    else {
        return false;
    };
    (now - checked_at) <= time::Duration::seconds(CLEANUP_CONFIRM_MAX_AGE_SECONDS)
}

#[allow(dead_code)]
fn _tick_hint() -> Duration {
    Duration::from_millis(CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS)
}
