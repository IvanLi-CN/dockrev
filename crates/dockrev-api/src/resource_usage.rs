use std::{
    collections::{BTreeMap, BTreeSet},
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
    metrics_store::MetricsStore,
};
#[cfg(test)]
use serde::Deserialize;

pub const RESOURCE_MONITOR_RETENTION_DAYS: u32 = 1;
pub const JOB_HISTORY_RETENTION_DAYS: u32 = 30;
pub const DEFAULT_SAMPLE_INTERVAL_SECONDS: u64 = 5;
const PARTIAL_SAMPLE_WARN_INTERVAL: Duration = Duration::from_secs(60);
const RESOURCE_COLLECTION_CACHE_MAX_AGE: Duration = Duration::from_secs(1);
const RESOURCE_HISTORY_GC_INTERVAL: Duration = Duration::from_secs(60);
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
        "7d" => Some(7 * 24 * 60 * 60),
        "30d" => Some(30 * 24 * 60 * 60),
        _ => None,
    }
}

#[derive(Clone)]
pub struct RealtimeSamplerHub {
    db: Db,
    coordinator: ResourceSamplingCoordinator,
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

#[derive(Clone)]
enum CachedProjectCollection {
    Ready(ResourceCollection),
    Failed(String),
}

impl CachedProjectCollection {
    fn into_result(self) -> anyhow::Result<ResourceCollection> {
        match self {
            Self::Ready(collection) => Ok(collection),
            Self::Failed(error) => Err(anyhow::anyhow!(error)),
        }
    }
}

struct CollectionCancellationGuard {
    state: Arc<Mutex<ResourceSamplingCoordinatorState>>,
    projects: BTreeSet<String>,
    armed: bool,
}

impl CollectionCancellationGuard {
    fn new(
        state: Arc<Mutex<ResourceSamplingCoordinatorState>>,
        projects: BTreeSet<String>,
    ) -> Self {
        Self {
            state,
            projects,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CollectionCancellationGuard {
    fn drop(&mut self) {
        if !self.armed || self.projects.is_empty() {
            return;
        }

        let state = self.state.clone();
        let projects = std::mem::take(&mut self.projects);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let mut state = state.lock().await;
            for project in projects {
                let Some(entry) = state.projects.get_mut(&project) else {
                    continue;
                };
                if !entry.in_flight {
                    continue;
                }
                entry.in_flight = false;
                entry.completed_at = None;
                entry.result = None;
                entry
                    .changed
                    .send_modify(|version| *version = version.wrapping_add(1));
            }
        });
    }
}

struct ProjectCollectionState {
    in_flight: bool,
    invalidated: bool,
    completed_at: Option<Instant>,
    result: Option<CachedProjectCollection>,
    changed: watch::Sender<u64>,
}

impl ProjectCollectionState {
    fn new() -> Self {
        let (changed, _) = watch::channel(0u64);
        Self {
            in_flight: false,
            invalidated: false,
            completed_at: None,
            result: None,
            changed,
        }
    }
}

fn evict_stale_project_collection_states(
    state: &mut ResourceSamplingCoordinatorState,
    requested_projects: &BTreeSet<String>,
    now: Instant,
) {
    state.projects.retain(|project, entry| {
        entry.in_flight
            || requested_projects.contains(project)
            || entry.completed_at.is_some_and(|completed_at| {
                now.saturating_duration_since(completed_at) < RESOURCE_COLLECTION_CACHE_MAX_AGE
            })
    });
}

#[derive(Default)]
struct ResourceSamplingCoordinatorState {
    projects: BTreeMap<String, ProjectCollectionState>,
}

#[derive(Clone)]
pub struct ResourceSamplingCoordinator {
    collector: Arc<dyn ResourceCollector>,
    state: Arc<Mutex<ResourceSamplingCoordinatorState>>,
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

