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
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<String>();
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
                if let Some(line) = parser.push_physical_line(&raw_line) {
                    let _ = processor_tx.send(CollectorMessage::Line(line));
                }
            }
            if let Some(line) = parser.finish() {
                let _ = processor_tx.send(CollectorMessage::Line(line));
            }
        });

        {
            let mut on_stdout = |chunk: String| {
                for raw_line in chunk.lines() {
                    let _ = raw_tx.send(raw_line.to_string());
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
        }
        drop(raw_tx);
        let _ = processor_handle.await;
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

    let mut lines = parse_service_log_lines(&output.stdout);
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
    has_continuation: bool,
}

impl PendingServiceLogLine {
    fn into_line(self) -> ServiceLogLine {
        ServiceLogLine {
            ts: self.ts,
            plain: self.raw.clone(),
            raw: self.raw,
        }
    }
}

#[derive(Debug, Default)]
struct ServiceLogFrameParser {
    current: Option<PendingServiceLogLine>,
}

impl ServiceLogFrameParser {
    fn push_physical_line(&mut self, raw_line: &str) -> Option<ServiceLogLine> {
        let trimmed = raw_line.trim_end_matches('\r').trim_end_matches('\n');
        if trimmed.trim().is_empty() && self.current.is_none() {
            return None;
        }

        let (ts, raw) = split_docker_log_line(trimmed);
        let next = PendingServiceLogLine {
            ts: ts.to_string(),
            raw: raw.to_string(),
            has_continuation: false,
        };

        if let Some(current) = self.current.as_mut()
            && is_service_log_continuation(&next.raw, current.has_continuation)
        {
            current.raw.push('\n');
            current.raw.push_str(&next.raw);
            current.has_continuation = true;
            return None;
        }

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
        .filter_map(|raw_line| parser.push_physical_line(raw_line))
        .collect::<Vec<_>>();
    if let Some(line) = parser.finish() {
        lines.push(line);
    }
    lines
}

fn split_docker_log_line(line: &str) -> (&str, &str) {
    if let Some((ts, rest)) = line.split_once(' ')
        && is_docker_log_timestamp(ts)
    {
        return (ts.trim(), rest);
    }
    ("", line)
}

fn is_docker_log_timestamp(value: &str) -> bool {
    value.ends_with('Z')
        && value.contains('T')
        && time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .is_ok()
}

fn is_service_log_continuation(raw: &str, current_has_continuation: bool) -> bool {
    if raw.is_empty() {
        return true;
    }
    if current_has_continuation && raw.starts_with(char::is_whitespace) {
        return true;
    }

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
