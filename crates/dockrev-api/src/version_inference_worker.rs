use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc};

use crate::{db::Db, ignore, registry};

pub const VERSION_INFERENCE_TTL_DAYS: i64 = 7;
pub const VERSION_INFERENCE_WORKER_MAX_CONCURRENCY: usize = 4;
const VERSION_INFERENCE_SCAN_LIMIT: usize = 60;
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(4);
const MANIFEST_BUDGET: Duration = Duration::from_secs(12);
const MANIFEST_CONCURRENCY: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionInferenceReason {
    CacheMiss,
    CacheStale,
    AllFailed,
    NewVersion,
    Force,
    Running,
    NotRequired,
}

impl VersionInferenceReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CacheMiss => "cache_miss",
            Self::CacheStale => "cache_stale",
            Self::AllFailed => "all_failed",
            Self::NewVersion => "new_version",
            Self::Force => "force",
            Self::Running => "running",
            Self::NotRequired => "not_required",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceScanSummary {
    pub semver_tags_total: usize,
    pub semver_tags_considered: usize,
    pub manifests_ok: usize,
    pub manifests_timeout: usize,
    pub manifests_error: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceSnapshot {
    pub checked_at: String,
    pub digests: BTreeMap<String, Vec<String>>,
    pub scan: VersionInferenceScanSummary,
    pub all_failed: bool,
}

#[derive(Debug)]
struct VersionInferenceTask {
    key: String,
    image_repo: String,
    host_platform: String,
    reason: VersionInferenceReason,
}

#[derive(Clone)]
pub struct VersionInferenceWorker {
    db: Db,
    registry: Arc<dyn registry::RegistryClient>,
    in_flight: Arc<Mutex<HashMap<String, VersionInferenceReason>>>,
    queue_tx: mpsc::UnboundedSender<VersionInferenceTask>,
}

impl VersionInferenceWorker {
    pub fn new(db: Db, registry: Arc<dyn registry::RegistryClient>) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        let worker = Self {
            db,
            registry,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            queue_tx,
        };
        worker.spawn_workers(queue_rx, VERSION_INFERENCE_WORKER_MAX_CONCURRENCY);
        worker
    }

    fn spawn_workers(
        &self,
        queue_rx: mpsc::UnboundedReceiver<VersionInferenceTask>,
        concurrency: usize,
    ) {
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
                        .run_single(&task.image_repo, &task.host_platform, task.reason)
                        .await;
                    if let Err(e) = run {
                        tracing::warn!(
                            image_repo = %task.image_repo,
                            host_platform = %task.host_platform,
                            reason = task.reason.as_str(),
                            error = %e,
                            "version inference worker task failed"
                        );
                    }

                    let mut in_flight = worker.in_flight.lock().await;
                    in_flight.remove(&task.key);
                }
            });
        }
    }

    pub async fn enqueue(
        &self,
        image_repo: &str,
        host_platform: &str,
        reason: VersionInferenceReason,
    ) -> bool {
        let image_repo = image_repo.trim().to_string();
        let host_platform = host_platform.trim().to_string();
        if image_repo.is_empty() || host_platform.is_empty() {
            return false;
        }

        let key = format!("{image_repo}@{host_platform}");
        {
            let mut in_flight = self.in_flight.lock().await;
            if in_flight.contains_key(&key) {
                return false;
            }
            in_flight.insert(key.clone(), reason);
        }

        let task = VersionInferenceTask {
            key: key.clone(),
            image_repo: image_repo.clone(),
            host_platform: host_platform.clone(),
            reason,
        };
        if self.queue_tx.send(task).is_err() {
            let mut in_flight = self.in_flight.lock().await;
            in_flight.remove(&key);
            return false;
        }
        true
    }

    pub async fn in_flight_reason(
        &self,
        image_repo: &str,
        host_platform: &str,
    ) -> Option<VersionInferenceReason> {
        let key = format!("{}@{}", image_repo.trim(), host_platform.trim());
        let in_flight = self.in_flight.lock().await;
        in_flight.get(&key).copied()
    }

    async fn run_single(
        &self,
        image_repo: &str,
        host_platform: &str,
        reason: VersionInferenceReason,
    ) -> anyhow::Result<()> {
        let Some(img) = image_ref_from_repo(image_repo) else {
            return Ok(());
        };

        let snapshot = build_snapshot(self.registry.clone(), img, host_platform).await;
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let snapshot_json = serde_json::to_string(&snapshot)?;
        self.db
            .upsert_image_version_inference_snapshot(
                image_repo,
                host_platform,
                &snapshot_json,
                snapshot.all_failed,
                &now,
                &now,
            )
            .await?;

        tracing::debug!(
            image_repo,
            host_platform,
            reason = reason.as_str(),
            all_failed = snapshot.all_failed,
            digest_keys = snapshot.digests.len(),
            "version inference worker task completed"
        );

        Ok(())
    }
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

fn sort_semver_desc(tags: Vec<String>) -> Vec<String> {
    let mut semver_tags: Vec<(semver::Version, String)> = tags
        .into_iter()
        .filter_map(|t| ignore::parse_version(&t).map(|v| (v, t)))
        .collect();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    semver_tags.into_iter().map(|(_, t)| t).collect()
}

pub fn normalize_digest(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(':') {
        return Some(trimmed.to_ascii_lowercase());
    }
    Some(format!("sha256:{}", trimmed.to_ascii_lowercase()))
}

pub fn lookup_tags_for_digest(snapshot: &VersionInferenceSnapshot, digest: &str) -> Vec<String> {
    let Some(norm) = normalize_digest(digest) else {
        return Vec::new();
    };
    snapshot.digests.get(&norm).cloned().unwrap_or_default()
}

