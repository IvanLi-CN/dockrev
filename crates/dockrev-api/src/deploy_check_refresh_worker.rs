use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Mutex;

use crate::{config::Config, db::Db, now_rfc3339, preflight, runner::CommandRunner};

pub const DEPLOY_CHECK_SNAPSHOT_KEY: &str = "global";
pub const DEPLOY_CHECK_PENDING_RETRY_AFTER_MS: u64 = 800;

#[derive(Clone)]
pub struct DeployCheckRefreshWorker {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    config: Config,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl DeployCheckRefreshWorker {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>, config: Config) -> Self {
        Self {
            db,
            runner,
            config,
            running: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn enqueue(&self) -> bool {
        self.pending.store(true, Ordering::SeqCst);
        if self.running.swap(true, Ordering::SeqCst) {
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

    async fn run_loop(self) {
        loop {
            self.pending.store(false, Ordering::SeqCst);
            let result = self.refresh_once().await;
            let mut last_error = self.last_error.lock().await;
            *last_error = result.err().map(|err| err.to_string());
            drop(last_error);

            if !self.pending.load(Ordering::SeqCst) {
                self.running.store(false, Ordering::SeqCst);
                if self.pending.load(Ordering::SeqCst) && !self.running.swap(true, Ordering::SeqCst)
                {
                    continue;
                }
                break;
            }
        }
    }

    async fn refresh_once(&self) -> anyhow::Result<()> {
        let report =
            preflight::build_report_with_parts(&self.config, &self.db, self.runner.clone()).await?;
        let now = now_rfc3339()?;
        let checked_at = report.generated_at.clone();
        let report_json = serde_json::to_string(&report)?;
        self.db
            .upsert_deploy_check_report_snapshot(
                DEPLOY_CHECK_SNAPSHOT_KEY,
                &report_json,
                &checked_at,
                &now,
            )
            .await?;
        Ok(())
    }
}
