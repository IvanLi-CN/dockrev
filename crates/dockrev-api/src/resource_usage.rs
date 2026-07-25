use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::{Mutex, broadcast, watch};

#[cfg(test)]
use crate::runner::{CommandRunner, CommandSpec};
use crate::{
    api::types::ServiceResourceSample,
    db::{Db, ServiceResourceSampleInput, ServiceResourceTarget},
    docker_engine::{DockerEngineClient, ProjectResourceCollection},
};
#[cfg(test)]
use serde::Deserialize;
#[cfg(test)]
use std::collections::BTreeSet;

pub const RESOURCE_MONITOR_RETENTION_DAYS: u32 = 7;
pub const JOB_HISTORY_RETENTION_DAYS: u32 = 30;
pub const DEFAULT_SAMPLE_INTERVAL_SECONDS: u64 = 5;
const PARTIAL_SAMPLE_WARN_INTERVAL: Duration = Duration::from_secs(60);
static PARTIAL_SAMPLE_WARNINGS: OnceLock<std::sync::Mutex<BTreeMap<String, Instant>>> =
    OnceLock::new();

pub fn is_valid_sample_interval_seconds(value: u64) -> bool {
    matches!(value, 5 | 10 | 30 | 60 | 300)
}

pub fn normalize_sample_interval_seconds(value: u64) -> u64 {
    if is_valid_sample_interval_seconds(value) {
        value
    } else {
        DEFAULT_SAMPLE_INTERVAL_SECONDS
    }
}

pub fn parse_window_to_seconds(window: &str) -> Option<u64> {
    match window.trim() {
        "3m" => Some(3 * 60),
        "1h" => Some(60 * 60),
        "24h" => Some(24 * 60 * 60),
        _ => None,
    }
}

#[derive(Clone)]
pub struct RealtimeSamplerHub {
    db: Db,
    collector: Arc<dyn ResourceCollector>,
    samplers: Arc<Mutex<BTreeMap<String, Arc<SamplerEntry>>>>,
}

