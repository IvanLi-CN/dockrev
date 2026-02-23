use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use crate::{
    api, compose,
    config::Config,
    db::Db,
    ids,
    registry::{ImageRef, ManifestInfo, RegistryClient},
    runner::{CommandOutput, CommandRunner, CommandSpec},
    state::AppState,
};

async fn response_json(resp: axum::response::Response) -> serde_json::Value {
    let payload = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&payload).unwrap()
}

#[derive(Clone, Default)]
struct FakeRegistry;

#[async_trait::async_trait]
impl RegistryClient for FakeRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2".to_string(), "5.3".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "5.2" => "sha256:old",
            "5.3" => "sha256:new",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct DigestOnlyUpdateRegistry;

#[async_trait::async_trait]
impl RegistryClient for DigestOnlyUpdateRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2".to_string(), "5.3".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        // Simulate "tag moved": registry digest is newer than what's currently running.
        let digest = match reference {
            "5.2" => "sha256:new",
            _ => "sha256:other",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct CrossTagSemverRegistry;

#[async_trait::async_trait]
impl RegistryClient for CrossTagSemverRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "8-alpine".to_string(),
            "9.0.1".to_string(),
            "9.0.2".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        // The current tag (8-alpine) points to a new digest, while the runtime still runs an old
        // digest. Higher semver tags exist but must never be selected as update targets.
        let digest = match reference {
            "8-alpine" => "sha256:new",
            "9.0.2" => "sha256:other",
            "9.0.1" => "sha256:other1",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone)]
struct SlowRegistry {
    delay: Duration,
}

