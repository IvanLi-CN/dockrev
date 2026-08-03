use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use serde_json::{Number, Value};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
    time::MissedTickBehavior,
};

use crate::{
    api::types::{
        ServiceLogEventEnvelope, ServiceLogLine, ServiceLogMeta, ServiceLogMetaFormat,
        ServiceLogSnapshotResponse,
    },
    db::{Db, ServiceResourceTarget},
    runner::{CommandRunner, CommandSpec},
};

pub const DEFAULT_SERVICE_LOG_TAIL: usize = 500;
pub const MAX_SERVICE_LOG_TAIL: usize = 2_000;
pub const SERVICE_LOG_RING_BUFFER_LIMIT: usize = 2_000;
const SERVICE_LOG_BROADCAST_CAPACITY: usize = 512;
const SERVICE_LOG_SNAPSHOT_PHYSICAL_TAIL_MULTIPLIER: usize = 8;
const SERVICE_LOG_SNAPSHOT_PHYSICAL_TAIL_MIN_EXTRA: usize = 32;
const SERVICE_LOG_IDLE_GRACE_SECONDS: u64 = 10;
const SERVICE_LOG_SCAN_INTERVAL_MS: u64 = 1_000;
const SERVICE_LOG_CMD_TIMEOUT_SECONDS: u64 = 20;
const SERVICE_LOG_FOLLOW_TIMEOUT_SECONDS: u64 = 60 * 60 * 24;
const SERVICE_LOG_FOLLOW_GROUP_DEBOUNCE_MS: u64 = 75;

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
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<ServiceLogRawChunk>();
        let processor_tx = tx.clone();
        let processor_handle = tokio::spawn(async move {
            let mut parser = ServiceLogFrameParser::default();
            loop {
                let next_line = if parser.has_current() {
                    match tokio::time::timeout(
                        Duration::from_millis(SERVICE_LOG_FOLLOW_GROUP_DEBOUNCE_MS),
                        raw_rx.recv(),
                    )
                    .await
                    {
                        Ok(value) => value,
                        Err(_) => {
                            if let Some(line) = parser.finish() {
                                let _ = processor_tx.send(CollectorMessage::Line(line));
                            }
                            continue;
                        }
                    }
                } else {
                    raw_rx.recv().await
                };

                let Some(raw_line) = next_line else {
                    break;
                };
                if let Some(line) =
                    parser.push_physical_line(&raw_line.text, raw_line.forced_fragment)
                {
                    let _ = processor_tx.send(CollectorMessage::Line(line));
                }
            }
            if let Some(line) = parser.finish() {
                let _ = processor_tx.send(CollectorMessage::Line(line));
            }
        });

        {
            let stdout_tx = raw_tx.clone();
            let mut stdout_buffer = LineChunkBuffer::default();
            let stderr_tx = raw_tx.clone();
            let mut stderr_buffer = LineChunkBuffer::default();
            {
                let mut on_stdout = |chunk: Vec<u8>| {
                    stdout_buffer.push(&stdout_tx, &chunk);
                };
                let mut on_stderr = |chunk: Vec<u8>| {
                    stderr_buffer.push(&stderr_tx, &chunk);
                };

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
            }
            stdout_buffer.finish(&stdout_tx);
            stderr_buffer.finish(&stderr_tx);
        }
        drop(raw_tx);
        let _ = processor_handle.await;
    })
}

fn normalize_tail(value: usize) -> usize {
    value.clamp(1, MAX_SERVICE_LOG_TAIL)
}

fn physical_tail_for_grouped_snapshot(logical_tail: usize) -> usize {
    logical_tail
        .saturating_mul(SERVICE_LOG_SNAPSHOT_PHYSICAL_TAIL_MULTIPLIER)
        .max(logical_tail.saturating_add(SERVICE_LOG_SNAPSHOT_PHYSICAL_TAIL_MIN_EXTRA))
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
                    physical_tail_for_grouped_snapshot(tail).to_string(),
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

    let mut lines = parse_command_log_lines(&output.stdout, &output.stderr);
    if lines.len() > tail {
        lines = lines.split_off(lines.len() - tail);
    }

    Ok(ServiceLogCollectorState { lines })
}

#[derive(Default)]
struct LineChunkBuffer {
    pending: Vec<u8>,
}

