use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use anyhow::Context as _;
use reqwest::Url;
use serde::{Deserialize, de::DeserializeOwned};

use crate::api::types::ServiceResourceSample;

const DEFAULT_DOCKER_SOCKET_PATH: &str = "/var/run/docker.sock";

#[derive(Clone)]
pub struct DockerEngineClient {
    http: reqwest::Client,
    base_url: String,
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
            Ok(Self {
                http,
                base_url: "http://docker".to_string(),
            })
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
        })
    }

    pub async fn collect_project_service_samples(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<BTreeMap<String, ServiceResourceSample>> {
        let containers = self.list_project_containers(compose_project).await?;
        if containers.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut join_set = tokio::task::JoinSet::new();
        for container in containers {
            let client = self.clone();
            join_set.spawn(async move {
                let sample = client.fetch_container_sample(&container.id).await?;
                Ok::<_, anyhow::Error>((container.service_name, sample))
            });
        }

        let mut aggregates = BTreeMap::<String, ServiceAggregate>::new();
        while let Some(joined) = join_set.join_next().await {
            let (service_name, sample) = joined.context("join Docker container stats task")??;
            aggregates
                .entry(service_name)
                .or_default()
                .merge_container_sample(sample);
        }

        Ok(aggregates
            .into_iter()
            .map(|(service_name, aggregate)| (service_name, aggregate.into_sample()))
            .collect())
    }

    async fn list_project_containers(
        &self,
        compose_project: &str,
    ) -> anyhow::Result<Vec<ProjectContainer>> {
        let filters = serde_json::json!({
            "label": [format!("com.docker.compose.project={compose_project}")]
        });
        let rows: Vec<DockerContainerSummary> = self
            .get_json(
                "/containers/json",
                &[("filters", filters.to_string())],
                Duration::from_secs(8),
            )
            .await
            .with_context(|| {
                format!("list Docker containers for compose project {compose_project}")
            })?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                row.labels
                    .get("com.docker.compose.service")
                    .cloned()
                    .filter(|service_name| !service_name.is_empty())
                    .map(|service_name| ProjectContainer {
                        id: row.id,
                        service_name,
                    })
            })
            .collect())
    }

    async fn fetch_container_sample(
        &self,
        container_id: &str,
    ) -> anyhow::Result<ContainerResourceSample> {
        let path = format!("/containers/{container_id}/stats");
        let stats: DockerStatsResponse = self
            .get_json(
                &path,
                &[("stream", "false".to_string())],
                Duration::from_secs(20),
            )
            .await
            .with_context(|| format!("fetch Docker stats for container {container_id}"))?;
        Ok(ContainerResourceSample {
            cpu_percent: calculate_cpu_percent(&stats).unwrap_or_default(),
            mem_used_bytes: calculate_memory_usage(&stats).map(|(used, _)| used),
            mem_limit_bytes: calculate_memory_usage(&stats).map(|(_, limit)| limit),
            net_rx_bytes: calculate_network_bytes(&stats).map(|(rx, _)| rx),
            net_tx_bytes: calculate_network_bytes(&stats).map(|(_, tx)| tx),
            block_read_bytes: calculate_block_io_bytes(&stats).map(|(read, _)| read),
            block_write_bytes: calculate_block_io_bytes(&stats).map(|(_, write)| write),
            pids: stats.pids_stats.current,
        })
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
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .query(query)
            .timeout(timeout)
            .send()
            .await
            .with_context(|| format!("request Docker Engine {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Docker Engine request {path} failed status={status} body={body}"
            ));
        }
        resp.json::<T>()
            .await
            .with_context(|| format!("decode Docker Engine response for {path}"))
    }
}