#[async_trait::async_trait]
impl RegistryClient for SlowRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        // 30 tags (the endpoint has its own cap); keep it deterministic for ordering assertions.
        for i in 0..30 {
            out.push(format!("5.{i}.0"));
        }
        Ok(out)
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        tokio::time::sleep(self.delay).await;
        Ok(ManifestInfo {
            digest: Some(format!("sha256:{reference}")),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct DigestTagsRegistry;

#[async_trait::async_trait]
impl RegistryClient for DigestTagsRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        // Intentionally > 30 so we can assert the digest-tags endpoint does not truncate.
        for i in 0..50 {
            out.push(format!("1.0.{i}"));
        }
        Ok(out)
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        Ok(ManifestInfo {
            digest: Some("sha256:match".to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct AnchoredSnapshotRegistry;

#[async_trait::async_trait]
impl RegistryClient for AnchoredSnapshotRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        // Keep the semver set deep enough so a non-semver anchor would be outside SNAPSHOT_DEPTH.
        for i in 0..130 {
            out.push(format!("1.0.{i}"));
        }
        out.push("legacy-1".to_string());
        Ok(out)
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = if reference == "legacy-1" {
            "sha256:match"
        } else {
            "sha256:other"
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct ListTagsFailRegistry;

#[async_trait::async_trait]
impl RegistryClient for ListTagsFailRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Err(anyhow::anyhow!("registry list tags failed"))
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        Err(anyhow::anyhow!("unexpected get_manifest"))
    }
}

#[derive(Clone, Default)]
struct SnapshotConcurrencyProbeRegistry {
    in_flight: Arc<AtomicUsize>,
    max_in_flight: Arc<AtomicUsize>,
}

impl SnapshotConcurrencyProbeRegistry {
    fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl RegistryClient for SnapshotConcurrencyProbeRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
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

        tokio::time::sleep(Duration::from_millis(80)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(Vec::new())
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        Err(anyhow::anyhow!("unexpected get_manifest"))
    }
}

#[derive(Clone, Default)]
struct PruneRegistry;

#[async_trait::async_trait]
impl RegistryClient for PruneRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "5.2.0".to_string(),
            "5.3.0".to_string(),
            "latest".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "5.2.0" => "sha256:cur",
            "5.3.0" => "sha256:cand",
            // Keep floating tags deterministic for snapshot scan.
            "latest" => "sha256:cur",
            _ => "sha256:other",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct FakeRunner;

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct StatefulRegistry {
    calls: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u32>>>,
}

impl Default for StatefulRegistry {
    fn default() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl RegistryClient for StatefulRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2.0".to_string(), "5.3.0".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let mut calls = self.calls.lock().unwrap();
        let count = calls.entry(reference.to_string()).or_insert(0);
        *count += 1;

        // Simulate a transient failure for the would-be candidate tag on its first lookup.
        if reference == "5.3.0" && *count == 1 {
            return Err(anyhow::anyhow!("transient registry error"));
        }

        let digest = match reference {
            "5.2.0" => "sha256:other",
            "5.3.0" => "sha256:match",
            // For floating tags (e.g. latest), we don't rely on this value in the test.
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct CandidateResolvedTagRegistry;

#[async_trait::async_trait]
impl RegistryClient for CandidateResolvedTagRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "latest".to_string(),
            "v0.2.15".to_string(),
            "0.2.15".to_string(),
            "v0.2.14".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" | "v0.2.15" | "0.2.15" => "sha256:new",
            "v0.2.14" => "sha256:old",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone)]
struct ScriptedRunner {
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl Default for ScriptedRunner {
    fn default() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for ScriptedRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;
        let (status, stdout) = if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, "cid1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{.Image}}")
        {
            (0, "img1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.get(3).map(|s| s.as_str()) == Some("--format")
            && args
                .get(4)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("RepoDigests"))
        {
            (0, "[\"ghcr.io/acme/web@sha256:match\"]".to_string())
        } else {
            (0, String::new())
        };
        Ok(CommandOutput {
            status,
            stdout,
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct PlatformDigestRunner {
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl Default for PlatformDigestRunner {
    fn default() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for PlatformDigestRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;
        let (status, stdout) = if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, "cid1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{.Image}}")
        {
            (0, "img1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.get(3).map(|s| s.as_str()) == Some("--format")
            && args
                .get(4)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("RepoDigests"))
        {
            (0, "[\"ghcr.io/acme/web@sha256:plat\"]".to_string())
        } else {
            (0, String::new())
        };
        Ok(CommandOutput {
            status,
            stdout,
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct CheckAndRuntimeScanRunner {
    runtime_digest: String,
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl CheckAndRuntimeScanRunner {
    fn new(runtime_digest: &str) -> Self {
        Self {
            runtime_digest: runtime_digest.to_string(),
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for CheckAndRuntimeScanRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;

        let (status, stdout) = if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, "cid1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{.Image}}")
        {
            (0, "img1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("com.docker.compose.service"))
        {
            (0, "web\timg1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.iter().any(|s| s.contains("RepoDigests"))
        {
            let digest = self.runtime_digest.clone();
            if args.iter().any(|s| s.contains("{{.Id}}")) {
                // runtime scan bulk path: id + repodigests
                (0, format!("img1\t[\"ghcr.io/acme/web@{digest}\"]\n"))
            } else {
                // check path: repodigests only
                (0, format!("[\"ghcr.io/acme/web@{digest}\"]"))
            }
        } else {
            (0, String::new())
        };

        Ok(CommandOutput {
            status,
            stdout,
            stderr: String::new(),
        })
    }
}

#[derive(Clone, Default)]
struct CountingRegistry {
    calls: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u32>>>,
}

impl CountingRegistry {
    fn total_calls(&self) -> u32 {
        self.calls.lock().unwrap().values().copied().sum()
    }
}

#[async_trait::async_trait]
impl RegistryClient for CountingRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let mut calls = self.calls.lock().unwrap();
        *calls.entry("list_tags".to_string()).or_insert(0) += 1;
        Ok(vec!["5.2".to_string(), "5.3".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let mut calls = self.calls.lock().unwrap();
        *calls
            .entry(format!("get_manifest:{reference}"))
            .or_insert(0) += 1;
        Ok(ManifestInfo {
            digest: Some(format!("sha256:{reference}")),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct DualDigestRegistry;

#[async_trait::async_trait]
impl RegistryClient for DualDigestRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2.0".to_string(), "5.3.0".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let (digest, platform_digest) = match reference {
            // Simulate multi-arch: registry header digest != platform child digest.
            "5.3.0" | "latest" => (
                Some("sha256:index".to_string()),
                Some("sha256:plat".to_string()),
            ),
            "5.2.0" => (
                Some("sha256:oldindex".to_string()),
                Some("sha256:oldplat".to_string()),
            ),
            _ => (None, None),
        };

        Ok(ManifestInfo {
            digest,
            platform_digest,
            arch: vec!["linux/amd64".to_string(), "linux/arm64".to_string()],
        })
    }
}

#[derive(Clone)]
struct CoalescingRegistry {
    list_tags_calls: Arc<AtomicUsize>,
    list_tags_delay: Duration,
}

impl CoalescingRegistry {
    fn new(list_tags_delay: Duration) -> Self {
        Self {
            list_tags_calls: Arc::new(AtomicUsize::new(0)),
            list_tags_delay,
        }
    }

    fn list_tags_calls(&self) -> usize {
        self.list_tags_calls.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl RegistryClient for CoalescingRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        self.list_tags_calls.fetch_add(1, Ordering::Relaxed);
        tokio::time::sleep(self.list_tags_delay).await;
        Ok(vec![
            "latest".to_string(),
            "5.2.0".to_string(),
            "5.3.0".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" | "5.3.0" => "sha256:new",
            "5.2.0" => "sha256:old",
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
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        check_concurrency: 8,
        registry_per_host_concurrency: 3,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
    };

    let db = Db::open(&config.db_path).await.unwrap();
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker)
}

async fn test_state(db_path: &str) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
        runtime_scan_interval_seconds: 600,
        check_concurrency: 8,
        registry_per_host_concurrency: 3,
        registry_retry_max_attempts: 3,
        registry_retry_base_ms: 250,
        registry_retry_max_ms: 2000,
    };

    let db = Db::open(&config.db_path).await.unwrap();

    let registry = Arc::new(FakeRegistry);
    let runner = Arc::new(FakeRunner);
    let snapshot_worker = Arc::new(crate::snapshot_worker::SnapshotWorker::new(
        db.clone(),
        registry.clone(),
    ));
    AppState::new(config, db, registry, runner, snapshot_worker)
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

#[tokio::test]
async fn health_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn version_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["version"], "0.1.0");
}

#[tokio::test]
async fn unknown_api_path_is_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn supervisor_paths_are_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/self-upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["code"], "supervisor_misrouted");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn service_digest_tags_lists_all_matches_without_truncation() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Use a bare hash to assert normalization (sha256: prefix added server-side).
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/digest-tags?digest=match"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = response_json(resp).await;
    let tags = body["tags"].as_array().unwrap();
    let repo_tags = body["repoTags"].as_array().unwrap();
    assert_eq!(tags.len(), 50);
    assert_eq!(repo_tags.len(), 50);
    assert_eq!(tags[0].as_str().unwrap(), "1.0.49");
    assert_eq!(tags[49].as_str().unwrap(), "1.0.0");
    assert_eq!(repo_tags[0].as_str().unwrap(), "1.0.0");
    assert_eq!(repo_tags[49].as_str().unwrap(), "1.0.49");
}

#[tokio::test]
async fn service_digest_tags_snapshot_returns_pending_when_missing() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/digest-tags-snapshot?digest=match"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap(), "pending");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert!(body["retryAfterMs"].as_u64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn service_digest_tags_snapshot_uses_anchor_tag_outside_depth() {
    let state = test_state_with(
        ":memory:",
        Arc::new(AnchoredSnapshotRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:legacy-1
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..40 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready");
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_str().unwrap(), "legacy-1");
    assert_eq!(body["scan"]["repoTagsTotal"].as_u64().unwrap(), 131);
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 100);
}

#[tokio::test]
async fn service_digest_tags_snapshot_failure_eventually_returns_ready() {
    let state = test_state_with(
        ":memory:",
        Arc::new(ListTagsFailRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..40 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready even after worker failure");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert_eq!(body["tags"].as_array().unwrap().len(), 0);
    assert!(body["scan"]["manifestsError"].as_u64().unwrap_or_default() >= 1);

    // Once the fallback snapshot is persisted, the endpoint should stop returning pending.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn snapshot_worker_limits_concurrent_runs() {
    let registry = Arc::new(SnapshotConcurrencyProbeRegistry::default());
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

    let image_repo = "ghcr.io/acme/web";
    let host_platform = "linux/amd64";
    let mut digests: Vec<String> = Vec::new();
    for i in 0..16 {
        let digest = format!("sha256:{:064x}", i + 1);
        digests.push(digest.clone());
        state
            .snapshot_worker
            .enqueue(image_repo, &digest, host_platform, "concurrency_probe")
            .await;
    }

    let mut all_ready = false;
    for _ in 0..200 {
        let mut ready = 0usize;
        for digest in &digests {
            if state
                .db
                .get_image_digest_tags_snapshot(image_repo, digest, host_platform)
                .await
                .unwrap()
                .is_some()
            {
                ready += 1;
            }
        }
        if ready == digests.len() {
            all_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(all_ready, "all queued snapshot tasks should complete");
    assert!(
        registry.max_in_flight() <= crate::snapshot_worker::SNAPSHOT_WORKER_MAX_CONCURRENCY,
        "observed list_tags concurrency {} > configured cap {}",
        registry.max_in_flight(),
        crate::snapshot_worker::SNAPSHOT_WORKER_MAX_CONCURRENCY
    );
}

#[tokio::test]
async fn check_enqueues_digest_tags_snapshot_and_endpoint_eventually_returns_ready() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestTagsRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    // Use the same scan-time code path as real jobs.
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    // Use a bare hash to assert normalization (sha256: prefix added server-side).
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=match",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let pending = response_json(resp).await;
    assert_eq!(pending["status"].as_str().unwrap(), "pending");

    let mut body: Option<serde_json::Value> = None;
    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=match",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            body = Some(response_json(resp).await);
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let body = body.expect("snapshot should become ready");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert!(body["checkedAt"].as_str().is_some_and(|s| !s.is_empty()));

    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 50);
    assert_eq!(tags[0].as_str().unwrap(), "1.0.49");
    assert_eq!(tags[49].as_str().unwrap(), "1.0.0");

    assert_eq!(body["scan"]["repoTagsTotal"].as_u64().unwrap(), 50);
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 50);
    assert_eq!(body["scan"]["manifestsOk"].as_u64().unwrap(), 50);
    assert_eq!(body["scan"]["manifestsTimeout"].as_u64().unwrap(), 0);
    assert_eq!(body["scan"]["manifestsError"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn digest_tags_snapshot_endpoint_ignores_legacy_service_snapshot_table() {
    let state = test_state_with(":memory:", Arc::new(PruneRegistry), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed extra historical digests to ensure the prune step is exercised.
    let seed_snapshot = |digest: &str| {
        serde_json::json!({
          "digest": digest,
          "tags": ["seed"],
          "checkedAt": now.as_str(),
          "scan": {
            "repoTagsTotal": 3,
            "repoTagsConsidered": 3,
            "manifestsOk": 3,
            "manifestsTimeout": 0,
            "manifestsError": 0,
          }
        })
        .to_string()
    };
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old1",
            &seed_snapshot("sha256:old1"),
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old2",
            &seed_snapshot("sha256:old2"),
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .upsert_service_digest_tags_snapshot(
            &svc.id,
            "sha256:old3",
            &seed_snapshot("sha256:old3"),
            &now,
            &now,
        )
        .await
        .unwrap();

    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        // Ensure current digest is known even if the registry is inconsistent.
        Some("sha256:cur".to_string()),
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    // Legacy service-scoped snapshot rows should no longer be served by the endpoint.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=old2",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    // current digest should be generated asynchronously and eventually become ready.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=cur",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let mut ready = false;
    for _ in 0..30 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/services/{}/digest-tags-snapshot?digest=cur",
                        svc.id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == 200 {
            ready = true;
            break;
        }
        assert_eq!(resp.status(), 202);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(ready, "current digest snapshot should become ready");
}

#[tokio::test]
async fn same_tag_digest_candidate_does_not_pick_higher_semver_tag() {
    let state = test_state_with(
        ":memory:",
        Arc::new(CrossTagSemverRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  valkey:
    image: valkey/valkey:8-alpine
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        // Simulate runtime being behind registry (digest-only update).
        Some("sha256:old".to_string()),
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let svc = &detail["stack"]["services"][0];
    assert_eq!(svc["image"]["tag"].as_str().unwrap(), "8-alpine");
    assert_eq!(svc["candidate"]["tag"].as_str().unwrap(), "8-alpine");
    assert_eq!(svc["candidate"]["digest"].as_str().unwrap(), "sha256:new");
}

#[tokio::test]
async fn service_candidates_endpoint_is_removed() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/candidates"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn service_update_conflicts_when_target_digest_mismatches_latest_scan() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestOnlyUpdateRegistry),
        Arc::new(FakeRunner),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();
    crate::service_check::check_service_and_persist(
        &state,
        "job-test",
        &svc,
        Some("sha256:old".to_string()),
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let expected_digest = detail["stack"]["services"][0]["candidate"]["digest"]
        .as_str()
        .unwrap()
        .to_string();

    let bad = serde_json::json!({
        "scope": "service",
        "serviceId": service_id,
        "targetDigest": "sha256:wrong",
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(bad.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "conflict");

    let ok = serde_json::json!({
        "scope": "service",
        "serviceId": svc.id,
        "targetDigest": expected_digest,
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(ok.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    assert!(triggered["jobId"].as_str().unwrap().starts_with("job_"));
}

#[tokio::test]
async fn register_stack_then_check_updates() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["id"].as_str().unwrap(), stack_id.as_str());

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn check_coalesces_repo_tags_fetch_for_same_image() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(120)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web1:
    image: ghcr.io/acme/web:latest
  web2:
    image: ghcr.io/acme/web:latest
  web3:
    image: ghcr.io/acme/web:latest
  web4:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..200 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");
    assert_eq!(
        registry.list_tags_calls(),
        1,
        "repo tags should be fetched once per repo in a single check job"
    );
}

#[tokio::test]
async fn checks_conflict_when_check_is_already_running() {
    let registry = Arc::new(SlowRegistry {
        delay: Duration::from_millis(250),
    });
    let runner = Arc::new(FakeRunner);
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2.0
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), 409);
    let body = response_json(resp2).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "conflict");
    assert_eq!(
        body["error"]["details"]["existingJobId"].as_str().unwrap(),
        check_id.as_str()
    );
}

#[tokio::test]
async fn checks_terminate_stale_running_job_then_start_new_one() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let old_dt = time::OffsetDateTime::now_utc() - time::Duration::hours(3);
    let old_now = old_dt
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let stale_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        stale_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &old_now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let new_id = triggered["checkId"].as_str().unwrap().to_string();
    assert_ne!(new_id, stale_id);

    let stale = state.db.get_job(&stale_id).await.unwrap().unwrap();
    assert_eq!(stale.status, "failed");
    assert!(stale.finished_at.is_some());
    assert_eq!(
        stale.summary_json["terminated"]["reason"].as_str().unwrap(),
        "stale_check"
    );
}

#[tokio::test]
async fn check_job_exposes_progress_in_detail_and_list() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut done = None;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            done = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let done = done.expect("check job did not finish in time");
    assert_eq!(done["job"]["progress"]["phase"].as_str().unwrap(), "done");
    assert_eq!(done["job"]["progress"]["percent"].as_u64().unwrap(), 100);
    assert_eq!(
        done["job"]["summary"]["progress"]["phase"]
            .as_str()
            .unwrap(),
        "done"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == check_id)
        .cloned()
        .expect("check job not in list");
    assert_eq!(item["progress"]["phase"].as_str().unwrap(), "done");
}

#[tokio::test]
async fn finish_job_preserves_existing_progress_when_summary_omits_progress() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_discovery_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &created_at,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let progress = serde_json::json!({
        "phase": "scan",
        "message": "scanned projects (3/5)",
        "current": 3,
        "total": 5,
        "percent": 60,
        "currentTarget": "demo",
        "updatedAt": created_at,
    });
    state.db.set_job_progress(&job_id, &progress).await.unwrap();

    let finished_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .finish_job(
            &job_id,
            "success",
            &finished_at,
            &serde_json::json!({ "scan": { "projectsSeen": 5 } }),
        )
        .await
        .unwrap();

    let detail_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), 200);
    let detail = response_json(detail_resp).await;
    assert_eq!(detail["job"]["progress"]["phase"].as_str().unwrap(), "scan");
    assert_eq!(detail["job"]["progress"]["percent"].as_u64().unwrap(), 60);
    assert_eq!(
        detail["job"]["summary"]["progress"]["phase"]
            .as_str()
            .unwrap(),
        "scan"
    );

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list = response_json(list_resp).await;
    let item = list["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .cloned()
        .expect("job not in list");
    assert_eq!(item["progress"]["phase"].as_str().unwrap(), "scan");
    assert_eq!(item["progress"]["percent"].as_u64().unwrap(), 60);
}

#[tokio::test]
async fn recover_incomplete_jobs_marks_running_as_failed() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let recovered = state
        .db
        .recover_incomplete_jobs(&now, "server_restart")
        .await
        .unwrap();
    assert!(recovered.iter().any(|id| id == &job_id));

    let got = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(got.status, "failed");
    assert!(got.finished_at.is_some());
    assert_eq!(
        got.summary_json["terminated"]["reason"].as_str().unwrap(),
        "server_restart"
    );
}

#[tokio::test]
async fn create_ignore_then_delete() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let _stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = response_json(resp).await;
    let stack_id = list["stacks"][0]["id"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let create = serde_json::json!({
        "enabled": true,
        "scope": { "type": "service", "serviceId": service_id },
        "match": { "kind": "prefix", "value": "5.3." },
        "note": "test"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ignores")
                .header("content-type", "application/json")
                .body(Body::from(create.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created = response_json(resp).await;
    let rule_id = created["ruleId"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/ignores")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert_eq!(list["rules"][0]["id"].as_str().unwrap(), rule_id);

    let del = serde_json::json!({ "ruleId": rule_id });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/ignores")
                .header("content-type", "application/json")
                .body(Body::from(del.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let deleted = response_json(resp).await;
    assert!(deleted["deleted"].as_bool().unwrap());
}

#[tokio::test]
async fn update_creates_job_and_logs() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated = response_json(resp).await;
    let job_id = updated["jobId"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert!(
        list["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|j| j["id"].as_str().unwrap() == job_id)
    );

    let job = {
        let mut out = None;
        for _ in 0..50 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/jobs/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            let job = response_json(resp).await;
            if job["job"]["status"].as_str().unwrap() != "running" {
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };
    assert_eq!(job["job"]["id"].as_str().unwrap(), job_id);
    assert!(!job["job"]["logs"].as_array().unwrap().is_empty());
    assert_eq!(
        job["job"]["summary"]["stacks"][0]["backup"]["status"]
            .as_str()
            .unwrap(),
        "skipped"
    );
}

#[test]
fn infer_resolved_tag_picks_highest_semver_and_exposes_all_matches() {
    let runtime_digest = "sha256:run";
    let current_tag = "latest";
    let tags: Vec<String> = ["latest", "v1.0.0-alpha.1", "1.0.0", "v1.0.0", "v0.9.0"]
        .into_iter()
        .map(str::to_string)
        .collect();

    let digest_for_tag = |tag: &str| -> Option<&'static str> {
        match tag {
            "v1.0.0" => Some("sha256:run"),
            "1.0.0" => Some("sha256:run"),
            "v1.0.0-alpha.1" => Some("sha256:run"),
            "v0.9.0" => Some("sha256:old"),
            _ => None,
        }
    };

    let mut semver_tags: Vec<(semver::Version, String)> = tags
        .iter()
        .filter_map(|t| crate::ignore::parse_version(t).map(|v| (v, t.clone())))
        .collect();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let mut resolved_tags: Vec<String> = Vec::new();
    for (_v, tag) in semver_tags {
        if let Some(d) = digest_for_tag(&tag)
            && d == runtime_digest
            && tag != current_tag
        {
            resolved_tags.push(tag);
        }
    }

    assert_eq!(resolved_tags, vec!["v1.0.0", "1.0.0", "v1.0.0-alpha.1"]);
    assert_eq!(resolved_tags.first().map(String::as_str), Some("v1.0.0"));
}

#[tokio::test]
async fn archived_stack_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .set_stack_archived(&stack_id, true, Some("user_archive"), &now)
        .await
        .unwrap();

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn archived_services_stack_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  worker:
    image: ghcr.io/acme/worker:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn archived_services_all_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "all",
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn empty_new_digests_does_not_skip_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    // Use apply mode to produce updater summary with `newDigests: {}` (FakeRunner returns empty container id).
    // Skip backups to keep the test isolated.
    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "skip",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                !logs
                    .iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped")),
                "notify should not be skipped just because newDigests is empty"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn webhook_trigger_check_creates_job_and_updates_stack() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "check",
        "scope": "stack",
        "stackId": stack_id,
        "allowArchMismatch": false,
        "backupMode": "inherit"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "webhook check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let jobs = response_json(resp).await;
    let job = jobs["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .unwrap();
    assert_eq!(job["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["type"].as_str().unwrap(), "check");
}

#[tokio::test]
async fn check_persists_registry_digest_when_runtime_digest_missing() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let digest = detail["stack"]["services"][0]["image"]["digest"]
        .as_str()
        .unwrap();
    assert_eq!(digest, "sha256:old");
}

#[tokio::test]
async fn resolved_tag_inference_does_not_skip_candidate_tag_when_candidate_digest_none() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();
    let compose_project = state.db.get_stack_compose_project(&stack_id).await.unwrap();
    assert_eq!(compose_project.as_deref(), Some("demo"));

    let img = crate::registry::ImageRef::parse("ghcr.io/acme/web:latest").unwrap();
    let runtime = super::docker_compose_service_runtime_digest(
        &state,
        "demo",
        "web",
        &super::repo_candidates(&img),
    )
    .await
    .unwrap();
    assert_eq!(runtime.as_deref(), Some("sha256:match"));

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{check_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job_detail = response_json(resp).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let image = &detail["stack"]["services"][0]["image"];
    let digest = image["digest"].as_str().unwrap_or("<none>");
    let resolved = image["resolvedTag"].as_str().unwrap_or("<none>");
    let runner_calls = runner.calls.lock().unwrap().clone();
    assert_eq!(
        digest, "sha256:match",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
    assert_eq!(
        resolved, "5.3.0",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
}

#[tokio::test]
async fn resolved_tag_inference_runs_for_major_minor_tags() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let resolved = detail["stack"]["services"][0]["image"]["resolvedTag"]
        .as_str()
        .unwrap_or("<none>");
    assert_ne!(
        resolved, "<none>",
        "expected resolvedTag for 5.2 tag: {detail}"
    );
}

#[tokio::test]
async fn candidate_resolved_tag_inference_prefers_semver_for_floating_candidate() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(CandidateResolvedTagRegistry),
        runner.clone(),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let candidate = &detail["stack"]["services"][0]["candidate"];
    assert_eq!(candidate["tag"].as_str().unwrap_or("<none>"), "latest");
    assert_eq!(
        candidate["resolvedTag"].as_str().unwrap_or("<none>"),
        "v0.2.15"
    );
    assert_eq!(
        candidate["digest"].as_str().unwrap_or("<none>"),
        "sha256:new"
    );
}

#[tokio::test]
async fn resolved_tag_inference_matches_platform_digest_and_clears_noop_candidate() {
    let runner: Arc<PlatformDigestRunner> = Arc::new(PlatformDigestRunner::default());
    let state = test_state_with(":memory:", Arc::new(DualDigestRegistry), runner.clone()).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let svc = &detail["stack"]["services"][0];
    let image = &svc["image"];

    let digest = image["digest"].as_str().unwrap_or("<none>");
    let resolved = image["resolvedTag"].as_str().unwrap_or("<none>");
    assert_eq!(digest, "sha256:plat", "unexpected stack detail: {detail}");
    assert_eq!(resolved, "5.3.0", "unexpected stack detail: {detail}");
    assert!(
        svc["candidate"].is_null(),
        "expected candidate to be cleared when digest matches: {detail}"
    );
}

#[tokio::test]
async fn webhook_trigger_update_creates_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/jobs/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            let job = response_json(resp).await;
            if job["job"]["status"].as_str().unwrap() != "running" {
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };
    assert_eq!(job["job"]["id"].as_str().unwrap(), job_id);
    assert_eq!(job["job"]["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["type"].as_str().unwrap(), "update");
    assert_eq!(job["job"]["summary"]["mode"].as_str().unwrap(), "apply");
    assert!(job["job"]["finishedAt"].as_str().unwrap().len() > 10);
}

#[tokio::test]
async fn settings_and_notifications_roundtrip() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert!(settings["backup"].is_object());
    assert!(settings["auth"].is_object());

    let put = serde_json::json!({
        "backup": {
            "enabled": true,
            "requireSuccess": true,
            "baseDir": "/tmp/dockrev-backups",
            "skipTargetsOverBytes": 123
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["backup"]["skipTargetsOverBytes"].as_u64().unwrap(),
        123
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"].is_object());

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "https://example.com/hook" },
        "telegram": { "enabled": false },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"]["enabled"].as_bool().unwrap());
    assert_eq!(conf["webhook"]["url"].as_str().unwrap(), "******");
}

#[tokio::test]
async fn github_packages_settings_masks_pat() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let put = serde_json::json!({
      "enabled": true,
      "callbackUrl": "https://dockrev.example.com/api/webhooks/github-packages",
      "targets": [],
      "repos": [],
      "pat": "ghp_example"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/github-packages/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/github-packages/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(
        body["callbackUrl"],
        "https://dockrev.example.com/api/webhooks/github-packages"
    );
    assert_eq!(body["patMasked"], "******");
}

#[tokio::test]
async fn github_packages_resolve_owner_requires_pat_saved() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/resolve")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"acme"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[test]
fn urls_match_is_tolerant_of_trailing_slash_and_default_ports() {
    assert!(super::urls_match(
        "https://dockrev.example.com/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages/",
    ));
    assert!(super::urls_match(
        "https://dockrev.example.com:443/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages",
    ));
    assert!(super::urls_match(
        "http://dockrev.example.com:80/api/webhooks/github-packages",
        "http://dockrev.example.com/api/webhooks/github-packages/",
    ));
    assert!(!super::urls_match(
        "https://dockrev.example.com/api/webhooks/github-packages",
        "https://dockrev.example.com/api/webhooks/github-packages?x=1",
    ));
}

#[test]
fn streamed_update_percent_uses_floor_to_match_stack_progress() {
    // Regression guard: streamed percent must not exceed the subsequent
    // stack-complete percent (which uses integer division / floor).
    let streamed = super::update_progress_percent(9, 13, 1.0);
    let stack_complete = super::progress_percent(10, 13);
    assert_eq!(streamed, 76);
    assert_eq!(stack_complete, 76);
    assert!(streamed <= stack_complete);
}

#[tokio::test]
async fn github_packages_webhook_validates_signature_and_dedupes_delivery() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    // Seed settings + selected repo.
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
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state.clone());

    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d1")
                .header("X-Hub-Signature-256", "sha256=deadbeef")
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert!(
        body["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );

    // Same delivery id should be ignored.
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "duplicate_delivery");
}

#[tokio::test]
async fn github_packages_webhook_respects_disabled_setting() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: false,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state);
    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "disabled-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "disabled");
}

#[tokio::test]
async fn github_packages_webhook_matches_selected_repos_case_insensitively() {
    use ring::hmac;

    let state = test_state(":memory:").await;

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
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    // Store with mixed casing.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("Acme"), String::from("Widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state);

    // Payload uses different casing than stored.
    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "case-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert!(
        body["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );
}

#[tokio::test]
async fn github_packages_repo_selected_upsert_is_case_insensitive_and_preserves_sync_state() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed a selected repo with mixed casing + a sync state.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("Acme"), String::from("Widgets"), true)],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_sync_result("Acme", "Widgets", Some(42), Some(&now), None, &now)
        .await
        .unwrap();

    // Toggle selection using different casing. This should update the existing row, not insert a
    // second case-variant duplicate, and should preserve sync state.
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", false, &now)
        .await
        .unwrap();

    let repos = state.db.list_github_packages_repos().await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].owner, "Acme");
    assert_eq!(repos[0].repo, "Widgets");
    assert!(!repos[0].selected);
    assert_eq!(repos[0].hook_id, Some(42));
}

#[tokio::test]
async fn github_packages_webhook_does_not_persist_delivery_for_unselected_repo() {
    use ring::hmac;

    let state = test_state(":memory:").await;

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
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    // Seed a different repo as selected so the incoming event is not eligible.
    state
        .db
        .put_github_packages_repos(&[(String::from("acme"), String::from("other"), true)], &now)
        .await
        .unwrap();

    let app = api::router(state.clone());

    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "unselected-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "repo_not_selected");
    assert!(
        !state
            .db
            .github_packages_delivery_exists("unselected-1")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn runtime_scan_updates_drifted_services() {
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:old".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let image = &detail["stack"]["services"][0]["image"];
    assert_eq!(image["digest"].as_str().unwrap(), "sha256:new");
    assert_eq!(image["resolvedTag"].as_str().unwrap(), "5.3");
}

#[tokio::test]
async fn runtime_scan_resolved_tag_inference_matches_check() {
    let compose = r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#;

    let compose_path_a = format!("/tmp/dockrev-test-check-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_path_a, compose).unwrap();
    let compose_path_b = format!("/tmp/dockrev-test-runtime-scan-{}.yml", ulid::Ulid::new());
    std::fs::write(&compose_path_b, compose).unwrap();

    // Check path
    let runner_a: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state_a = test_state_with(":memory:", Arc::new(FakeRegistry), runner_a).await;
    let app_a = api::router(state_a.clone());
    let stack_id_a = seed_stack_from_compose(&state_a, "demo", &compose_path_a).await;
    let now_a = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state_a
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id_a.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now_a.clone()),
            last_scan_at: now_a.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path_a.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check_payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id_a,
        "reason": "ui",
    });
    let resp = app_a
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();
    let mut finished = false;
    for _ in 0..120 {
        let resp = app_a
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app_a
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail_a = response_json(resp).await;
    let image_a = &detail_a["stack"]["services"][0]["image"];
    let digest_a = image_a["digest"].as_str().unwrap().to_string();
    let resolved_a = image_a["resolvedTag"].as_str().unwrap().to_string();
    let resolved_tags_a = image_a["resolvedTags"].clone();

    // Runtime scan path
    let runner_b: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:new"));
    let state_b = test_state_with(":memory:", Arc::new(FakeRegistry), runner_b).await;
    let app_b = api::router(state_b.clone());
    let stack_id_b = seed_stack_from_compose(&state_b, "demo", &compose_path_b).await;
    let now_b = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state_b
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id_b.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now_b.clone()),
            last_scan_at: now_b.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path_b.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let scan_payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id_b,
        "reason": "ui",
    });
    let resp = app_b
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(scan_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();
    let mut finished = false;
    for _ in 0..120 {
        let resp = app_b
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    let resp = app_b
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail_b = response_json(resp).await;
    let image_b = &detail_b["stack"]["services"][0]["image"];
    let digest_b = image_b["digest"].as_str().unwrap().to_string();
    let resolved_b = image_b["resolvedTag"].as_str().unwrap().to_string();
    let resolved_tags_b = image_b["resolvedTags"].clone();

    assert_eq!(digest_a, digest_b);
    assert_eq!(resolved_a, resolved_b);
    assert_eq!(resolved_tags_a, resolved_tags_b);
}

#[tokio::test]
async fn runtime_scan_no_drift_does_not_hit_registry() {
    let registry = Arc::new(CountingRegistry::default());
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:match"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now.clone(),
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let service_id = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap()
        .id;
    state
        .db
        .update_service_check_result(
            &service_id,
            Some("sha256:match".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let payload = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runtime-scans")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "runtime scan job did not finish in time");

    assert_eq!(
        registry.total_calls(),
        0,
        "runtime scan should not hit registry when there is no drift"
    );
}
