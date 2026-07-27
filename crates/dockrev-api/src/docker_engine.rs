use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use reqwest::Url;
use serde::{Deserialize, de::DeserializeOwned};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};

use crate::api::types::ServiceResourceSample;

const DEFAULT_DOCKER_SOCKET_PATH: &str = "/var/run/docker.sock";
const DOCKER_ENGINE_MAX_IN_FLIGHT_REQUESTS: usize = 4;
const DOCKER_ENGINE_FAILURE_THRESHOLD: u32 = 2;
const DOCKER_ENGINE_INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const DOCKER_ENGINE_MAX_BACKOFF: Duration = Duration::from_secs(60);
const CPU_BASELINE_MAX_AGE: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct DockerEngineProtectionConfig {
    max_in_flight_requests: usize,
    failure_threshold: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl Default for DockerEngineProtectionConfig {
    fn default() -> Self {
        Self {
            max_in_flight_requests: DOCKER_ENGINE_MAX_IN_FLIGHT_REQUESTS,
            failure_threshold: DOCKER_ENGINE_FAILURE_THRESHOLD,
            initial_backoff: DOCKER_ENGINE_INITIAL_BACKOFF,
            max_backoff: DOCKER_ENGINE_MAX_BACKOFF,
        }
    }
}

#[derive(Clone)]
struct DockerEngineProtection {
    permits: Arc<Semaphore>,
    circuit: Arc<Mutex<DockerEngineCircuit>>,
    circuit_opened: watch::Sender<u64>,
    config: DockerEngineProtectionConfig,
}

#[derive(Clone, Copy)]
struct DockerEngineCircuit {
    consecutive_failures: u32,
    mode: DockerEngineCircuitMode,
}

impl Default for DockerEngineCircuit {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            mode: DockerEngineCircuitMode::Closed,
        }
    }
}

#[derive(Clone, Copy)]
enum DockerEngineCircuitMode {
    Closed,
    Open {
        retry_at: Instant,
        backoff: Duration,
    },
    HalfOpen {
        backoff: Duration,
    },
}

#[derive(Clone, Copy)]
enum DockerEngineCircuitAdmission {
    Closed,
    HalfOpenProbe,
}

#[derive(Clone, Copy)]
enum DockerEngineRequestHealth {
    Responsive,
    RecoverableFailure,
}

struct ProtectedDockerEngineRequest {
    protection: DockerEngineProtection,
    _permit: Option<OwnedSemaphorePermit>,
    admission: DockerEngineCircuitAdmission,
    completed: bool,
}

impl ProtectedDockerEngineRequest {
    fn complete(&mut self) {
        self.completed = true;
    }
}

impl Drop for ProtectedDockerEngineRequest {
    fn drop(&mut self) {
        if !self.completed {
            self.protection.abandon_admission(self.admission);
        }
    }
}

impl DockerEngineProtection {
    fn new(config: DockerEngineProtectionConfig) -> Self {
        let (circuit_opened, _) = watch::channel(0u64);
        Self {
            permits: Arc::new(Semaphore::new(config.max_in_flight_requests)),
            circuit: Arc::new(Mutex::new(DockerEngineCircuit::default())),
            circuit_opened,
            config,
        }
    }

    async fn begin_request(&self) -> anyhow::Result<ProtectedDockerEngineRequest> {
        // Subscribe before admission so an open transition cannot be missed while waiting.
        let mut circuit_opened = self.circuit_opened.subscribe();
        let admission = self.admit()?;
        let mut request = ProtectedDockerEngineRequest {
            protection: self.clone(),
            _permit: None,
            admission,
            completed: false,
        };
        let permit = loop {
            tokio::select! {
                permit = self.permits.clone().acquire_owned() => {
                    break permit.map_err(|_| anyhow::anyhow!("Docker Engine request limiter closed"))?;
                }
                changed = circuit_opened.changed() => {
                    changed.map_err(|_| anyhow::anyhow!("Docker Engine circuit state notifier closed"))?;
                    self.validate_admission(admission)?;
                }
            }
        };
        request._permit = Some(permit);
        self.validate_admission(admission)?;
        Ok(request)
    }

