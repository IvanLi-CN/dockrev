use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
    time::MissedTickBehavior,
};

use crate::{
    api::types::{ServiceLogEventEnvelope, ServiceLogLine, ServiceLogSnapshotResponse},
    db::{Db, ServiceResourceTarget},
    runner::{CommandRunner, CommandSpec},
};

pub const DEFAULT_SERVICE_LOG_TAIL: usize = 500;
pub const MAX_SERVICE_LOG_TAIL: usize = 2_000;
pub const SERVICE_LOG_RING_BUFFER_LIMIT: usize = 2_000;
const SERVICE_LOG_BROADCAST_CAPACITY: usize = 512;
const SERVICE_LOG_IDLE_GRACE_SECONDS: u64 = 10;
const SERVICE_LOG_SCAN_INTERVAL_MS: u64 = 1_000;
const SERVICE_LOG_CMD_TIMEOUT_SECONDS: u64 = 20;
const SERVICE_LOG_FOLLOW_TIMEOUT_SECONDS: u64 = 60 * 60 * 24;

#[derive(Clone, Debug)]
pub enum ServiceLogRealtimeMessage {
    Event(ServiceLogEventEnvelope),
}

pub struct ServiceLogSubscription {
    receiver: broadcast::Receiver<ServiceLogRealtimeMessage>,
    _guard: ServiceLogSubscriptionGuard,
}

impl ServiceLogSubscription {
    pub async fn recv(&mut self) -> Result<ServiceLogRealtimeMessage, broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

struct ServiceLogSubscriptionGuard {
    subscribers: Arc<AtomicUsize>,
}

impl Drop for ServiceLogSubscriptionGuard {
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

#[derive(Clone)]
pub struct ServiceLogHub {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    entries: Arc<Mutex<BTreeMap<String, Arc<ServiceLogEntry>>>>,
}

struct ServiceLogEntry {
    tx: broadcast::Sender<ServiceLogRealtimeMessage>,
    subscribers: Arc<AtomicUsize>,
    state: Arc<Mutex<ServiceLogState>>,
}

#[derive(Clone, Debug, Default)]
struct ServiceLogState {
    buffer: VecDeque<ServiceLogEventEnvelope>,
    last_event_id: u64,
}

#[derive(Clone, Debug)]
struct ServiceLogCollectorState {
    lines: Vec<ServiceLogLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ServiceLogSource {
    id: String,
    name: String,
}

#[derive(Debug)]
enum CollectorMessage {
    Line(ServiceLogLine),
}

impl ServiceLogHub {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            db,
            runner,
            entries: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub async fn subscribe(&self, service_id: &str) -> ServiceLogSubscription {
        let service_id = service_id.to_string();
        let entry = {
            let mut map = self.entries.lock().await;
            if let Some(existing) = map.get(&service_id) {
                existing.subscribers.fetch_add(1, Ordering::SeqCst);
                existing.clone()
            } else {
                let (tx, _rx) = broadcast::channel(SERVICE_LOG_BROADCAST_CAPACITY);
                let created = Arc::new(ServiceLogEntry {
                    tx,
                    subscribers: Arc::new(AtomicUsize::new(1)),
                    state: Arc::new(Mutex::new(ServiceLogState::default())),
                });
                map.insert(service_id.clone(), created.clone());
                self.spawn_collector_task(service_id.clone(), created.clone());
                created
            }
        };

        ServiceLogSubscription {
            receiver: entry.tx.subscribe(),
            _guard: ServiceLogSubscriptionGuard {
                subscribers: entry.subscribers.clone(),
            },
        }
    }

    pub async fn snapshot(
        &self,
        service_id: &str,
        requested_tail: usize,
    ) -> anyhow::Result<Option<ServiceLogSnapshotResponse>> {
        let entry = {
            let map = self.entries.lock().await;
            map.get(service_id).cloned()
        };
        if let Some(entry) = entry
            && let Some(snapshot) = snapshot_from_entry(&entry, service_id, requested_tail).await
        {
            return Ok(Some(snapshot));
        }

        let target = self
            .db
            .get_service_resource_target(service_id)
            .await
            .context("lookup service target for logs")?;
        let Some(target) = target else {
            return Ok(None);
        };

        let tail = normalize_tail(requested_tail);
        let collector = collect_service_logs(self.runner.as_ref(), &target, tail).await?;
        let last_event_id = collector.lines.len() as u64;

        Ok(Some(ServiceLogSnapshotResponse {
            service_id: service_id.to_string(),
            lines: collector.lines,
            last_event_id,
            buffer_limit: SERVICE_LOG_RING_BUFFER_LIMIT,
        }))
    }

