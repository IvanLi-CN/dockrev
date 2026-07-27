use super::*;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    http::StatusCode,
    routing::get,
};
use serde_json::json;

type EnginePathTestState = (Arc<Mutex<Vec<String>>>, Arc<AtomicUsize>);

#[derive(Clone)]
struct ProtectionTestState {
    statuses: Arc<Mutex<VecDeque<StatusCode>>>,
    requests: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    response_delay_ms: Arc<AtomicU64>,
}

impl ProtectionTestState {
    fn new(statuses: impl IntoIterator<Item = StatusCode>) -> Self {
        Self {
            statuses: Arc::new(Mutex::new(statuses.into_iter().collect())),
            requests: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            response_delay_ms: Arc::new(AtomicU64::new(0)),
        }
    }
}

async fn protected_response(
    State(state): State<ProtectionTestState>,
) -> (StatusCode, Json<serde_json::Value>) {
    state.requests.fetch_add(1, Ordering::SeqCst);
    let active = state.active.fetch_add(1, Ordering::SeqCst) + 1;
    let mut observed = state.max_active.load(Ordering::SeqCst);
    while active > observed {
        match state.max_active.compare_exchange(
            observed,
            active,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(current) => observed = current,
        }
    }

    let delay = state.response_delay_ms.load(Ordering::SeqCst);
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    state.active.fetch_sub(1, Ordering::SeqCst);
    let status = state
        .statuses
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or(StatusCode::OK);
    (status, Json(json!({})))
}

async fn protected_test_client(
    state: ProtectionTestState,
    config: DockerEngineProtectionConfig,
) -> (DockerEngineClient, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/health", get(protected_response))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        DockerEngineClient::from_http_base_with_protection(&format!("http://{addr}"), config)
            .unwrap(),
        server,
    )
}

async fn wait_for_request_count(state: &ProtectionTestState, expected: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if state.requests.load(Ordering::SeqCst) >= expected {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn test_protection_config() -> DockerEngineProtectionConfig {
    DockerEngineProtectionConfig {
        max_in_flight_requests: 4,
        failure_threshold: 2,
        initial_backoff: Duration::from_millis(20),
        max_backoff: Duration::from_millis(80),
    }
}

#[test]
fn tcp_docker_host_normalizes_to_http_base_url() {
    let client = DockerEngineClient::from_docker_host("tcp://docker-socket-proxy:2375").unwrap();
    assert_eq!(client.base_url, "http://docker-socket-proxy:2375");
}

#[test]
fn http_docker_host_preserves_scheme_and_strips_path() {
    let client = DockerEngineClient::from_docker_host("https://docker.example.com/root").unwrap();
    assert_eq!(client.base_url, "https://docker.example.com");
}

#[test]
fn unix_docker_host_normalizes_to_unversioned_engine_base_url() {
    let client = DockerEngineClient::from_docker_host("unix:///var/run/docker.sock").unwrap();
    assert_eq!(client.base_url, "http://docker");
}

#[tokio::test]
async fn docker_engine_request_limit_is_shared_across_client_clones() {
    let state = ProtectionTestState::new(std::iter::repeat_n(StatusCode::OK, 12));
    state.response_delay_ms.store(40, Ordering::SeqCst);
    let (client, server) = protected_test_client(state.clone(), test_protection_config()).await;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let client = client.clone();
        requests.spawn(async move {
            client
                .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
                .await
        });
    }

    while let Some(result) = requests.join_next().await {
        result.unwrap().unwrap();
    }

    assert_eq!(state.requests.load(Ordering::SeqCst), 12);
    assert_eq!(state.max_active.load(Ordering::SeqCst), 4);
    server.abort();
}

#[tokio::test]
async fn queued_request_degrades_when_the_circuit_opens() {
    let protection = DockerEngineProtection::new(test_protection_config());
    let mut in_flight = Vec::new();
    for _ in 0..DOCKER_ENGINE_MAX_IN_FLIGHT_REQUESTS {
        in_flight.push(protection.begin_request().await.unwrap());
    }

    let queued_protection = protection.clone();
    let queued = tokio::spawn(async move { queued_protection.begin_request().await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while protection.circuit_opened.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    {
        let mut circuit = protection.lock_circuit();
        protection.open_circuit(
            &mut circuit,
            Duration::from_millis(20),
            "test circuit opened",
        );
    }

    let result = tokio::time::timeout(Duration::from_millis(100), queued)
        .await
        .unwrap()
        .unwrap();
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("queued request unexpectedly acquired a permit after circuit opened"),
    };
    assert!(
        error
            .to_string()
            .contains("opened while request waited for capacity")
    );
    drop(in_flight);
}

#[tokio::test]
async fn docker_engine_circuit_breaker_backs_off_and_recovers_with_one_probe() {
    let state = ProtectionTestState::new([
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::OK,
        StatusCode::OK,
    ]);
    let (client, server) = protected_test_client(state.clone(), test_protection_config()).await;

    for _ in 0..2 {
        assert!(
            client
                .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
                .await
                .is_err()
        );
    }
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .unwrap_err()
            .to_string()
            .contains("circuit breaker open")
    );
    assert_eq!(state.requests.load(Ordering::SeqCst), 2);

    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
    assert_eq!(state.requests.load(Ordering::SeqCst), 3);

    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
    assert_eq!(state.requests.load(Ordering::SeqCst), 3);

    tokio::time::sleep(Duration::from_millis(25)).await;
    state.response_delay_ms.store(40, Ordering::SeqCst);
    let probe_client = client.clone();
    let probe = tokio::spawn(async move {
        probe_client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
    });
    wait_for_request_count(&state, 4).await;
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .unwrap_err()
            .to_string()
            .contains("probe already in progress")
    );
    assert_eq!(state.requests.load(Ordering::SeqCst), 4);
    probe.await.unwrap().unwrap();

    client
        .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(state.requests.load(Ordering::SeqCst), 5);
    server.abort();
}

