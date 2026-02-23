use std::{collections::HashSet, sync::Arc};

use tokio::sync::{Mutex, mpsc};

use crate::{
    api::types::{ServiceDigestTagsScanSummary, ServiceDigestTagsSnapshotResponse},
    db::Db,
    registry, service_check,
};

pub const SNAPSHOT_PENDING_RETRY_AFTER_MS: u64 = 800;
pub const SNAPSHOT_WORKER_MAX_CONCURRENCY: usize = 4;

#[derive(Debug)]
struct SnapshotTask {
    key: String,
    repo: String,
    digest: String,
    host_platform: String,
    reason: String,
}

#[derive(Clone)]
pub struct SnapshotWorker {
    db: Db,
    registry: Arc<dyn registry::RegistryClient>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    queue_tx: mpsc::UnboundedSender<SnapshotTask>,
}

impl SnapshotWorker {
    pub fn new(db: Db, registry: Arc<dyn registry::RegistryClient>) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        let worker = Self {
            db,
            registry,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            queue_tx,
        };
        worker.spawn_workers(queue_rx, SNAPSHOT_WORKER_MAX_CONCURRENCY);
        worker
    }

    fn spawn_workers(&self, queue_rx: mpsc::UnboundedReceiver<SnapshotTask>, concurrency: usize) {
        let queue_rx = Arc::new(Mutex::new(queue_rx));
        for _ in 0..concurrency {
            let queue_rx = queue_rx.clone();
            let worker = self.clone();
            tokio::spawn(async move {
                loop {
                    let task = {
                        let mut rx = queue_rx.lock().await;
                        rx.recv().await
                    };
                    let Some(task) = task else {
                        break;
                    };

                    let run = worker
                        .run_single_snapshot(
                            &task.repo,
                            &task.digest,
                            &task.host_platform,
                            &task.reason,
                        )
                        .await;
                    if let Err(e) = run {
                        tracing::debug!(
                            image_repo = %task.repo,
                            digest = %task.digest,
                            host_platform = %task.host_platform,
                            reason = %task.reason,
                            error = %e,
                            "snapshot worker task failed"
                        );
                    }

                    let mut inflight = worker.in_flight.lock().await;
                    inflight.remove(&task.key);
                }
            });
        }
    }

    pub async fn enqueue(&self, image_repo: &str, digest: &str, host_platform: &str, reason: &str) {
        let repo = image_repo.trim().to_string();
        let host_platform = host_platform.trim().to_string();
        let reason = reason.trim().to_string();
        let Some(digest) = normalize_digest(digest) else {
            return;
        };
        if repo.is_empty() || host_platform.is_empty() {
            return;
        }

        let key = format!("{repo}@{digest}@{host_platform}");
        {
            let mut inflight = self.in_flight.lock().await;
            if !inflight.insert(key.clone()) {
                return;
            }
        }

        let task = SnapshotTask {
            key: key.clone(),
            repo: repo.clone(),
            digest: digest.clone(),
            host_platform: host_platform.clone(),
            reason: reason.clone(),
        };
        if self.queue_tx.send(task).is_err() {
            let mut inflight = self.in_flight.lock().await;
            inflight.remove(&key);
            tracing::debug!(
                image_repo = %repo,
                digest = %digest,
                host_platform = %host_platform,
                reason = %reason,
                "snapshot worker queue closed; dropped enqueue"
            );
        }
    }

    pub fn spawn_startup_warmup(&self, host_platform: &str) {
        let host_platform = host_platform.trim().to_string();
        if host_platform.is_empty() {
            return;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            let seeds = match worker.db.list_snapshot_seed_targets().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(error = %e, "snapshot warmup list seeds failed");
                    return;
                }
            };
            for (image_ref, digest) in seeds {
                if let Some(repo) = image_repo_from_image_ref(&image_ref) {
                    worker
                        .enqueue(&repo, &digest, &host_platform, "startup_warmup")
                        .await;
                }
            }
        });
    }

    async fn run_single_snapshot(
        &self,
        image_repo: &str,
        digest: &str,
        host_platform: &str,
        reason: &str,
    ) -> anyhow::Result<()> {
        let Some(img) = image_ref_from_repo(image_repo) else {
            return Ok(());
        };
        let anchors = match self.db.list_snapshot_anchor_tags(image_repo, digest).await {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(
                    image_repo = %image_repo,
                    digest = %digest,
                    host_platform = %host_platform,
                    error = %e,
                    "snapshot worker failed to load anchor tags; fallback to empty anchors"
                );
                Vec::new()
            }
        };
        let (tags, scan) = match self.registry.list_tags(&img).await {
            Ok(repo_tags) => {
                service_check::scan_digest_tags_snapshot_best_effort(
                    self.registry.clone(),
                    img,
                    host_platform,
                    &repo_tags,
                    digest,
                    &anchors,
                )
                .await
            }
            Err(e) => {
                tracing::warn!(
                    image_repo = %image_repo,
                    digest = %digest,
                    host_platform = %host_platform,
                    reason = %reason,
                    error = %e,
                    "snapshot worker list_tags failed; persisting fallback error snapshot"
                );
                (
                    Vec::new(),
                    ServiceDigestTagsScanSummary {
                        repo_tags_total: 0,
                        repo_tags_considered: 0,
                        manifests_ok: 0,
                        manifests_timeout: 0,
                        manifests_error: 1,
                    },
                )
            }
        };

        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let snapshot = ServiceDigestTagsSnapshotResponse {
            digest: digest.to_string(),
            tags,
            checked_at: now.clone(),
            scan,
        };
        let snapshot_json = serde_json::to_string(&snapshot)?;
        self.db
            .upsert_image_digest_tags_snapshot(
                image_repo,
                digest,
                host_platform,
                &snapshot_json,
                &now,
                &now,
            )
            .await?;

        tracing::debug!(
            image_repo = %image_repo,
            digest = %digest,
            host_platform = %host_platform,
            reason = %reason,
            "snapshot worker task completed"
        );

        Ok(())
    }
}

pub fn normalize_digest(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(':') {
        return Some(trimmed.to_string());
    }
    Some(format!("sha256:{trimmed}"))
}

pub fn image_repo_from_image_ref(image_ref: &str) -> Option<String> {
    registry::ImageRef::parse(image_ref)
        .ok()
        .map(|img| format!("{}/{}", img.registry, img.name))
}

fn image_ref_from_repo(image_repo: &str) -> Option<registry::ImageRef> {
    let repo = image_repo.trim();
    let slash = repo.find('/')?;
    let (registry, name_with_slash) = repo.split_at(slash);
    let name = name_with_slash.strip_prefix('/')?.trim();
    if registry.trim().is_empty() || name.is_empty() {
        return None;
    }
    Some(registry::ImageRef {
        registry: registry.trim().to_string(),
        name: name.to_string(),
        reference: "latest".to_string(),
    })
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}
