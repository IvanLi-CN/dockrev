use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex, mpsc};

use crate::{db::Db, ignore, registry};

pub const VERSION_INFERENCE_TTL_DAYS: i64 = 7;
pub const VERSION_INFERENCE_ALL_FAILED_RETRY_MINUTES: i64 = 10;
pub const VERSION_INFERENCE_WORKER_MAX_CONCURRENCY: usize = 4;
pub const VERSION_INFERENCE_GC_RETENTION_DAYS: i64 = 30;
pub const VERSION_INFERENCE_GC_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const VERSION_INFERENCE_SCAN_LIMIT: usize = 60;
const VERSION_INFERENCE_EVENT_RING_CAPACITY: usize = 2000;
const VERSION_INFERENCE_PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(250);
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

#[derive(Clone, Debug)]
pub struct VersionInferenceTaskProgress {
    pub phase: String,
    pub message: String,
    pub current: u32,
    pub total: u32,
    pub percent: u32,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct VersionInferenceTaskSnapshot {
    pub key: String,
    pub image_repo: String,
    pub host_platform: String,
    pub status: String,
    pub reason: String,
    pub enqueued_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub progress: Option<VersionInferenceTaskProgress>,
}

#[derive(Clone, Debug)]
pub struct VersionInferenceWorkerSnapshot {
    pub max_concurrency: u32,
    pub queued: u32,
    pub running: u32,
    pub in_flight: u32,
}

#[derive(Clone, Debug)]
pub struct VersionInferenceGcSnapshot {
    pub retention_days: i64,
    pub interval_seconds: u64,
    pub last_run_at: Option<String>,
    pub last_deleted: Option<u64>,
    pub last_duration_ms: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for VersionInferenceGcSnapshot {
    fn default() -> Self {
        Self {
            retention_days: VERSION_INFERENCE_GC_RETENTION_DAYS,
            interval_seconds: VERSION_INFERENCE_GC_INTERVAL_SECONDS,
            last_run_at: None,
            last_deleted: None,
            last_duration_ms: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VersionInferenceEventRecord {
    pub id: i64,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct VersionInferenceEventBatch {
    pub events: Vec<VersionInferenceEventRecord>,
    pub oldest_id: Option<i64>,
    pub latest_id: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VersionInferenceTaskStatus {
    Queued,
    Running,
}

impl VersionInferenceTaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
        }
    }
}

#[derive(Clone, Debug)]
struct VersionInferenceTaskRuntime {
    key: String,
    image_repo: String,
    host_platform: String,
    reason: VersionInferenceReason,
    status: VersionInferenceTaskStatus,
    enqueued_at: String,
    started_at: Option<String>,
    updated_at: String,
    progress: Option<VersionInferenceTaskProgress>,
}

#[derive(Clone, Debug)]
struct VersionInferenceTask {
    key: String,
    image_repo: String,
    host_platform: String,
    reason: VersionInferenceReason,
}

#[derive(Clone, Debug)]
struct BuildProgress {
    phase: String,
    message: String,
    current: u32,
    total: u32,
}

#[derive(Clone, Debug)]
struct VersionInferenceRunOutcome {
    checked_at: String,
    all_failed: bool,
    scan: VersionInferenceScanSummary,
}

#[derive(Debug)]
struct VersionInferenceRuntime {
    tasks: HashMap<String, VersionInferenceTaskRuntime>,
    events: VecDeque<VersionInferenceEventRecord>,
    next_event_id: i64,
    gc: VersionInferenceGcSnapshot,
}

impl Default for VersionInferenceRuntime {
    fn default() -> Self {
        Self {
            tasks: HashMap::new(),
            events: VecDeque::new(),
            next_event_id: 1,
            gc: VersionInferenceGcSnapshot::default(),
        }
    }
}

#[derive(Clone)]
pub struct VersionInferenceWorker {
    db: Db,
    registry: Arc<dyn registry::RegistryClient>,
    runtime: Arc<Mutex<VersionInferenceRuntime>>,
    queue_tx: mpsc::UnboundedSender<VersionInferenceTask>,
}

impl VersionInferenceWorker {
    pub fn new(db: Db, registry: Arc<dyn registry::RegistryClient>) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        let worker = Self {
            db,
            registry,
            runtime: Arc::new(Mutex::new(VersionInferenceRuntime::default())),
            queue_tx,
        };
        worker.spawn_workers(queue_rx, VERSION_INFERENCE_WORKER_MAX_CONCURRENCY);
        worker
    }

    pub fn spawn_gc_task(self: &Arc<Self>) {
        let worker = self.clone();
        tokio::spawn(async move {
            if let Err(e) = worker.run_gc_once().await {
                tracing::warn!(error = %e, "version inference gc run failed");
            }

            let mut ticker =
                tokio::time::interval(Duration::from_secs(VERSION_INFERENCE_GC_INTERVAL_SECONDS));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = worker.run_gc_once().await {
                    tracing::warn!(error = %e, "version inference gc tick failed");
                }
            }
        });
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

                    worker.mark_task_started(&task).await;

                    match worker.run_single(&task).await {
                        Ok(outcome) => {
                            worker.mark_task_finished(&task, Some(outcome), None).await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                image_repo = %task.image_repo,
                                host_platform = %task.host_platform,
                                reason = task.reason.as_str(),
                                error = %e,
                                "version inference worker task failed"
                            );
                            worker
                                .mark_task_finished(&task, None, Some(e.to_string()))
                                .await;
                        }
                    }
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
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());