const MAX_PENDING_SERVICE_LOG_BYTES: usize = 64 * 1024;

#[derive(Debug)]
struct ServiceLogRawChunk {
    text: String,
    forced_fragment: bool,
}

impl LineChunkBuffer {
    fn push(&mut self, tx: &mpsc::UnboundedSender<ServiceLogRawChunk>, chunk: &[u8]) {
        self.pending.extend_from_slice(chunk);
        while let Some(delimiter) = self
            .pending
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')
        {
            let delimiter_byte = self.pending[delimiter];
            let mut line = self.pending.drain(..=delimiter).collect::<Vec<_>>();
            line.pop();
            if delimiter_byte == b'\r' && self.pending.first() == Some(&b'\n') {
                self.pending.remove(0);
            } else if line.last() == Some(&b'\r') {
                line.pop();
            }
            let _ = tx.send(ServiceLogRawChunk {
                text: String::from_utf8_lossy(&line).to_string(),
                forced_fragment: false,
            });
        }
        while self.pending.len() > MAX_PENDING_SERVICE_LOG_BYTES {
            let line = take_bounded_utf8_prefix(&mut self.pending, MAX_PENDING_SERVICE_LOG_BYTES);
            let _ = tx.send(ServiceLogRawChunk {
                text: String::from_utf8_lossy(&line).to_string(),
                forced_fragment: true,
            });
        }
    }

    fn finish(&mut self, tx: &mpsc::UnboundedSender<ServiceLogRawChunk>) {
        if self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }
        if !self.pending.is_empty() {
            let _ = tx.send(ServiceLogRawChunk {
                text: String::from_utf8_lossy(&self.pending).to_string(),
                forced_fragment: false,
            });
            self.pending.clear();
        }
    }
}

fn take_bounded_utf8_prefix(bytes: &mut Vec<u8>, max: usize) -> Vec<u8> {
    let limit = bytes.len().min(max);
    if limit == 0 {
        return Vec::new();
    }
    let boundary = match std::str::from_utf8(&bytes[..limit]) {
        Ok(_) => limit,
        Err(error) if error.valid_up_to() > 0 => error.valid_up_to(),
        Err(_) => 1,
    };
    bytes.drain(..boundary).collect()
}

async fn discover_active_source(
    runner: &dyn CommandRunner,
    compose_project: &str,
    service_name: &str,
) -> anyhow::Result<Option<ServiceLogSource>> {
    let strict_ids = docker_ps_ids(
        runner,
        vec![
            format!("label=com.docker.compose.project={compose_project}"),
            format!("label=com.docker.compose.service={service_name}"),
        ],
    )
    .await?;
    if !strict_ids.is_empty() {
        return select_service_log_source(runner, strict_ids, service_name).await;
    }

    let project_ids = docker_ps_ids(
        runner,
        vec![format!(
            "label=com.docker.compose.project={compose_project}"
        )],
    )
    .await?;
    if project_ids.is_empty() {
        return Ok(None);
    }

    select_service_log_source(runner, project_ids, service_name).await
}

async fn docker_ps_ids(
    runner: &dyn CommandRunner,
    filters: Vec<String>,
) -> anyhow::Result<Vec<String>> {
    let mut args = vec!["ps".to_string(), "-q".to_string()];
    for filter in filters {
        args.push("--filter".to_string());
        args.push(filter);
    }
    let ps = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args,
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

    Ok(ps
        .stdout
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>())
}

