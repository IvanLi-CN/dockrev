use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::Mutex;

use crate::{cleanup, db::Db, now_rfc3339, runner::CommandRunner};

pub const CLEANUP_SNAPSHOT_KEY: &str = "aggressive_all";
pub const CLEANUP_SNAPSHOT_PENDING_RETRY_AFTER_MS: u64 = 800;
pub const CLEANUP_CONFIRM_MAX_AGE_SECONDS: i64 = 30;

#[derive(Clone)]
pub struct CleanupSnapshotWorker {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl CleanupSnapshotWorker {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            db,
            runner,
            running: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
        }
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

            if self.pending.swap(false, Ordering::SeqCst) {
                continue;
            }

            self.running.store(false, Ordering::SeqCst);
            if self.pending.load(Ordering::SeqCst)
                && self
                    .running
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                continue;
            }
            break;
        }
    }

    async fn refresh_once(&self) -> anyhow::Result<()> {
        let snapshot =
            cleanup::build_inventory_snapshot(self.db.clone(), self.runner.clone()).await?;
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