async fn build_snapshot(
    registry: Arc<dyn registry::RegistryClient>,
    img: registry::ImageRef,
    host_platform: &str,
) -> VersionInferenceSnapshot {
    let checked_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    let repo_tags = match registry.list_tags(&img).await {
        Ok(tags) => tags,
        Err(_) => {
            return VersionInferenceSnapshot {
                checked_at,
                digests: BTreeMap::new(),
                scan: VersionInferenceScanSummary {
                    semver_tags_total: 0,
                    semver_tags_considered: 0,
                    manifests_ok: 0,
                    manifests_timeout: 0,
                    manifests_error: 1,
                },
                all_failed: true,
            };
        }
    };

    let semver_tags = sort_semver_desc(repo_tags);
    let semver_tags_total = semver_tags.len();
    let considered: Vec<String> = semver_tags
        .into_iter()
        .take(VERSION_INFERENCE_SCAN_LIMIT)
        .collect();
    let semver_tags_considered = considered.len();

    if semver_tags_considered == 0 {
        return VersionInferenceSnapshot {
            checked_at,
            digests: BTreeMap::new(),
            scan: VersionInferenceScanSummary {
                semver_tags_total,
                semver_tags_considered,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            },
            // No semver tags means "not applicable", not a failed scan.
            all_failed: false,
        };
    }

    enum ScanOutcome {
        Ok {
            tag: String,
            digest: Option<String>,
            platform_digest: Option<String>,
        },
        Timeout,
        Error,
    }

    let host_platform = host_platform.to_string();
    let mut join_set: tokio::task::JoinSet<ScanOutcome> = tokio::task::JoinSet::new();
    let mut queue = considered.into_iter();

    let spawn_one = |join_set: &mut tokio::task::JoinSet<ScanOutcome>,
                     tag: String,
                     registry: Arc<dyn registry::RegistryClient>,
                     img: registry::ImageRef,
                     host_platform: String| {
        join_set.spawn(async move {
            match tokio::time::timeout(
                MANIFEST_TIMEOUT,
                registry.get_manifest(&img, &tag, &host_platform),
            )
            .await
            {
                Ok(Ok(manifest)) => ScanOutcome::Ok {
                    tag,
                    digest: manifest.digest,
                    platform_digest: manifest.platform_digest,
                },
                Ok(Err(_)) => ScanOutcome::Error,
                Err(_) => ScanOutcome::Timeout,
            }
        });
    };

    for _ in 0..MANIFEST_CONCURRENCY {
        let Some(tag) = queue.next() else { break };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
        );
    }

    let mut digests: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut manifests_ok = 0usize;
    let mut manifests_timeout = 0usize;
    let mut manifests_error = 0usize;

    let deadline = tokio::time::Instant::now() + MANIFEST_BUDGET;
    while !join_set.is_empty() {
        let next = match tokio::time::timeout_at(deadline, join_set.join_next()).await {
            Ok(next) => next,
            Err(_) => {
                join_set.abort_all();
                break;
            }
        };

        let Some(joined) = next else { break };
        match joined {
            Ok(ScanOutcome::Ok {
                tag,
                digest,
                platform_digest,
            }) => {
                manifests_ok += 1;
                if let Some(d) = digest.as_deref().and_then(normalize_digest) {
                    digests.entry(d).or_default().push(tag.clone());
                }
                if let Some(d) = platform_digest.as_deref().and_then(normalize_digest) {
                    digests.entry(d).or_default().push(tag);
                }
            }
            Ok(ScanOutcome::Timeout) => manifests_timeout += 1,
            Ok(ScanOutcome::Error) => manifests_error += 1,
            Err(_) => manifests_error += 1,
        }

        let Some(tag) = queue.next() else {
            continue;
        };
        spawn_one(
            &mut join_set,
            tag,
            registry.clone(),
            img.clone(),
            host_platform.clone(),
        );
    }

    let processed = manifests_ok + manifests_timeout + manifests_error;
    if processed < semver_tags_considered {
        manifests_timeout += semver_tags_considered - processed;
    }

    for tags in digests.values_mut() {
        let mut seen = HashSet::<String>::new();
        tags.retain(|t| seen.insert(t.clone()));
        *tags = sort_semver_desc(std::mem::take(tags));
    }

    let all_failed = digests.is_empty();
    VersionInferenceSnapshot {
        checked_at,
        digests,
        scan: VersionInferenceScanSummary {
            semver_tags_total,
            semver_tags_considered,
            manifests_ok,
            manifests_timeout,
            manifests_error,
        },
        all_failed,
    }
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{ImageRef, ManifestInfo, RegistryClient};

    #[derive(Clone, Default)]
    struct NonSemverOnlyRegistry;

    #[async_trait::async_trait]
    impl RegistryClient for NonSemverOnlyRegistry {
        async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
            Ok(vec!["latest".to_string(), "main".to_string()])
        }

        async fn get_manifest(
            &self,
            _image: &ImageRef,
            _reference: &str,
            _host_platform: &str,
        ) -> anyhow::Result<ManifestInfo> {
            anyhow::bail!("manifest lookup should not run without semver tags")
        }
    }

    #[tokio::test]
    async fn non_semver_only_snapshot_is_not_all_failed() {
        let registry: Arc<dyn RegistryClient> = Arc::new(NonSemverOnlyRegistry);
        let image = ImageRef::parse("ghcr.io/acme/web:latest").unwrap();

        let snapshot = build_snapshot(registry, image, "linux/amd64").await;

        assert_eq!(snapshot.scan.semver_tags_total, 0);
        assert_eq!(snapshot.scan.semver_tags_considered, 0);
        assert!(!snapshot.all_failed);
        assert!(snapshot.digests.is_empty());
    }
}