async fn select_service_log_source(
    runner: &dyn CommandRunner,
    container_ids: Vec<String>,
    service_name: &str,
) -> anyhow::Result<Option<ServiceLogSource>> {
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

#[derive(Clone, Debug)]
struct PendingServiceLogLine {
    ts: String,
    raw: String,
    allows_indented_continuation: bool,
}

impl PendingServiceLogLine {
    fn into_line(self) -> ServiceLogLine {
        let meta = parse_service_log_meta(&self.raw);
        ServiceLogLine {
            ts: self.ts,
            plain: self.raw.clone(),
            raw: self.raw,
            meta: Some(meta),
        }
    }
}

#[derive(Debug, Default)]
struct ServiceLogFrameParser {
    current: Option<PendingServiceLogLine>,
    dropping_leading_continuation: bool,
}

impl ServiceLogFrameParser {
    fn push_physical_line(
        &mut self,
        raw_line: &str,
        forced_fragment: bool,
    ) -> Option<ServiceLogLine> {
        let trimmed = raw_line.trim_end_matches('\r').trim_end_matches('\n');
        if trimmed.trim().is_empty() && self.current.is_none() {
            return None;
        }

        let (ts, raw) = split_docker_log_line(trimmed);
        if ts.is_empty() {
            if let Some(current) = self.current.as_mut() {
                if !forced_fragment {
                    current.raw.push('\n');
                }
                current.raw.push_str(trimmed);
            }
            return None;
        }
        let next = PendingServiceLogLine {
            ts: ts.to_string(),
            raw: raw.to_string(),
            allows_indented_continuation: false,
        };

        if let Some(current) = self.current.as_mut()
            && is_service_log_continuation(&next.raw, current.allows_indented_continuation)
        {
            let allows_indented = current.allows_indented_continuation
                || is_service_log_continuation_marker(&next.raw);
            current.raw.push('\n');
            current.raw.push_str(&next.raw);
            current.allows_indented_continuation = allows_indented;
            return None;
        }

        if self.current.is_none() && is_service_log_continuation_marker(&next.raw) {
            self.dropping_leading_continuation = true;
            return None;
        }

        if self.current.is_none()
            && self.dropping_leading_continuation
            && next.raw.starts_with(char::is_whitespace)
        {
            return None;
        }

        self.dropping_leading_continuation = false;
        self.current
            .replace(next)
            .map(PendingServiceLogLine::into_line)
    }

    fn finish(&mut self) -> Option<ServiceLogLine> {
        self.current.take().map(PendingServiceLogLine::into_line)
    }

    fn has_current(&self) -> bool {
        self.current.is_some()
    }
}

fn parse_service_log_lines(output: &str) -> Vec<ServiceLogLine> {
    let mut parser = ServiceLogFrameParser::default();
    let mut lines = output
        .lines()
        .filter_map(|raw_line| parser.push_physical_line(raw_line, false))
        .collect::<Vec<_>>();
    if let Some(line) = parser.finish() {
        lines.push(line);
    }
    lines
}

fn parse_command_log_lines(stdout: &str, stderr: &str) -> Vec<ServiceLogLine> {
    if stdout.is_empty() {
        return parse_service_log_lines(stderr);
    }
    if stderr.is_empty() {
        return parse_service_log_lines(stdout);
    }

    let mut lines = parse_service_log_lines(stdout);
    lines.extend(parse_service_log_lines(stderr));
    lines.sort_by(|left, right| left.ts.cmp(&right.ts));
    lines
}

fn split_docker_log_line(line: &str) -> (&str, &str) {
    if let Some(separator_index) = line.find(' ') {
        let ts = &line[..separator_index];
        let rest = &line[separator_index + 1..];
        if is_docker_log_timestamp(ts) {
            return (ts.trim(), rest);
        }
    }
    ("", line)
}

fn is_docker_log_timestamp(value: &str) -> bool {
    value.ends_with('Z')
        && value.contains('T')
        && time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .is_ok()
}

fn is_service_log_continuation(raw: &str, allows_indented_continuation: bool) -> bool {
    if raw.is_empty() {
        return true;
    }
    if allows_indented_continuation && raw.starts_with(char::is_whitespace) {
        return true;
    }

    is_service_log_continuation_marker(raw)
}

fn is_service_log_continuation_marker(raw: &str) -> bool {
    let plain = strip_ansi_sgr(raw);
    let trimmed = plain.trim_start();
    trimmed == "Caused by:"
        || trimmed.starts_with("Caused by:")
        || trimmed.starts_with("Stack backtrace:")
}

fn strip_ansi_sgr(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for code_ch in chars.by_ref() {
                if code_ch == 'm' {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
}

fn parse_service_log_meta(raw: &str) -> ServiceLogMeta {
    let plain = strip_ansi_sgr(raw);
    let trimmed = plain.trim();
    if let Some(meta) = parse_json_log_meta(trimmed) {
        return meta;
    }
    if let Some(meta) = parse_logfmt_meta(trimmed) {
        return meta;
    }
    if let Some(meta) = parse_tracing_text_meta(trimmed) {
        return meta;
    }

    ServiceLogMeta {
        format: ServiceLogMetaFormat::Text,
        level: None,
        timestamp: None,
        message: (!trimmed.is_empty()).then(|| trimmed.to_string()),
        attributes: BTreeMap::new(),
        highlights: Vec::new(),
    }
}

fn parse_json_log_meta(input: &str) -> Option<ServiceLogMeta> {
    let Value::Object(mut object) = serde_json::from_str::<Value>(input).ok()? else {
        return None;
    };

    let level = take_stringish(&mut object, &["level", "severity"]);
    let timestamp = take_stringish(&mut object, &["timestamp", "time", "ts"]);
    let message = take_stringish(&mut object, &["message", "msg"]);
    let attributes = object.into_iter().collect::<BTreeMap<_, _>>();
    let highlights = highlight_keys(&attributes);

    Some(ServiceLogMeta {
        format: ServiceLogMetaFormat::Json,
        level: level.map(normalize_meta_level),
        timestamp,
        message,
        attributes,
        highlights,
    })
}

fn take_stringish(object: &mut serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.remove(*key) {
            return value_to_display_string(&value);
        }
    }
    None
}

fn value_to_display_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}

fn normalize_meta_level(value: String) -> String {
    match value.trim().to_lowercase().as_str() {
        "warning" => "warn".to_string(),
        "err" | "fatal" | "critical" => "error".to_string(),
        "verbose" => "debug".to_string(),
        other => other.to_string(),
    }
}

fn parse_logfmt_meta(input: &str) -> Option<ServiceLogMeta> {
    let attributes = parse_logfmt_attributes(input)?;
    if attributes.len() < 2 {
        return None;
    }
    let mut attributes = attributes;
    let level = take_btree_stringish(&mut attributes, &["level", "severity"]);
    let timestamp = take_btree_stringish(&mut attributes, &["timestamp", "time", "ts"]);
    let message = take_btree_stringish(&mut attributes, &["message", "msg"]);
    let highlights = highlight_keys(&attributes);

    Some(ServiceLogMeta {
        format: ServiceLogMetaFormat::Logfmt,
        level: level.map(normalize_meta_level),
        timestamp,
        message,
        attributes,
        highlights,
    })
}

fn parse_tracing_text_meta(input: &str) -> Option<ServiceLogMeta> {
    let first_line = input.lines().next()?.trim();
    let (timestamp, rest) = take_leading_rfc3339_token(first_line)?;
    let rest = rest.trim_start();
    let (level, rest) = take_leading_level_token(rest)?;
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }

    let (rest, mut attributes) = strip_tracing_context_prefixes(rest);
    let (message, event_attributes) = split_tracing_message_and_attributes(rest);
    attributes.extend(event_attributes);
    let message = message.trim().trim_end_matches(':').trim();
    if message.is_empty() && attributes.is_empty() {
        return None;
    }

    if message
        .split_once(": ")
        .is_some_and(|(target, _)| is_tracing_target(target))
    {
        let (target, rest) = message.split_once(": ")?;
        attributes.insert("target".to_string(), Value::String(target.to_string()));
        let highlights = highlight_keys(&attributes);
        return Some(ServiceLogMeta {
            format: ServiceLogMetaFormat::Text,
            level: Some(normalize_meta_level(level.to_string())),
            timestamp: Some(timestamp.to_string()),
            message: Some(rest.trim().to_string()),
            attributes,
            highlights,
        });
    }

    let highlights = highlight_keys(&attributes);
    Some(ServiceLogMeta {
        format: ServiceLogMetaFormat::Text,
        level: Some(normalize_meta_level(level.to_string())),
        timestamp: Some(timestamp.to_string()),
        message: Some(message.to_string()),
        attributes,
        highlights,
    })
}