        {
            let mut runtime = self.runtime.lock().await;
            if runtime.tasks.contains_key(&key) {
                return false;
            }
            runtime.tasks.insert(
                key.clone(),
                VersionInferenceTaskRuntime {
                    key: key.clone(),
                    image_repo: image_repo.clone(),
                    host_platform: host_platform.clone(),
                    reason,
                    status: VersionInferenceTaskStatus::Queued,
                    enqueued_at: now.clone(),
                    started_at: None,
                    updated_at: now.clone(),
                    progress: None,
                },
            );
            let _ = push_event_locked(
                &mut runtime,
                json!({
                    "type": "task_enqueued",
                    "ts": now,
                    "key": key,
                    "imageRepo": image_repo,
                    "hostPlatform": host_platform,
                    "reason": reason.as_str(),
                }),
            );
        }

        let task = VersionInferenceTask {
            key: key.clone(),
            image_repo,
            host_platform,
            reason,
        };

        if self.queue_tx.send(task).is_err() {
            let mut runtime = self.runtime.lock().await;
            runtime.tasks.remove(&key);
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
        let runtime = self.runtime.lock().await;
        runtime.tasks.get(&key).map(|t| t.reason)
    }

    pub async fn worker_snapshot(&self) -> VersionInferenceWorkerSnapshot {
        let runtime = self.runtime.lock().await;
        let queued = runtime
            .tasks
            .values()
            .filter(|task| task.status == VersionInferenceTaskStatus::Queued)
            .count() as u32;
        let running = runtime
            .tasks
            .values()
            .filter(|task| task.status == VersionInferenceTaskStatus::Running)
            .count() as u32;
        VersionInferenceWorkerSnapshot {
            max_concurrency: VERSION_INFERENCE_WORKER_MAX_CONCURRENCY as u32,
            queued,
            running,
            in_flight: queued + running,
        }
    }

    pub async fn gc_snapshot(&self) -> VersionInferenceGcSnapshot {
        let runtime = self.runtime.lock().await;
        runtime.gc.clone()
    }

