#[async_trait::async_trait]
impl RegistryClient for StaggeredCheckRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let started = std::time::Instant::now();
        self.started_at.lock().unwrap().push(started);

        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        let mut seen = self.max_in_flight.load(Ordering::SeqCst);
        while current > seen {
            match self.max_in_flight.compare_exchange(
                seen,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(v) => seen = v,
            }
        }

        if let Some(target_in_flight) = self.hold_until_in_flight {
            if current >= target_in_flight && !self.peak_reached.swap(true, Ordering::SeqCst) {
                self.peak_notify.notify_waiters();
            }
            let notified = self.peak_notify.notified();
            if !self.peak_reached.load(Ordering::SeqCst) {
                let _ = tokio::time::timeout(self.delay, notified).await;
            }
        } else {
            tokio::time::sleep(self.delay).await;
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);

        Ok(ManifestInfo {
            digest: Some("sha256:new".to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[async_trait::async_trait]
impl RegistryClient for StrictSemverDriftRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        tokio::time::sleep(self.list_tags_delay).await;
        Ok(vec!["5.2.0".to_string(), "5.3.0".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "5.2.0" => "sha256:new",
            "5.3.0" => "sha256:newer",
            _ => "sha256:other",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

async fn test_state_with(
    db_path: &str,
    registry: Arc<dyn RegistryClient>,
    runner: Arc<dyn CommandRunner>,
) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        deploy_check_local_command_timeout_seconds: 12,
        registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
        update_idempotent_retry_max_attempts: 3,
        update_idempotent_retry_base_ms: 300,
        update_idempotent_retry_max_ms: 3000,
    };

    let db = Db::open(&config.db_path).await.unwrap();
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let resource_hub = Arc::new(crate::resource_usage::RealtimeSamplerHub::new(
        db.clone(),
        runner.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker, resource_hub)
}

async fn test_state(db_path: &str) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        deploy_check_local_command_timeout_seconds: 12,
        registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
        update_idempotent_retry_max_attempts: 3,
        update_idempotent_retry_base_ms: 300,
        update_idempotent_retry_max_ms: 3000,
    };

    let db = Db::open(&config.db_path).await.unwrap();

    let registry = Arc::new(FakeRegistry);
    let runner = Arc::new(FakeRunner);
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let resource_hub = Arc::new(crate::resource_usage::RealtimeSamplerHub::new(
        db.clone(),
        runner.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker, resource_hub)
}

async fn test_state_auth_required(db_path: &str) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: None,
        auth_allowed_group: None,
        auth_allow_anonymous_in_dev: false,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        deploy_check_local_command_timeout_seconds: 12,
        registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
        update_idempotent_retry_max_attempts: 3,
        update_idempotent_retry_base_ms: 300,
        update_idempotent_retry_max_ms: 3000,
    };

    let db = Db::open(&config.db_path).await.unwrap();
    let registry = Arc::new(FakeRegistry);
    let runner = Arc::new(FakeRunner);
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let resource_hub = Arc::new(crate::resource_usage::RealtimeSamplerHub::new(
        db.clone(),
        runner.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker, resource_hub)
}

async fn test_state_with_authz(
    db_path: &str,
    allowed_user: Option<&str>,
    allowed_group: Option<&str>,
    allow_anonymous_in_dev: bool,
) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_group_header_name: "Remote-Groups".parse().unwrap(),
        auth_allowed_user: allowed_user.map(ToString::to_string),
        auth_allowed_group: allowed_group.map(ToString::to_string),
        auth_allow_anonymous_in_dev: allow_anonymous_in_dev,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        deploy_check_local_command_timeout_seconds: 12,
        registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
        update_idempotent_retry_max_attempts: 3,
        update_idempotent_retry_base_ms: 300,
        update_idempotent_retry_max_ms: 3000,
    };

    let db = Db::open(&config.db_path).await.unwrap();
    let registry = Arc::new(FakeRegistry);
    let runner = Arc::new(FakeRunner);
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    let resource_hub = Arc::new(crate::resource_usage::RealtimeSamplerHub::new(
        db.clone(),
        runner.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker, resource_hub)
}

async fn seed_stack_from_compose(state: &Arc<AppState>, name: &str, compose_file: &str) -> String {
    let contents = std::fs::read_to_string(compose_file).unwrap();
    let parsed = compose::parse_services(&contents).unwrap();
    let mut merged = BTreeMap::<String, compose::ServiceFromCompose>::new();
    merged = compose::merge_services(merged, parsed);

    let stack_id = ids::new_stack_id();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let stack = crate::api::types::StackRecord {
        id: stack_id.clone(),
        name: name.to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec![compose_file.to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: Vec::new(),
    };

    let mut seeds = Vec::new();
    for svc in merged.values() {
        seeds.push(crate::api::types::ServiceSeed {
            id: ids::new_service_id(),
            name: svc.name.clone(),
            image_ref: svc.image_ref.clone(),
            image_tag: svc.image_tag.clone(),
            auto_rollback: true,
            backup_bind_paths: BTreeMap::new(),
            backup_volume_names: BTreeMap::new(),
        });
    }

    state.db.insert_stack(&stack, &seeds, &now).await.unwrap();
    stack_id
}

async fn upsert_image_digest_snapshot_for_test(
    state: &Arc<AppState>,
    image_repo: &str,
    digest: &str,
    host_platform: &str,
    checked_at: &str,
    tags: Vec<String>,
    scan: crate::api::types::ServiceDigestTagsScanSummary,
) {
    let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
        digest: crate::snapshot_worker::normalize_digest(digest)
            .unwrap_or_else(|| digest.to_string()),
        tags,
        checked_at: checked_at.to_string(),
        scan,
    };
    state
        .db
        .upsert_image_digest_tags_snapshot(
            image_repo,
            &snapshot.digest,
            host_platform,
            &serde_json::to_string(&snapshot).unwrap(),
            checked_at,
            checked_at,
        )
        .await
        .unwrap();
}

async fn set_single_service_check_result(
    state: &Arc<AppState>,
    stack_id: &str,
    current_digest: Option<&str>,
    candidate_tag: Option<&str>,
    candidate_digest: Option<&str>,
) -> String {
    let services = state.db.list_services_for_check(stack_id).await.unwrap();
    let service = services.first().expect("service must exist");
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &service.id,
            current_digest.and_then(crate::snapshot_worker::normalize_digest),
            None,
            None,
            candidate_tag.map(ToString::to_string),
            None,
            candidate_digest.and_then(crate::snapshot_worker::normalize_digest),
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    service.id.clone()
}

async fn seed_manual_rollback_service(state: &Arc<AppState>) -> (String, String, String) {
    let compose_path = format!("/tmp/dockrev-manual-rollback-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(state, "demo", &compose_path).await;
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    let now = test_now_rfc3339();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:new".to_string()),
            Some("5.3.0".to_string()),
            Some(serde_json::to_string(&vec!["5.3.0"]).unwrap()),
            Some("5.2".to_string()),
            None,
            None,
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["5.3.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    upsert_image_digest_snapshot_for_test(
        state,
        "ghcr.io/acme/web",
        "sha256:old",
        "linux/amd64",
        &now,
        vec!["5.2.0".to_string(), "5.2".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;
    (stack_id, service.id, compose_path)
}

fn make_update_history_summary_for_test(
    stack_id: &str,
    service_id: &str,
    old_digest: &str,
    final_digest: &str,
) -> serde_json::Value {
    serde_json::json!({
        "mode": "apply",
        "stacks": [{
            "stackId": stack_id,
            "update": {
                "oldDigests": { service_id: old_digest },
                "newDigests": { service_id: final_digest },
                "finalDigests": { service_id: final_digest },
                "changedServices": 1,
                "targetTagsPulled": [],
                "pullTagsPulled": [],
                "pullTagWarnings": [],
                "skippedVersionAnomaly": [],
            }
        }]
    })
}

async fn insert_successful_update_history_job(
    state: &Arc<AppState>,
    scope: crate::api::types::JobScope,
    stack_id: Option<&str>,
    service_id: Option<&str>,
    created_at: &str,
    finished_at: &str,
    summary: serde_json::Value,
) -> String {
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        scope,
        stack_id.map(ToString::to_string),
        service_id.map(ToString::to_string),
        created_at,
    )
    .to_db();
    job.created_by = "ui".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();
    state
        .db
        .finish_job(&job_id, "success", finished_at, &summary)
        .await
        .unwrap();
    job_id
}

async fn seed_discovered_project(state: &Arc<AppState>, stack_id: &str, project: &str) {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: project.to_string(),
            stack_id: Some(stack_id.to_string()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: None,
            unarchive_if_active: true,
        })
        .await
        .unwrap();
}

async fn enable_github_packages_webhook(
    state: &Arc<AppState>,
    secret: &str,
    repos: &[(&str, &str, bool)],
) {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: true,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some(secret.to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    let repos = repos
        .iter()
        .map(|(owner, repo, selected)| (owner.to_string(), repo.to_string(), *selected))
        .collect::<Vec<_>>();
    state
        .db
        .put_github_packages_repos(&repos, &now)
        .await
        .unwrap();
}

fn sign_github_package_payload(secret: &str, payload: &serde_json::Value) -> (Vec<u8>, String) {
    use ring::hmac;

    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));
    (payload_bytes, sig)
}

fn github_delivery_event_payload(
    delivery_id: &str,
    received_at: &str,
    decision: &str,
    attempt_count: u32,
) -> serde_json::Value {
    serde_json::json!({
        "type": "github_packages_delivery_event",
        "deliveryId": delivery_id,
        "receivedAt": received_at,
        "firstReceivedAt": received_at,
        "owner": "acme",
        "repo": "widgets",
        "fullName": "acme/widgets",
        "event": "package",
        "action": "published",
        "decision": decision,
        "reason": serde_json::Value::Null,
        "responseStatus": 200,
        "jobId": serde_json::Value::Null,
        "jobIds": [],
        "attemptCount": attempt_count,
    })
}

async fn wait_for_job_terminal(
    state: &Arc<AppState>,
    job_id: &str,
) -> crate::api::types::JobListItem {
    for _ in 0..300 {
        let job = state.db.get_job(job_id).await.unwrap().unwrap();
        if job.status != "queued" && job.status != "running" {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for job {job_id} to finish");
}

async fn wait_for_job_log_contains(state: &Arc<AppState>, job_id: &str, needle: &str) {
    for _ in 0..300 {
        let logs = state.db.list_job_logs(job_id).await.unwrap();
        if logs.iter().any(|line| line.msg.contains(needle)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for job {job_id} log containing {needle}");
}

async fn insert_check_job(state: &Arc<AppState>, reason: &str, now: &str) -> String {
    let job_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        now,
    )
    .to_db();
    job.created_by = reason.to_string();
    job.reason = reason.to_string();
    state.db.insert_job(job).await.unwrap();
    job_id
}

fn make_new_version_summary_for_test(
    service_id: &str,
    current_tag: &str,
    current_display_tag: &str,
    current_digest: &str,
    candidate_tag: &str,
    candidate_display_tag: &str,
    candidate_digest: &str,
) -> serde_json::Value {
    make_new_version_summary_for_test_with_image_ref(
        service_id,
        "ghcr.io/acme/web",
        current_tag,
        current_display_tag,
        current_digest,
        candidate_tag,
        candidate_display_tag,
        candidate_digest,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_new_version_summary_for_test_with_image_ref(
    service_id: &str,
    image_ref: &str,
    current_tag: &str,
    current_display_tag: &str,
    current_digest: &str,
    candidate_tag: &str,
    candidate_display_tag: &str,
    candidate_digest: &str,
) -> serde_json::Value {
    json!({
        "newVersions": {
            "count": 1,
            "services": [{
                "stackId": "unused",
                "serviceId": service_id,
                "serviceName": "web",
                "imageRef": image_ref,
                "currentTag": current_tag,
                "currentDigest": current_digest,
                "currentDisplayTag": current_display_tag,
                "candidateTag": candidate_tag,
                "candidateDisplayTag": candidate_display_tag,
                "candidateDigest": candidate_digest,
            }],
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn reserve_new_version_notification_for_test(
    state: &Arc<AppState>,
    service_id: &str,
    job_id: &str,
    image_ref: &str,
    current_tag: &str,
    current_display_tag: &str,
    candidate_tag: &str,
    candidate_display_tag: &str,
    candidate_digest: &str,
    created_at: &str,
) {
    state
        .db
        .reserve_new_version_notification(&crate::db::NewVersionNotificationPending {
            id: format!("nvn_{}", ulid::Ulid::new()),
            service_id: service_id.to_string(),
            job_id: job_id.to_string(),
            reason: "schedule".to_string(),
            image_ref: image_ref.to_string(),
            image_tag: current_tag.to_string(),
            current_tag: current_tag.to_string(),
            current_display_tag: current_display_tag.to_string(),
            candidate_tag: candidate_tag.to_string(),
            candidate_display_tag: candidate_display_tag.to_string(),
            candidate_digest: candidate_digest.to_string(),
            created_at: created_at.to_string(),
        })
        .await
        .unwrap();
}

async fn configure_webhook_notifications(
    state: &Arc<AppState>,
) -> (
    tokio::sync::mpsc::Receiver<serde_json::Value>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel::<serde_json::Value>(8);
    let hook_app = Router::new().route(
        "/hook",
        post({
            let tx = tx.clone();
            move |Json(payload): Json<serde_json::Value>| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(payload).await;
                    axum::http::StatusCode::OK
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, hook_app).await.unwrap();
    });

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some(format!("http://{addr}/hook"));
    notification.event_new_version_enabled = true;
    state
        .db
        .put_notification_settings(&notification, &now)
        .await
        .unwrap();

    (rx, server)
}

#[derive(Clone)]
enum CleanupRunnerMode {
    StaleOnSecondScan,
    VolumeInUse,
    VolumeEstimateFallback,
    VolumeMountpointFallback,
    VolumeMissingIdentity,
    BuilderCacheNoInventoryHint,
    BuilderCacheTextFallback,
    BuilderCacheSharedLowerBound,
}

#[derive(Clone)]
struct CleanupRunner {
    mode: CleanupRunnerMode,
    scan_generation: Arc<AtomicUsize>,
}

impl CleanupRunner {
    fn stale_on_second_scan() -> Self {
        Self {
            mode: CleanupRunnerMode::StaleOnSecondScan,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn volume_in_use() -> Self {
        Self {
            mode: CleanupRunnerMode::VolumeInUse,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn volume_estimate_fallback() -> Self {
        Self {
            mode: CleanupRunnerMode::VolumeEstimateFallback,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn volume_mountpoint_fallback() -> Self {
        Self {
            mode: CleanupRunnerMode::VolumeMountpointFallback,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn volume_missing_identity() -> Self {
        Self {
            mode: CleanupRunnerMode::VolumeMissingIdentity,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn builder_cache_no_inventory_hint() -> Self {
        Self {
            mode: CleanupRunnerMode::BuilderCacheNoInventoryHint,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn builder_cache_shared_lower_bound() -> Self {
        Self {
            mode: CleanupRunnerMode::BuilderCacheSharedLowerBound,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn builder_cache_text_fallback() -> Self {
        Self {
            mode: CleanupRunnerMode::BuilderCacheTextFallback,
            scan_generation: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn stale_generation(&self) -> usize {
        self.scan_generation.load(Ordering::SeqCst)
    }
}