    pub async fn events_since(
        &self,
        service_id: &str,
        after_id: u64,
    ) -> anyhow::Result<Option<ServiceLogReplay>> {
        let entry = {
            let map = self.entries.lock().await;
            map.get(service_id).cloned()
        };

        let Some(entry) = entry else {
            let target = self
                .db
                .get_service_resource_target(service_id)
                .await
                .context("lookup service target for logs replay")?;
            if target.is_none() {
                return Ok(None);
            }
            return Ok(Some(ServiceLogReplay::default()));
        };

        let state = entry.state.lock().await;
        if after_id == 0 {
            return Ok(Some(ServiceLogReplay {
                events: state.buffer.iter().cloned().collect(),
                reset_required: false,
            }));
        }

        let first_id = state
            .buffer
            .front()
            .map(ServiceLogEventEnvelope::id)
            .unwrap_or(0);
        if after_id < first_id.saturating_sub(1) && first_id > 0 {
            return Ok(Some(ServiceLogReplay {
                events: Vec::new(),
                reset_required: true,
            }));
        }

        let events = state
            .buffer
            .iter()
            .filter(|event| event.id() > after_id)
            .cloned()
            .collect();
        Ok(Some(ServiceLogReplay {
            events,
            reset_required: false,
        }))
    }

    #[cfg(test)]
    pub async fn seed_test_buffer(&self, service_id: &str, events: Vec<ServiceLogEventEnvelope>) {
        let entry = {
            let mut map = self.entries.lock().await;
            map.entry(service_id.to_string())
                .or_insert_with(|| {
                    let (tx, _rx) = broadcast::channel(SERVICE_LOG_BROADCAST_CAPACITY);
                    Arc::new(ServiceLogEntry {
                        tx,
                        subscribers: Arc::new(AtomicUsize::new(0)),
                        state: Arc::new(Mutex::new(ServiceLogState::default())),
                    })
                })
                .clone()
        };

        let mut state = entry.state.lock().await;
        state.buffer = events.into_iter().collect();
        state.last_event_id = state
            .buffer
            .back()
            .map(ServiceLogEventEnvelope::id)
            .unwrap_or(0);
    }