    pub async fn list_tasks(&self) -> Vec<VersionInferenceTaskSnapshot> {
        let runtime = self.runtime.lock().await;
        let mut tasks = runtime
            .tasks
            .values()
            .cloned()
            .map(|task| VersionInferenceTaskSnapshot {
                key: task.key,
                image_repo: task.image_repo,
                host_platform: task.host_platform,
                status: task.status.as_str().to_string(),
                reason: task.reason.as_str().to_string(),
                enqueued_at: task.enqueued_at,
                started_at: task.started_at,
                updated_at: task.updated_at,
                progress: task.progress,
            })
            .collect::<Vec<_>>();

        tasks.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.image_repo.cmp(&b.image_repo))
                .then_with(|| a.host_platform.cmp(&b.host_platform))
        });
        tasks
    }

    pub async fn latest_event_id(&self) -> i64 {
        let runtime = self.runtime.lock().await;
        runtime
            .events
            .back()
            .map(|evt| evt.id)
            .unwrap_or(runtime.next_event_id.saturating_sub(1))
    }

    pub async fn events_since(&self, after_id: i64, limit: usize) -> VersionInferenceEventBatch {
        let runtime = self.runtime.lock().await;
        let oldest_id = runtime.events.front().map(|evt| evt.id);
        let latest_id = runtime
            .events
            .back()
            .map(|evt| evt.id)
            .unwrap_or(runtime.next_event_id.saturating_sub(1));

        let mut events = Vec::new();
        for evt in runtime.events.iter() {
            if evt.id <= after_id {
                continue;
            }
            events.push(evt.clone());
            if events.len() >= limit {
                break;
            }
        }

        VersionInferenceEventBatch {
            events,
            oldest_id,
            latest_id,
        }
    }

    pub async fn emit_resync_required(
        &self,
        requested_after_id: i64,
        oldest_available_id: i64,
        latest_event_id: i64,
    ) -> VersionInferenceEventRecord {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        push_event_locked(
            &mut runtime,
            json!({
                "type": "resync_required",
                "ts": now,
                "requestedAfterId": requested_after_id,
                "oldestAvailableId": oldest_available_id,
                "latestEventId": latest_event_id,
                "reason": "buffer_overflow",
            }),
        )
    }

    async fn run_single(
        &self,
        task: &VersionInferenceTask,
    ) -> anyhow::Result<VersionInferenceRunOutcome> {
        let Some(img) = image_ref_from_repo(&task.image_repo) else {
            let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
            return Ok(VersionInferenceRunOutcome {
                checked_at: now,
                all_failed: false,
                scan: VersionInferenceScanSummary {
                    semver_tags_total: 0,
                    semver_tags_considered: 0,
                    manifests_ok: 0,
                    manifests_timeout: 0,
                    manifests_error: 0,
                },
            });
        };

        self.record_task_progress(
            &task.key,
            BuildProgress {
                phase: "prepare".to_string(),
                message: "preparing version inference".to_string(),
                current: 0,
                total: 0,
            },
        )
        .await;

        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<BuildProgress>();
        let progress_worker = {
            let worker = self.clone();
            let task_key = task.key.clone();
            tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    worker.record_task_progress(&task_key, progress).await;
                }
            })
        };

        let snapshot = build_snapshot(
            self.registry.clone(),
            img,
            &task.host_platform,
            Some(progress_tx),
        )
        .await;

        self.record_task_progress(
            &task.key,
            BuildProgress {
                phase: "persist".to_string(),
                message: "persisting snapshot".to_string(),
                current: 1,
                total: 1,
            },
        )
        .await;

        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let snapshot_json = serde_json::to_string(&snapshot)?;
        self.db
            .upsert_image_version_inference_snapshot(
                &task.image_repo,
                &task.host_platform,
                &snapshot_json,
                snapshot.all_failed,
                &snapshot.checked_at,
                &now,
            )
            .await?;

        let _ = progress_worker.await;

        tracing::debug!(
            image_repo = %task.image_repo,
            host_platform = %task.host_platform,
            reason = task.reason.as_str(),
            all_failed = snapshot.all_failed,
            digest_keys = snapshot.digests.len(),
            "version inference worker task completed"
        );

        Ok(VersionInferenceRunOutcome {
            checked_at: snapshot.checked_at,
            all_failed: snapshot.all_failed,
            scan: snapshot.scan,
        })
    }

    async fn run_gc_once(&self) -> anyhow::Result<()> {
        let start = Instant::now();
        let now_dt = time::OffsetDateTime::now_utc();
        let run_at = now_dt.format(&time::format_description::well_known::Rfc3339)?;
        let cutoff_dt = now_dt - time::Duration::days(VERSION_INFERENCE_GC_RETENTION_DAYS);
        let cutoff = cutoff_dt.format(&time::format_description::well_known::Rfc3339)?;

        let delete_result = self
            .db
            .delete_image_version_inference_snapshots_older_than(&cutoff)
            .await;

        let duration_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let mut runtime = self.runtime.lock().await;
        runtime.gc.last_run_at = Some(run_at.clone());
        runtime.gc.last_duration_ms = Some(duration_ms);

        match delete_result {
            Ok(deleted) => {
                runtime.gc.last_deleted = Some(deleted);
                runtime.gc.last_error = None;
                let _ = push_event_locked(
                    &mut runtime,
                    json!({
                        "type": "gc_ran",
                        "ts": run_at,
                        "cutoff": cutoff,
                        "deleted": deleted,
                        "durationMs": duration_ms,
                        "ok": true,
                    }),
                );
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                runtime.gc.last_error = Some(msg.clone());
                let _ = push_event_locked(
                    &mut runtime,
                    json!({
                        "type": "gc_ran",
                        "ts": run_at,
                        "cutoff": cutoff,
                        "deleted": 0,
                        "durationMs": duration_ms,
                        "ok": false,
                        "error": msg,
                    }),
                );
                Err(err)
            }
        }
    }

    async fn mark_task_started(&self, task: &VersionInferenceTask) {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        let payload = if let Some(entry) = runtime.tasks.get_mut(&task.key) {
            entry.status = VersionInferenceTaskStatus::Running;
            entry.started_at = Some(now.clone());
            entry.updated_at = now.clone();
            Some(json!({
                "type": "task_started",
                "ts": now,
                "key": entry.key,
                "imageRepo": entry.image_repo,
                "hostPlatform": entry.host_platform,
                "reason": entry.reason.as_str(),
            }))
        } else {
            None
        };
        if let Some(payload) = payload {
            let _ = push_event_locked(&mut runtime, payload);
        }
    }

    async fn record_task_progress(&self, task_key: &str, progress: BuildProgress) {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        let payload = if let Some(entry) = runtime.tasks.get_mut(task_key) {
            let percent = if progress.total == 0 {
                0
            } else {
                ((progress.current.saturating_mul(100)) / progress.total).min(100)
            };
            let snapshot = VersionInferenceTaskProgress {
                phase: progress.phase.clone(),
                message: progress.message.clone(),
                current: progress.current,
                total: progress.total,
                percent,
                updated_at: now.clone(),
            };
            entry.progress = Some(snapshot.clone());
            entry.updated_at = now.clone();

            Some(json!({
                "type": "task_progress",
                "ts": now,
                "key": entry.key,
                "imageRepo": entry.image_repo,
                "hostPlatform": entry.host_platform,
                "reason": entry.reason.as_str(),
                "phase": snapshot.phase,
                "message": snapshot.message,
                "current": snapshot.current,
                "total": snapshot.total,
                "percent": snapshot.percent,
                "updatedAt": snapshot.updated_at,
            }))
        } else {
            None
        };
        if let Some(payload) = payload {
            let _ = push_event_locked(&mut runtime, payload);
        }
    }

    async fn mark_task_finished(
        &self,
        task: &VersionInferenceTask,
        outcome: Option<VersionInferenceRunOutcome>,
        error: Option<String>,
    ) {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        let removed = runtime.tasks.remove(&task.key);
        let reason = removed
            .as_ref()
            .map(|entry| entry.reason)
            .unwrap_or(task.reason)
            .as_str()
            .to_string();

        let payload = if let Some(outcome) = outcome {
            json!({
                "type": "task_finished",
                "ts": now,
                "key": task.key,
                "imageRepo": task.image_repo,
                "hostPlatform": task.host_platform,
                "reason": reason,
                "status": "success",
                "checkedAt": outcome.checked_at,
                "allFailed": outcome.all_failed,
                "scan": outcome.scan,
            })
        } else {
            json!({
                "type": "task_finished",
                "ts": now,
                "key": task.key,
                "imageRepo": task.image_repo,
                "hostPlatform": task.host_platform,
                "reason": reason,
                "status": "error",
                "error": error.unwrap_or_else(|| "unknown error".to_string()),
            })
        };

        let _ = push_event_locked(&mut runtime, payload);
    }
}