fn take_leading_rfc3339_token(input: &str) -> Option<(&str, &str)> {
    let mut parts = input.splitn(2, char::is_whitespace);
    let timestamp = parts.next()?.trim();
    if !is_docker_log_timestamp(timestamp) {
        return None;
    }
    Some((timestamp, parts.next().unwrap_or_default()))
}

fn take_leading_level_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let level = parts.next()?.trim();
    if matches!(
        normalize_meta_level(level.to_string()).as_str(),
        "trace" | "debug" | "info" | "warn" | "error"
    ) {
        return Some((level, parts.next().unwrap_or_default()));
    }
    None
}

fn strip_tracing_context_prefixes(input: &str) -> (&str, BTreeMap<String, Value>) {
    let mut rest = input.trim_start();
    let mut attributes = BTreeMap::new();

    if let Some((target, after)) = rest.split_once(": ")
        && !target.contains('=')
        && !target.contains('{')
        && is_tracing_target(target)
    {
        attributes.insert("target".to_string(), Value::String(target.to_string()));
        rest = after.trim_start();
    }

    while let Some((span, span_fields, after)) = take_leading_tracing_span(rest) {
        attributes
            .entry("span".to_string())
            .or_insert_with(|| Value::String(span.to_string()));
        if let Some(span_attributes) = parse_logfmt_attributes(span_fields) {
            for (key, value) in span_attributes {
                attributes.entry(key).or_insert(value);
            }
        }
        rest = after.trim_start();
    }

    (rest, attributes)
}