    fn spawn_collector_task(&self, service_id: String, entry: Arc<ServiceLogEntry>) {
        let db = self.db.clone();
        let runner = self.runner.clone();
        let entries = self.entries.clone();

        tokio::spawn(async move {
            let (collector_tx, mut collector_rx) = mpsc::unbounded_channel::<CollectorMessage>();
            let mut idle_since: Option<Instant> = None;
            let mut next_event_id = 0u64;
            let mut active_source: Option<ServiceLogSource> = None;
            let mut follower_handle: Option<JoinHandle<()>> = None;
            let mut scan =
                tokio::time::interval(Duration::from_millis(SERVICE_LOG_SCAN_INTERVAL_MS));
            scan.set_missed_tick_behavior(MissedTickBehavior::Delay);

            if let Ok(Some(target)) = db.get_service_resource_target(&service_id).await
                && let Ok(initial) =
                    collect_service_logs(runner.as_ref(), &target, DEFAULT_SERVICE_LOG_TAIL).await
            {
                next_event_id = seed_entry_with_snapshot(&entry, &service_id, initial).await;
                if let Ok(source) = discover_active_source(
                    runner.as_ref(),
                    &target.compose_project,
                    &target.service_name,
                )
                .await
                {
                    active_source = source;
                }
            }

            loop {
                tokio::select! {
                    _ = scan.tick() => {
                        let subscribers = entry.subscribers.load(Ordering::SeqCst);
                        if subscribers == 0 {
                            abort_follower(&mut follower_handle);
                            match idle_since {
                                None => idle_since = Some(Instant::now()),
                                Some(started)
                                    if started.elapsed() >= Duration::from_secs(SERVICE_LOG_IDLE_GRACE_SECONDS) =>
                                {
                                    let removed = {
                                        let mut map = entries.lock().await;
                                        try_remove_idle_entry(&mut map, &service_id, &entry)
                                    };
                                    if removed {
                                        break;
                                    }
                                    idle_since = None;
                                }
                                _ => {}
                            }
                            continue;
                        }
                        idle_since = None;

                        prune_finished_follower(&mut follower_handle);

                        let target = match db.get_service_resource_target(&service_id).await {
                            Ok(Some(target)) => target,
                            Ok(None) => {
                                next_event_id = publish_reset_if_source_changed(
                                    &entry,
                                    &service_id,
                                    next_event_id,
                                    &mut active_source,
                                    &mut follower_handle,
                                    None,
                                    "service_not_found",
                                ).await;
                                continue;
                            }
                            Err(_) => continue,
                        };

                        let next_source = match discover_active_source(
                            runner.as_ref(),
                            &target.compose_project,
                            &target.service_name,
                        ).await {
                            Ok(source) => source,
                            Err(_) => continue,
                        };

                        next_event_id = publish_reset_if_source_changed(
                            &entry,
                            &service_id,
                            next_event_id,
                            &mut active_source,
                            &mut follower_handle,
                            next_source,
                            "log_source_changed",
                        ).await;

                        if follower_handle.is_none() && let Some(source) = active_source.clone() {
                            follower_handle = Some(spawn_source_follower(
                                runner.clone(),
                                collector_tx.clone(),
                                source,
                            ));
                        }
                    }
                    maybe_message = collector_rx.recv() => {
                        let Some(message) = maybe_message else {
                            break;
                        };
                        match message {
                            CollectorMessage::Line(line) => {
                                next_event_id = next_event_id.saturating_add(1);
                                let event = ServiceLogEventEnvelope::Line {
                                    id: next_event_id,
                                    service_id: service_id.clone(),
                                    line,
                                };
                                push_event(&entry, event).await;
                            }
                        }
                    }
                }
            }

            abort_follower(&mut follower_handle);
        });
    }
}

#[derive(Clone, Debug, Default)]
pub struct ServiceLogReplay {
    pub events: Vec<ServiceLogEventEnvelope>,
    pub reset_required: bool,
}

fn try_remove_idle_entry(
    map: &mut BTreeMap<String, Arc<ServiceLogEntry>>,
    service_id: &str,
    entry: &Arc<ServiceLogEntry>,
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

async fn snapshot_from_entry(
    entry: &Arc<ServiceLogEntry>,
    service_id: &str,
    requested_tail: usize,
) -> Option<ServiceLogSnapshotResponse> {
    let state = entry.state.lock().await;
    if state.last_event_id == 0 && state.buffer.is_empty() {
        return None;
    }

    let tail = normalize_tail(requested_tail);
    let mut lines = state
        .buffer
        .iter()
        .filter_map(|event| match event {
            ServiceLogEventEnvelope::Line { line, .. } => Some(line.clone()),
            ServiceLogEventEnvelope::Reset { .. } => None,
        })
        .collect::<Vec<_>>();
    if lines.len() > tail {
        lines = lines.split_off(lines.len() - tail);
    }

    Some(ServiceLogSnapshotResponse {
        service_id: service_id.to_string(),
        lines,
        last_event_id: state.last_event_id,
        buffer_limit: SERVICE_LOG_RING_BUFFER_LIMIT,
    })
}

async fn seed_entry_with_snapshot(
    entry: &Arc<ServiceLogEntry>,
    service_id: &str,
    snapshot: ServiceLogCollectorState,
) -> u64 {
    let buffer = snapshot
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| ServiceLogEventEnvelope::Line {
            id: (index + 1) as u64,
            service_id: service_id.to_string(),
            line,
        })
        .collect::<VecDeque<_>>();
    let last_event_id = buffer.back().map(ServiceLogEventEnvelope::id).unwrap_or(0);

    let mut state = entry.state.lock().await;
    state.buffer = buffer;
    state.last_event_id = last_event_id;
    last_event_id
}

async fn publish_reset_if_source_changed(
    entry: &Arc<ServiceLogEntry>,
    service_id: &str,
    mut next_event_id: u64,
    active_source: &mut Option<ServiceLogSource>,
    follower_handle: &mut Option<JoinHandle<()>>,
    next_source: Option<ServiceLogSource>,
    reason: &str,
) -> u64 {
    if *active_source == next_source {
        return next_event_id;
    }

    abort_follower(follower_handle);
    *active_source = next_source;

    next_event_id = next_event_id.saturating_add(1);
    push_event(
        entry,
        ServiceLogEventEnvelope::Reset {
            id: next_event_id,
            service_id: service_id.to_string(),
            reason: reason.to_string(),
        },
    )
    .await;
    next_event_id
}

async fn push_event(entry: &Arc<ServiceLogEntry>, event: ServiceLogEventEnvelope) {
    {
        let mut state = entry.state.lock().await;
        state.last_event_id = event.id();
        state.buffer.push_back(event.clone());
        while state.buffer.len() > SERVICE_LOG_RING_BUFFER_LIMIT {
            state.buffer.pop_front();
        }
    }
    let _ = entry.tx.send(ServiceLogRealtimeMessage::Event(event));
}

fn abort_follower(handle: &mut Option<JoinHandle<()>>) {
    if let Some(existing) = handle.take() {
        existing.abort();
    }
}

fn prune_finished_follower(handle: &mut Option<JoinHandle<()>>) {
    if handle.as_ref().is_some_and(JoinHandle::is_finished) {
        *handle = None;
    }
}

fn spawn_source_follower(
    runner: Arc<dyn CommandRunner>,
    tx: mpsc::UnboundedSender<CollectorMessage>,
    source: ServiceLogSource,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let tx_for_stdout = tx.clone();
        let mut on_stdout = move |chunk: String| {
            for raw_line in chunk.lines() {
                if let Some(line) = parse_service_log_line(raw_line) {
                    let _ = tx_for_stdout.send(CollectorMessage::Line(line));
                }
            }
        };
        let mut on_stderr = |_chunk: String| {};

        let _ = runner
            .run_stream(
                CommandSpec {
                    program: "docker".to_string(),
                    args: vec![
                        "logs".to_string(),
                        "--timestamps".to_string(),
                        "--follow".to_string(),
                        "--tail".to_string(),
                        "0".to_string(),
                        source.id,
                    ],
                    env: Vec::new(),
                },
                Duration::from_secs(SERVICE_LOG_FOLLOW_TIMEOUT_SECONDS),
                &mut on_stdout,
                &mut on_stderr,
            )
            .await;
    })
}

fn normalize_tail(value: usize) -> usize {
    value.clamp(1, MAX_SERVICE_LOG_TAIL)
}

async fn collect_service_logs(
    runner: &dyn CommandRunner,
    target: &ServiceResourceTarget,
    tail: usize,
) -> anyhow::Result<ServiceLogCollectorState> {
    let source = match discover_active_source(runner, &target.compose_project, &target.service_name)
        .await?
    {
        Some(source) => source,
        None => return Ok(ServiceLogCollectorState { lines: Vec::new() }),
    };

    let output = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "logs".to_string(),
                    "--timestamps".to_string(),
                    "--tail".to_string(),
                    tail.to_string(),
                    source.id,
                ],
                env: Vec::new(),
            },
            Duration::from_secs(SERVICE_LOG_CMD_TIMEOUT_SECONDS),
        )
        .await
        .context("docker logs for service source")?;

    if output.status != 0 {
        return Err(anyhow::anyhow!(
            "docker logs failed status={} stderr={}",
            output.status,
            output.stderr
        ));
    }

    let mut lines = output
        .stdout
        .lines()
        .filter_map(parse_service_log_line)
        .collect::<Vec<_>>();
    if lines.len() > tail {
        lines = lines.split_off(lines.len() - tail);
    }

    Ok(ServiceLogCollectorState { lines })
}