fn push_event_locked(
    runtime: &mut VersionInferenceRuntime,
    mut data: serde_json::Value,
) -> VersionInferenceEventRecord {
    let id = runtime.next_event_id;
    runtime.next_event_id = runtime.next_event_id.saturating_add(1);

    if let Some(obj) = data.as_object_mut() {
        obj.entry("ts".to_string()).or_insert_with(|| {
            json!(now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()))
        });
    }

    let record = VersionInferenceEventRecord { id, data };
    runtime.events.push_back(record.clone());
    while runtime.events.len() > VERSION_INFERENCE_EVENT_RING_CAPACITY {
        runtime.events.pop_front();
    }
    record
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

fn send_build_progress(
    progress_tx: &Option<mpsc::UnboundedSender<BuildProgress>>,
    phase: &str,
    message: String,
    current: u32,
    total: u32,
) {
    let Some(tx) = progress_tx.as_ref() else {
        return;
    };
    let _ = tx.send(BuildProgress {
        phase: phase.to_string(),
        message,
        current,
        total,
    });
}

async fn build_snapshot(
    registry: Arc<dyn registry::RegistryClient>,
    img: registry::ImageRef,
    host_platform: &str,
    progress_tx: Option<mpsc::UnboundedSender<BuildProgress>>,
) -> VersionInferenceSnapshot {
    let checked_at = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
    send_build_progress(
        &progress_tx,
        "listing_tags",
        "listing semver tags".to_string(),
        0,
        0,
    );

    let repo_tags = match registry.list_tags(&img).await {
        Ok(tags) => tags,
        Err(_) => {
            send_build_progress(
                &progress_tx,
                "listing_tags",
                "list tags failed".to_string(),
                1,
                1,
            );
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
        send_build_progress(
            &progress_tx,
            "scanning_manifests",
            "no semver tags to scan".to_string(),
            1,
            1,
        );
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

    send_build_progress(
        &progress_tx,
        "scanning_manifests",
        format!("scanning manifests (0/{semver_tags_considered})"),
        0,
        semver_tags_considered as u32,
    );

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

    let mut processed = 0usize;
    let mut last_progress_emit = Instant::now() - VERSION_INFERENCE_PROGRESS_EMIT_INTERVAL;

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

        processed = processed.saturating_add(1);

        let should_emit = processed >= semver_tags_considered
            || last_progress_emit.elapsed() >= VERSION_INFERENCE_PROGRESS_EMIT_INTERVAL;
        if should_emit {
            last_progress_emit = Instant::now();
            send_build_progress(
                &progress_tx,
                "scanning_manifests",
                format!("scanning manifests ({processed}/{semver_tags_considered})"),
                processed as u32,
                semver_tags_considered as u32,
            );
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

    if processed < semver_tags_considered {
        manifests_timeout += semver_tags_considered - processed;
        processed = semver_tags_considered;
    }

    send_build_progress(
        &progress_tx,
        "scanning_manifests",
        format!("scanning manifests ({processed}/{semver_tags_considered})"),
        processed as u32,
        semver_tags_considered as u32,
    );

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

        let snapshot = build_snapshot(registry, image, "linux/amd64", None).await;

        assert_eq!(snapshot.scan.semver_tags_total, 0);
        assert_eq!(snapshot.scan.semver_tags_considered, 0);
        assert!(!snapshot.all_failed);
        assert!(snapshot.digests.is_empty());
    }
}