fn take_leading_tracing_span(input: &str) -> Option<(&str, &str, &str)> {
    let open_index = input.find('{')?;
    let span = input[..open_index].trim();
    if !is_tracing_span_name(span) {
        return None;
    }

    let mut depth = 0usize;
    let mut close_index = None;
    for (offset, ch) in input[open_index..].char_indices() {
        if ch == '{' {
            depth += 1;
            continue;
        }
        if ch == '}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                close_index = Some(open_index + offset);
                break;
            }
        }
    }
    let close_index = close_index?;
    let after = input[close_index + 1..].trim_start();
    let after = after.strip_prefix(':')?;
    Some((span, &input[open_index + 1..close_index], after))
}

fn split_tracing_message_and_attributes(input: &str) -> (String, BTreeMap<String, Value>) {
    let tokens = shellish_split(input);
    let attr_start = tokens
        .iter()
        .position(|token| token.find('=').is_some_and(|index| index > 0));
    let Some(attr_start) = attr_start else {
        return (input.to_string(), BTreeMap::new());
    };

    let mut attributes = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    let flush_current = |attributes: &mut BTreeMap<String, Value>,
                         current_key: &mut Option<String>,
                         current_value: &mut String| {
        if let Some(key) = current_key.take() {
            attributes.insert(key, logfmt_value(std::mem::take(current_value)));
        }
    };

    for token in tokens.iter().skip(attr_start) {
        if let Some(index) = token.find('=')
            && index > 0
        {
            flush_current(&mut attributes, &mut current_key, &mut current_value);
            current_key = Some(token[..index].trim_matches(':').to_string());
            current_value = token[index + 1..].to_string();
            continue;
        }
        if current_key.is_some() {
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(token);
        }
    }
    flush_current(&mut attributes, &mut current_key, &mut current_value);

    (tokens[..attr_start].join(" "), attributes)
}

fn shellish_split(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(quote_ch) = quote {
            if ch == quote_ch {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_tracing_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-'))
        && (value.contains("::") || value.contains('_') || value.contains('-'))
}

fn is_tracing_span_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '-'))
}

fn take_btree_stringish(attributes: &mut BTreeMap<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = attributes.remove(*key) {
            return value_to_display_string(&value);
        }
    }
    None
}

fn parse_logfmt_attributes(input: &str) -> Option<BTreeMap<String, Value>> {
    let mut attributes = BTreeMap::new();
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let key_start = index;
        while index < chars.len() && chars[index] != '=' && !chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() || chars[index] != '=' || index == key_start {
            return None;
        }
        let key = chars[key_start..index].iter().collect::<String>();
        index += 1;

        let value = if index < chars.len() && chars[index] == '"' {
            index += 1;
            let mut value = String::new();
            while index < chars.len() {
                let ch = chars[index];
                index += 1;
                if ch == '"' {
                    break;
                }
                if ch == '\\' && index < chars.len() {
                    value.push(chars[index]);
                    index += 1;
                } else {
                    value.push(ch);
                }
            }
            value
        } else {
            let value_start = index;
            while index < chars.len() && !chars[index].is_whitespace() {
                index += 1;
            }
            chars[value_start..index].iter().collect::<String>()
        };
        attributes.insert(key, logfmt_value(value));
    }

    (!attributes.is_empty()).then_some(attributes)
}

