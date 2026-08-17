use super::*;

#[test]
fn realtime_and_history_collectors_share_docker_engine_protection() {
    let client = DockerEngineClient::for_test_http_base("http://docker.test").unwrap();
    let realtime = DockerApiResourceCollector::from_client(client.clone());
    let history = DockerApiResourceCollector::from_client(client);

    assert!(realtime.client.shares_protection_with(&history.client));
    assert!(realtime.client.shares_sampling_state_with(&history.client));
}

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
    batch_calls: usize,
    batch_sizes: Vec<usize>,
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

    async fn batch_calls(&self) -> usize {
        self.state.lock().await.batch_calls
    }

    async fn batch_sizes(&self) -> Vec<usize> {
        self.state.lock().await.batch_sizes.clone()
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
            behavior
        };

        tokio::time::sleep(behavior.delay).await;
        Ok(ResourceCollection {
            samples: behavior.samples,
            failures: Vec::new(),
        })
    }

    async fn collect_projects_service_aggregates(
        &self,
        compose_projects: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeMap<String, ResourceCollection>> {
        let behaviors = {
            let mut state = self.state.lock().await;
            state.batch_calls += 1;
            state.batch_sizes.push(compose_projects.len());
            compose_projects
                .iter()
                .map(|project| {
                    let behavior = state
                        .behaviors
                        .get(project)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("missing test project {project}"))?;
                    *state.calls.entry(project.clone()).or_default() += 1;
                    Ok((project.clone(), behavior))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };
        let delay = behaviors
            .iter()
            .map(|(_, behavior)| behavior.delay)
            .max()
            .unwrap_or_default();
        tokio::time::sleep(delay).await;
        Ok(behaviors
            .into_iter()
            .map(|(project, behavior)| {
                (
                    project,
                    ResourceCollection {
                        samples: behavior.samples,
                        failures: Vec::new(),
                    },
                )
            })
            .collect())
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

#[test]
fn coordinator_evicts_expired_idle_project_states() {
    let now = Instant::now();
    let mut state = ResourceSamplingCoordinatorState::default();
    let mut expired = ProjectCollectionState::new();
    expired.completed_at = Some(
        now.checked_sub(RESOURCE_COLLECTION_CACHE_MAX_AGE + Duration::from_millis(1))
            .unwrap(),
    );
    state.projects.insert("expired".to_string(), expired);

    let mut in_flight = ProjectCollectionState::new();
    in_flight.in_flight = true;
    state.projects.insert("in-flight".to_string(), in_flight);

    evict_stale_project_collection_states(
        &mut state,
        &BTreeSet::from(["requested".to_string()]),
        now,
    );

    assert!(!state.projects.contains_key("expired"));
    assert!(state.projects.contains_key("in-flight"));
}

#[tokio::test]
async fn coordinator_recovers_after_owned_collection_is_cancelled() {
    let collector = Arc::new(TestHistoryCollector::new(BTreeMap::from([(
        "project-0".to_string(),
        TestProjectBehavior {
            delay: Duration::from_millis(50),
            samples: BTreeMap::new(),
        },
    )])));
    let coordinator = ResourceSamplingCoordinator::with_collector(collector.clone());
    let projects = BTreeSet::from(["project-0".to_string()]);

    let pending = {
        let coordinator = coordinator.clone();
        let projects = projects.clone();
        tokio::spawn(async move { coordinator.collect_projects(&projects).await })
    };
    tokio::time::sleep(Duration::from_millis(5)).await;
    pending.abort();
    let _ = pending.await;

    let recovered = tokio::time::timeout(
        Duration::from_secs(1),
        coordinator.collect_projects(&projects),
    )
    .await
    .expect("cancelled collection must not strand project state")
    .unwrap();
    assert!(recovered.contains_key("project-0"));
    assert_eq!(collector.call_count("project-0").await, 2);
}

#[tokio::test]
async fn coordinator_clears_cached_collections_when_monitoring_is_disabled() {
    let collector = Arc::new(TestHistoryCollector::new(BTreeMap::from([(
        "project-0".to_string(),
        TestProjectBehavior {
            delay: Duration::ZERO,
            samples: BTreeMap::new(),
        },
    )])));
    let coordinator = ResourceSamplingCoordinator::with_collector(collector);
    coordinator
        .collect_project("project-0")
        .await
        .expect("initial collection should succeed");
    assert_eq!(coordinator.state.lock().await.projects.len(), 1);

    coordinator.clear_cached_collections().await;

    assert!(coordinator.state.lock().await.projects.is_empty());
}

#[tokio::test]
async fn coordinator_batches_history_projects_and_reuses_active_sse_collection() {
    let behaviors = (0..25)
        .map(|index| {
            (
                format!("project-{index}"),
                TestProjectBehavior {
                    delay: Duration::from_millis(50),
                    samples: BTreeMap::from([(
                        format!("service-{index}"),
                        make_sample(index as f64, 1_024),
                    )]),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let collector = Arc::new(TestHistoryCollector::new(behaviors));
    let coordinator = ResourceSamplingCoordinator::with_collector(collector.clone());
    let projects = (0..25)
        .map(|index| format!("project-{index}"))
        .collect::<BTreeSet<_>>();

    let history_coordinator = coordinator.clone();
    let history_projects = projects.clone();
    let history = tokio::spawn(async move {
        history_coordinator
            .collect_projects(&history_projects)
            .await
            .unwrap()
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let realtime = coordinator.collect_project("project-0");
    let (history_collections, realtime_collection) = tokio::join!(history, realtime);

    assert_eq!(history_collections.unwrap().len(), 25);
    assert_eq!(realtime_collection.unwrap().samples.len(), 1);
    assert_eq!(collector.batch_calls().await, 1);
    assert_eq!(collector.batch_sizes().await, vec![25]);
    assert_eq!(collector.call_count("project-0").await, 1);
}

fn make_sample(cpu_percent: f64, mem_used_bytes: u64) -> ServiceResourceSample {
    ServiceResourceSample {
        sampled_at: String::new(),
        cpu_percent,
        mem_used_bytes: Some(mem_used_bytes),
        mem_limit_bytes: Some(16_384),
        net_rx_bytes: Some(10_000),
        net_tx_bytes: Some(11_000),
        net_rx_rate_bps: None,
        net_tx_rate_bps: None,
        block_read_bytes: Some(12_000),
        block_write_bytes: Some(13_000),
        block_read_rate_bps: None,
        block_write_rate_bps: None,
        pids: Some(7),
        container_count: 1,
    }
}
