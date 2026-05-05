use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context as _;
use serde::Deserialize;
use tokio::sync::{Mutex, broadcast};

use crate::{
    api::types::ServiceResourceSample,
    db::{Db, ServiceResourceSampleInput, ServiceResourceTarget},
    runner::{CommandRunner, CommandSpec},
};

pub const RESOURCE_MONITOR_RETENTION_DAYS: u32 = 30;
pub const DEFAULT_SAMPLE_INTERVAL_SECONDS: u64 = 10;

pub fn is_valid_sample_interval_seconds(value: u64) -> bool {
    matches!(value, 10 | 30 | 60 | 300)
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
        "15m" => Some(15 * 60),
        "1h" => Some(60 * 60),
        "6h" => Some(6 * 60 * 60),
        _ => None,
    }
}

#[derive(Clone)]
pub struct RealtimeSamplerHub {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    samplers: Arc<Mutex<BTreeMap<String, Arc<SamplerEntry>>>>,
}

#[derive(Clone)]
struct SamplerEntry {
    tx: broadcast::Sender<RealtimeMessage>,
    subscribers: Arc<AtomicUsize>,
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

impl RealtimeSamplerHub {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            db,
            runner,
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
        sample_for_target(self.runner.as_ref(), &target).await
    }

    fn spawn_sampler_task(&self, service_id: String, entry: Arc<SamplerEntry>) {
        let db = self.db.clone();
        let runner = self.runner.clone();
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

                match sample_for_target(runner.as_ref(), &target).await {
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

pub fn spawn_history_sampler(db: Db, runner: Arc<dyn CommandRunner>) {
    tokio::spawn(async move {
        let mut last_gc = Instant::now();

        loop {
            let settings = match db.get_resource_monitor_settings().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "resource monitor settings unavailable");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            if last_gc.elapsed() >= Duration::from_secs(60 * 60) {
                if let Err(e) = gc_history(&db).await {
                    tracing::warn!(error = %e, "resource monitor history gc failed");
                }
                last_gc = Instant::now();
            }

            if !settings.enabled {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            let interval_seconds =
                normalize_sample_interval_seconds(settings.sample_interval_seconds);
            if let Err(e) = sample_history_once(&db, runner.as_ref()).await {
                tracing::warn!(error = %e, "resource monitor history sampling failed");
            }

            tokio::time::sleep(Duration::from_secs(interval_seconds)).await;
        }
    });
}

async fn gc_history(db: &Db) -> anyhow::Result<()> {
    let now = time::OffsetDateTime::now_utc();
    let older_than = (now - time::Duration::days(RESOURCE_MONITOR_RETENTION_DAYS as i64))
        .format(&time::format_description::well_known::Rfc3339)?;
    let deleted = db
        .delete_expired_service_resource_samples(&older_than)
        .await
        .context("delete expired resource samples")?;
    tracing::info!(deleted, "resource monitor history gc completed");
    Ok(())
}

async fn sample_history_once(db: &Db, runner: &dyn CommandRunner) -> anyhow::Result<()> {
    let targets = db
        .list_service_resource_targets()
        .await
        .context("list service resource targets")?;
    if targets.is_empty() {
        return Ok(());
    }

    let mut by_project = BTreeMap::<String, Vec<ServiceResourceTarget>>::new();
    for target in targets {
        by_project
            .entry(target.compose_project.clone())
            .or_default()
            .push(target);
    }

    let sampled_at = now_rfc3339()?;
    let mut rows = Vec::<ServiceResourceSampleInput>::new();

    for (project, project_targets) in by_project {
        let aggregates = match collect_project_service_aggregates(runner, &project).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    project = %project,
                    error = %e,
                    "resource monitor history sampling skipped project"
                );
                continue;
            }
        };

        for target in &project_targets {
            let Some(sample) = aggregates.get(&target.service_name) else {
                continue;
            };
            rows.push(ServiceResourceSampleInput {
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
            });
        }
    }

    if rows.is_empty() {
        return Ok(());
    }

    let inserted = db
        .insert_service_resource_samples(&rows)
        .await
        .context("insert resource samples")?;
    tracing::debug!(inserted, "resource monitor samples inserted");
    Ok(())
}

async fn sample_for_target(
    runner: &dyn CommandRunner,
    target: &ServiceResourceTarget,
) -> anyhow::Result<Option<ServiceResourceSample>> {
    let aggregates = collect_project_service_aggregates(runner, &target.compose_project)
        .await
        .with_context(|| {
            format!(
                "collect live stats for compose project {}",
                target.compose_project
            )
        })?;

    let Some(sample) = aggregates.get(&target.service_name).cloned() else {
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

async fn collect_project_service_aggregates(
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

fn parse_cpu_percent(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().trim_end_matches('%').trim();
    cleaned.parse::<f64>().ok()
}

fn parse_u64_str(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

fn parse_pair_bytes(raw: &str) -> Option<(u64, u64)> {
    let (left, right) = raw.split_once('/')?;
    let a = parse_size_to_bytes(left)?;
    let b = parse_size_to_bytes(right)?;
    Some((a, b))
}

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

    fn make_entry(subscribers: usize) -> Arc<SamplerEntry> {
        let (tx, _rx) = broadcast::channel(4);
        Arc::new(SamplerEntry {
            tx,
            subscribers: Arc::new(AtomicUsize::new(subscribers)),
        })
    }

    #[test]
    fn normalize_sample_interval_seconds_uses_ten_seconds_default() {
        assert_eq!(normalize_sample_interval_seconds(10), 10);
        assert_eq!(normalize_sample_interval_seconds(30), 30);
        assert_eq!(normalize_sample_interval_seconds(60), 60);
        assert_eq!(normalize_sample_interval_seconds(300), 300);
        assert_eq!(normalize_sample_interval_seconds(7), 10);
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
}