fn logfmt_value(value: String) -> Value {
    if value == "true" {
        return Value::Bool(true);
    }
    if value == "false" {
        return Value::Bool(false);
    }
    if let Ok(int_value) = value.parse::<i64>() {
        return Value::Number(Number::from(int_value));
    }
    if let Ok(float_value) = value.parse::<f64>()
        && let Some(number) = Number::from_f64(float_value)
    {
        return Value::Number(number);
    }
    Value::String(value)
}

fn highlight_keys(attributes: &BTreeMap<String, Value>) -> Vec<String> {
    const PREFERRED: &[&str] = &[
        "component",
        "event",
        "route",
        "scope",
        "phase",
        "elapsed_ms",
        "status",
        "method",
        "uri",
        "latency",
        "proxy_request_id",
        "trace",
        "degraded",
    ];
    PREFERRED
        .iter()
        .filter(|key| attributes.contains_key(**key))
        .take(8)
        .map(|key| (*key).to_string())
        .collect()
}

#[cfg(test)]
#[path = "service_logs_tracing_tests.rs"]
mod tracing_tests;

#[cfg(test)]
mod tests {
    use super::{
        LineChunkBuffer, MAX_PENDING_SERVICE_LOG_BYTES, ServiceLogFrameParser,
        parse_service_log_lines, take_bounded_utf8_prefix,
    };
    use crate::api::types::ServiceLogMetaFormat;
    use tokio::sync::mpsc;