    async fn collect_projects_service_aggregates(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeMap<String, ResourceCollection>> {
        let mut collections = BTreeMap::new();
        for compose_project in compose_projects {
            collections.insert(
                compose_project.clone(),
                self.collect_project_service_aggregates(compose_project)
                    .await?,
            );
        }
        Ok(collections)
    }
}

#[derive(Clone)]
struct DockerApiResourceCollector {
    client: DockerEngineClient,
}

impl DockerApiResourceCollector {
    fn from_client(client: DockerEngineClient) -> Self {
        Self { client }
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

    async fn collect_projects_service_aggregates(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeMap<String, ResourceCollection>> {
        Ok(self
            .client
            .collect_projects_service_samples(compose_projects)
            .await?
            .into_iter()
            .map(|(project, collection)| (project, collection.into()))
            .collect())
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

impl ResourceSamplingCoordinator {
    pub fn with_docker_engine(client: DockerEngineClient) -> Self {
        Self::with_collector(Arc::new(DockerApiResourceCollector::from_client(client)))
    }

    fn with_collector(collector: Arc<dyn ResourceCollector>) -> Self {
        Self {
            collector,
            state: Arc::new(Mutex::new(ResourceSamplingCoordinatorState::default())),
        }
    }

    async fn clear_cached_collections(&self) {
        let mut state = self.state.lock().await;
        state.projects.retain(|_, entry| {
            if entry.in_flight {
                entry.invalidated = true;
                true
            } else {
                false
            }
        });
    }

    async fn collect_project(&self, compose_project: &str) -> anyhow::Result<ResourceCollection> {
        self.collect_projects(&BTreeSet::from([compose_project.to_string()]))
            .await?
            .remove(compose_project)
            .unwrap_or_else(|| Ok(ResourceCollection::default()))
    }

    async fn collect_projects(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeMap<String, anyhow::Result<ResourceCollection>>> {
        'retry: loop {
            let now = Instant::now();
            let mut ready = BTreeMap::new();
            let mut owned = BTreeSet::new();
            let mut waiting = Vec::new();
            {
                let mut state = self.state.lock().await;
                evict_stale_project_collection_states(&mut state, compose_projects, now);
                for compose_project in compose_projects {
                    let entry = state
                        .projects
                        .entry(compose_project.clone())
                        .or_insert_with(ProjectCollectionState::new);
                    if let (Some(completed_at), Some(result)) =
                        (entry.completed_at, entry.result.clone())
                        && now.saturating_duration_since(completed_at)
                            < RESOURCE_COLLECTION_CACHE_MAX_AGE
                    {
                        ready.insert(compose_project.clone(), result.into_result());
                    } else if entry.in_flight {
                        waiting.push((compose_project.clone(), entry.changed.subscribe()));
                    } else {
                        entry.in_flight = true;
                        entry.invalidated = false;
                        owned.insert(compose_project.clone());
                    }
                }
            }

            if !owned.is_empty() {
                let mut cancellation_guard =
                    CollectionCancellationGuard::new(self.state.clone(), owned.clone());
                let batch_result = self
                    .collector
                    .collect_projects_service_aggregates(&owned)
                    .await;
                let mut state = self.state.lock().await;
                for compose_project in &owned {
                    let result = match &batch_result {
                        Ok(collections) => CachedProjectCollection::Ready(
                            collections
                                .get(compose_project)
                                .cloned()
                                .unwrap_or_default(),
                        ),
                        Err(error) => CachedProjectCollection::Failed(error.to_string()),
                    };
                    let entry = state
                        .projects
                        .get_mut(compose_project)
                        .expect("owned project state must exist");
                    entry.in_flight = false;
                    let invalidated = entry.invalidated;
                    entry.invalidated = false;
                    if invalidated {
                        let result = CachedProjectCollection::Failed(
                            "resource collection invalidated".to_string(),
                        );
                        entry.completed_at = Some(Instant::now());
                        entry.result = Some(result.clone());
                        entry
                            .changed
                            .send_modify(|version| *version = version.wrapping_add(1));
                        ready.insert(compose_project.clone(), result.into_result());
                        continue;
                    }
                    entry.completed_at = Some(Instant::now());
                    entry.result = Some(result.clone());
                    entry
                        .changed
                        .send_modify(|version| *version = version.wrapping_add(1));
                    ready.insert(compose_project.clone(), result.into_result());
                }
                cancellation_guard.disarm();
            }

            for (compose_project, mut changed) in waiting {
                let _ = changed.changed().await;
                let Some(result) = self
                    .state
                    .lock()
                    .await
                    .projects
                    .get(&compose_project)
                    .and_then(|entry| entry.result.clone())
                else {
                    continue 'retry;
                };
                ready.insert(compose_project, result.into_result());
            }
            return Ok(ready);
        }
    }
}

impl RealtimeSamplerHub {
    pub fn with_coordinator(db: Db, coordinator: ResourceSamplingCoordinator) -> Self {
        Self {
            db,
            coordinator,
            samplers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self::with_coordinator(
            db,
            ResourceSamplingCoordinator::with_collector(Arc::new(RunnerBackedResourceCollector {
                runner,
            })),
        )
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
        sample_for_target(&self.coordinator, &target).await
    }

    fn spawn_sampler_task(&self, service_id: String, entry: Arc<SamplerEntry>) {
        let db = self.db.clone();
        let coordinator = self.coordinator.clone();
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
                    coordinator.clear_cached_collections().await;
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

                match sample_for_target(&coordinator, &target).await {
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

pub fn spawn_history_sampler(
    db: Db,
    metrics: MetricsStore,
    coordinator: ResourceSamplingCoordinator,
) {
    spawn_history_gc_task(db.clone(), metrics.clone());
    tokio::spawn(async move {
        let mut interval = Duration::from_secs(DEFAULT_SAMPLE_INTERVAL_SECONDS);
        let mut next_run_at = Instant::now();

        loop {
            let settings = match db.get_resource_monitor_settings().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "resource monitor settings unavailable");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            if !settings.enabled {
                coordinator.clear_cached_collections().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let interval_seconds =
                normalize_sample_interval_seconds(settings.sample_interval_seconds);
            let configured_interval = Duration::from_secs(interval_seconds);
            if configured_interval != interval {
                interval = configured_interval;
                next_run_at = Instant::now();
            }
            let targets = match db.list_service_resource_targets().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "resource monitor history targets unavailable");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            if Instant::now() >= next_run_at {
                let snapshots = build_project_history_snapshots(targets, interval_seconds);
                let started_at = Instant::now();
                let outcomes = sample_history_cycle_once(&metrics, &coordinator, &snapshots).await;
                let duration = started_at.elapsed();
                let skipped_ticks =
                    advance_fixed_cadence(&mut next_run_at, interval, Instant::now());
                for snapshot in snapshots.values() {
                    let outcome = outcomes
                        .get(&snapshot.compose_project)
                        .expect("history cycle must return every project outcome");
                    log_project_history_run(snapshot, outcome, duration, skipped_ticks);
                }
                continue;
            }

            let poll_deadline = std::cmp::min(next_run_at, Instant::now() + Duration::from_secs(1));
            tokio::time::sleep_until(poll_deadline.into()).await;
        }
    });
}

fn spawn_history_gc_task(db: Db, metrics: MetricsStore) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = gc_history(&db, &metrics).await {
                tracing::warn!(error = %e, "resource monitor history gc failed");
            }
            tokio::time::sleep(RESOURCE_HISTORY_GC_INTERVAL).await;
        }
    });
}

async fn gc_history(db: &Db, metrics: &MetricsStore) -> anyhow::Result<()> {
    let active_service_ids = db
        .list_active_service_ids_for_metrics()
        .await
        .context("list active services for metrics gc")?;
    metrics
        .gc(&active_service_ids)
        .await
        .context("gc metrics store")?;
    let now = time::OffsetDateTime::now_utc();
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

async fn sample_history_cycle_once(
    metrics: &MetricsStore,
    coordinator: &ResourceSamplingCoordinator,
    snapshots: &BTreeMap<String, ProjectHistorySnapshot>,
) -> BTreeMap<String, ProjectHistoryRunOutcome> {
    let projects = snapshots.keys().cloned().collect::<BTreeSet<_>>();
    let collections = match coordinator.collect_projects(&projects).await {
        Ok(collections) => collections,
        Err(error) => {
            return snapshots
                .keys()
                .map(|project| {
                    (
                        project.clone(),
                        ProjectHistoryRunOutcome::error(anyhow::anyhow!(error.to_string())),
                    )
                })
                .collect();
        }
    };
    let sampled_at = match now_rfc3339() {
        Ok(value) => value,
        Err(error) => {
            let message = format!("create resource sample timestamp: {error}");
            return snapshots
                .keys()
                .map(|project| {
                    (
                        project.clone(),
                        ProjectHistoryRunOutcome::error(anyhow::anyhow!(message.clone())),
                    )
                })
                .collect();
        }
    };
    let mut outcomes = BTreeMap::new();
    let mut cycle_rows = Vec::new();
    for (project, snapshot) in snapshots {
        let outcome = match collections.get(project) {
            Some(Ok(collection)) => {
                let rows = sample_history_project_collection(snapshot, collection, &sampled_at);
                if rows.is_empty() {
                    ProjectHistoryRunOutcome::empty()
                } else {
                    let inserted = rows.len();
                    cycle_rows.extend(rows);
                    ProjectHistoryRunOutcome::ok(inserted)
                }
            }
            Some(Err(error)) => ProjectHistoryRunOutcome::error(anyhow::anyhow!(error.to_string())),
            None => ProjectHistoryRunOutcome::empty(),
        };
        outcomes.insert(project.clone(), outcome);
    }
    if !cycle_rows.is_empty()
        && let Err(error) = metrics
            .insert_samples(&cycle_rows)
            .await
            .context("commit resource sample cycle")
    {
        for outcome in outcomes.values_mut().filter(|outcome| outcome.inserted > 0) {
            *outcome = ProjectHistoryRunOutcome::error(anyhow::anyhow!(error.to_string()));
        }
    }
    outcomes
}

fn sample_history_project_collection(
    snapshot: &ProjectHistorySnapshot,
    collection: &ResourceCollection,
    sampled_at: &str,
) -> Vec<ServiceResourceSampleInput> {
    log_partial_collection_failures(&snapshot.compose_project, collection);
    snapshot
        .services
        .iter()
        .filter_map(|target| {
            let sample = collection.samples.get(&target.service_name)?;
            Some(ServiceResourceSampleInput {
                service_id: target.service_id.clone(),
                sampled_at: sampled_at.to_string(),
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
        .collect::<Vec<_>>()
}

async fn sample_for_target(
    coordinator: &ResourceSamplingCoordinator,
    target: &ServiceResourceTarget,
) -> anyhow::Result<Option<ServiceResourceSample>> {
    let collection = coordinator
        .collect_project(&target.compose_project)
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
        net_rx_rate_bps: None,
        net_tx_rate_bps: None,
        block_read_bytes: sample.block_read_bytes,
        block_write_bytes: sample.block_write_bytes,
        block_read_rate_bps: None,
        block_write_rate_bps: None,
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
            net_rx_rate_bps: None,
            net_tx_rate_bps: None,
            block_read_bytes: self.block_seen.then_some(self.block_read_sum),
            block_write_bytes: self.block_seen.then_some(self.block_write_sum),
            block_read_rate_bps: None,
            block_write_rate_bps: None,
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
#[path = "resource_usage_tests.rs"]
mod tests;