async fn discover_active_source(
    runner: &dyn CommandRunner,
    compose_project: &str,
    service_name: &str,
) -> anyhow::Result<Option<ServiceLogSource>> {
    let ps = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.project={compose_project}"),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.service={service_name}"),
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
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if container_ids.is_empty() {
        return Ok(None);
    }

    let inspect = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args: {
                    let mut args = vec![
                        "inspect".to_string(),
                        "--format".to_string(),
                        "{{.Id}}\t{{.Name}}\t{{index .Config.Labels \"com.docker.compose.service\"}}".to_string(),
                    ];
                    args.extend(container_ids.iter().cloned());
                    args
                },
                env: Vec::new(),
            },
            Duration::from_secs(SERVICE_LOG_CMD_TIMEOUT_SECONDS),
        )
        .await?;

    if inspect.status != 0 {
        return Err(anyhow::anyhow!(
            "docker inspect failed status={} stderr={}",
            inspect.status,
            inspect.stderr
        ));
    }

    let mut sources = inspect
        .stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('\t');
            let id = parts.next()?.trim().to_string();
            let name = parts.next()?.trim().trim_start_matches('/').to_string();
            let inspect_service = parts.next()?.trim().to_string();
            if id.is_empty() || inspect_service != service_name {
                return None;
            }
            Some(ServiceLogSource { id, name })
        })
        .collect::<Vec<_>>();

    sources.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(sources.into_iter().next())
}

fn parse_service_log_line(raw_line: &str) -> Option<ServiceLogLine> {
    let trimmed = raw_line.trim_end_matches('\r').trim_end_matches('\n');
    if trimmed.is_empty() {
        return None;
    }
    let (ts, message) = split_docker_log_line(trimmed);
    let message = message.to_string();
    Some(ServiceLogLine {
        ts: ts.to_string(),
        raw: message.clone(),
        plain: message,
    })
}

fn split_docker_log_line(line: &str) -> (&str, &str) {
    if let Some((ts, rest)) = line.split_once(' ') {
        return (ts.trim(), rest.trim_start());
    }
    ("", line)
}
