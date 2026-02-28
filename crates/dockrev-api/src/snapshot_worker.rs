use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::json;
use tokio::sync::{Mutex, mpsc};

use crate::{
    api::types::{ServiceDigestTagsScanSummary, ServiceDigestTagsSnapshotResponse},
    db::Db,
    registry, service_check,
};

pub const SNAPSHOT_PENDING_RETRY_AFTER_MS: u64 = 800;
pub const SNAPSHOT_WORKER_MAX_CONCURRENCY: usize = 4;
pub const SNAPSHOT_CACHE_TTL_DAYS: i64 = 7;
pub const SNAPSHOT_ALL_FAILED_RETRY_MINUTES: i64 = 10;
pub const SNAPSHOT_GC_RETENTION_DAYS: i64 = 30;
pub const SNAPSHOT_GC_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const SNAPSHOT_EVENT_RING_CAPACITY: usize = 2000;

#[derive(Clone, Debug)]
pub struct SnapshotTaskProgress {
    pub phase: String,
    pub message: String,
    pub current: u32,
    pub total: u32,
    pub percent: u32,
    pub assigned_current: u32,
    pub assigned_total: u32,
    pub assigned_percent: u32,
    pub result_current: u32,
    pub result_total: u32,
    pub result_percent: u32,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct SnapshotTaskSnapshot {
    pub key: String,
    pub image_repo: String,
    pub digest: String,
    pub host_platform: String,
    pub status: String,
    pub reason: String,
    pub enqueued_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub progress: Option<SnapshotTaskProgress>,
}

#[derive(Clone, Debug)]
pub struct SnapshotWorkerSnapshot {
    pub max_concurrency: u32,
    pub queued: u32,
    pub running: u32,
    pub in_flight: u32,
}

#[derive(Clone, Debug)]
pub struct SnapshotGcSnapshot {
    pub retention_days: i64,
    pub interval_seconds: u64,
    pub last_run_at: Option<String>,
    pub last_deleted: Option<u64>,
    pub last_duration_ms: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for SnapshotGcSnapshot {
    fn default() -> Self {
        Self {
            retention_days: SNAPSHOT_GC_RETENTION_DAYS,
            interval_seconds: SNAPSHOT_GC_INTERVAL_SECONDS,
            last_run_at: None,
            last_deleted: None,
            last_duration_ms: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotEventRecord {
    pub id: i64,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct SnapshotEventBatch {
    pub events: Vec<SnapshotEventRecord>,
    pub oldest_id: Option<i64>,
    pub latest_id: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotTaskStatus {
    Queued,
    Running,
}

impl SnapshotTaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
        }
    }
}

#[derive(Clone, Debug)]
struct SnapshotTaskRuntime {
    key: String,
    image_repo: String,
    digest: String,
    host_platform: String,
    reason: String,
    status: SnapshotTaskStatus,
    enqueued_at: String,
    started_at: Option<String>,
    updated_at: String,
    progress: Option<SnapshotTaskProgress>,
}

#[derive(Clone, Debug)]
struct SnapshotTask {
    key: String,
    repo: String,
    digest: String,
    host_platform: String,
    reason: String,
}

#[derive(Clone, Debug)]
struct BuildProgress {
    phase: String,
    message: String,
    current: u32,
    total: u32,
    assigned_current: u32,
    assigned_total: u32,
    result_current: u32,
    result_total: u32,
}

#[derive(Debug)]
struct SnapshotRuntime {
    tasks: HashMap<String, SnapshotTaskRuntime>,
    events: VecDeque<SnapshotEventRecord>,
    next_event_id: i64,
    gc: SnapshotGcSnapshot,
}

impl Default for SnapshotRuntime {
    fn default() -> Self {
        Self {
            tasks: HashMap::new(),
            events: VecDeque::new(),
            next_event_id: 1,
            gc: SnapshotGcSnapshot::default(),
        }
    }
}

#[derive(Clone)]
pub struct SnapshotWorker {
    db: Db,
    registry: Arc<dyn registry::RegistryClient>,
    runtime: Arc<Mutex<SnapshotRuntime>>,
    queue_tx: mpsc::UnboundedSender<SnapshotTask>,
}

impl SnapshotWorker {
    pub fn new(db: Db, registry: Arc<dyn registry::RegistryClient>) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        let worker = Self {
            db,
            registry,
            runtime: Arc::new(Mutex::new(SnapshotRuntime::default())),
            queue_tx,
        };
        worker.spawn_workers(queue_rx, SNAPSHOT_WORKER_MAX_CONCURRENCY);
        worker
    }

    pub fn spawn_gc_task(self: &Arc<Self>) {
        let worker = self.clone();
        tokio::spawn(async move {
            if let Err(e) = worker.run_gc_once().await {
                tracing::warn!(error = %e, "snapshot gc run failed");
            }

            let mut ticker =
                tokio::time::interval(Duration::from_secs(SNAPSHOT_GC_INTERVAL_SECONDS));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = worker.run_gc_once().await {
                    tracing::warn!(error = %e, "snapshot gc tick failed");
                }
            }
        });
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

                    worker.mark_task_started(&task).await;

                    let run = worker
                        .run_single_snapshot(
                            &task.repo,
                            &task.digest,
                            &task.host_platform,
                            &task.reason,
                            &task.key,
                        )
                        .await;

                    match run {
                        Ok((checked_at, scan, all_failed)) => {
                            worker
                                .mark_task_finished(
                                    &task,
                                    Some((checked_at, scan, all_failed)),
                                    None,
                                )
                                .await;
                        }
                        Err(e) => {
                            tracing::debug!(
                                image_repo = %task.repo,
                                digest = %task.digest,
                                host_platform = %task.host_platform,
                                reason = %task.reason,
                                error = %e,
                                "snapshot worker task failed"
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
        digest: &str,
        host_platform: &str,
        reason: &str,
    ) -> bool {
        let repo = image_repo.trim().to_string();
        let host_platform = host_platform.trim().to_string();
        let reason = reason.trim().to_string();
        let Some(digest) = normalize_digest(digest) else {
            return false;
        };
        if repo.is_empty() || host_platform.is_empty() {
            return false;
        }

        let key = format!("{repo}@{digest}@{host_platform}");
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());

        {
            let mut runtime = self.runtime.lock().await;
            if runtime.tasks.contains_key(&key) {
                return false;
            }
            runtime.tasks.insert(
                key.clone(),
                SnapshotTaskRuntime {
                    key: key.clone(),
                    image_repo: repo.clone(),
                    digest: digest.clone(),
                    host_platform: host_platform.clone(),
                    reason: reason.clone(),
                    status: SnapshotTaskStatus::Queued,
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
                    "imageRepo": repo,
                    "digest": digest,
                    "hostPlatform": host_platform,
                    "reason": reason,
                }),
            );
        }

        let task = SnapshotTask {
            key: key.clone(),
            repo,
            digest,
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
        digest: &str,
        host_platform: &str,
    ) -> Option<String> {
        let digest = normalize_digest(digest)?;
        let key = format!("{}@{}@{}", image_repo.trim(), digest, host_platform.trim());
        let runtime = self.runtime.lock().await;
        runtime.tasks.get(&key).map(|task| task.reason.clone())
    }

    pub async fn worker_stats(&self) -> SnapshotWorkerSnapshot {
        let runtime = self.runtime.lock().await;
        let queued = runtime
            .tasks
            .values()
            .filter(|task| task.status == SnapshotTaskStatus::Queued)
            .count() as u32;
        let running = runtime
            .tasks
            .values()
            .filter(|task| task.status == SnapshotTaskStatus::Running)
            .count() as u32;
        SnapshotWorkerSnapshot {
            max_concurrency: SNAPSHOT_WORKER_MAX_CONCURRENCY as u32,
            queued,
            running,
            in_flight: queued + running,
        }
    }

    pub async fn gc_status(&self) -> SnapshotGcSnapshot {
        let runtime = self.runtime.lock().await;
        runtime.gc.clone()
    }

    pub async fn snapshot_tasks(&self) -> Vec<SnapshotTaskSnapshot> {
        let runtime = self.runtime.lock().await;
        let mut tasks = runtime
            .tasks
            .values()
            .cloned()
            .map(|task| SnapshotTaskSnapshot {
                key: task.key,
                image_repo: task.image_repo,
                digest: task.digest,
                host_platform: task.host_platform,
                status: task.status.as_str().to_string(),
                reason: task.reason,
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
                .then_with(|| a.digest.cmp(&b.digest))
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

    pub async fn events_since(&self, after_id: i64, limit: usize) -> SnapshotEventBatch {
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

        SnapshotEventBatch {
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
    ) -> SnapshotEventRecord {
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
                    let _ = worker
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
        task_key: &str,
    ) -> anyhow::Result<(String, ServiceDigestTagsScanSummary, bool)> {
        self.record_task_progress(
            task_key,
            BuildProgress {
                phase: "prepare".to_string(),
                message: "preparing snapshot".to_string(),
                current: 0,
                total: 0,
                assigned_current: 0,
                assigned_total: 0,
                result_current: 0,
                result_total: 0,
            },
        )
        .await;

        let Some(img) = image_ref_from_repo(image_repo) else {
            let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
            let scan = ServiceDigestTagsScanSummary {
                repo_tags_total: 0,
                repo_tags_considered: 0,
                manifests_ok: 0,
                manifests_timeout: 0,
                manifests_error: 0,
            };
            return Ok((now, scan, false));
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

        self.record_task_progress(
            task_key,
            BuildProgress {
                phase: "listing_tags".to_string(),
                message: "listing repository tags".to_string(),
                current: 0,
                total: 0,
                assigned_current: 0,
                assigned_total: 0,
                result_current: 0,
                result_total: 0,
            },
        )
        .await;

        let (tags, scan) = match self.registry.list_tags(&img).await {
            Ok(repo_tags) => {
                let task_key = task_key.to_string();
                let worker = self.clone();
                let (progress_tx, mut progress_rx) =
                    mpsc::unbounded_channel::<service_check::SnapshotScanProgress>();
                let progress_forwarder = tokio::spawn(async move {
                    const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
                    let mut last_emit_at: Option<Instant> = None;
                    let mut last_processed: Option<u32> = None;
                    let mut last_success: Option<u32> = None;

                    while let Some(progress) = progress_rx.recv().await {
                        let processed = progress.processed.min(u32::MAX as usize) as u32;
                        let task_total = progress.task_total.min(u32::MAX as usize) as u32;
                        let repo_total = progress.repo_total.min(u32::MAX as usize) as u32;
                        let success = progress.success.min(u32::MAX as usize) as u32;
                        let timeout = progress.timeout.min(u32::MAX as usize) as u32;
                        let error = progress.error.min(u32::MAX as usize) as u32;
                        let done = task_total > 0 && processed >= task_total;
                        let changed =
                            last_processed != Some(processed) || last_success != Some(success);
                        let interval_ok =
                            last_emit_at.is_none_or(|at| at.elapsed() >= PROGRESS_EMIT_INTERVAL);

                        if !done && (!changed || !interval_ok) {
                            continue;
                        }

                        last_emit_at = Some(Instant::now());
                        last_processed = Some(processed);
                        last_success = Some(success);

                        worker
                            .record_task_progress(
                                &task_key,
                                BuildProgress {
                                    phase: "scanning".to_string(),
                                    message: format!(
                                        "scanning manifests ({processed}/{task_total}) · repo total {repo_total} · timeout {timeout} · error {error}"
                                    ),
                                    // Keep `current` aligned with successful manifest parses so
                                    // `resultCurrent` does not get inflated by timed out/errored entries.
                                    current: success,
                                    total: task_total,
                                    assigned_current: processed,
                                    assigned_total: task_total,
                                    result_current: success,
                                    result_total: task_total,
                                },
                            )
                            .await;
                    }
                });

                let scan = service_check::scan_digest_tags_snapshot_best_effort_with_progress(
                    self.registry.clone(),
                    img,
                    host_platform,
                    &repo_tags,
                    digest,
                    &anchors,
                    |progress| {
                        let _ = progress_tx.send(progress);
                    },
                )
                .await;

                drop(progress_tx);
                let _ = progress_forwarder.await;
                scan
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

        self.record_task_progress(
            task_key,
            BuildProgress {
                phase: "persist".to_string(),
                message: "persisting snapshot".to_string(),
                current: 1,
                total: 1,
                assigned_current: 1,
                assigned_total: 1,
                result_current: 1,
                result_total: 1,
            },
        )
        .await;

        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let snapshot = ServiceDigestTagsSnapshotResponse {
            digest: digest.to_string(),
            tags,
            checked_at: now.clone(),
            scan: scan.clone(),
        };
        let all_failed = snapshot_is_all_failed(&snapshot);
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
            all_failed,
            "snapshot worker task completed"
        );

        Ok((now, scan, all_failed))
    }

    async fn run_gc_once(&self) -> anyhow::Result<()> {
        let start = std::time::Instant::now();
        let now_dt = time::OffsetDateTime::now_utc();
        let run_at = now_dt.format(&time::format_description::well_known::Rfc3339)?;
        let cutoff_dt = now_dt - time::Duration::days(SNAPSHOT_GC_RETENTION_DAYS);
        let cutoff = cutoff_dt.format(&time::format_description::well_known::Rfc3339)?;

        let delete_result = self
            .db
            .delete_expired_image_digest_tags_snapshots(&cutoff)
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

    async fn mark_task_started(&self, task: &SnapshotTask) {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        let payload = if let Some(entry) = runtime.tasks.get_mut(&task.key) {
            entry.status = SnapshotTaskStatus::Running;
            entry.started_at = Some(now.clone());
            entry.updated_at = now.clone();
            Some(json!({
                "type": "task_started",
                "ts": now,
                "key": entry.key,
                "imageRepo": entry.image_repo,
                "digest": entry.digest,
                "hostPlatform": entry.host_platform,
                "reason": entry.reason,
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
            let result_total = progress.result_total.max(progress.total);
            let result_current = progress
                .result_current
                .max(progress.current)
                .min(result_total);
            let result_percent = if result_total == 0 {
                0
            } else {
                ((result_current.saturating_mul(100)) / result_total).min(100)
            };
            let assigned_total = progress
                .assigned_total
                .max(result_total)
                .max(progress.total);
            let assigned_current = progress
                .assigned_current
                .max(result_current)
                .min(assigned_total);
            let assigned_percent = if assigned_total == 0 {
                0
            } else {
                ((assigned_current.saturating_mul(100)) / assigned_total).min(100)
            };
            let snapshot = SnapshotTaskProgress {
                phase: progress.phase,
                message: progress.message,
                current: result_current,
                total: result_total,
                percent: result_percent,
                assigned_current,
                assigned_total,
                assigned_percent,
                result_current,
                result_total,
                result_percent,
                updated_at: now.clone(),
            };
            entry.progress = Some(snapshot.clone());
            entry.updated_at = now.clone();

            Some(json!({
                "type": "task_progress",
                "ts": now,
                "key": entry.key,
                "imageRepo": entry.image_repo,
                "digest": entry.digest,
                "hostPlatform": entry.host_platform,
                "reason": entry.reason,
                "phase": snapshot.phase,
                "message": snapshot.message,
                "current": snapshot.current,
                "total": snapshot.total,
                "percent": snapshot.percent,
                "assignedCurrent": snapshot.assigned_current,
                "assignedTotal": snapshot.assigned_total,
                "assignedPercent": snapshot.assigned_percent,
                "resultCurrent": snapshot.result_current,
                "resultTotal": snapshot.result_total,
                "resultPercent": snapshot.result_percent,
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
        task: &SnapshotTask,
        outcome: Option<(String, ServiceDigestTagsScanSummary, bool)>,
        error: Option<String>,
    ) {
        let now = now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string());
        let mut runtime = self.runtime.lock().await;
        let removed = runtime.tasks.remove(&task.key);
        let reason = removed
            .as_ref()
            .map(|entry| entry.reason.clone())
            .unwrap_or_else(|| task.reason.clone());

        let payload = if let Some((checked_at, scan, all_failed)) = outcome {
            json!({
                "type": "task_finished",
                "ts": now,
                "key": task.key,
                "imageRepo": task.repo,
                "digest": task.digest,
                "hostPlatform": task.host_platform,
                "reason": reason,
                "status": "success",
                "checkedAt": checked_at,
                "allFailed": all_failed,
                "scan": scan,
            })
        } else {
            json!({
                "type": "task_finished",
                "ts": now,
                "key": task.key,
                "imageRepo": task.repo,
                "digest": task.digest,
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
    runtime: &mut SnapshotRuntime,
    mut data: serde_json::Value,
) -> SnapshotEventRecord {
    let id = runtime.next_event_id;
    runtime.next_event_id = runtime.next_event_id.saturating_add(1);

    if let Some(obj) = data.as_object_mut() {
        obj.entry("ts".to_string()).or_insert_with(|| {
            json!(now_rfc3339().unwrap_or_else(|_| time::OffsetDateTime::now_utc().to_string()))
        });
    }

    let record = SnapshotEventRecord { id, data };
    runtime.events.push_back(record.clone());
    while runtime.events.len() > SNAPSHOT_EVENT_RING_CAPACITY {
        runtime.events.pop_front();
    }
    record
}

pub fn snapshot_is_all_failed(snapshot: &ServiceDigestTagsSnapshotResponse) -> bool {
    snapshot.tags.is_empty()
        && snapshot.scan.manifests_ok == 0
        && (snapshot.scan.manifests_timeout > 0 || snapshot.scan.manifests_error > 0)
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