#[derive(Clone, Debug)]
struct ProjectContainer {
    id: String,
    service_name: String,
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
    precpu_stats: DockerCpuStats,
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
    #[serde(default)]
    io_service_bytes_recursive: Vec<DockerBlkioBytesEntry>,
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

fn calculate_cpu_percent(stats: &DockerStatsResponse) -> Option<f64> {
    let cpu_delta = stats
        .cpu_stats
        .cpu_usage
        .total_usage
        .checked_sub(stats.precpu_stats.cpu_usage.total_usage)? as f64;
    let system_delta = stats
        .cpu_stats
        .system_cpu_usage?
        .checked_sub(stats.precpu_stats.system_cpu_usage?)? as f64;
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
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::{Json, Router, extract::OriginalUri, routing::get};
    use serde_json::json;

    #[test]
    fn tcp_docker_host_normalizes_to_http_base_url() {
        let client =
            DockerEngineClient::from_docker_host("tcp://docker-socket-proxy:2375").unwrap();
        assert_eq!(client.base_url, "http://docker-socket-proxy:2375");
    }

    #[test]
    fn http_docker_host_preserves_scheme_and_strips_path() {
        let client =
            DockerEngineClient::from_docker_host("https://docker.example.com/root").unwrap();
        assert_eq!(client.base_url, "https://docker.example.com");
    }

    #[test]
    fn unix_docker_host_normalizes_to_unversioned_engine_base_url() {
        let client = DockerEngineClient::from_docker_host("unix:///var/run/docker.sock").unwrap();
        assert_eq!(client.base_url, "http://docker");
    }

    #[tokio::test]
    async fn collect_project_service_samples_uses_unversioned_engine_paths() {
        async fn list_containers(
            OriginalUri(uri): OriginalUri,
            axum::extract::State(seen_paths): axum::extract::State<Arc<Mutex<Vec<String>>>>,
        ) -> Json<serde_json::Value> {
            seen_paths
                .lock()
                .unwrap()
                .push(uri.path_and_query().unwrap().as_str().to_string());
            Json(json!([
                {
                    "Id": "container-1",
                    "Labels": {
                        "com.docker.compose.service": "web"
                    }
                }
            ]))
        }

        async fn stats(
            OriginalUri(uri): OriginalUri,
            axum::extract::State(seen_paths): axum::extract::State<Arc<Mutex<Vec<String>>>>,
        ) -> Json<serde_json::Value> {
            seen_paths
                .lock()
                .unwrap()
                .push(uri.path_and_query().unwrap().as_str().to_string());
            Json(json!({
                "cpu_stats": {
                    "cpu_usage": {
                        "total_usage": 5_000_000,
                        "percpu_usage": [2_500_000, 2_500_000]
                    },
                    "system_cpu_usage": 20_000_000,
                    "online_cpus": 2
                },
                "precpu_stats": {
                    "cpu_usage": {
                        "total_usage": 1_000_000
                    },
                    "system_cpu_usage": 12_000_000
                },
                "memory_stats": {
                    "usage": 150_000_000,
                    "limit": 1_000_000_000,
                    "stats": {
                        "inactive_file": 10_000_000
                    }
                },
                "networks": {
                    "eth0": { "rx_bytes": 1000, "tx_bytes": 2000 }
                },
                "blkio_stats": {
                    "io_service_bytes_recursive": [
                        { "op": "Read", "value": 4096 },
                        { "op": "Write", "value": 8192 }
                    ]
                },
                "pids_stats": {
                    "current": 12
                }
            }))
        }

        let seen_paths = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route("/containers/json", get(list_containers))
            .route("/containers/{id}/stats", get(stats))
            .with_state(seen_paths.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client =
            DockerEngineClient::from_http_base(&format!("http://{addr}/docker-root")).unwrap();
        let samples = client
            .collect_project_service_samples("demo")
            .await
            .unwrap();

        let sample = samples.get("web").unwrap();
        assert!((sample.cpu_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(sample.mem_used_bytes, Some(140_000_000));
        assert_eq!(sample.mem_limit_bytes, Some(1_000_000_000));
        assert_eq!(sample.net_rx_bytes, Some(1000));
        assert_eq!(sample.net_tx_bytes, Some(2000));
        assert_eq!(sample.block_read_bytes, Some(4096));
        assert_eq!(sample.block_write_bytes, Some(8192));
        assert_eq!(sample.pids, Some(12));
        assert_eq!(sample.container_count, 1);
        assert_eq!(
            seen_paths.lock().unwrap().as_slice(),
            [
                "/containers/json?filters=%7B%22label%22%3A%5B%22com.docker.compose.project%3Ddemo%22%5D%7D",
                "/containers/container-1/stats?stream=false",
            ]
        );

        server.abort();
    }

    #[test]
    fn stats_calculations_match_docker_style_fields() {
        let stats: DockerStatsResponse = serde_json::from_value(serde_json::json!({
            "cpu_stats": {
                "cpu_usage": {
                    "total_usage": 5_000_000,
                    "percpu_usage": [2_500_000, 2_500_000]
                },
                "system_cpu_usage": 20_000_000,
                "online_cpus": 2
            },
            "precpu_stats": {
                "cpu_usage": {
                    "total_usage": 1_000_000
                },
                "system_cpu_usage": 12_000_000
            },
            "memory_stats": {
                "usage": 150_000_000,
                "limit": 1_000_000_000,
                "stats": {
                    "inactive_file": 10_000_000
                }
            },
            "networks": {
                "eth0": { "rx_bytes": 1000, "tx_bytes": 2000 },
                "eth1": { "rx_bytes": 3000, "tx_bytes": 4000 }
            },
            "blkio_stats": {
                "io_service_bytes_recursive": [
                    { "op": "Read", "value": 4096 },
                    { "op": "Write", "value": 8192 }
                ]
            },
            "pids_stats": {
                "current": 12
            }
        }))
        .unwrap();

        let cpu = calculate_cpu_percent(&stats).unwrap();
        assert!((cpu - 100.0).abs() < f64::EPSILON);
        assert_eq!(
            calculate_memory_usage(&stats),
            Some((140_000_000, 1_000_000_000))
        );
        assert_eq!(calculate_network_bytes(&stats), Some((4000, 6000)));
        assert_eq!(calculate_block_io_bytes(&stats), Some((4096, 8192)));
        assert_eq!(stats.pids_stats.current, Some(12));
    }

    #[test]
    fn service_aggregate_sums_container_samples() {
        let mut aggregate = ServiceAggregate::default();
        aggregate.merge_container_sample(ContainerResourceSample {
            cpu_percent: 12.5,
            mem_used_bytes: Some(10),
            mem_limit_bytes: Some(100),
            net_rx_bytes: Some(20),
            net_tx_bytes: Some(30),
            block_read_bytes: Some(40),
            block_write_bytes: Some(50),
            pids: Some(2),
        });
        aggregate.merge_container_sample(ContainerResourceSample {
            cpu_percent: 7.5,
            mem_used_bytes: Some(20),
            mem_limit_bytes: Some(100),
            net_rx_bytes: Some(10),
            net_tx_bytes: Some(20),
            block_read_bytes: Some(30),
            block_write_bytes: Some(40),
            pids: Some(3),
        });

        let sample = aggregate.into_sample();
        assert_eq!(sample.cpu_percent, 20.0);
        assert_eq!(sample.mem_used_bytes, Some(30));
        assert_eq!(sample.mem_limit_bytes, Some(200));
        assert_eq!(sample.net_rx_bytes, Some(30));
        assert_eq!(sample.net_tx_bytes, Some(50));
        assert_eq!(sample.block_read_bytes, Some(70));
        assert_eq!(sample.block_write_bytes, Some(90));
        assert_eq!(sample.pids, Some(5));
        assert_eq!(sample.container_count, 2);
    }
}
