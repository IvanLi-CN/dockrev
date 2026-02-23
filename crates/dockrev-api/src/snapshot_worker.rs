use std::{collections::HashSet, sync::Arc};

use crate::{api::types::ServiceDigestTagsSnapshotResponse, db::Db, registry, service_check};

pub const SNAPSHOT_PENDING_RETRY_AFTER_MS: u64 = 800;

#[derive(Clone)]
pub struct SnapshotWorker {
    db: Db,
    registry: Arc<dyn registry::RegistryClient>,
    in_flight: Arc<tokio::sync::Mutex<HashSet<String>>>,
}

impl SnapshotWorker {
    pub fn new(db: Db, registry: Arc<dyn registry::RegistryClient>) -> Self {
        Self {
            db,
            registry,
            in_flight: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
        }
    }

    pub fn enqueue(&self, image_repo: &str, digest: &str, host_platform: &str, reason: &str) {
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
        let worker = self.clone();
        tokio::spawn(async move {
            {
                let mut inflight = worker.in_flight.lock().await;
                if !inflight.insert(key.clone()) {
                    return;
                }
            }

            let run = worker
                .run_single_snapshot(&repo, &digest, &host_platform, &reason)
                .await;
            if let Err(e) = run {
                tracing::debug!(
                    image_repo = %repo,
                    digest = %digest,
                    host_platform = %host_platform,
                    reason = %reason,
                    error = %e,
                    "snapshot worker task failed"
                );
            }

            let mut inflight = worker.in_flight.lock().await;
            inflight.remove(&key);
        });
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
                    worker.enqueue(&repo, &digest, &host_platform, "startup_warmup");
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
        let repo_tags = self.registry.list_tags(&img).await?;
        let anchors: Vec<String> = Vec::new();
        let (tags, scan) = service_check::scan_digest_tags_snapshot_best_effort(
            self.registry.clone(),
            img,
            host_platform,
            &repo_tags,
            digest,
            &anchors,
        )
        .await;

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