#[tokio::test]
async fn cancelled_half_open_probe_reopens_the_circuit() {
    let state = ProtectionTestState::new([
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::OK,
        StatusCode::OK,
    ]);
    let (client, server) = protected_test_client(state.clone(), test_protection_config()).await;

    for _ in 0..2 {
        assert!(
            client
                .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
                .await
                .is_err()
        );
    }

    tokio::time::sleep(Duration::from_millis(25)).await;
    state.response_delay_ms.store(100, Ordering::SeqCst);
    let probe_client = client.clone();
    let probe = tokio::spawn(async move {
        probe_client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
    });
    wait_for_request_count(&state, 3).await;
    probe.abort();
    assert!(probe.await.unwrap_err().is_cancelled());

    tokio::time::sleep(Duration::from_millis(25)).await;
    state.response_delay_ms.store(0, Ordering::SeqCst);
    client
        .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(state.requests.load(Ordering::SeqCst), 4);
    server.abort();
}

#[tokio::test]
async fn docker_engine_client_errors_do_not_trip_the_circuit_breaker() {
    let state = ProtectionTestState::new([
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::NOT_FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::OK,
    ]);
    let (client, server) = protected_test_client(state.clone(), test_protection_config()).await;

    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
    assert!(
        client
            .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
            .await
            .is_err()
    );
    client
        .get_json::<serde_json::Value>("/health", &[], Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(state.requests.load(Ordering::SeqCst), 4);
    server.abort();
}

#[tokio::test]
async fn collect_project_service_samples_uses_unversioned_engine_paths() {
    async fn list_containers(
        OriginalUri(uri): OriginalUri,
        axum::extract::State((seen_paths, _stats_calls)): axum::extract::State<EnginePathTestState>,
    ) -> Json<serde_json::Value> {
        let filters = url::form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
            .find_map(|(key, value)| (key == "filters").then_some(value.into_owned()))
            .expect("single project discovery must filter by compose project");
        assert_eq!(
            filters,
            serde_json::json!({
                "label": ["com.docker.compose.project=demo"]
            })
            .to_string()
        );
        seen_paths.lock().unwrap().push(uri.path().to_string());
        Json(json!([
            {
                "Id": "container-1",
                "Labels": {
                    "com.docker.compose.project": "demo",
                    "com.docker.compose.service": "web"
                }
            }
        ]))
    }

    async fn stats(
        OriginalUri(uri): OriginalUri,
        axum::extract::State((seen_paths, stats_calls)): axum::extract::State<EnginePathTestState>,
    ) -> Json<serde_json::Value> {
        seen_paths
            .lock()
            .unwrap()
            .push(uri.path_and_query().unwrap().as_str().to_string());
        let call = stats_calls.fetch_add(1, Ordering::SeqCst);
        let (total_usage, system_cpu_usage) = if call == 0 {
            (5_000_000, 20_000_000)
        } else {
            (9_000_000, 28_000_000)
        };
        Json(json!({
            "cpu_stats": {
                "cpu_usage": {
                    "total_usage": total_usage,
                    "percpu_usage": [2_500_000, 2_500_000]
                },
                "system_cpu_usage": system_cpu_usage,
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
    let stats_calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/containers/json", get(list_containers))
        .route("/containers/{id}/stats", get(stats))
        .with_state((seen_paths.clone(), stats_calls));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = DockerEngineClient::from_http_base(&format!("http://{addr}/docker-root")).unwrap();
    let first_samples = client
        .collect_project_service_samples("demo")
        .await
        .unwrap();
    let samples = client
        .collect_project_service_samples("demo")
        .await
        .unwrap();

    let sample = samples.samples.get("web").unwrap();
    assert!(samples.failures.is_empty());
    assert_eq!(first_samples.samples.get("web").unwrap().cpu_percent, 0.0);
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
            "/containers/json",
            "/containers/container-1/stats?stream=false&one-shot=true",
            "/containers/json",
            "/containers/container-1/stats?stream=false&one-shot=true",
        ]
    );

    server.abort();
}

#[test]
fn pruning_cpu_baselines_only_evicts_requested_projects() {
    let client = DockerEngineClient::for_test_http_base("http://docker.test").unwrap();
    let compose_projects = BTreeSet::from(["demo".to_string()]);
    let containers = [ProjectContainer {
        id: "demo-running".to_string(),
        compose_project: "demo".to_string(),
        service_name: "web".to_string(),
    }];
    let baseline = |compose_project: &str| CpuBaseline {
        compose_project: compose_project.to_string(),
        total_usage: 1,
        system_cpu_usage: 2,
        last_seen_at: Instant::now(),
    };
    {
        let mut baselines = client.cpu_baselines.lock().unwrap();
        baselines.insert("demo-running".to_string(), baseline("demo"));
        baselines.insert("demo-stale".to_string(), baseline("demo"));
        baselines.insert("other-running".to_string(), baseline("other"));
    }

    let active_container_ids = containers
        .iter()
        .map(|container| container.id.clone())
        .collect();
    client.prune_cpu_baselines(&compose_projects, &active_container_ids, false);

    let baselines = client.cpu_baselines.lock().unwrap();
    assert!(baselines.contains_key("demo-running"));
    assert!(!baselines.contains_key("demo-stale"));
    assert!(baselines.contains_key("other-running"));
}

#[test]
fn global_pruning_evicts_cpu_baselines_for_removed_projects() {
    let client = DockerEngineClient::for_test_http_base("http://docker.test").unwrap();
    let compose_projects = BTreeSet::from(["demo".to_string()]);
    let containers = [ProjectContainer {
        id: "demo-running".to_string(),
        compose_project: "demo".to_string(),
        service_name: "web".to_string(),
    }];
    let baseline = |compose_project: &str| CpuBaseline {
        compose_project: compose_project.to_string(),
        total_usage: 1,
        system_cpu_usage: 2,
        last_seen_at: Instant::now(),
    };
    {
        let mut baselines = client.cpu_baselines.lock().unwrap();
        baselines.insert("demo-running".to_string(), baseline("demo"));
        baselines.insert("removed-running".to_string(), baseline("removed"));
    }

    let active_container_ids = containers
        .iter()
        .map(|container| container.id.clone())
        .collect();
    client.prune_cpu_baselines(&compose_projects, &active_container_ids, true);

    let baselines = client.cpu_baselines.lock().unwrap();
    assert!(baselines.contains_key("demo-running"));
    assert!(!baselines.contains_key("removed-running"));
}

#[test]
fn global_pruning_preserves_active_containers_outside_partial_batch() {
    let client = DockerEngineClient::for_test_http_base("http://docker.test").unwrap();
    let compose_projects = BTreeSet::from(["demo".to_string()]);
    let baseline = |compose_project: &str| CpuBaseline {
        compose_project: compose_project.to_string(),
        total_usage: 1,
        system_cpu_usage: 2,
        last_seen_at: Instant::now(),
    };
    {
        let mut baselines = client.cpu_baselines.lock().unwrap();
        baselines.insert("demo-running".to_string(), baseline("demo"));
        baselines.insert("other-running".to_string(), baseline("other"));
        baselines.insert("removed-running".to_string(), baseline("removed"));
    }

    let active_container_ids =
        BTreeSet::from(["demo-running".to_string(), "other-running".to_string()]);
    client.prune_cpu_baselines(&compose_projects, &active_container_ids, true);

    let baselines = client.cpu_baselines.lock().unwrap();
    assert!(baselines.contains_key("demo-running"));
    assert!(baselines.contains_key("other-running"));
    assert!(!baselines.contains_key("removed-running"));
}

#[test]
fn pruning_expires_old_requested_cpu_baselines() {
    let client = DockerEngineClient::for_test_http_base("http://docker.test").unwrap();
    let compose_projects = BTreeSet::from(["demo".to_string()]);
    let stale = CpuBaseline {
        compose_project: "demo".to_string(),
        total_usage: 1,
        system_cpu_usage: 2,
        last_seen_at: Instant::now()
            .checked_sub(CPU_BASELINE_MAX_AGE + Duration::from_secs(1))
            .unwrap(),
    };
    {
        let mut baselines = client.cpu_baselines.lock().unwrap();
        baselines.insert("demo-stale".to_string(), stale);
    }

    let active_container_ids = BTreeSet::from(["demo-stale".to_string()]);
    client.prune_cpu_baselines(&compose_projects, &active_container_ids, false);

    assert!(
        !client
            .cpu_baselines
            .lock()
            .unwrap()
            .contains_key("demo-stale")
    );
}

#[tokio::test]
async fn batch_collection_discovers_25_projects_and_74_containers_once() {
    async fn list_containers(
        OriginalUri(uri): OriginalUri,
        axum::extract::State(discovery_requests): axum::extract::State<Arc<AtomicUsize>>,
    ) -> Json<serde_json::Value> {
        assert!(
            uri.query().is_none(),
            "batch discovery must remain unfiltered"
        );
        discovery_requests.fetch_add(1, Ordering::SeqCst);
        Json(json!(
            (0..74)
                .map(|index| json!({
                    "Id": format!("container-{index}"),
                    "Labels": {
                        "com.docker.compose.project": format!("project-{}", index % 25),
                        "com.docker.compose.service": format!("service-{index}"),
                    },
                }))
                .collect::<Vec<_>>()
        ))
    }

    async fn stats() -> Json<serde_json::Value> {
        Json(json!({
            "cpu_stats": {
                "cpu_usage": { "total_usage": 1_000_000, "percpu_usage": [1_000_000] },
                "system_cpu_usage": 2_000_000,
                "online_cpus": 1,
            },
            "memory_stats": { "usage": 1024, "limit": 2048 },
        }))
    }

    let discovery_requests = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/containers/json", get(list_containers))
        .route("/containers/{id}/stats", get(stats))
        .with_state(discovery_requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = DockerEngineClient::from_http_base(&format!("http://{address}")).unwrap();
    let projects = (0..25)
        .map(|index| format!("project-{index}"))
        .collect::<BTreeSet<_>>();

    let collections = client
        .collect_projects_service_samples(&projects)
        .await
        .unwrap();

    assert_eq!(discovery_requests.load(Ordering::SeqCst), 1);
    assert_eq!(collections.len(), 25);
    assert_eq!(
        collections
            .values()
            .flat_map(|collection| collection.samples.values())
            .map(|sample| sample.container_count as usize)
            .sum::<usize>(),
        74
    );
    server.abort();
}

#[test]
fn stats_accept_null_block_io_entries() {
    let stats: DockerStatsResponse = serde_json::from_value(serde_json::json!({
        "blkio_stats": { "io_service_bytes_recursive": null }
    }))
    .unwrap();
    assert!(stats.blkio_stats.io_service_bytes_recursive.is_empty());
    assert_eq!(calculate_block_io_bytes(&stats), None);
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
