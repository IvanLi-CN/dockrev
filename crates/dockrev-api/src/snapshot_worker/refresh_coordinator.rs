use std::{sync::Arc, time::Duration};

use super::{SNAPSHOT_REASON_STARTUP_WARMUP, SnapshotWorker, image_repo_from_image_ref};

const SNAPSHOT_REASON_PERIODIC_REFRESH: &str = "periodic_refresh";
const SNAPSHOT_REFRESH_INTERVAL_SECONDS: u64 = 30 * 60;

impl SnapshotWorker {
    pub fn spawn_startup_warmup(&self, host_platform: &str) {
        let host_platform = host_platform.trim().to_string();
        if host_platform.is_empty() {
            return;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            worker
                .enqueue_snapshot_seeds(&host_platform, SNAPSHOT_REASON_STARTUP_WARMUP)
                .await;
        });
    }

    pub fn spawn_periodic_refresh(self: &Arc<Self>, host_platform: &str) {
        let host_platform = host_platform.trim().to_string();
        if host_platform.is_empty() {
            return;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(Duration::from_secs(SNAPSHOT_REFRESH_INTERVAL_SECONDS));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                worker
                    .enqueue_snapshot_seeds(&host_platform, SNAPSHOT_REASON_PERIODIC_REFRESH)
                    .await;
            }
        });
    }

    async fn enqueue_snapshot_seeds(&self, host_platform: &str, reason: &str) {
        let seeds = match self.db.list_snapshot_seed_targets().await {
            Ok(seeds) => seeds,
            Err(error) => {
                tracing::debug!(%error, reason, "snapshot refresh list seeds failed");
                return;
            }
        };
        for (image_ref, digest) in seeds {
            if let Some(repo) = image_repo_from_image_ref(&image_ref) {
                let _ = self.enqueue(&repo, &digest, host_platform, reason).await;
            }
        }
    }
}