#[derive(Clone)]
struct SamplerEntry {
    tx: broadcast::Sender<RealtimeMessage>,
    subscribers: Arc<AtomicUsize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectHistoryTarget {
    service_id: String,
    service_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectHistorySnapshot {
    compose_project: String,
    interval: Duration,
    services: Vec<ProjectHistoryTarget>,
}

#[derive(Clone)]
struct HistoryWorkerHandle {
    tx: watch::Sender<ProjectHistorySnapshot>,
    snapshot: ProjectHistorySnapshot,
}

struct ProjectHistoryRunOutcome {
    inserted: usize,
    result: &'static str,
    error: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct ResourceCollection {
    samples: BTreeMap<String, ServiceResourceSample>,
    failures: Vec<ResourceCollectionFailure>,
}

#[derive(Clone, Debug)]
struct ResourceCollectionFailure {
    container_id: String,
    service_name: String,
    error: String,
}

impl From<ProjectResourceCollection> for ResourceCollection {
    fn from(value: ProjectResourceCollection) -> Self {
        Self {
            samples: value.samples,
            failures: value
                .failures
                .into_iter()
                .map(|failure| ResourceCollectionFailure {
                    container_id: failure.container_id,
                    service_name: failure.service_name,
                    error: failure.error,
                })
                .collect(),
        }
    }
}

impl ProjectHistoryRunOutcome {
    fn ok(inserted: usize) -> Self {
        Self {
            inserted,
            result: "ok",
            error: None,
        }
    }

    fn empty() -> Self {
        Self {
            inserted: 0,
            result: "empty",
            error: None,
        }
    }

    fn error(error: anyhow::Error) -> Self {
        Self {
            inserted: 0,
            result: "error",
            error: Some(error.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum RealtimeMessage {
    Tick(ServiceResourceSample),
    Error(String),
}

pub struct RealtimeSubscription {
    receiver: broadcast::Receiver<RealtimeMessage>,
    _guard: SubscriptionGuard,
}

impl RealtimeSubscription {
    pub async fn recv(&mut self) -> Result<RealtimeMessage, broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

struct SubscriptionGuard {
    subscribers: Arc<AtomicUsize>,
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        let _ = self
            .subscribers
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                if current == 0 {
                    Some(0)
                } else {
                    Some(current - 1)
                }
            });
    }
}

#[async_trait]
trait ResourceCollector: Send + Sync {
    async fn collect_project_service_aggregates(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<ResourceCollection>;
}

#[derive(Clone)]
struct DockerApiResourceCollector {
    client: DockerEngineClient,
}

impl DockerApiResourceCollector {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: DockerEngineClient::from_env()?,
        })
    }
}

#[async_trait]
impl ResourceCollector for DockerApiResourceCollector {
    async fn collect_project_service_aggregates(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<ResourceCollection> {
        Ok(self
            .client
            .collect_project_service_samples(compose_project)
            .await?
            .into())
    }
}

#[cfg(test)]
#[derive(Clone)]
struct RunnerBackedResourceCollector {
    runner: Arc<dyn CommandRunner>,
}

#[cfg(test)]
#[async_trait]
impl ResourceCollector for RunnerBackedResourceCollector {
    async fn collect_project_service_aggregates(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<ResourceCollection> {
        Ok(ResourceCollection {
            samples: collect_project_service_aggregates_via_runner(
                self.runner.as_ref(),
                compose_project,
            )
            .await?,
            failures: Vec::new(),
        })
    }
}

impl RealtimeSamplerHub {
    pub fn from_env(db: Db) -> anyhow::Result<Self> {
        Ok(Self::with_collector(
            db,
            Arc::new(DockerApiResourceCollector::new()?),
        ))
    }

    #[cfg(test)]
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self::with_collector(db, Arc::new(RunnerBackedResourceCollector { runner }))
    }

    fn with_collector(db: Db, collector: Arc<dyn ResourceCollector>) -> Self {
        Self {
            db,
            collector,
            samplers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub async fn subscribe(&self, service_id: &str) -> RealtimeSubscription {
        let service_id = service_id.to_string();
        let entry = {
            let mut map = self.samplers.lock().await;
            if let Some(existing) = map.get(&service_id) {
                existing.subscribers.fetch_add(1, Ordering::SeqCst);
                existing.clone()
            } else {
                let (tx, _rx) = broadcast::channel(64);
                let created = Arc::new(SamplerEntry {
                    tx,
                    subscribers: Arc::new(AtomicUsize::new(1)),
                });
                map.insert(service_id.clone(), created.clone());
                self.spawn_sampler_task(service_id.clone(), created.clone());
                created
            }
        };

        RealtimeSubscription {
            receiver: entry.tx.subscribe(),
            _guard: SubscriptionGuard {
                subscribers: entry.subscribers.clone(),
            },
        }
    }

    pub async fn sample_once(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceResourceSample>> {
        let target = self
            .db
            .get_service_resource_target(service_id)
            .await
            .context("lookup service target")?;
        let Some(target) = target else {
            return Ok(None);
        };
        sample_for_target(self.collector.as_ref(), &target).await
    }

    fn spawn_sampler_task(&self, service_id: String, entry: Arc<SamplerEntry>) {
        let db = self.db.clone();
        let collector = self.collector.clone();
        let samplers = self.samplers.clone();

        tokio::spawn(async move {
            let mut idle_since: Option<Instant> = None;

            loop {
                let subs = entry.subscribers.load(Ordering::SeqCst);
                if subs == 0 {
                    match idle_since {
                        None => idle_since = Some(Instant::now()),
                        Some(t) if t.elapsed() >= Duration::from_secs(10) => {
                            let removed = {
                                let mut map = samplers.lock().await;
                                try_remove_idle_sampler_entry(&mut map, &service_id, &entry)
                            };
                            if removed {
                                break;
                            }
                            idle_since = None;
                        }
                        _ => {}
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                idle_since = None;

                let settings = match db.get_resource_monitor_settings().await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = entry
                            .tx
                            .send(RealtimeMessage::Error(format!("settings unavailable: {e}")));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                if !settings.enabled {
                    let _ = entry.tx.send(RealtimeMessage::Error(
                        "resource_monitor_disabled".to_string(),
                    ));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                let target = match db.get_service_resource_target(&service_id).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = entry.tx.send(RealtimeMessage::Error(format!(
                            "service target unavailable: {e}"
                        )));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                let Some(target) = target else {
                    let _ = entry
                        .tx
                        .send(RealtimeMessage::Error("service_not_found".to_string()));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };

                match sample_for_target(collector.as_ref(), &target).await {
                    Ok(Some(sample)) => {
                        let _ = entry.tx.send(RealtimeMessage::Tick(sample));
                    }
                    Ok(None) => {
                        let _ = entry.tx.send(RealtimeMessage::Error(
                            "runtime_stats_unavailable".to_string(),
                        ));
                    }
                    Err(e) => {
                        let _ = entry.tx.send(RealtimeMessage::Error(e.to_string()));
                    }
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

fn try_remove_idle_sampler_entry(
    map: &mut BTreeMap<String, Arc<SamplerEntry>>,
    service_id: &str,
    entry: &Arc<SamplerEntry>,
) -> bool {
    if let Some(existing) = map.get(service_id)
        && Arc::ptr_eq(existing, entry)
        && entry.subscribers.load(Ordering::SeqCst) == 0
    {
        map.remove(service_id);
        return true;
    }
    false
}

pub fn spawn_history_sampler_from_env(db: Db) -> anyhow::Result<()> {
    spawn_history_sampler_with_collector(db, Arc::new(DockerApiResourceCollector::new()?));
    Ok(())
}

fn spawn_history_sampler_with_collector(db: Db, collector: Arc<dyn ResourceCollector>) {
    tokio::spawn(async move {
        let mut last_gc = Instant::now() - Duration::from_secs(60 * 60);
        let mut workers = BTreeMap::<String, HistoryWorkerHandle>::new();

        loop {
            if last_gc.elapsed() >= Duration::from_secs(60 * 60) {
                if let Err(e) = gc_history(&db).await {
                    tracing::warn!(error = %e, "resource monitor history gc failed");
                }
                last_gc = Instant::now();
            }

            let settings = match db.get_resource_monitor_settings().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "resource monitor settings unavailable");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            if !settings.enabled {
                workers.clear();
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let interval_seconds =
                normalize_sample_interval_seconds(settings.sample_interval_seconds);
            let targets = match db.list_service_resource_targets().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "resource monitor history targets unavailable");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            let desired = build_project_history_snapshots(targets, interval_seconds);
            sync_history_workers(&mut workers, desired, &db, &collector);

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn gc_history(db: &Db) -> anyhow::Result<()> {
    let now = time::OffsetDateTime::now_utc();
    let older_than = (now - time::Duration::days(RESOURCE_MONITOR_RETENTION_DAYS as i64))
        .format(&time::format_description::well_known::Rfc3339)?;
    let started_at = Instant::now();
    let deleted = db
        .delete_expired_service_resource_samples(&older_than, 10_000)
        .await
        .context("delete expired resource samples")?;
    if deleted > 0 {
        tracing::info!(
            deleted,
            retention_days = RESOURCE_MONITOR_RETENTION_DAYS,
            batch_size = 10_000,
            duration_ms = started_at.elapsed().as_millis() as u64,
            "resource monitor history gc completed"
        );
    }
    let job_older_than = (now - time::Duration::days(JOB_HISTORY_RETENTION_DAYS as i64))
        .format(&time::format_description::well_known::Rfc3339)?;
    let started_at = Instant::now();
    let deleted_jobs = db
        .purge_expired_terminal_jobs(&job_older_than, 2_000)
        .await
        .context("delete expired terminal jobs")?;
    if deleted_jobs > 0 {
        tracing::info!(
            deleted = deleted_jobs,
            retention_days = JOB_HISTORY_RETENTION_DAYS,
            batch_size = 2_000,
            duration_ms = started_at.elapsed().as_millis() as u64,
            "terminal job history gc completed"
        );
    }
    Ok(())
}

fn build_project_history_snapshots(
    targets: Vec<ServiceResourceTarget>,
    interval_seconds: u64,
) -> BTreeMap<String, ProjectHistorySnapshot> {
    let mut grouped = BTreeMap::<String, Vec<ProjectHistoryTarget>>::new();
    for target in targets {
        grouped
            .entry(target.compose_project.clone())
            .or_default()
            .push(ProjectHistoryTarget {
                service_id: target.service_id,
                service_name: target.service_name,
            });
    }

    grouped
        .into_iter()
        .map(|(compose_project, mut services)| {
            services.sort_by(|left, right| {
                left.service_name
                    .cmp(&right.service_name)
                    .then_with(|| left.service_id.cmp(&right.service_id))
            });
            let snapshot = ProjectHistorySnapshot {
                compose_project: compose_project.clone(),
                interval: Duration::from_secs(interval_seconds),
                services,
            };
            (compose_project, snapshot)
        })
        .collect()
}

fn sync_history_workers(
    workers: &mut BTreeMap<String, HistoryWorkerHandle>,
    desired: BTreeMap<String, ProjectHistorySnapshot>,
    db: &Db,
    collector: &Arc<dyn ResourceCollector>,
) {
    let stale_projects = workers
        .keys()
        .filter(|project| !desired.contains_key(*project))
        .cloned()
        .collect::<Vec<_>>();
    for project in stale_projects {
        workers.remove(&project);
    }

    for (project, snapshot) in desired {
        match workers.get_mut(&project) {
            Some(existing) if existing.snapshot == snapshot => {}
            Some(existing) => {
                let _ = existing.tx.send_replace(snapshot.clone());
                existing.snapshot = snapshot;
            }
            None => {
                let (tx, rx) = watch::channel(snapshot.clone());
                spawn_project_history_worker(db.clone(), collector.clone(), rx);
                workers.insert(project, HistoryWorkerHandle { tx, snapshot });
            }
        }
    }
}

fn spawn_project_history_worker(
    db: Db,
    collector: Arc<dyn ResourceCollector>,
    mut rx: watch::Receiver<ProjectHistorySnapshot>,
) {
    tokio::spawn(async move {
        let mut snapshot = rx.borrow().clone();
        let mut next_run_at = Instant::now();

        loop {
            tokio::select! {
                changed = rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    snapshot = rx.borrow_and_update().clone();
                    next_run_at = Instant::now();
                }
                _ = tokio::time::sleep_until(next_run_at.into()) => {
                    let started_at = Instant::now();
                    let run_snapshot = snapshot.clone();
                    let outcome = sample_history_project_once(&db, collector.as_ref(), &run_snapshot).await;
                    let duration = started_at.elapsed();

                    match rx.has_changed() {
                        Ok(true) => {
                            log_project_history_run(&run_snapshot, &outcome, duration, 0);
                            snapshot = rx.borrow_and_update().clone();
                            next_run_at = Instant::now();
                        }
                        Ok(false) => {
                            let skipped_ticks = advance_fixed_cadence(
                                &mut next_run_at,
                                run_snapshot.interval,
                                Instant::now(),
                            );
                            log_project_history_run(&run_snapshot, &outcome, duration, skipped_ticks);
                        }
                        Err(_) => {
                            log_project_history_run(&run_snapshot, &outcome, duration, 0);
                            break;
                        }
                    }
                }
            }
        }
    });
}

fn advance_fixed_cadence(next_run_at: &mut Instant, interval: Duration, now: Instant) -> u64 {
    *next_run_at += interval;
    let mut skipped_ticks = 0u64;
    while *next_run_at < now {
        *next_run_at += interval;
        skipped_ticks = skipped_ticks.saturating_add(1);
    }
    skipped_ticks
}

fn log_project_history_run(
    snapshot: &ProjectHistorySnapshot,
    outcome: &ProjectHistoryRunOutcome,
    duration: Duration,
    skipped_ticks: u64,
) {
    let duration_ms = duration.as_millis() as u64;
    let interval_seconds = snapshot.interval.as_secs();
    let service_count = snapshot.services.len();

    match outcome.error.as_deref() {
        Some(error) => tracing::warn!(
            compose_project = %snapshot.compose_project,
            interval_seconds,
            duration_ms,
            skipped_ticks,
            service_count,
            result = outcome.result,
            error = %error,
            "resource monitor history project sample failed"
        ),
        None if skipped_ticks > 0 => tracing::info!(
            compose_project = %snapshot.compose_project,
            interval_seconds,
            duration_ms,
            skipped_ticks,
            service_count,
            inserted = outcome.inserted,
            result = outcome.result,
            "resource monitor history project sample completed"
        ),
        None => tracing::debug!(
            compose_project = %snapshot.compose_project,
            interval_seconds,
            duration_ms,
            skipped_ticks,
            service_count,
            inserted = outcome.inserted,
            result = outcome.result,
            "resource monitor history project sample completed"
        ),
    }
}

async fn sample_history_project_once(
    db: &Db,
    collector: &dyn ResourceCollector,
    snapshot: &ProjectHistorySnapshot,
) -> ProjectHistoryRunOutcome {
    let collection = match collector
        .collect_project_service_aggregates(&snapshot.compose_project)
        .await
        .with_context(|| {
            format!(
                "collect history stats for compose project {}",
                snapshot.compose_project
            )
        }) {
        Ok(v) => v,
        Err(e) => return ProjectHistoryRunOutcome::error(e),
    };

    let sampled_at = match now_rfc3339() {
        Ok(v) => v,
        Err(e) => return ProjectHistoryRunOutcome::error(e),
    };

    log_partial_collection_failures(&snapshot.compose_project, &collection);
    let rows = snapshot
        .services
        .iter()
        .filter_map(|target| {
            let sample = collection.samples.get(&target.service_name)?;
            Some(ServiceResourceSampleInput {
                service_id: target.service_id.clone(),
                sampled_at: sampled_at.clone(),
                cpu_percent: sample.cpu_percent,
                mem_used_bytes: sample.mem_used_bytes,
                mem_limit_bytes: sample.mem_limit_bytes,
                net_rx_bytes: sample.net_rx_bytes,
                net_tx_bytes: sample.net_tx_bytes,
                block_read_bytes: sample.block_read_bytes,
                block_write_bytes: sample.block_write_bytes,
                pids: sample.pids,
                container_count: sample.container_count,
            })
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return ProjectHistoryRunOutcome::empty();
    }

    match db
        .insert_service_resource_samples(&rows)
        .await
        .context("insert resource samples")
    {
        Ok(inserted) if inserted > 0 => ProjectHistoryRunOutcome::ok(inserted),
        Ok(_) => ProjectHistoryRunOutcome::empty(),
        Err(e) => ProjectHistoryRunOutcome::error(e),
    }
}

async fn sample_for_target(
    collector: &dyn ResourceCollector,
    target: &ServiceResourceTarget,
) -> anyhow::Result<Option<ServiceResourceSample>> {
    let collection = collector
        .collect_project_service_aggregates(&target.compose_project)
        .await
        .with_context(|| {
            format!(
                "collect live stats for compose project {}",
                target.compose_project
            )
        })?;

    log_partial_collection_failures(&target.compose_project, &collection);
    let Some(sample) = collection.samples.get(&target.service_name).cloned() else {
        return Ok(None);
    };

    Ok(Some(ServiceResourceSample {
        sampled_at: now_rfc3339()?,
        cpu_percent: sample.cpu_percent,
        mem_used_bytes: sample.mem_used_bytes,
        mem_limit_bytes: sample.mem_limit_bytes,
        net_rx_bytes: sample.net_rx_bytes,
        net_tx_bytes: sample.net_tx_bytes,
        block_read_bytes: sample.block_read_bytes,
        block_write_bytes: sample.block_write_bytes,
        pids: sample.pids,
        container_count: sample.container_count,
    }))
}

fn log_partial_collection_failures(compose_project: &str, collection: &ResourceCollection) {
    if collection.failures.is_empty() {
        return;
    }

    let warnings = PARTIAL_SAMPLE_WARNINGS.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()));
    let now = Instant::now();
    let mut warnings = warnings
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for failure in &collection.failures {
        let key = format!("{compose_project}:{}", failure.container_id);
        if !should_emit_partial_sample_warning(&mut warnings, key, now) {
            continue;
        }
        tracing::warn!(
            compose_project,
            service = %failure.service_name,
            container_id = %failure.container_id,
            error = %failure.error,
            successful_services = collection.samples.len(),
            failed_containers = collection.failures.len(),
            "resource monitor partial Docker stats collection failure"
        );
    }
}

fn should_emit_partial_sample_warning(
    warnings: &mut BTreeMap<String, Instant>,
    key: String,
    now: Instant,
) -> bool {
    warnings.retain(|_, last| now.saturating_duration_since(*last) < PARTIAL_SAMPLE_WARN_INTERVAL);
    if warnings.contains_key(&key) {
        return false;
    }
    warnings.insert(key, now);
    true
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct ServiceAggregate {
    cpu_percent: f64,
    mem_used_sum: u64,
    mem_limit_sum: u64,
    net_rx_sum: u64,
    net_tx_sum: u64,
    block_read_sum: u64,
    block_write_sum: u64,
    pids_sum: u64,
    mem_seen: bool,
    net_seen: bool,
    block_seen: bool,
    pids_seen: bool,
    container_count: u32,
}

#[cfg(test)]
impl ServiceAggregate {
    fn into_sample(self) -> ServiceResourceSample {
        ServiceResourceSample {
            sampled_at: String::new(),
            cpu_percent: self.cpu_percent,
            mem_used_bytes: self.mem_seen.then_some(self.mem_used_sum),
            mem_limit_bytes: self.mem_seen.then_some(self.mem_limit_sum),
            net_rx_bytes: self.net_seen.then_some(self.net_rx_sum),
            net_tx_bytes: self.net_seen.then_some(self.net_tx_sum),
            block_read_bytes: self.block_seen.then_some(self.block_read_sum),
            block_write_bytes: self.block_seen.then_some(self.block_write_sum),
            pids: self.pids_seen.then_some(self.pids_sum),
            container_count: self.container_count,
        }
    }
}

#[cfg(test)]
async fn collect_project_service_aggregates_via_runner(
    runner: &dyn CommandRunner,
    compose_project: &str,
) -> anyhow::Result<BTreeMap<String, ServiceResourceSample>> {
    let ps = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.project={compose_project}"),
                ],
                env: Vec::new(),
            },
            Duration::from_secs(8),
        )
        .await?;

    if ps.status != 0 {
        return Err(anyhow::anyhow!(
            "docker ps failed status={} stderr={}",
            ps.status,
            ps.stderr
        ));
    }

    let container_ids = ps
        .stdout
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if container_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let inspect = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{.Id}}\t{{index .Config.Labels \"com.docker.compose.service\"}}"
                            .to_string(),
                    ];
                    args.extend(container_ids.iter().cloned());
                    args
                },
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;

    if inspect.status != 0 {
        return Err(anyhow::anyhow!(
            "docker inspect failed status={} stderr={}",
            inspect.status,
            inspect.stderr
        ));
    }

    let mut id_to_service = BTreeMap::<String, String>::new();
    let mut normalized_ids = BTreeSet::<String>::new();
    for line in inspect.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((id_raw, service_raw)) = line.split_once('\t') else {
            continue;
        };
        let id = id_raw.trim().to_string();
        let service = service_raw.trim().to_string();
        if id.is_empty() || service.is_empty() {
            continue;
        }
        id_to_service.insert(id.clone(), service.clone());
        if id.len() >= 12 {
            id_to_service.insert(id[..12].to_string(), service.clone());
        }
        normalized_ids.insert(id);
    }

    if normalized_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let stats = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec![
                        "stats".to_string(),
                        "--no-stream".to_string(),
                        "--no-trunc".to_string(),
                        "--format".to_string(),
                        "{{ json . }}".to_string(),
                    ];
                    args.extend(normalized_ids.iter().cloned());
                    args
                },
                env: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await?;

    if stats.status != 0 {
        return Err(anyhow::anyhow!(
            "docker stats failed status={} stderr={}",
            stats.status,
            stats.stderr
        ));
    }

    let mut aggregates = BTreeMap::<String, ServiceAggregate>::new();

    for line in stats.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: DockerStatsLine = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let service_name = match id_to_service.get(parsed.id.trim()) {
            Some(v) => v.clone(),
            None => continue,
        };

        let entry = aggregates.entry(service_name).or_default();
        entry.container_count = entry.container_count.saturating_add(1);

        if let Some(cpu) = parse_cpu_percent(&parsed.cpu_perc) {
            entry.cpu_percent += cpu;
        }

        if let Some((used, limit)) = parse_pair_bytes(&parsed.mem_usage) {
            entry.mem_seen = true;
            entry.mem_used_sum = entry.mem_used_sum.saturating_add(used);
            entry.mem_limit_sum = entry.mem_limit_sum.saturating_add(limit);
        }

        if let Some((rx, tx)) = parse_pair_bytes(&parsed.net_io) {
            entry.net_seen = true;
            entry.net_rx_sum = entry.net_rx_sum.saturating_add(rx);
            entry.net_tx_sum = entry.net_tx_sum.saturating_add(tx);
        }

        if let Some((read, write)) = parse_pair_bytes(&parsed.block_io) {
            entry.block_seen = true;
            entry.block_read_sum = entry.block_read_sum.saturating_add(read);
            entry.block_write_sum = entry.block_write_sum.saturating_add(write);
        }

        if let Some(pids) = parse_u64_str(&parsed.pids) {
            entry.pids_seen = true;
            entry.pids_sum = entry.pids_sum.saturating_add(pids);
        }
    }

    let out = aggregates
        .into_iter()
        .map(|(service, agg)| (service, agg.into_sample()))
        .collect::<BTreeMap<_, _>>();
    Ok(out)
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct DockerStatsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "CPUPerc")]
    cpu_perc: String,
    #[serde(rename = "MemUsage")]
    mem_usage: String,
    #[serde(rename = "NetIO")]
    net_io: String,
    #[serde(rename = "BlockIO")]
    block_io: String,
    #[serde(rename = "PIDs")]
    pids: String,
}

#[cfg(test)]
fn parse_cpu_percent(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().trim_end_matches('%').trim();
    cleaned.parse::<f64>().ok()
}

#[cfg(test)]
fn parse_u64_str(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

#[cfg(test)]
fn parse_pair_bytes(raw: &str) -> Option<(u64, u64)> {
    let (left, right) = raw.split_once('/')?;
    let a = parse_size_to_bytes(left)?;
    let b = parse_size_to_bytes(right)?;
    Some((a, b))
}

#[cfg(test)]
fn parse_size_to_bytes(input: &str) -> Option<u64> {
    let trimmed = input
        .trim()
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | ','));
    if trimmed.is_empty() {
        return None;
    }

    let mut split_idx = None;
    for (idx, ch) in trimmed.char_indices() {
        if !(ch.is_ascii_digit() || ch == '.') {
            split_idx = Some(idx);
            break;
        }
    }
    let idx = split_idx.unwrap_or(trimmed.len());
    if idx == 0 {
        return None;
    }

    let num = trimmed[..idx].parse::<f64>().ok()?;
    let unit = trimmed[idx..].trim().to_ascii_uppercase();
    let factor = match unit.as_str() {
        "" | "B" => 1.0,
        "K" | "KB" | "KIB" => 1024.0,
        "M" | "MB" | "MIB" => 1024.0 * 1024.0,
        "G" | "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let value = (num * factor).round();
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(value as u64)
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::types::{ComposeConfig, StackBackupConfig},
        models::{ServiceSeed, StackRecord},
    };

    #[test]
    fn partial_sample_warning_state_expires_recreated_container_ids() {
        let now = Instant::now();
        let mut warnings = BTreeMap::new();

        assert!(should_emit_partial_sample_warning(
            &mut warnings,
            "project:container-old".to_string(),
            now,
        ));
        assert!(!should_emit_partial_sample_warning(
            &mut warnings,
            "project:container-old".to_string(),
            now + Duration::from_secs(1),
        ));
        assert!(should_emit_partial_sample_warning(
            &mut warnings,
            "project:container-new".to_string(),
            now + PARTIAL_SAMPLE_WARN_INTERVAL,
        ));
        assert_eq!(warnings.len(), 1);
        assert!(warnings.contains_key("project:container-new"));
    }

    #[derive(Clone)]
    struct TestProjectBehavior {
        delay: Duration,
        samples: BTreeMap<String, ServiceResourceSample>,
    }

    #[derive(Default)]
    struct TestCollectorState {
        behaviors: BTreeMap<String, TestProjectBehavior>,
        calls: BTreeMap<String, usize>,
        inflight: BTreeMap<String, usize>,
        max_inflight: BTreeMap<String, usize>,
        starts: BTreeMap<String, Vec<Instant>>,
    }

    #[derive(Clone)]
    struct TestHistoryCollector {
        state: Arc<Mutex<TestCollectorState>>,
    }

    impl TestHistoryCollector {
        fn new(behaviors: BTreeMap<String, TestProjectBehavior>) -> Self {
            Self {
                state: Arc::new(Mutex::new(TestCollectorState {
                    behaviors,
                    ..Default::default()
                })),
            }
        }

        async fn call_count(&self, project: &str) -> usize {
            let state = self.state.lock().await;
            state.calls.get(project).copied().unwrap_or(0)
        }

        async fn max_inflight(&self, project: &str) -> usize {
            let state = self.state.lock().await;
            state.max_inflight.get(project).copied().unwrap_or(0)
        }

        async fn start_times(&self, project: &str) -> Vec<Instant> {
            let state = self.state.lock().await;
            state.starts.get(project).cloned().unwrap_or_default()
        }
    }

    #[async_trait]
    impl ResourceCollector for TestHistoryCollector {
        async fn collect_project_service_aggregates(
            &self,
            compose_project: &str,
        ) -> anyhow::Result<ResourceCollection> {
            let behavior = {
                let mut state = self.state.lock().await;
                let behavior = state
                    .behaviors
                    .get(compose_project)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing test project {compose_project}"))?;
                *state.calls.entry(compose_project.to_string()).or_default() += 1;
                let inflight = state
                    .inflight
                    .entry(compose_project.to_string())
                    .or_default();
                *inflight += 1;
                let current_inflight = *inflight;
                let max_inflight = state
                    .max_inflight
                    .entry(compose_project.to_string())
                    .or_default();
                *max_inflight = (*max_inflight).max(current_inflight);
                state
                    .starts
                    .entry(compose_project.to_string())
                    .or_default()
                    .push(Instant::now());
                behavior
            };

            tokio::time::sleep(behavior.delay).await;

            let mut state = self.state.lock().await;
            if let Some(inflight) = state.inflight.get_mut(compose_project) {
                *inflight = inflight.saturating_sub(1);
            }
            Ok(ResourceCollection {
                samples: behavior.samples,
                failures: Vec::new(),
            })
        }
    }

    fn make_entry(subscribers: usize) -> Arc<SamplerEntry> {
        let (tx, _rx) = broadcast::channel(4);
        Arc::new(SamplerEntry {
            tx,
            subscribers: Arc::new(AtomicUsize::new(subscribers)),
        })
    }

    #[test]
    fn normalize_sample_interval_seconds_uses_five_seconds_default() {
        assert_eq!(normalize_sample_interval_seconds(5), 5);
        assert_eq!(normalize_sample_interval_seconds(10), 10);
        assert_eq!(normalize_sample_interval_seconds(30), 30);
        assert_eq!(normalize_sample_interval_seconds(60), 60);
        assert_eq!(normalize_sample_interval_seconds(300), 300);
        assert_eq!(normalize_sample_interval_seconds(7), 5);
    }

    #[test]
    fn try_remove_idle_sampler_entry_removes_when_subscribers_is_zero() {
        let mut map = BTreeMap::new();
        let entry = make_entry(0);
        map.insert("svc".to_string(), entry.clone());

        assert!(try_remove_idle_sampler_entry(&mut map, "svc", &entry));
        assert!(map.is_empty());
    }

    #[test]
    fn try_remove_idle_sampler_entry_keeps_entry_when_subscribers_exist() {
        let mut map = BTreeMap::new();
        let entry = make_entry(1);
        map.insert("svc".to_string(), entry.clone());

        assert!(!try_remove_idle_sampler_entry(&mut map, "svc", &entry));
        assert!(map.contains_key("svc"));
    }

    #[test]
    fn advance_fixed_cadence_skips_overdue_ticks() {
        let base = Instant::now();
        let mut next_run_at = base;

        let skipped = advance_fixed_cadence(
            &mut next_run_at,
            Duration::from_secs(5),
            base + Duration::from_secs(12),
        );

        assert_eq!(skipped, 2);
        assert_eq!(next_run_at.duration_since(base), Duration::from_secs(15));
    }

    #[tokio::test]
    async fn history_project_worker_does_not_overlap_or_backlog_overdue_ticks() {
        let db = open_test_db().await;
        seed_stack_services(&db, "stack-slow", &[("svc-slow", "slow-api")]).await;

        let collector = Arc::new(TestHistoryCollector::new(BTreeMap::from([(
            "slow-project".to_string(),
            TestProjectBehavior {
                delay: Duration::from_millis(95),
                samples: BTreeMap::from([("slow-api".to_string(), make_sample(62.5, 2_048))]),
            },
        )])));
        let snapshot = ProjectHistorySnapshot {
            compose_project: "slow-project".to_string(),
            interval: Duration::from_millis(40),
            services: vec![ProjectHistoryTarget {
                service_id: "svc-slow".to_string(),
                service_name: "slow-api".to_string(),
            }],
        };

        let (tx, rx) = watch::channel(snapshot);
        spawn_project_history_worker(db.clone(), collector.clone(), rx);
        wait_for_call_count(&collector, "slow-project", 2).await;
        drop(tx);
        tokio::time::sleep(Duration::from_millis(20)).await;

        let starts = collector.start_times("slow-project").await;
        assert!(
            starts.len() >= 2,
            "expected at least two runs, got {}",
            starts.len()
        );
        assert_eq!(collector.max_inflight("slow-project").await, 1);
        assert!(
            starts[1].duration_since(starts[0]) >= Duration::from_millis(110),
            "expected overdue ticks to be skipped instead of immediate catch-up: {starts:?}"
        );

        let rows = db
            .list_service_resource_samples_since("svc-slow", "1970-01-01T00:00:00Z")
            .await
            .unwrap();
        assert!(
            !rows.is_empty(),
            "expected history worker to persist at least one sample"
        );
    }

    #[tokio::test]
    async fn slow_project_worker_does_not_block_fast_project_worker() {
        let db = open_test_db().await;
        seed_stack_services(
            &db,
            "stack-mixed",
            &[("svc-fast", "fast-api"), ("svc-slow", "slow-api")],
        )
        .await;

        let collector = Arc::new(TestHistoryCollector::new(BTreeMap::from([
            (
                "fast-project".to_string(),
                TestProjectBehavior {
                    delay: Duration::from_millis(5),
                    samples: BTreeMap::from([("fast-api".to_string(), make_sample(18.0, 1_024))]),
                },
            ),
            (
                "slow-project".to_string(),
                TestProjectBehavior {
                    delay: Duration::from_millis(95),
                    samples: BTreeMap::from([("slow-api".to_string(), make_sample(77.0, 4_096))]),
                },
            ),
        ])));

        let (fast_tx, fast_rx) = watch::channel(ProjectHistorySnapshot {
            compose_project: "fast-project".to_string(),
            interval: Duration::from_millis(40),
            services: vec![ProjectHistoryTarget {
                service_id: "svc-fast".to_string(),
                service_name: "fast-api".to_string(),
            }],
        });
        let (slow_tx, slow_rx) = watch::channel(ProjectHistorySnapshot {
            compose_project: "slow-project".to_string(),
            interval: Duration::from_millis(40),
            services: vec![ProjectHistoryTarget {
                service_id: "svc-slow".to_string(),
                service_name: "slow-api".to_string(),
            }],
        });

        spawn_project_history_worker(db.clone(), collector.clone(), fast_rx);
        spawn_project_history_worker(db.clone(), collector.clone(), slow_rx);
        wait_for_call_count(&collector, "fast-project", 4).await;
        drop(fast_tx);
        drop(slow_tx);
        tokio::time::sleep(Duration::from_millis(20)).await;

        let fast_calls = collector.call_count("fast-project").await;
        let slow_calls = collector.call_count("slow-project").await;
        assert!(
            fast_calls >= slow_calls.saturating_add(2),
            "fast worker should continue sampling while slow worker lags: fast={fast_calls} slow={slow_calls}"
        );

        let fast_rows = db
            .list_service_resource_samples_since("svc-fast", "1970-01-01T00:00:00Z")
            .await
            .unwrap();
        let slow_rows = db
            .list_service_resource_samples_since("svc-slow", "1970-01-01T00:00:00Z")
            .await
            .unwrap();
        assert!(
            fast_rows.len() >= slow_rows.len().saturating_add(2),
            "expected fast project history cadence to stay ahead: fast={} slow={}",
            fast_rows.len(),
            slow_rows.len()
        );
    }

    async fn open_test_db() -> Db {
        let path = std::env::temp_dir().join(format!(
            "dockrev-resource-usage-{}.sqlite",
            ulid::Ulid::new()
        ));
        Db::open(&path).await.unwrap()
    }

    async fn seed_stack_services(db: &Db, stack_id: &str, services: &[(&str, &str)]) {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let stack = StackRecord {
            id: stack_id.to_string(),
            name: stack_id.to_string(),
            archived: false,
            compose: ComposeConfig {
                kind: "compose".to_string(),
                compose_files: vec!["compose.yml".to_string()],
                env_file: None,
            },
            backup: StackBackupConfig::default(),
            services: Vec::new(),
        };
        let seeds = services
            .iter()
            .map(|(service_id, service_name)| ServiceSeed {
                id: (*service_id).to_string(),
                name: (*service_name).to_string(),
                image_ref: format!("ghcr.io/acme/{service_name}:latest"),
                image_tag: "latest".to_string(),
                homepage: None,
                update_guard: None,
                auto_rollback: false,
                backup_bind_paths: BTreeMap::new(),
                backup_volume_names: BTreeMap::new(),
            })
            .collect::<Vec<_>>();
        db.insert_stack(&stack, &seeds, &now).await.unwrap();
    }

    fn make_sample(cpu_percent: f64, mem_used_bytes: u64) -> ServiceResourceSample {
        ServiceResourceSample {
            sampled_at: String::new(),
            cpu_percent,
            mem_used_bytes: Some(mem_used_bytes),
            mem_limit_bytes: Some(16_384),
            net_rx_bytes: Some(10_000),
            net_tx_bytes: Some(11_000),
            block_read_bytes: Some(12_000),
            block_write_bytes: Some(13_000),
            pids: Some(7),
            container_count: 1,
        }
    }

    async fn wait_for_call_count(collector: &TestHistoryCollector, project: &str, minimum: usize) {
        for _ in 0..100 {
            if collector.call_count(project).await >= minimum {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {project} to reach {minimum} calls");
    }
}