    fn admit(&self) -> anyhow::Result<DockerEngineCircuitAdmission> {
        let mut circuit = self.lock_circuit();
        match circuit.mode {
            DockerEngineCircuitMode::Closed => Ok(DockerEngineCircuitAdmission::Closed),
            DockerEngineCircuitMode::Open { retry_at, backoff } => {
                let now = Instant::now();
                if now < retry_at {
                    return Err(anyhow::anyhow!(
                        "Docker Engine circuit breaker open; retry after {} ms",
                        retry_at.saturating_duration_since(now).as_millis()
                    ));
                }
                circuit.mode = DockerEngineCircuitMode::HalfOpen { backoff };
                tracing::info!(
                    backoff_ms = backoff.as_millis() as u64,
                    "Docker Engine circuit breaker entering half-open probe"
                );
                Ok(DockerEngineCircuitAdmission::HalfOpenProbe)
            }
            DockerEngineCircuitMode::HalfOpen { .. } => Err(anyhow::anyhow!(
                "Docker Engine circuit breaker probe already in progress"
            )),
        }
    }

    fn validate_admission(&self, admission: DockerEngineCircuitAdmission) -> anyhow::Result<()> {
        let circuit = self.lock_circuit();
        let valid = matches!(
            (admission, circuit.mode),
            (
                DockerEngineCircuitAdmission::Closed,
                DockerEngineCircuitMode::Closed
            ) | (
                DockerEngineCircuitAdmission::HalfOpenProbe,
                DockerEngineCircuitMode::HalfOpen { .. }
            )
        );
        if valid {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Docker Engine circuit breaker opened while request waited for capacity"
            ))
        }
    }

    fn record_result(
        &self,
        admission: DockerEngineCircuitAdmission,
        health: DockerEngineRequestHealth,
    ) {
        let mut circuit = self.lock_circuit();
        match (admission, health, circuit.mode) {
            (
                DockerEngineCircuitAdmission::Closed,
                DockerEngineRequestHealth::Responsive,
                DockerEngineCircuitMode::Closed,
            ) => circuit.consecutive_failures = 0,
            (
                DockerEngineCircuitAdmission::HalfOpenProbe,
                DockerEngineRequestHealth::Responsive,
                DockerEngineCircuitMode::HalfOpen { .. },
            ) => {
                circuit.consecutive_failures = 0;
                circuit.mode = DockerEngineCircuitMode::Closed;
                tracing::info!("Docker Engine circuit breaker recovered");
            }
            (
                DockerEngineCircuitAdmission::Closed,
                DockerEngineRequestHealth::RecoverableFailure,
                DockerEngineCircuitMode::Closed,
            ) => {
                circuit.consecutive_failures = circuit.consecutive_failures.saturating_add(1);
                if circuit.consecutive_failures >= self.config.failure_threshold {
                    self.open_circuit(
                        &mut circuit,
                        self.config.initial_backoff,
                        "failure threshold reached",
                    );
                }
            }
            (
                DockerEngineCircuitAdmission::HalfOpenProbe,
                DockerEngineRequestHealth::RecoverableFailure,
                DockerEngineCircuitMode::HalfOpen { backoff },
            ) => {
                let next_backoff = backoff.saturating_mul(2).min(self.config.max_backoff);
                self.open_circuit(&mut circuit, next_backoff, "half-open probe failed");
            }
            _ => {}
        }
    }

    fn abandon_admission(&self, admission: DockerEngineCircuitAdmission) {
        if !matches!(admission, DockerEngineCircuitAdmission::HalfOpenProbe) {
            return;
        }

        let mut circuit = self.lock_circuit();
        if let DockerEngineCircuitMode::HalfOpen { backoff } = circuit.mode {
            self.open_circuit(&mut circuit, backoff, "half-open probe cancelled");
        }
    }

    fn lock_circuit(&self) -> MutexGuard<'_, DockerEngineCircuit> {
        self.circuit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn open_circuit(
        &self,
        circuit: &mut DockerEngineCircuit,
        backoff: Duration,
        reason: &'static str,
    ) {
        circuit.consecutive_failures = 0;
        circuit.mode = DockerEngineCircuitMode::Open {
            retry_at: Instant::now() + backoff,
            backoff,
        };
        self.circuit_opened
            .send_modify(|version| *version = version.wrapping_add(1));
        tracing::warn!(
            reason,
            backoff_ms = backoff.as_millis() as u64,
            "Docker Engine circuit breaker opened"
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectResourceCollection {
    pub samples: BTreeMap<String, ServiceResourceSample>,
    pub failures: Vec<ContainerStatsFailure>,
}

#[derive(Clone, Debug)]
pub struct ContainerStatsFailure {
    pub container_id: String,
    pub service_name: String,
    pub error: String,
}

#[derive(Clone)]
pub struct DockerEngineClient {
    http: reqwest::Client,
    base_url: String,
    protection: DockerEngineProtection,
    cpu_baselines: Arc<Mutex<BTreeMap<String, CpuBaseline>>>,
}

impl DockerEngineClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let docker_host = std::env::var("DOCKER_HOST")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        match docker_host {
            Some(host) => Self::from_docker_host(&host),
            None => Self::from_unix_socket(DEFAULT_DOCKER_SOCKET_PATH),
        }
    }

    fn from_docker_host(raw: &str) -> anyhow::Result<Self> {
        if let Some(path) = raw.strip_prefix("unix://") {
            return Self::from_unix_socket(path);
        }

        if let Some(authority) = raw.strip_prefix("tcp://") {
            return Self::from_http_base(&format!("http://{authority}"));
        }

        if raw.starts_with("http://") || raw.starts_with("https://") {
            return Self::from_http_base(raw);
        }

        Err(anyhow::anyhow!(
            "unsupported DOCKER_HOST for Docker Engine API: {raw}"
        ))
    }

    fn from_unix_socket(path: &str) -> anyhow::Result<Self> {
        #[cfg(unix)]
        {
            let socket_path = PathBuf::from(path);
            let http = reqwest::Client::builder()
                .unix_socket(socket_path)
                .build()
                .context("build Docker Engine unix socket client")?;
            Ok(Self::with_http_client(http, "http://docker".to_string()))
        }

        #[cfg(not(unix))]
        {
            let _ = path;
            Err(anyhow::anyhow!(
                "unix Docker socket is unsupported on this platform"
            ))
        }
    }

    fn from_http_base(raw: &str) -> anyhow::Result<Self> {
        Self::from_http_base_with_protection(raw, DockerEngineProtectionConfig::default())
    }

    fn from_http_base_with_protection(
        raw: &str,
        protection_config: DockerEngineProtectionConfig,
    ) -> anyhow::Result<Self> {
        let mut url = Url::parse(raw).context("parse DOCKER_HOST as URL")?;
        url.set_query(None);
        url.set_fragment(None);
        url.set_path("");
        let http = reqwest::Client::builder()
            .build()
            .context("build Docker Engine HTTP client")?;
        Ok(Self {
            http,
            base_url: url.as_str().trim_end_matches('/').to_string(),
            protection: DockerEngineProtection::new(protection_config),
            cpu_baselines: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    fn with_http_client(http: reqwest::Client, base_url: String) -> Self {
        Self {
            http,
            base_url,
            protection: DockerEngineProtection::new(DockerEngineProtectionConfig::default()),
            cpu_baselines: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test_http_base(raw: &str) -> anyhow::Result<Self> {
        Self::from_http_base(raw)
    }

    #[cfg(test)]
    pub(crate) fn shares_protection_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.protection.permits, &other.protection.permits)
            && Arc::ptr_eq(&self.protection.circuit, &other.protection.circuit)
    }

    #[cfg(test)]
    pub(crate) fn shares_sampling_state_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cpu_baselines, &other.cpu_baselines)
    }

    pub async fn collect_project_service_samples(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<ProjectResourceCollection> {
        let compose_projects = BTreeSet::from([compose_project.to_string()]);
        Ok(self
            .collect_projects_service_samples(&compose_projects)
            .await?
            .remove(compose_project)
            .unwrap_or_default())
    }

    pub async fn collect_projects_service_samples(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeMap<String, ProjectResourceCollection>> {
        let mut collections = compose_projects
            .iter()
            .map(|project| (project.clone(), ProjectResourceCollection::default()))
            .collect::<BTreeMap<_, _>>();
        if compose_projects.is_empty() {
            return Ok(collections);
        }

        let listing = self
            .list_compose_project_containers(compose_projects)
            .await?;
        self.prune_cpu_baselines(
            compose_projects,
            &listing.active_container_ids,
            listing.global_discovery,
        );
        let containers = listing.containers;

        let mut aggregates = BTreeMap::<String, BTreeMap<String, ServiceAggregate>>::new();
        let mut join_set = tokio::task::JoinSet::new();
        for container in containers {
            let client = self.clone();
            join_set.spawn(async move {
                let result = client
                    .fetch_container_sample(&container.id, &container.compose_project)
                    .await;
                (container, result)
            });
        }

        while let Some(joined) = join_set.join_next().await {
            let (container, result) = joined.context("join Docker container stats task")?;
            let collection = collections
                .get_mut(&container.compose_project)
                .expect("listed project must have a collection");
            match result {
                Ok(sample) => {
                    aggregates
                        .entry(container.compose_project)
                        .or_default()
                        .entry(container.service_name)
                        .or_default()
                        .merge_container_sample(sample);
                }
                Err(error) => collection.failures.push(ContainerStatsFailure {
                    container_id: container.id,
                    service_name: container.service_name,
                    error: error.to_string(),
                }),
            }
        }

        for (project, collection) in &mut collections {
            collection.samples = aggregates
                .remove(project)
                .unwrap_or_default()
                .into_iter()
                .map(|(service_name, aggregate)| (service_name, aggregate.into_sample()))
                .collect();
        }
        Ok(collections)
    }

    async fn list_compose_project_containers(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<ContainerListing> {
        let global_discovery = compose_projects.len() != 1;
        let rows: Vec<DockerContainerSummary> = if !global_discovery {
            let compose_project = compose_projects
                .first()
                .expect("single project collection must have a project");
            let filters = serde_json::json!({
                "label": [format!("com.docker.compose.project={compose_project}")]
            });
            self.get_json(
                "/containers/json",
                &[("filters", filters.to_string())],
                Duration::from_secs(8),
            )
            .await
        } else {
            self.get_json("/containers/json", &[], Duration::from_secs(8))
                .await
        }
        .context("list running Docker containers for resource monitoring")?;
        let active_container_ids = rows
            .iter()
            .map(|row| row.id.clone())
            .collect::<BTreeSet<_>>();
        let containers = rows
            .into_iter()
            .filter_map(|row| {
                let compose_project = row.labels.get("com.docker.compose.project")?.clone();
                if !compose_projects.contains(&compose_project) {
                    return None;
                }
                row.labels
                    .get("com.docker.compose.service")
                    .cloned()
                    .filter(|service_name| !service_name.is_empty())
                    .map(|service_name| ProjectContainer {
                        id: row.id,
                        compose_project,
                        service_name,
                    })
            })
            .collect();
        Ok(ContainerListing {
            containers,
            active_container_ids,
            global_discovery,
        })
    }

    fn prune_cpu_baselines(
        &self,
        compose_projects: &BTreeSet<String>,
        active_container_ids: &BTreeSet<String>,
        global_discovery: bool,
    ) {
        let mut baselines = self
            .cpu_baselines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        baselines.retain(|container_id, baseline| {
            let active = active_container_ids.contains(container_id);
            let fresh = baseline.last_seen_at <= now
                && now.saturating_duration_since(baseline.last_seen_at) < CPU_BASELINE_MAX_AGE;
            if compose_projects.contains(&baseline.compose_project) {
                return active && fresh;
            }
            fresh && (!global_discovery || active)
        });
    }

    async fn fetch_container_sample(
        &self,
        container_id: &str,
        compose_project: &str,
    ) -> anyhow::Result<ContainerResourceSample> {
        let path = format!("/containers/{container_id}/stats");
        let stats: DockerStatsResponse = self
            .get_json(
                &path,
                &[
                    ("stream", "false".to_string()),
                    ("one-shot", "true".to_string()),
                ],
                Duration::from_secs(20),
            )
            .await
            .with_context(|| format!("fetch Docker stats for container {container_id}"))?;
        Ok(ContainerResourceSample {
            cpu_percent: self.cpu_percent_from_baseline(container_id, compose_project, &stats),
            mem_used_bytes: calculate_memory_usage(&stats).map(|(used, _)| used),
            mem_limit_bytes: calculate_memory_usage(&stats).map(|(_, limit)| limit),
            net_rx_bytes: calculate_network_bytes(&stats).map(|(rx, _)| rx),
            net_tx_bytes: calculate_network_bytes(&stats).map(|(_, tx)| tx),
            block_read_bytes: calculate_block_io_bytes(&stats).map(|(read, _)| read),
            block_write_bytes: calculate_block_io_bytes(&stats).map(|(_, write)| write),
            pids: stats.pids_stats.current,
        })
    }

    fn cpu_percent_from_baseline(
        &self,
        container_id: &str,
        compose_project: &str,
        stats: &DockerStatsResponse,
    ) -> f64 {
        let current = CpuBaseline::from_stats(compose_project, stats);
        let mut baselines = self
            .cpu_baselines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = baselines.insert(container_id.to_string(), current);
        previous
            .and_then(|previous| calculate_cpu_percent_from_baseline(&previous, stats))
            .unwrap_or_default()
    }

    async fn get_json<T>(
        &self,
        path: &str,
        query: &[(&str, String)],
        timeout: Duration,
    ) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let mut request = self.protection.begin_request().await?;
        let admission = request.admission;
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .query(query)
            .timeout(timeout)
            .send()
            .await;
        let (result, health) = match response {
            Err(error) => {
                let health = if error.is_timeout() || error.is_connect() {
                    DockerEngineRequestHealth::RecoverableFailure
                } else {
                    DockerEngineRequestHealth::Responsive
                };
                (
                    Err(error).with_context(|| format!("request Docker Engine {path}")),
                    health,
                )
            }
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    let health = if status.is_server_error() {
                        DockerEngineRequestHealth::RecoverableFailure
                    } else {
                        DockerEngineRequestHealth::Responsive
                    };
                    (
                        Err(anyhow::anyhow!(
                            "Docker Engine request {path} failed status={status} body={body}"
                        )),
                        health,
                    )
                } else {
                    match response.json::<T>().await {
                        Ok(value) => (Ok(value), DockerEngineRequestHealth::Responsive),
                        Err(error) => (
                            Err(error).with_context(|| {
                                format!("decode Docker Engine response for {path}")
                            }),
                            DockerEngineRequestHealth::RecoverableFailure,
                        ),
                    }
                }
            }
        };
        self.protection.record_result(admission, health);
        request.complete();
        result
    }
}

#[derive(Clone, Debug)]
struct ProjectContainer {
    id: String,
    compose_project: String,
    service_name: String,
}

struct ContainerListing {
    containers: Vec<ProjectContainer>,
    active_container_ids: BTreeSet<String>,
    global_discovery: bool,
}

#[derive(Clone, Debug, Default)]
struct ContainerResourceSample {
    cpu_percent: f64,
    mem_used_bytes: Option<u64>,
    mem_limit_bytes: Option<u64>,
    net_rx_bytes: Option<u64>,
    net_tx_bytes: Option<u64>,
    block_read_bytes: Option<u64>,
    block_write_bytes: Option<u64>,
    pids: Option<u64>,
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
    fn merge_container_sample(&mut self, sample: ContainerResourceSample) {
        self.container_count = self.container_count.saturating_add(1);
        self.cpu_percent += sample.cpu_percent;

        if let (Some(used), Some(limit)) = (sample.mem_used_bytes, sample.mem_limit_bytes) {
            self.mem_seen = true;
            self.mem_used_sum = self.mem_used_sum.saturating_add(used);
            self.mem_limit_sum = self.mem_limit_sum.saturating_add(limit);
        }

        if let (Some(rx), Some(tx)) = (sample.net_rx_bytes, sample.net_tx_bytes) {
            self.net_seen = true;
            self.net_rx_sum = self.net_rx_sum.saturating_add(rx);
            self.net_tx_sum = self.net_tx_sum.saturating_add(tx);
        }

        if let (Some(read), Some(write)) = (sample.block_read_bytes, sample.block_write_bytes) {
            self.block_seen = true;
            self.block_read_sum = self.block_read_sum.saturating_add(read);
            self.block_write_sum = self.block_write_sum.saturating_add(write);
        }

        if let Some(pids) = sample.pids {
            self.pids_seen = true;
            self.pids_sum = self.pids_sum.saturating_add(pids);
        }
    }

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

#[derive(Debug, Deserialize)]
struct DockerContainerSummary {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Labels", default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerStatsResponse {
    #[serde(default)]
    cpu_stats: DockerCpuStats,
    #[serde(default)]
    memory_stats: DockerMemoryStats,
    #[serde(default)]
    networks: BTreeMap<String, DockerNetworkStats>,
    #[serde(default)]
    blkio_stats: DockerBlkioStats,
    #[serde(default)]
    pids_stats: DockerPidsStats,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuStats {
    #[serde(default)]
    cpu_usage: DockerCpuUsage,
    system_cpu_usage: Option<u64>,
    online_cpus: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuUsage {
    #[serde(default)]
    total_usage: u64,
    #[serde(default)]
    percpu_usage: Vec<u64>,
}

#[derive(Clone)]
struct CpuBaseline {
    compose_project: String,
    total_usage: u64,
    system_cpu_usage: u64,
    last_seen_at: Instant,
}

impl CpuBaseline {
    fn from_stats(compose_project: &str, stats: &DockerStatsResponse) -> Self {
        Self {
            compose_project: compose_project.to_string(),
            total_usage: stats.cpu_stats.cpu_usage.total_usage,
            system_cpu_usage: stats.cpu_stats.system_cpu_usage.unwrap_or_default(),
            last_seen_at: Instant::now(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct DockerMemoryStats {
    usage: Option<u64>,
    limit: Option<u64>,
    #[serde(default)]
    stats: BTreeMap<String, u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerNetworkStats {
    rx_bytes: Option<u64>,
    tx_bytes: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioStats {
    #[serde(default, deserialize_with = "deserialize_null_default")]
    io_service_bytes_recursive: Vec<DockerBlkioBytesEntry>,
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioBytesEntry {
    #[serde(alias = "Op")]
    op: Option<String>,
    #[serde(alias = "Value")]
    value: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerPidsStats {
    current: Option<u64>,
}

fn calculate_cpu_percent_from_baseline(
    previous: &CpuBaseline,
    stats: &DockerStatsResponse,
) -> Option<f64> {
    let cpu_delta = stats
        .cpu_stats
        .cpu_usage
        .total_usage
        .checked_sub(previous.total_usage)? as f64;
    let system_delta = stats
        .cpu_stats
        .system_cpu_usage?
        .checked_sub(previous.system_cpu_usage)? as f64;
    if cpu_delta <= 0.0 || system_delta <= 0.0 {
        return None;
    }
    let online_cpus = stats
        .cpu_stats
        .online_cpus
        .filter(|value| *value > 0)
        .unwrap_or_else(|| stats.cpu_stats.cpu_usage.percpu_usage.len().max(1) as u64);
    Some((cpu_delta / system_delta) * online_cpus as f64 * 100.0)
}

fn calculate_memory_usage(stats: &DockerStatsResponse) -> Option<(u64, u64)> {
    let usage = stats.memory_stats.usage?;
    let limit = stats.memory_stats.limit?;
    if limit == 0 {
        return None;
    }
    let inactive_file = stats
        .memory_stats
        .stats
        .get("inactive_file")
        .copied()
        .or_else(|| stats.memory_stats.stats.get("total_inactive_file").copied())
        .unwrap_or(0);
    Some((usage.saturating_sub(inactive_file.min(usage)), limit))
}

fn calculate_network_bytes(stats: &DockerStatsResponse) -> Option<(u64, u64)> {
    if stats.networks.is_empty() {
        return None;
    }
    let mut rx_sum = 0u64;
    let mut tx_sum = 0u64;
    let mut seen = false;
    for iface in stats.networks.values() {
        if let (Some(rx), Some(tx)) = (iface.rx_bytes, iface.tx_bytes) {
            seen = true;
            rx_sum = rx_sum.saturating_add(rx);
            tx_sum = tx_sum.saturating_add(tx);
        }
    }
    seen.then_some((rx_sum, tx_sum))
}

fn calculate_block_io_bytes(stats: &DockerStatsResponse) -> Option<(u64, u64)> {
    let mut read_sum = 0u64;
    let mut write_sum = 0u64;
    let mut seen = false;
    for entry in &stats.blkio_stats.io_service_bytes_recursive {
        let Some(op) = entry.op.as_deref() else {
            continue;
        };
        let Some(value) = entry.value else {
            continue;
        };
        if op.eq_ignore_ascii_case("read") {
            seen = true;
            read_sum = read_sum.saturating_add(value);
        } else if op.eq_ignore_ascii_case("write") {
            seen = true;
            write_sum = write_sum.saturating_add(value);
        }
    }
    seen.then_some((read_sum, write_sum))
}

#[cfg(test)]
#[path = "docker_engine_tests.rs"]
mod tests;