    #[test]
    fn parse_service_log_lines_drops_truncated_leading_continuation() {
        let lines = parse_service_log_lines(
            "2026-07-01T08:12:51.833074000Z Caused by:\n\
             2026-07-01T08:12:51.833081000Z     (code: 5) database is locked\n\
             2026-07-01T08:12:53.763043000Z worker ready\n",
        );

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].raw, "worker ready");
        assert_eq!(lines[0].ts, "2026-07-01T08:12:53.763043000Z");
    }

    #[test]
    fn parse_service_log_lines_preserves_leading_indented_entry() {
        let lines = parse_service_log_lines(
            "2026-07-01T08:12:51.833081000Z     standalone indented output\n\
             2026-07-01T08:12:53.763043000Z worker ready\n",
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].raw, "    standalone indented output");
        assert_eq!(lines[1].raw, "worker ready");
    }

    #[test]
    fn line_chunk_buffer_bounds_newline_free_output_without_loss() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut buffer = LineChunkBuffer::default();
        buffer.push(&tx, &vec![b'x'; MAX_PENDING_SERVICE_LOG_BYTES + 1]);

        let emitted = rx
            .try_recv()
            .expect("oversized partial output should flush");
        assert_eq!(emitted.text.len(), MAX_PENDING_SERVICE_LOG_BYTES);
        assert!(emitted.forced_fragment);
        buffer.finish(&tx);
        assert_eq!(
            rx.try_recv().expect("tail should be preserved").text.len(),
            1
        );
        assert!(buffer.pending.is_empty());
    }

    #[test]
    fn line_chunk_buffer_keeps_utf8_code_points_intact_at_forced_boundary() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut buffer = LineChunkBuffer::default();
        let mut chunk = vec![b'x'; MAX_PENDING_SERVICE_LOG_BYTES - 1];
        chunk.extend_from_slice("界".as_bytes());
        chunk.extend_from_slice("尾".as_bytes());
        buffer.push(&tx, &chunk);

        let emitted = rx
            .try_recv()
            .expect("oversized partial output should flush");
        assert!(emitted.forced_fragment);
        assert!(emitted.text.ends_with('x'));
        assert!(!emitted.text.contains('\u{fffd}'));
        buffer.finish(&tx);
        let tail = rx.try_recv().expect("utf8 tail should be preserved");
        assert_eq!(tail.text, "界尾");
        assert!(!tail.forced_fragment);
    }

    #[test]
    fn bounded_utf8_prefix_leaves_incomplete_code_point_for_next_chunk() {
        let mut bytes = "x界".as_bytes().to_vec();
        let prefix = take_bounded_utf8_prefix(&mut bytes, 2);
        assert_eq!(prefix, b"x");
        assert_eq!(bytes, "界".as_bytes());
    }

    #[test]
    fn service_log_parser_rejoins_bounded_continuation_chunks() {
        let timestamp = "2026-07-01T08:12:51.833074000Z ";
        let first_chunk = format!("{timestamp}{}", "x".repeat(MAX_PENDING_SERVICE_LOG_BYTES));
        let mut parser = ServiceLogFrameParser::default();

        assert!(parser.push_physical_line(&first_chunk, false).is_none());
        assert!(parser.push_physical_line("tail", true).is_none());

        let line = parser
            .finish()
            .expect("continuation should complete the line");
        assert!(line.raw.ends_with("tail"));
    }

    #[test]
    fn line_chunk_buffer_splits_carriage_return_progress() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut buffer = LineChunkBuffer::default();
        buffer.push(&tx, b"first\rsecond");

        assert_eq!(
            rx.try_recv().expect("carriage return should flush").text,
            "first"
        );
        buffer.finish(&tx);
        assert_eq!(
            rx.try_recv().expect("final partial line should flush").text,
            "second"
        );
    }

    #[test]
    fn service_log_parser_separates_live_unstamped_continuations() {
        let mut parser = ServiceLogFrameParser::default();
        assert!(
            parser
                .push_physical_line("2026-07-01T08:12:51.833074000Z worker failed", false)
                .is_none()
        );
        assert!(
            parser
                .push_physical_line("    database is locked", false)
                .is_none()
        );
        let line = parser
            .finish()
            .expect("continuation should complete the line");
        assert_eq!(line.raw, "worker failed\n    database is locked");
    }

    #[test]
    fn parse_service_log_lines_removes_only_docker_separator_space() {
        let lines = parse_service_log_lines(
            "2026-07-01T08:12:51.833063000Z worker ready\n\
             2026-07-01T08:12:51.833070000Z \n\
             2026-07-01T08:12:51.833081000Z     standalone indented output\n",
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].raw, "worker ready\n");
        assert_eq!(lines[1].raw, "    standalone indented output");
    }

    #[test]
    fn parse_service_log_lines_adds_json_metadata() {
        let lines = parse_service_log_lines(
            "2026-07-06T16:15:16.433978000Z {\"timestamp\":\"2026-07-06T16:15:16.433978Z\",\"level\":\"INFO\",\"message\":\"runtime perf\",\"component\":\"admin_read\",\"event\":\"dashboard_overview_phase\",\"elapsed_ms\":24,\"route\":\"/api/dashboard/overview\"}\n",
        );

        assert_eq!(lines.len(), 1);
        let meta = lines[0].meta.as_ref().expect("json metadata");
        assert_eq!(meta.format, ServiceLogMetaFormat::Json);
        assert_eq!(meta.level.as_deref(), Some("info"));
        assert_eq!(
            meta.timestamp.as_deref(),
            Some("2026-07-06T16:15:16.433978Z")
        );
        assert_eq!(meta.message.as_deref(), Some("runtime perf"));
        assert_eq!(meta.attributes["component"].as_str(), Some("admin_read"));
        assert_eq!(
            meta.attributes["event"].as_str(),
            Some("dashboard_overview_phase")
        );
        assert_eq!(meta.attributes["elapsed_ms"].as_i64(), Some(24));
        assert!(meta.highlights.contains(&"component".to_string()));
        assert!(meta.highlights.contains(&"event".to_string()));
    }

    #[test]
    fn parse_service_log_lines_adds_logfmt_metadata() {
        let lines = parse_service_log_lines(
            "2026-07-06T16:15:16.433978000Z level=warn msg=\"slow query\" route=/api/services elapsed_ms=242 degraded=true\n",
        );

        let meta = lines[0].meta.as_ref().expect("logfmt metadata");
        assert_eq!(meta.format, ServiceLogMetaFormat::Logfmt);
        assert_eq!(meta.level.as_deref(), Some("warn"));
        assert_eq!(meta.message.as_deref(), Some("slow query"));
        assert_eq!(meta.attributes["route"].as_str(), Some("/api/services"));
        assert_eq!(meta.attributes["elapsed_ms"].as_i64(), Some(242));
        assert_eq!(meta.attributes["degraded"].as_bool(), Some(true));
    }

    #[test]
    fn parse_service_log_lines_falls_back_to_text_metadata() {
        let lines = parse_service_log_lines("2026-07-06T16:15:16.433978000Z worker ready\n");

        let meta = lines[0].meta.as_ref().expect("text metadata");
        assert_eq!(meta.format, ServiceLogMetaFormat::Text);
        assert_eq!(meta.message.as_deref(), Some("worker ready"));
        assert!(meta.attributes.is_empty());
    }
}
