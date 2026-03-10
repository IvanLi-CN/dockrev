use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Json, Router, body::Body, http::Request, response::IntoResponse as _, routing::post};
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

#[derive(Debug)]
struct ParsedSseEvent {
    id: Option<String>,
    event: String,
    data: String,
}

fn parse_sse_block(block: &str) -> Option<ParsedSseEvent> {
    if block.trim().is_empty() || block.starts_with(':') {
        return None;
    }

    let mut id: Option<String> = None;
    let mut event = String::from("message");
    let mut data_lines = Vec::<String>::new();

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("id:") {
            id = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
            continue;
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some(ParsedSseEvent {
        id,
        event,
        data: data_lines.join("\n"),
    })
}

async fn wait_for_sse_event(
    body: &mut Body,
    expected_event: &str,
    timeout: Duration,
) -> ParsedSseEvent {
    let start = tokio::time::Instant::now();
    let mut buf = String::new();

    loop {
        let elapsed = tokio::time::Instant::now().saturating_duration_since(start);
        assert!(
            elapsed < timeout,
            "timed out waiting for SSE event `{expected_event}`"
        );
        let remaining = timeout.saturating_sub(elapsed);
        let frame = tokio::time::timeout(remaining, body.frame())
            .await
            .expect("waiting for SSE frame timed out")
            .expect("SSE stream ended unexpectedly")
            .expect("SSE frame read failed");

        let Ok(bytes) = frame.into_data() else {
            continue;
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(idx) = buf.find("\n\n") {
            let block = buf[..idx].to_string();
            buf = buf[(idx + 2)..].to_string();
            let Some(evt) = parse_sse_block(&block) else {
                continue;
            };
            if evt.event == expected_event {
                return evt;
            }
        }
    }
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

#[derive(Clone)]
struct PartialFailureRegistry {
    delay: Duration,
}

#[async_trait::async_trait]
impl RegistryClient for PartialFailureRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        for i in 0..24 {
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
        let n = reference
            .split('.')
            .nth(1)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        if n.is_multiple_of(2) {
            return Err(anyhow::anyhow!("manifest fetch failed"));
        }
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

#[derive(Clone, Default)]
struct FailAllRunner;

#[async_trait::async_trait]
impl CommandRunner for FailAllRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: "forced_failure".to_string(),
        })
    }
}

#[derive(Clone, Default)]
struct ResourceUsageStreamRunner {
    stats_calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl CommandRunner for ResourceUsageStreamRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let args = spec.args;
        if args.first().map(String::as_str) == Some("ps")
            && args.get(1).map(String::as_str) == Some("-q")
        {
            return Ok(CommandOutput {
                status: 0,
                stdout: "cid1\n".to_string(),
                stderr: String::new(),
            });
        }

        if args.first().map(String::as_str) == Some("inspect")
            && args.get(1).map(String::as_str) == Some("--format")
            && args.get(2).map(String::as_str)
                == Some("{{.Id}}\t{{index .Config.Labels \"com.docker.compose.service\"}}")
        {
            return Ok(CommandOutput {
                status: 0,
                stdout: "cid1\tweb\n".to_string(),
                stderr: String::new(),
            });
        }

        if args.first().map(String::as_str) == Some("stats")
            && args.get(1).map(String::as_str) == Some("--no-stream")
        {
            let sample = self.stats_calls.fetch_add(1, Ordering::SeqCst) + 1;
            let payload = serde_json::json!({
                "ID": "cid1",
                "CPUPerc": format!("{sample}.0%"),
                "MemUsage": "10MiB / 1GiB",
                "NetIO": format!("{}MiB / {}MiB", sample, sample + 1),
                "BlockIO": format!("{}MiB / {}MiB", sample / 2, sample),
                "PIDs": "5",
            });
            return Ok(CommandOutput {
                status: 0,
                stdout: format!("{}\n", payload),
                stderr: String::new(),
            });
        }

        Ok(CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: format!("unexpected command args: {:?}", args),
        })
    }
}

#[derive(Clone, Default)]
struct SemverRetryFailRunner {
    step: Arc<std::sync::Mutex<usize>>,
}

#[async_trait::async_trait]
impl CommandRunner for SemverRetryFailRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
        let out = match *step {
            0 => {
                assert!(args.ends_with(&["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "container_old\n".to_string(),
                    stderr: String::new(),
                }
            }
            1 => {
                assert_eq!(
                    args,
                    vec!["inspect", "--format", "{{.Image}}", "container_old"]
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            2 => {
                assert!(args.ends_with(&["pull", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            3 => {
                assert!(args.ends_with(&["up", "-d", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            4 => {
                assert!(args.ends_with(&["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "container_new\n".to_string(),
                    stderr: String::new(),
                }
            }
            5 => {
                assert_eq!(
                    args,
                    vec![
                        "inspect",
                        "--format",
                        "{{if .State.Health}}1{{else}}0{{end}}",
                        "container_new"
                    ]
                );
                CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                }
            }
            6 => {
                assert_eq!(
                    args,
                    vec!["inspect", "--format", "{{.Image}}", "container_new"]
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            }
            7 => {
                assert_eq!(
                    args,
                    vec!["image", "tag", "sha256:new", "ghcr.io/acme/web:latest"]
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            8 => {
                assert_eq!(
                    args,
                    vec![
                        "image",
                        "inspect",
                        "--format",
                        r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#,
                        "sha256:new"
                    ]
                );
                CommandOutput {
                    status: 0,
                    stdout: "0.7.7\n".to_string(),
                    stderr: String::new(),
                }
            }
            9 => {
                assert_eq!(
                    args,
                    vec![
                        "image",
                        "inspect",
                        "--format",
                        "{{json .RepoTags}}",
                        "sha256:new"
                    ]
                );
                CommandOutput {
                    status: 0,
                    stdout: r#"["ghcr.io/acme/web:latest"]"#.to_string(),
                    stderr: String::new(),
                }
            }
            10..=12 => {
                assert_eq!(args, vec!["pull", "ghcr.io/acme/web:0.7.7"]);
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "not found".to_string(),
                }
            }
            _ => CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: format!("unexpected step {}", *step),
            },
        };
        *step += 1;
        Ok(out)
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

#[derive(Clone)]
struct UpdateAndRuntimeScanRunner {
    updated: Arc<std::sync::Mutex<bool>>,
}

impl UpdateAndRuntimeScanRunner {
    fn new() -> Self {
        Self {
            updated: Arc::new(std::sync::Mutex::new(false)),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for UpdateAndRuntimeScanRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let args = spec.args;
        let updated_now = *self.updated.lock().unwrap();

        let (status, stdout) = if (args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
            && args
                .iter()
                .any(|arg| arg.contains("com.docker.compose.project=")))
            || args.ends_with(&["ps".to_string(), "-q".to_string(), "web".to_string()])
        {
            (
                0,
                if updated_now {
                    "container_new
"
                    .to_string()
                } else {
                    "container_old
"
                    .to_string()
                },
            )
        } else if args.ends_with(&["pull".to_string(), "web".to_string()]) {
            (0, String::new())
        } else if args.ends_with(&["up".to_string(), "-d".to_string(), "web".to_string()]) {
            *self.updated.lock().unwrap() = true;
            (0, String::new())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{.Image}}")
        {
            match args.get(3).map(|s| s.as_str()) {
                Some("container_old") => (
                    0,
                    "img_old
"
                    .to_string(),
                ),
                Some("container_new") => (
                    0,
                    "img_new
"
                    .to_string(),
                ),
                _ => (
                    0,
                    "img_new
"
                    .to_string(),
                ),
            }
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("com.docker.compose.service"))
        {
            let image = if updated_now { "img_new" } else { "img_old" };
            (
                0,
                format!(
                    "web	{image}
"
                ),
            )
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{if .State.Health}}1{{else}}0{{end}}")
        {
            (
                0,
                "0
"
                .to_string(),
            )
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.iter().any(|s| s.contains("RepoDigests"))
        {
            let emit = |image_id: &str| -> String {
                let digest = if image_id == "img_old" {
                    "sha256:old"
                } else {
                    "sha256:new"
                };
                format!("{image_id}	[\"ghcr.io/acme/web@{digest}\"]")
            };
            if args.iter().any(|s| s.contains("{{.Id}}")) {
                let lines = args
                    .iter()
                    .filter(|arg| arg.as_str() == "img_old" || arg.as_str() == "img_new")
                    .map(|arg| emit(arg))
                    .collect::<Vec<_>>();
                (
                    0,
                    format!(
                        "{}
",
                        lines.join(
                            "
"
                        )
                    ),
                )
            } else {
                let image_id = args
                    .iter()
                    .find(|arg| arg.as_str() == "img_old" || arg.as_str() == "img_new")
                    .map(String::as_str)
                    .unwrap_or("img_new");
                let digest = if image_id == "img_old" {
                    "sha256:old"
                } else {
                    "sha256:new"
                };
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

#[derive(Clone)]
struct StrictSemverDriftRegistry {
    list_tags_delay: Duration,
}

impl StrictSemverDriftRegistry {
    fn new(list_tags_delay: Duration) -> Self {
        Self { list_tags_delay }
    }
}

#[derive(Clone)]
struct StaggeredCheckRegistry {
    delay: Duration,
    hold_until_in_flight: Option<usize>,
    in_flight: Arc<AtomicUsize>,
    max_in_flight: Arc<AtomicUsize>,
    started_at: Arc<std::sync::Mutex<Vec<std::time::Instant>>>,
    peak_reached: Arc<AtomicBool>,
    peak_notify: Arc<tokio::sync::Notify>,
}

impl StaggeredCheckRegistry {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            hold_until_in_flight: None,
            in_flight: Arc::new(AtomicUsize::new(0)),
            max_in_flight: Arc::new(AtomicUsize::new(0)),
            started_at: Arc::new(std::sync::Mutex::new(Vec::new())),
            peak_reached: Arc::new(AtomicBool::new(false)),
            peak_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    fn with_peak_gate(delay: Duration, target_in_flight: usize) -> Self {
        let mut registry = Self::new(delay);
        registry.hold_until_in_flight = Some(target_in_flight.max(1));
        registry
    }

    fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }

    fn started_at(&self) -> Vec<std::time::Instant> {
        self.started_at.lock().unwrap().clone()
    }
}

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
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap(), "pending");
    assert_eq!(body["digest"].as_str().unwrap(), "sha256:match");
    assert!(body["retryAfterMs"].as_u64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn service_digest_tags_snapshot_returns_pending_while_target_digest_is_in_flight() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:match"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate",
        "linux/amd64",
        &checked_at,
        vec!["v0.1.9".to_string(), "0.1.9".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/web",
            "sha256:candidate",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest=sha256:candidate",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
}

#[tokio::test]
async fn service_digest_tags_snapshot_unknown_digest_is_not_enqueued() {
    let registry = Arc::new(CountingRegistry::default());
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
        None,
        "linux/amd64",
        &now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let calls_before = registry.total_calls();
    let unknown_digest = "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{}/digest-tags-snapshot?digest={unknown_digest}",
                    svc.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        registry.total_calls(),
        calls_before,
        "unknown digest should not trigger snapshot worker scans"
    );

    let image_repo = crate::snapshot_worker::image_repo_from_image_ref(&svc.image_ref).unwrap();
    let snapshot = state
        .db
        .get_image_digest_tags_snapshot(&image_repo, unknown_digest, "linux/amd64")
        .await
        .unwrap();
    assert!(snapshot.is_none(), "unknown digest should not be persisted");
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
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 40);
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
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
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
    for _ in 0..800 {
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
    assert_eq!(tags.len(), 40);
    assert_eq!(tags[0].as_str().unwrap(), "1.0.49");
    assert_eq!(tags[39].as_str().unwrap(), "1.0.10");

    assert_eq!(body["scan"]["repoTagsTotal"].as_u64().unwrap(), 50);
    assert_eq!(body["scan"]["repoTagsConsidered"].as_u64().unwrap(), 40);
    assert_eq!(body["scan"]["manifestsOk"].as_u64().unwrap(), 40);
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
    assert_eq!(resp.status(), 404);

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

    let missing_tag = serde_json::json!({
        "scope": "service",
        "serviceId": service_id.clone(),
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
                .body(Body::from(missing_tag.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");

    let wrong_tag = serde_json::json!({
        "scope": "service",
        "serviceId": service_id.clone(),
        "targetTag": "cross-tag-not-allowed",
        "targetDigest": expected_digest.clone(),
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
                .body(Body::from(wrong_tag.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");

    let bad = serde_json::json!({
        "scope": "service",
        "serviceId": service_id,
        "targetTag": svc.image_tag,
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
        "targetTag": svc.image_tag,
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
        "stackId": stack_id.clone(),
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
async fn check_progress_event_includes_planned_fields() {
    let registry = Arc::new(StaggeredCheckRegistry::new(Duration::from_millis(900)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
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

    let sse_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{check_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sse_resp.status(), 200);

    let mut body = sse_resp.into_body();
    let evt = wait_for_sse_event(&mut body, "job_progress", Duration::from_secs(5)).await;
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert!(payload["plannedCurrent"].is_number());
    assert!(payload["plannedTotal"].is_number());
    assert!(payload["plannedPercent"].is_number());
}

#[tokio::test]
async fn check_uses_fixed_parallelism_stagger_and_dual_progress() {
    let registry = Arc::new(StaggeredCheckRegistry::with_peak_gate(
        Duration::from_secs(8),
        crate::config::FIXED_CHECK_PARALLELISM,
    ));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web1:
    image: ghcr.io/acme/web1:5.2
  web2:
    image: ghcr.io/acme/web2:5.2
  web3:
    image: ghcr.io/acme/web3:5.2
  web4:
    image: ghcr.io/acme/web4:5.2
  web5:
    image: ghcr.io/acme/web5:5.2
  web6:
    image: ghcr.io/acme/web6:5.2
  web7:
    image: ghcr.io/acme/web7:5.2
  web8:
    image: ghcr.io/acme/web8:5.2
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
    let mut saw_split_progress = false;
    for _ in 0..500 {
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
        let progress = &job["job"]["progress"];
        let planned_current = progress["plannedCurrent"].as_u64().unwrap_or(0);
        let completed_current = progress["current"].as_u64().unwrap_or(0);
        if planned_current > completed_current {
            saw_split_progress = true;
        }
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            assert_eq!(progress["plannedCurrent"], progress["current"]);
            assert_eq!(progress["plannedTotal"], progress["total"]);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(finished, "check job did not finish in time");
    assert!(
        saw_split_progress,
        "check progress should expose planned > completed while running"
    );

    let max_in_flight = registry.max_in_flight();
    assert!(
        max_in_flight <= crate::config::FIXED_CHECK_PARALLELISM,
        "max in-flight should be capped at {}, got {max_in_flight}",
        crate::config::FIXED_CHECK_PARALLELISM
    );
    assert!(
        max_in_flight == crate::config::FIXED_CHECK_PARALLELISM,
        "max in-flight should reach fixed parallelism {}, got {max_in_flight}",
        crate::config::FIXED_CHECK_PARALLELISM
    );

    let starts = registry.started_at();
    assert!(
        starts.len() >= 2,
        "expected at least two scheduled manifest requests, got {}",
        starts.len()
    );
    for pair in starts.windows(2) {
        let gap = pair[1].duration_since(pair[0]);
        assert!(
            gap >= Duration::from_millis(800),
            "spawn gap should be ~1s, got {:?}",
            gap
        );
    }
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
    for _ in 0..800 {
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
        0,
        "check main path should not block on version inference tag scans"
    );
}

#[tokio::test]
async fn get_stack_version_inference_cache_miss_returns_pending() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

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
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "pending"
    );
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["reason"]
            .as_str()
            .unwrap_or("<none>"),
        "cache_miss"
    );
}

#[tokio::test]
async fn get_stack_shows_pending_when_new_version_task_is_inflight_even_with_cache() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(400)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["0.13.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/web",
            "sha256:new",
            "linux/amd64",
            "new_version",
        )
        .await;
    assert!(enqueued);

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
    let status = detail["stack"]["services"][0]["versionInference"]["status"]
        .as_str()
        .unwrap_or("<none>");
    assert!(
        status == "pending" || status == "ready",
        "unexpected stack detail: {detail}"
    );
    if status == "pending" {
        assert_eq!(
            detail["stack"]["services"][0]["versionInference"]["reason"]
                .as_str()
                .unwrap_or("<none>"),
            "new_version"
        );
    }
}

#[tokio::test]
async fn get_stack_all_failed_recent_snapshot_is_ready_without_reenqueue() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    set_single_service_check_result(&state, &stack_id, Some("sha256:new"), None, None).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        Vec::new(),
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 2,
        },
    )
    .await;

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
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "ready"
    );
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["reason"]
            .as_str()
            .unwrap_or("<none>"),
        "all_failed"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        registry.list_tags_calls(),
        0,
        "recent all_failed cache should not immediately re-enqueue inference"
    );
}

#[tokio::test]
async fn force_refresh_endpoint_requires_known_digest_and_dedupes_per_digest() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or("<none>"),
        "invalid_argument"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:missing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:current"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "force");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:current"
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:current", "linux/amd64")
            .await
            .as_deref(),
        Some("force")
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:candidate", "linux/amd64")
            .await,
        None
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:current"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "running");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:current"
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:candidate"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body = response_json(resp).await;
    assert_eq!(body["status"].as_str().unwrap_or("<none>"), "pending");
    assert_eq!(body["reason"].as_str().unwrap_or("<none>"), "force");
    assert_eq!(
        body["digest"].as_str().unwrap_or("<none>"),
        "sha256:candidate"
    );
    assert_eq!(
        state
            .snapshot_worker
            .in_flight_reason("ghcr.io/acme/web", "sha256:candidate", "linux/amd64")
            .await
            .as_deref(),
        Some("force")
    );
}

#[tokio::test]
async fn stack_detail_does_not_go_pending_when_only_force_task_is_in_flight() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    let service_id = set_single_service_check_result(
        &state,
        &stack_id,
        Some("sha256:current"),
        Some("latest"),
        Some("sha256:candidate"),
    )
    .await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["v1.0.0".to_string(), "latest".to_string()],
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
        &state,
        "ghcr.io/acme/web",
        "sha256:candidate",
        "linux/amd64",
        &now,
        vec!["v1.1.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    // Trigger a digest-scoped force refresh (manual), which should stay local to the popover UX
    // and must not flip stack-level `versionInference.status` to `pending`.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/services/{}/version-inference/refresh",
                    service_id
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"digest":"sha256:candidate"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

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
    assert_eq!(
        detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("<none>"),
        "ready"
    );
}

#[tokio::test]
async fn stack_detail_clears_resolved_tag_when_snapshot_has_no_semver_tags() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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
    let _service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["v0.8.7".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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
    assert_eq!(image["resolvedTag"].as_str().unwrap_or("<none>"), "v0.8.7");

    // Snapshot refreshed, but it no longer contains any semver tags.
    let now2 = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now2,
        vec!["latest".to_string(), "stable".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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
    assert!(
        image.get("resolvedTag").is_none(),
        "expected resolvedTag to be cleared when snapshot has no semver tags: {detail}"
    );
    assert!(
        image.get("resolvedTags").is_none(),
        "expected resolvedTags to be cleared when snapshot has no semver tags: {detail}"
    );
}

#[tokio::test]
async fn stack_detail_preserves_resolved_tag_when_snapshot_has_no_semver_tags_but_scan_is_incomplete()
 {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service = services.first().expect("service must exist");

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed a last-known-good resolved tag on the service itself.
    state
        .db
        .update_service_check_result(
            &service.id,
            crate::snapshot_worker::normalize_digest("sha256:current"),
            Some("v0.8.7".to_string()),
            Some(serde_json::to_string(&vec!["v0.8.7"]).unwrap()),
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

    // Snapshot refreshed, but it no longer contains any semver tags. The scan is incomplete
    // (`repo_tags_considered` < `repo_tags_total`), so it must not wipe the last-known-good
    // resolved tag values.
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec!["latest".to_string(), "stable".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 100,
            repo_tags_considered: 40,
            manifests_ok: 40,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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
    assert_eq!(
        image["resolvedTag"].as_str().unwrap_or("<none>"),
        "v0.8.7",
        "expected resolvedTag to be preserved for incomplete scan: {detail}"
    );
    assert_eq!(
        image["resolvedTags"][0].as_str().unwrap_or("<none>"),
        "v0.8.7",
        "expected resolvedTags to be preserved for incomplete scan: {detail}"
    );
}

#[tokio::test]
async fn stack_detail_preserves_resolved_tag_when_snapshot_is_all_failed() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(300)));
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
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

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service = services.first().expect("service must exist");

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // Seed a last-known-good resolved tag on the service itself. This should not be wiped when
    // the latest snapshot is an all_failed/error snapshot.
    state
        .db
        .update_service_check_result(
            &service.id,
            crate::snapshot_worker::normalize_digest("sha256:current"),
            Some("v0.8.7".to_string()),
            Some(serde_json::to_string(&vec!["v0.8.7"]).unwrap()),
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

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:current",
        "linux/amd64",
        &now,
        vec![],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 0,
            repo_tags_considered: 0,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 1,
        },
    )
    .await;

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
    assert_eq!(
        image["resolvedTag"].as_str().unwrap_or("<none>"),
        "v0.8.7",
        "expected resolvedTag to be preserved for all_failed snapshot: {detail}"
    );
}

#[tokio::test]
async fn check_candidate_digest_change_enqueues_new_version_inference() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
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
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &now,
        vec!["5.2.0".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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
    let status = detail["stack"]["services"][0]["versionInference"]["status"]
        .as_str()
        .unwrap_or("<none>");
    assert!(
        status == "pending" || status == "ready",
        "unexpected stack detail: {detail}"
    );
    if status == "pending" {
        assert_eq!(
            detail["stack"]["services"][0]["versionInference"]["reason"]
                .as_str()
                .unwrap_or("<none>"),
            "new_version"
        );
    }
}

#[tokio::test]
async fn check_candidate_digest_change_for_strict_semver_does_not_enqueue_inference() {
    let registry = Arc::new(StrictSemverDriftRegistry::new(Duration::from_millis(400)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
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
    for _ in 0..120 {
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
    assert_eq!(
        detail["stack"]["services"][0]["candidate"]["digest"]
            .as_str()
            .unwrap_or("<none>"),
        "sha256:new"
    );

    let in_flight = state
        .snapshot_worker
        .in_flight_reason("ghcr.io/acme/web", "sha256:new", "linux/amd64")
        .await;
    assert!(
        in_flight.is_none(),
        "strict semver check candidate changes should not enqueue version inference"
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
    assert!(
        detail["job"]["progress"]["plannedCurrent"].is_null(),
        "legacy progress should keep planned* absent"
    );
    assert!(
        detail["job"]["progress"]["plannedTotal"].is_null(),
        "legacy progress should keep planned* absent"
    );
    assert!(
        detail["job"]["progress"]["plannedPercent"].is_null(),
        "legacy progress should keep planned* absent"
    );
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
    assert!(
        item["progress"]["plannedCurrent"].is_null(),
        "legacy progress should keep planned* absent"
    );
    assert!(
        item["progress"]["plannedTotal"].is_null(),
        "legacy progress should keep planned* absent"
    );
    assert!(
        item["progress"]["plannedPercent"].is_null(),
        "legacy progress should keep planned* absent"
    );
}

#[tokio::test]
async fn jobs_endpoints_include_planned_progress_fields_and_invariants() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let created_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
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
        "phase": "check",
        "message": "scheduled",
        "current": 2,
        "total": 5,
        "percent": 40,
        "plannedCurrent": 4,
        "plannedTotal": 6,
        "plannedPercent": 67,
        "currentTarget": "svc-web",
        "updatedAt": created_at,
    });
    state.db.set_job_progress(&job_id, &progress).await.unwrap();

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
    let detail_progress = &detail["job"]["progress"];
    assert_eq!(detail_progress["plannedCurrent"].as_u64().unwrap(), 4);
    assert_eq!(detail_progress["plannedTotal"].as_u64().unwrap(), 6);
    assert_eq!(detail_progress["plannedPercent"].as_u64().unwrap(), 67);
    assert!(
        detail_progress["plannedCurrent"].as_u64().unwrap()
            >= detail_progress["current"].as_u64().unwrap()
    );
    assert!(
        detail_progress["plannedTotal"].as_u64().unwrap()
            >= detail_progress["total"].as_u64().unwrap()
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
    assert_eq!(item["progress"]["plannedCurrent"].as_u64().unwrap(), 4);
    assert_eq!(item["progress"]["plannedTotal"].as_u64().unwrap(), 6);
    assert_eq!(item["progress"]["plannedPercent"].as_u64().unwrap(), 67);
    assert!(
        item["progress"]["plannedCurrent"].as_u64().unwrap()
            >= item["progress"]["current"].as_u64().unwrap()
    );
    assert!(
        item["progress"]["plannedTotal"].as_u64().unwrap()
            >= item["progress"]["total"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn jobs_events_stream_emits_job_event_for_new_event_log() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );

    let mut body = resp.into_body();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({
                    "type": "job_progress",
                    "jobId": job_id.clone(),
                    "phase": "scan",
                    "message": "in progress",
                    "current": 1,
                    "total": 2,
                    "percent": 50,
                })
                .to_string(),
            },
        )
        .await
        .unwrap();

    let evt = wait_for_sse_event(&mut body, "job_event", Duration::from_secs(3)).await;
    assert!(evt.id.is_some(), "SSE event should include id");
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["jobId"].as_str().unwrap(), job_id);
    assert_eq!(payload["type"].as_str().unwrap(), "job_progress");
}

#[tokio::test]
async fn jobs_events_stream_honors_after_id_or_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "first" }).to_string(),
            },
        )
        .await
        .unwrap();
    let first_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "second" }).to_string(),
            },
        )
        .await
        .unwrap();
    let second_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();
    let second_id_s = second_id.to_string();

    let resp_query = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/events?afterId={first_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_query.status(), 200);
    let mut body_query = resp_query.into_body();
    let evt_query = wait_for_sse_event(&mut body_query, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt_query.id.as_deref(), Some(second_id_s.as_str()));
    let payload_query: serde_json::Value = serde_json::from_str(&evt_query.data).unwrap();
    assert_eq!(payload_query["step"].as_str().unwrap(), "second");

    let resp_header = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .header("Last-Event-ID", first_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_header.status(), 200);
    let mut body_header = resp_header.into_body();
    let evt_header =
        wait_for_sse_event(&mut body_header, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt_header.id.as_deref(), Some(second_id_s.as_str()));
    let payload_header: serde_json::Value = serde_json::from_str(&evt_header.data).unwrap();
    assert_eq!(payload_header["step"].as_str().unwrap(), "second");
}

#[tokio::test]
async fn jobs_events_stream_default_starts_from_tail_without_replay() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "old" }).to_string(),
            },
        )
        .await
        .unwrap();
    let old_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "new" }).to_string(),
            },
        )
        .await
        .unwrap();
    let new_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();
    let new_id_s = new_id.to_string();
    let old_id_s = old_id.to_string();

    let evt = wait_for_sse_event(&mut body, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt.id.as_deref(), Some(new_id_s.as_str()));
    assert_ne!(evt.id.as_deref(), Some(old_id_s.as_str()));
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["step"].as_str().unwrap(), "new");
}

#[tokio::test]
async fn version_inference_overview_reports_rows_filters_and_pagination() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  worker:
    image: ghcr.io/acme/worker:latest
  stable:
    image: ghcr.io/acme/stable:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:web",
        "linux/amd64",
        &now,
        vec!["1.2.3".to_string(), "latest".to_string()],
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
        &state,
        "ghcr.io/acme/worker",
        "sha256:worker",
        "linux/amd64",
        &now,
        Vec::new(),
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 2,
        },
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?page=1&perPage=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;

    assert_eq!(body["summary"]["snapshotsTotal"].as_u64(), Some(2));
    assert_eq!(body["summary"]["ready"].as_u64(), Some(1));
    assert_eq!(body["summary"]["allFailed"].as_u64(), Some(1));
    assert_eq!(body["page"].as_u64(), Some(1));
    assert_eq!(body["perPage"].as_u64(), Some(10));

    let rows = body["rows"].as_array().expect("rows must be an array");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/web")
            && row["status"].as_str() == Some("ready")
            && row["serviceCount"].as_u64() == Some(1)
    }));
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/worker")
            && row["status"].as_str() == Some("all_failed")
            && row["serviceCount"].as_u64() == Some(1)
    }));

    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?status=all_failed&page=1&perPage=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(filtered.status(), 200);
    let filtered_body = response_json(filtered).await;
    assert_eq!(filtered_body["total"].as_u64(), Some(1));
    assert_eq!(filtered_body["summary"]["snapshotsTotal"].as_u64(), Some(2));
    assert_eq!(filtered_body["summary"]["ready"].as_u64(), Some(1));
    assert_eq!(filtered_body["summary"]["allFailed"].as_u64(), Some(1));
    let filtered_rows = filtered_body["rows"].as_array().unwrap();
    assert_eq!(filtered_rows.len(), 1);
    assert_eq!(filtered_rows[0]["status"].as_str(), Some("all_failed"));

    let overflow_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?page=4294967295&perPage=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(overflow_page.status(), 200);
    let overflow_body = response_json(overflow_page).await;
    assert_eq!(overflow_body["page"].as_u64(), Some(4_294_967_295));
    assert_eq!(overflow_body["perPage"].as_u64(), Some(200));
    assert_eq!(overflow_body["total"].as_u64(), Some(2));
    assert_eq!(overflow_body["rows"].as_array().unwrap().len(), 0);

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?status=missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
}

#[tokio::test]
async fn version_inference_overview_merges_cached_and_in_flight_without_missing_rows() {
    let registry = Arc::new(SlowRegistry {
        delay: Duration::from_millis(250),
    });
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/cached",
        "sha256:cached",
        "linux/amd64",
        &now,
        vec!["1.2.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 1,
            repo_tags_considered: 1,
            manifests_ok: 1,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/running",
            "sha256:running",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let mut observed: Option<serde_json::Value> = None;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/version-inference/overview?page=1&perPage=20")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = response_json(resp).await;

        let tasks = body["tasks"].as_array().cloned().unwrap_or_default();
        let has_task = tasks.iter().any(|task| {
            task["key"].as_str() == Some("ghcr.io/acme/running@sha256:running@linux/amd64")
        });
        let has_progress = tasks
            .iter()
            .any(|task| task["status"].as_str() == Some("running") && task["progress"].is_object());
        let progress_advanced = tasks.iter().any(|task| {
            task["status"].as_str() == Some("running")
                && task["progress"]["assignedCurrent"].as_u64().unwrap_or(0) > 0
        });
        if has_task && has_progress && progress_advanced {
            observed = Some(body);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let body = observed.expect("expected in-flight task with progress in overview");
    assert_eq!(body["summary"]["snapshotsTotal"].as_u64(), Some(1));

    let rows = body["rows"].as_array().expect("rows should be array");
    assert!(
        rows.iter()
            .all(|row| row["status"].as_str() != Some("missing")),
        "overview rows should not include missing status"
    );
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/cached")
            && row["status"].as_str() == Some("ready")
    }));
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/running")
            && matches!(row["status"].as_str(), Some("running") | Some("queued"))
    }));

    let running_task = body["tasks"]
        .as_array()
        .and_then(|tasks| {
            tasks.iter().find(|task| {
                task["key"].as_str() == Some("ghcr.io/acme/running@sha256:running@linux/amd64")
            })
        })
        .expect("in-flight task should be present");
    assert!(
        matches!(
            running_task["status"].as_str(),
            Some("running") | Some("queued")
        ),
        "task status should be queued or running"
    );
    if running_task["status"].as_str() == Some("running") {
        let progress = running_task["progress"]
            .as_object()
            .expect("running task should include progress");
        assert!(progress.contains_key("phase"));
        assert!(progress.contains_key("current"));
        assert!(progress.contains_key("total"));
        assert!(progress.contains_key("percent"));
        assert!(progress.contains_key("assignedCurrent"));
        assert!(progress.contains_key("assignedTotal"));
        assert!(progress.contains_key("assignedPercent"));
        assert!(progress.contains_key("resultCurrent"));
        assert!(progress.contains_key("resultTotal"));
        assert!(progress.contains_key("resultPercent"));
        assert!(
            progress["assignedCurrent"].as_u64().unwrap_or(0) > 0,
            "running task should expose advancing in-task progress"
        );
    }
}

#[tokio::test]
async fn version_inference_overview_progress_keeps_success_lower_than_assignment_on_errors() {
    let registry = Arc::new(PartialFailureRegistry {
        delay: Duration::from_millis(140),
    });
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/partial-failure",
            "sha256:partial-failure",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let mut observed = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/version-inference/overview?page=1&perPage=20")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = response_json(resp).await;
        let maybe_progress = body["tasks"].as_array().and_then(|tasks| {
            tasks.iter().find_map(|task| {
                if task["key"].as_str()
                    != Some("ghcr.io/acme/partial-failure@sha256:partial-failure@linux/amd64")
                {
                    return None;
                }
                if task["status"].as_str() != Some("running") {
                    return None;
                }
                task["progress"].as_object()
            })
        });

        if let Some(progress) = maybe_progress {
            let assigned = progress
                .get("assignedCurrent")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let result = progress
                .get("resultCurrent")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if assigned > result {
                observed = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    assert!(
        observed,
        "expected running progress to show assignedCurrent > resultCurrent when manifest errors occur"
    );
}

#[tokio::test]
async fn version_inference_events_stream_emits_task_enqueued() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );

    let mut body = resp.into_body();
    let enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/web", "sha256:web", "linux/amd64", "force")
        .await;
    assert!(enqueued);

    let evt =
        wait_for_sse_event(&mut body, "version_inference_event", Duration::from_secs(3)).await;
    assert!(evt.id.is_some(), "SSE event should include id");
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("task_enqueued"));
    assert_eq!(payload["imageRepo"].as_str(), Some("ghcr.io/acme/web"));
}

#[tokio::test]
async fn version_inference_events_stream_reconnects_after_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut first_body = resp.into_body();

    let first_enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/web", "sha256:first", "linux/amd64", "force")
        .await;
    assert!(first_enqueued);

    let first_evt = wait_for_sse_event(
        &mut first_body,
        "version_inference_event",
        Duration::from_secs(3),
    )
    .await;
    let first_id = first_evt.id.expect("first SSE event should include id");
    let first_id_num = first_id.parse::<u64>().unwrap();

    let reconnect = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/version-inference/events?afterId={first_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconnect.status(), 200);
    let mut reconnect_body = reconnect.into_body();

    let second_enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/api", "sha256:second", "linux/amd64", "force")
        .await;
    assert!(second_enqueued);

    let second_evt = wait_for_sse_event(
        &mut reconnect_body,
        "version_inference_event",
        Duration::from_secs(3),
    )
    .await;
    let second_id = second_evt
        .id
        .expect("reconnected SSE event should include id");
    let second_id_num = second_id.parse::<u64>().unwrap();
    assert!(
        second_id_num > first_id_num,
        "expected reconnected stream to resume after {first_id_num}, got {second_id_num}"
    );

    let payload: serde_json::Value = serde_json::from_str(&second_evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("task_enqueued"));
    assert_eq!(payload["imageRepo"].as_str(), Some("ghcr.io/acme/api"));
}

#[tokio::test]
async fn version_inference_events_stream_emits_resync_required_when_after_id_is_too_old() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    for i in 0..2105 {
        let image_repo = format!("ghcr.io/acme/resync-{i}");
        let enqueued = state
            .snapshot_worker
            .enqueue(&image_repo, "sha256:resync", "linux/amd64", "force")
            .await;
        assert!(enqueued);
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events?afterId=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

    let evt =
        wait_for_sse_event(&mut body, "version_inference_event", Duration::from_secs(3)).await;
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("resync_required"));
    assert_eq!(payload["requestedAfterId"].as_i64(), Some(1));
    assert!(
        payload["oldestAvailableId"].as_i64().unwrap_or_default() > 1,
        "expected ring buffer oldest event id to move forward"
    );
}

#[tokio::test]
async fn version_inference_gc_runs_on_start_and_deletes_expired_snapshots() {
    let state = test_state(":memory:").await;

    let old = (time::OffsetDateTime::now_utc() - time::Duration::days(40))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/old",
        "sha256:old",
        "linux/amd64",
        &old,
        vec!["1.0.0".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 0,
            repo_tags_considered: 0,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    assert!(
        state
            .db
            .get_image_digest_tags_snapshot("ghcr.io/acme/old", "sha256:old", "linux/amd64")
            .await
            .unwrap()
            .is_some()
    );

    state.snapshot_worker.spawn_gc_task();

    let mut deleted = false;
    for _ in 0..80 {
        if state
            .db
            .get_image_digest_tags_snapshot("ghcr.io/acme/old", "sha256:old", "linux/amd64")
            .await
            .unwrap()
            .is_none()
        {
            deleted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        deleted,
        "expired version inference snapshots should be deleted"
    );

    let gc = state.snapshot_worker.gc_status().await;
    assert!(
        gc.last_run_at.is_some(),
        "gc should record last run timestamp"
    );
    assert!(
        gc.last_deleted.unwrap_or(0) >= 1,
        "gc should report at least one deleted snapshot"
    );
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
async fn recover_incomplete_jobs_keeps_queued_jobs_pending() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: job_id.clone(),
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: "ivan".to_string(),
            reason: "ui".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({ "op": "register", "repos": ["acme/widgets"] }),
        })
        .await
        .unwrap();

    let recovered = state
        .db
        .recover_incomplete_jobs(&now, "server_restart")
        .await
        .unwrap();
    assert!(
        !recovered.iter().any(|id| id == &job_id),
        "queued job should not be force-failed by startup recovery"
    );

    let got = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(got.status, "queued");
    assert!(got.started_at.is_none());
    assert!(got.finished_at.is_none());
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
async fn update_apply_settles_service_snapshot_before_job_terminal() {
    let runner = Arc::new(UpdateAndRuntimeScanRunner::new());
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-update-settle-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-update-settle").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()[0]
        .clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            None,
            None,
            Some("5.2".to_string()),
            Some("5.2".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let update = serde_json::json!({
        "scope": "service",
        "stackId": stack_id,
        "serviceId": service.id,
        "targetTag": "5.2",
        "targetDigest": "sha256:new",
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

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.status, "success");

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let service = stack.services.iter().find(|svc| svc.name == "web").unwrap();
    assert_eq!(service.image.digest.as_deref(), Some("sha256:new"));
    assert!(
        service.candidate.is_none(),
        "candidate should be cleared after apply settle"
    );

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("update_state_settled"))
    );
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

    let mut detail = serde_json::json!({});
    for _ in 0..120 {
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
        detail = response_json(resp).await;
        let status = detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("");
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

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

    let mut detail = serde_json::json!({});
    for _ in 0..120 {
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
        detail = response_json(resp).await;
        let status = detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("");
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

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

    let mut detail = serde_json::json!({});
    for _ in 0..120 {
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
        detail = response_json(resp).await;
        let status = detail["stack"]["services"][0]["versionInference"]["status"]
            .as_str()
            .unwrap_or("");
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
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
async fn webhook_trigger_update_rejects_service_scope() {
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
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap();

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "service",
        "serviceId": svc.id,
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
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"].as_str().unwrap(), "invalid_argument");
}

#[tokio::test]
async fn webhook_update_skips_semver_downgrade_anomaly_candidates() {
    let state = test_state(":memory:").await;
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
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

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

    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert_eq!(update["changedServices"].as_u64(), Some(0));
    assert_eq!(
        update["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(
        update["skippedVersionAnomaly"][0]["serviceId"].as_str(),
        Some(svc.id.as_str())
    );
}

#[tokio::test]
async fn webhook_update_anomaly_only_skips_backup_when_no_actionable_services() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
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
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let trigger = serde_json::json!({
        "action": "update",
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

    assert_eq!(job["job"]["status"].as_str(), Some("success"));
    let stack = &job["job"]["summary"]["stacks"][0];
    assert_eq!(stack["backup"]["status"].as_str(), Some("skipped"));
    assert_eq!(
        stack["backup"]["reason"].as_str(),
        Some("no_actionable_services_after_anomaly_skip")
    );
    assert_eq!(stack["update"]["changedServices"].as_u64(), Some(0));
    assert_eq!(
        stack["update"]["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
}

#[tokio::test]
async fn webhook_update_failure_summary_keeps_skipped_anomaly() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  api:
    image: ghcr.io/acme/api:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc_web = services.iter().find(|svc| svc.name == "web").unwrap();
    let svc_api = services.iter().find(|svc| svc.name == "api").unwrap();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc_web.id,
            Some("sha256:cur-web".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.2.53".to_string()),
            Some("sha256:cand-web".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc_api.id,
            Some("sha256:cur-api".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.3.2".to_string()),
            Some("sha256:cand-api".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

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

    assert_eq!(job["job"]["status"].as_str(), Some("failed"));
    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert!(
        update["error"]
            .as_str()
            .unwrap_or_default()
            .contains("command failed"),
        "unexpected update summary: {update}"
    );
    assert_eq!(
        update["skippedVersionAnomaly"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(1)
    );
    assert_eq!(
        update["skippedVersionAnomaly"][0]["serviceId"].as_str(),
        Some(svc_web.id.as_str())
    );
}

#[tokio::test]
async fn webhook_update_failure_summary_includes_retry_details_for_idempotent_steps() {
    let state = test_state_with(
        ":memory:",
        Arc::new(FakeRegistry),
        Arc::new(SemverRetryFailRunner::default()),
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
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:cur".to_string()),
            Some("v0.3.1".to_string()),
            Some(r#"["v0.3.1"]"#.to_string()),
            Some("latest".to_string()),
            Some("v0.3.2".to_string()),
            Some("sha256:cand".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

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
        for _ in 0..300 {
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
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        out.expect("job did not finish in time")
    };

    assert_eq!(job["job"]["status"].as_str(), Some("failed"));
    let update = &job["job"]["summary"]["stacks"][0]["update"];
    assert_eq!(update["changedServices"].as_u64(), Some(1));
    assert_eq!(
        update["oldDigests"][svc.id.as_str()].as_str(),
        Some("sha256:old")
    );
    assert_eq!(
        update["newDigests"][svc.id.as_str()].as_str(),
        Some("sha256:new")
    );
    assert_eq!(update["failureStep"].as_str(), Some("semver_pull"));
    assert_eq!(update["retry"]["attempts"].as_u64(), Some(3));
    assert_eq!(update["retry"]["maxAttempts"].as_u64(), Some(3));
    assert_eq!(update["retry"]["baseMs"].as_u64(), Some(300));
    assert_eq!(update["retry"]["maxMs"].as_u64(), Some(3000));
    assert!(
        update["lastError"]
            .as_str()
            .unwrap_or_default()
            .contains("status=1"),
        "unexpected update summary: {update}"
    );
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
    assert!(settings["resourceMonitor"].is_object());
    assert!(settings["schedules"].is_object());
    assert!(settings["auth"].is_object());
    assert!(settings["instance"].is_object());
    assert!(settings["instance"]["publicBaseUrl"].is_null());
    assert_eq!(settings["resourceMonitor"]["enabled"].as_bool(), Some(true));
    assert_eq!(
        settings["resourceMonitor"]["sampleIntervalSeconds"].as_u64(),
        Some(30)
    );
    assert_eq!(
        settings["resourceMonitor"]["retentionDays"].as_u64(),
        Some(30)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("*/30 * * * *")
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["cron"].as_str(),
        Some("0 3 * * *")
    );

    let put = serde_json::json!({
        "backup": {
            "enabled": true,
            "requireSuccess": true,
            "baseDir": "/tmp/dockrev-backups",
            "skipTargetsOverBytes": 123
        },
        "resourceMonitor": {
            "enabled": false,
            "sampleIntervalSeconds": 60
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
    assert!(settings["instance"].is_object());
    assert!(settings["instance"]["publicBaseUrl"].is_null());
    assert_eq!(
        settings["resourceMonitor"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["resourceMonitor"]["sampleIntervalSeconds"].as_u64(),
        Some(60)
    );
    assert_eq!(
        settings["resourceMonitor"]["retentionDays"].as_u64(),
        Some(30)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("*/30 * * * *")
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["ghcrWebhookAudit"]["cron"].as_str(),
        Some("0 3 * * *")
    );

    let invalid = serde_json::json!({
        "backup": settings["backup"],
        "resourceMonitor": {
            "enabled": true,
            "sampleIntervalSeconds": 7
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let set_base_url = serde_json::json!({
        "backup": settings["backup"],
        "instance": {
            "publicBaseUrl": "https://dockrev.example.com"
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(set_base_url.to_string()))
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
        settings["instance"]["publicBaseUrl"].as_str(),
        Some("https://dockrev.example.com/")
    );

    let invalid_base_url = serde_json::json!({
        "backup": settings["backup"],
        "instance": {
            "publicBaseUrl": "ftp://dockrev.example.com/"
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid_base_url.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("instance_public_base_url_invalid")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/svc-test/resource-usage/history?window=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("resource_monitor_disabled")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/services/svc-test/resource-usage/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("resource_monitor_disabled")
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
        "telegram": {
            "enabled": true,
            "botToken": "123456:telegram-bot-token",
            "chatId": "-1001234567890"
        },
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
    assert_eq!(conf["telegram"]["botToken"].as_str(), None);
    assert_eq!(conf["telegram"]["botTokenConfigured"].as_bool(), Some(true));
    assert_eq!(conf["telegram"]["chatId"].as_str(), Some("-1001234567890"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "  -10055667788  " },
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

    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(
        db_conf.telegram_bot_token.as_deref(),
        Some("123456:telegram-bot-token")
    );
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "******" },
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
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true },
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
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id.as_deref(), Some("-10055667788"));

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "******" },
        "telegram": { "enabled": true, "chatId": "   " },
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
    let db_conf = state.db.get_notification_settings().await.unwrap();
    assert_eq!(db_conf.telegram_chat_id, None);

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
    assert_eq!(conf["telegram"]["botToken"].as_str(), None);
    assert_eq!(conf["telegram"]["botTokenConfigured"].as_bool(), Some(true));
    assert!(conf["telegram"]["chatId"].is_null());
}

#[tokio::test]
async fn settings_schedule_cron_validation() {
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
    let backup = settings["backup"].clone();

    let invalid = serde_json::json!({
        "backup": backup,
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "not a cron" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(invalid.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let payload = response_json(resp).await;
    assert_eq!(
        payload["error"]["details"]["reason"].as_str(),
        Some("cron_invalid")
    );
    assert_eq!(
        payload["error"]["details"]["field"].as_str(),
        Some("schedules.updateCheck.cron")
    );

    let put_5 = serde_json::json!({
        "backup": settings["backup"],
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "* * * * *" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put_5.to_string()))
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
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("* * * * *")
    );

    let put_6 = serde_json::json!({
        "backup": settings["backup"],
        "schedules": {
            "updateCheck": { "enabled": true, "cron": "0 * * * * *" }
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put_6.to_string()))
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
        settings["schedules"]["updateCheck"]["enabled"].as_bool(),
        Some(true)
    );
    assert_eq!(
        settings["schedules"]["updateCheck"]["cron"].as_str(),
        Some("0 * * * * *")
    );
}

#[tokio::test]
async fn notifications_test_endpoint_supports_channel_override() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                        "channel": "webhook",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["ok"].as_bool(), Some(true));
    let results = payload["results"].as_object().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results["webhook"]["ok"].as_bool(), Some(false));
    assert!(
        results["webhook"]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("webhook.url missing")
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["ok"].as_bool(), Some(true));
    let results = payload["results"].as_object().unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn notifications_test_endpoint_emits_v2_payload_to_webhook() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(1);
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
    state
        .db
        .put_notification_settings(&notification, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "message": "dockrev: test notification",
                        "channel": "webhook",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("webhook payload missing");
    assert_eq!(
        payload["schema"].as_str(),
        Some("dockrev.notification.test.v2")
    );
    assert_eq!(payload["kind"].as_str(), Some("notification_test"));
    assert_eq!(payload["channel"].as_str(), Some("webhook"));
    assert_eq!(
        payload["human"]["summary"].as_str(),
        Some("dockrev: test notification")
    );
    assert_eq!(
        payload["debug"]["requestedChannel"].as_str(),
        Some("webhook")
    );
    assert!(payload.get("type").is_none());
    assert!(payload.get("ts").is_none());
    assert!(payload.get("message").is_none());
    assert!(payload.get("title").is_none());
    assert!(payload.get("body").is_none());

    server.abort();
}

#[tokio::test]
async fn resource_usage_history_returns_samples_for_window() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-history-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let now = time::OffsetDateTime::now_utc();
    let sampled_at_1 = (now - time::Duration::minutes(20))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let sampled_at_2 = (now - time::Duration::minutes(5))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .insert_service_resource_samples(&[
            crate::db::ServiceResourceSampleInput {
                service_id: service_id.clone(),
                sampled_at: sampled_at_1,
                cpu_percent: 12.5,
                mem_used_bytes: Some(128 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(5_000_000),
                net_tx_bytes: Some(2_500_000),
                block_read_bytes: Some(1_300_000),
                block_write_bytes: Some(900_000),
                pids: Some(8),
                container_count: 1,
            },
            crate::db::ServiceResourceSampleInput {
                service_id: service_id.clone(),
                sampled_at: sampled_at_2,
                cpu_percent: 18.0,
                mem_used_bytes: Some(156 * 1024 * 1024),
                mem_limit_bytes: Some(1024 * 1024 * 1024),
                net_rx_bytes: Some(8_000_000),
                net_tx_bytes: Some(4_800_000),
                block_read_bytes: Some(2_300_000),
                block_write_bytes: Some(1_700_000),
                pids: Some(11),
                container_count: 1,
            },
        ])
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/services/{service_id}/resource-usage/history?window=1h"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload = response_json(resp).await;
    assert_eq!(payload["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(payload["window"].as_str(), Some("1h"));
    let samples = payload["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0]["containerCount"].as_u64(), Some(1));
    assert_eq!(samples[1]["cpuPercent"].as_f64(), Some(18.0));
}

#[tokio::test]
async fn resource_usage_events_emits_error_when_runtime_stats_unavailable() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-resource-events-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    let mut body = resp.into_body();
    let evt = wait_for_sse_event(&mut body, "resource_usage_error", Duration::from_secs(2)).await;
    let data: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(data["serviceId"].as_str(), Some(service_id.as_str()));
    assert_eq!(data["error"].as_str(), Some("runtime_stats_unavailable"));
}

#[tokio::test]
async fn resource_usage_events_emits_error_when_initial_snapshot_fails() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FailAllRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-resource-events-initial-fail-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let evt = wait_for_sse_event(&mut body, "resource_usage_error", Duration::from_secs(2)).await;
    let data: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(data["serviceId"].as_str(), Some(service_id.as_str()));
    assert!(!data["error"].as_str().unwrap_or_default().is_empty());
}

#[tokio::test]
async fn resource_usage_events_keep_streaming_past_sampler_idle_window() {
    let runner = Arc::new(ResourceUsageStreamRunner::default());
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), runner.clone()).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-resource-events-stream-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: nginx:1.27
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-resource-stream").await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_id = services[0].id.clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/services/{service_id}/resource-usage/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let snapshot =
        wait_for_sse_event(&mut body, "resource_usage_snapshot", Duration::from_secs(2)).await;
    let snapshot_data: serde_json::Value = serde_json::from_str(&snapshot.data).unwrap();
    assert_eq!(
        snapshot_data["serviceId"].as_str(),
        Some(service_id.as_str())
    );

    let tick_ids = tokio::time::timeout(Duration::from_secs(20), async {
        let mut ids = Vec::new();
        while ids.len() < 12 {
            let evt =
                wait_for_sse_event(&mut body, "resource_usage_tick", Duration::from_secs(15)).await;
            ids.push(
                evt.id
                    .as_deref()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or_default(),
            );
        }
        ids
    })
    .await
    .expect("resource usage SSE should stay alive past the sampler idle window");

    assert!(tick_ids.last().copied().unwrap_or_default() >= 13);
    assert!(runner.stats_calls.load(Ordering::SeqCst) >= 12);
}

#[tokio::test]
async fn deploy_welcome_roundtrip() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-welcome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["neverAutoOpen"], false);
    assert!(body["updatedAt"].is_null());

    let put = serde_json::json!({ "neverAutoOpen": true });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/deploy-welcome")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["neverAutoOpen"], true);
    assert!(body["updatedAt"].as_str().unwrap().len() > 10);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-welcome")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["neverAutoOpen"], true);
}

#[tokio::test]
async fn deploy_check_report_is_read_only() {
    let state = test_state_with_authz(":memory:", Some("ops"), None, false).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let app = api::router(state.clone());
    let before_jobs = state.db.list_jobs().await.unwrap().len();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-check/report")
                .header("X-Forwarded-User", "ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["overall"]["result"], "pass");
    assert_eq!(body["overall"]["blockingCheckIds"], serde_json::json!([]));
    let checks = body["checks"].as_array().unwrap();
    assert!(checks.iter().any(|c| c["id"] == "core.docker_engine"));
    assert!(checks.iter().any(|c| c["id"] == "core.compose_access"));
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "core.service_image_ref_valid")
    );
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "core.update_executor_ready")
    );
    let registry_auth = checks
        .iter()
        .find(|c| c["id"] == "feature.registry_auth")
        .unwrap();
    assert_eq!(registry_auth["status"], "na");
    assert_eq!(registry_auth["naReason"], "missing_prerequisite");
    let webhook = checks
        .iter()
        .find(|c| c["id"] == "feature.notifications.webhook")
        .unwrap();
    assert_eq!(webhook["status"], "na");
    assert_eq!(webhook["required"], false);

    let after_jobs = state.db.list_jobs().await.unwrap().len();
    assert_eq!(before_jobs, after_jobs);
}

#[tokio::test]
async fn deploy_check_report_fails_when_enabled_feature_is_misconfigured() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = None;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_notification_settings(&notification, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["overall"]["result"], "fail");
    let blocking = body["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.notifications.webhook"));
}

#[tokio::test]
async fn deploy_check_report_fails_when_webhook_scheme_is_not_http() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some("ftp://dockrev.example.com/hook".to_string());
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_notification_settings(&notification, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["overall"]["result"], "fail");
    let blocking = body["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.notifications.webhook"));
}

#[tokio::test]
async fn deploy_check_report_fails_when_github_packages_callback_scheme_is_not_http() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "ftp://dockrev.example.com/api/webhooks/github-packages".to_string();
    settings.pat = Some("ghp_example".to_string());
    settings.webhook_secret = Some("secret123".to_string());
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", true, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["overall"]["result"], "fail");
    let blocking = body["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.github_packages"));

    let github_packages = body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "feature.github_packages")
        .unwrap();
    assert_eq!(github_packages["status"], "fail");
    assert!(
        github_packages["evidence"]
            .as_str()
            .unwrap()
            .contains("callbackUrl(invalid_scheme)")
    );
}

#[tokio::test]
async fn deploy_check_report_fails_when_github_packages_has_no_selected_repos() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_file = format!("/tmp/dockrev-preflight-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_file,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "prod", &compose_file).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    settings.pat = Some("ghp_example".to_string());
    settings.webhook_secret = Some("secret123".to_string());
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let app = api::router(state.clone());
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["overall"]["result"], "fail");
    let blocking = body["overall"]["blockingCheckIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(blocking.contains(&"feature.github_packages"));

    let github_packages = body["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "feature.github_packages")
        .unwrap();
    assert_eq!(github_packages["status"], "fail");
    assert!(
        github_packages["evidence"]
            .as_str()
            .unwrap()
            .contains("repos(selected=0)")
    );
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
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "ghcr_pat_missing"
    );
}

#[tokio::test]
async fn github_packages_resolve_repo_returns_visibility_and_activity_fields() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/resolve")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"acme/widgets"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["kind"], "repo");
    assert_eq!(body["owner"], "acme");
    assert_eq!(body["repos"][0]["fullName"], "acme/widgets");
    assert_eq!(body["repos"][0]["selected"], true);
    assert_eq!(body["repos"][0]["visibility"], "unknown");
    assert!(body["repos"][0]["lastActivityAt"].is_null());
}

#[test]
fn github_http_status_from_error_parses_status_code() {
    let err = anyhow::anyhow!("github http 403 Forbidden: bad credentials");
    assert_eq!(super::github_http_status_from_error(&err), Some(403));
}

#[tokio::test]
async fn github_owner_resolve_error_map_timeout_reason() {
    let err = anyhow::anyhow!("upstream request timed out");
    let api_err = super::map_github_owner_resolve_error("acme", err);
    let resp = api_err.into_response();
    assert_eq!(resp.status(), 500);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "github_upstream_timeout"
    );
}

#[tokio::test]
async fn github_owner_resolve_error_map_auth_reason() {
    let err = anyhow::anyhow!("github http 401 Unauthorized: bad credentials");
    let api_err = super::map_github_owner_resolve_error("acme", err);
    let resp = api_err.into_response();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(
        body["error"]["details"]["reason"]
            .as_str()
            .unwrap_or_default(),
        "ghcr_pat_invalid_or_scope_insufficient"
    );
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

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=rejected&q=d1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(0));

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

    // Same delivery id should be ignored even if repo selection changed after first processing.
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), false)],
            &now,
        )
        .await
        .unwrap();

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
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "duplicate_delivery");
    assert_eq!(body["attemptCount"], 2);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?q=d2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["deliveries"][0]["deliveryId"], "d2");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["reason"], serde_json::Value::Null);
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
    assert_eq!(body["deliveries"][0]["attemptCount"], 2);
    assert!(
        body["deliveries"][0]["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );
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
        .clone()
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
    assert!(
        !state
            .db
            .github_packages_delivery_exists("disabled-1")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn github_packages_webhook_ignores_non_package_event_without_persisting() {
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

    let app = api::router(state.clone());
    let payload = serde_json::json!({ "zen": "keep it simple" });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "ping")
                .header("X-GitHub-Delivery", "ping-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "not_package_event");
    assert!(
        !state
            .db
            .github_packages_delivery_exists("ping-1")
            .await
            .unwrap()
    );
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
        .clone()
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
async fn github_packages_webhook_matches_managed_service_and_enqueues_check() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-single-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-single").await;
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-match-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("chk_"));
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "check");
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(job.summary_json["repo"].as_str(), Some("ghcr.io/acme/web"));
    assert_eq!(job.summary_json["deliveryId"].as_str(), Some("svc-match-1"));
    assert_eq!(job.summary_json["fallbackUsed"], false);
}

#[tokio::test]
async fn github_packages_webhook_matches_multiple_services_without_discovery_noise() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path_a = format!(
        "/tmp/dockrev-ghcr-webhook-multi-a-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path_a,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  other:
    image: ghcr.io/acme/other:1.0
"#,
    )
    .unwrap();
    let stack_a = seed_stack_from_compose(&state, "demo-a", &compose_path_a).await;
    seed_discovered_project(&state, &stack_a, "demo-a").await;

    let compose_path_b = format!(
        "/tmp/dockrev-ghcr-webhook-multi-b-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path_b,
        r#"
services:
  api:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_b = seed_stack_from_compose(&state, "demo-b", &compose_path_b).await;
    seed_discovered_project(&state, &stack_b, "demo-b").await;

    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let service_ids = vec![
        state
            .db
            .list_services_for_check(&stack_a)
            .await
            .unwrap()
            .into_iter()
            .find(|service| service.name == "web")
            .unwrap()
            .id,
        state.db.list_services_for_check(&stack_b).await.unwrap()[0]
            .id
            .clone(),
    ];

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-match-2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let matched = body["matchedServiceIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert_eq!(matched.len(), 2);
    for service_id in &service_ids {
        assert!(matched.contains(&service_id.as_str()));
    }
    let job_ids = body["jobIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(job_ids.len(), 2);
    assert!(job_ids.iter().all(|job_id| job_id.starts_with("chk_")));

    for job_id in &job_ids {
        let job = wait_for_job_terminal(&state, job_id).await;
        assert_eq!(job.r#type.as_str(), "check");
        assert_eq!(job.scope.as_str(), "service");
    }

    let jobs = state.db.list_jobs().await.unwrap();
    assert_eq!(
        jobs.iter()
            .filter(|job| job.r#type.as_str() == "check")
            .count(),
        2
    );
    assert_eq!(
        jobs.iter()
            .filter(|job| job.r#type.as_str() == "discovery")
            .count(),
        0
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let delivery_job_ids = body["deliveries"][0]["jobIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(delivery_job_ids.len(), 2);
    for job_id in &job_ids {
        assert!(delivery_job_ids.contains(&job_id.as_str()));
    }
}

#[tokio::test]
async fn github_packages_webhook_zero_match_falls_back_to_discovery() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(body["fallbackReason"], "no_managed_service_match");
    assert_eq!(body["matchedServiceIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("dsc_"));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.r#type.as_str(), "discovery");
    assert_eq!(job.reason, "github_webhook");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(job.summary_json["fallbackUsed"], true);
    assert_eq!(
        job.summary_json["fallbackReason"].as_str(),
        Some("no_managed_service_match")
    );
}

#[tokio::test]
async fn github_packages_webhook_reuses_pending_discovery_fallback_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_discovery_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    existing.created_by = "schedule".to_string();
    existing.reason = "schedule".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["jobId"], existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([existing_id.clone()]));
    assert_eq!(
        body["reusedJobIds"],
        serde_json::json!([existing_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn github_packages_webhook_replaces_stale_discovery_fallback_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());
    enable_github_packages_webhook(&state, "secret123", &[("acme", "widgets", true)]).await;

    let stale_at = (time::OffsetDateTime::now_utc() - time::Duration::hours(3))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_discovery_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Discovery,
        crate::api::types::JobScope::All,
        None,
        None,
        &stale_at,
    )
    .to_db();
    existing.created_by = "schedule".to_string();
    existing.reason = "schedule".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "fallback-stale-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["fallbackUsed"], true);
    assert_eq!(body["fallbackReason"], "no_managed_service_match");
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert!(job_id.starts_with("dsc_"));
    assert_ne!(job_id, existing_id);

    let stale = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(stale.status, "failed");
    assert_eq!(
        stale.summary_json["terminated"]["reason"].as_str(),
        Some("stale_check")
    );
    let stale_logs = state.db.list_job_logs(&existing_id).await.unwrap();
    assert!(
        stale_logs
            .iter()
            .any(|line| line.msg.contains("job terminated: reason=stale_check"))
    );

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.reason, "github_webhook");
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);
}

#[tokio::test]
async fn github_packages_webhook_reuses_pending_service_check_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-reuse-{}.yml", ulid::Ulid::new());
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
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_check_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Service,
        Some(stack_id.clone()),
        Some(service_id.clone()),
        &now,
    )
    .to_db();
    existing.created_by = "ivan".to_string();
    existing.reason = "ui".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-reuse-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["jobId"], existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([existing_id.clone()]));
    assert_eq!(
        body["reusedJobIds"],
        serde_json::json!([existing_id.clone()])
    );
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn github_packages_webhook_does_not_reuse_covering_stack_check_job() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-stack-running-{}.yml",
        ulid::Ulid::new()
    );
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
    seed_discovered_project(&state, &stack_id, "demo-stack-running").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_check_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::Stack,
        Some(stack_id.clone()),
        None,
        &now,
    )
    .to_db();
    existing.created_by = "ivan".to_string();
    existing.reason = "ui".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-stack-running-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert_ne!(job_id, existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));

    let existing = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(existing.scope.as_str(), "stack");
    assert_eq!(existing.status, "running");
}

#[tokio::test]
async fn github_packages_webhook_does_not_reuse_covering_all_check_job() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-all-running-{}.yml",
        ulid::Ulid::new()
    );
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
    seed_discovered_project(&state, &stack_id, "demo-all-running").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let existing_id = ids::new_check_id();
    let mut existing = crate::api::types::JobRecord::new_running(
        existing_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    existing.created_by = "ivan".to_string();
    existing.reason = "ui".to_string();
    state.db.insert_job(existing).await.unwrap();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "svc-all-running-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();
    assert_ne!(job_id, existing_id);
    assert_eq!(body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(body["reusedJobIds"], serde_json::json!([]));
    assert_eq!(
        body["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(body["fallbackUsed"], false);
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 2);

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.scope.as_str(), "service");
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.stack_id.as_deref(), Some(stack_id.as_str()));
    assert_eq!(job.service_id.as_deref(), Some(service_id.as_str()));

    let existing = state.db.get_job(&existing_id).await.unwrap().unwrap();
    assert_eq!(existing.scope.as_str(), "all");
    assert_eq!(existing.status, "running");
}

#[tokio::test]
async fn github_packages_webhook_dedupes_concurrent_service_checks_across_deliveries() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(200)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-concurrent-{}.yml",
        ulid::Ulid::new()
    );
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
    seed_discovered_project(&state, &stack_id, "demo-concurrent").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let req1 = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/webhooks/github-packages")
            .header("X-GitHub-Event", "package")
            .header("X-GitHub-Delivery", "svc-concurrent-1")
            .header("X-Hub-Signature-256", sig.clone())
            .body(Body::from(payload_bytes.clone()))
            .unwrap(),
    );
    let req2 = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/webhooks/github-packages")
            .header("X-GitHub-Event", "package")
            .header("X-GitHub-Delivery", "svc-concurrent-2")
            .header("X-Hub-Signature-256", sig)
            .body(Body::from(payload_bytes))
            .unwrap(),
    );
    let (resp1, resp2) = tokio::join!(req1, req2);
    let resp1 = resp1.unwrap();
    let resp2 = resp2.unwrap();
    assert_eq!(resp1.status(), 200);
    assert_eq!(resp2.status(), 200);

    let body1 = response_json(resp1).await;
    let body2 = response_json(resp2).await;
    let job_id_1 = body1["jobId"].as_str().unwrap().to_string();
    let job_id_2 = body2["jobId"].as_str().unwrap().to_string();
    assert_eq!(job_id_1, job_id_2);
    assert_eq!(
        body1["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(
        body2["matchedServiceIds"],
        serde_json::json!([service_id.clone()])
    );
    assert_eq!(state.db.list_jobs().await.unwrap().len(), 1);

    let reused_count = [body1["reusedJobIds"].clone(), body2["reusedJobIds"].clone()]
        .into_iter()
        .filter(|value| value == &serde_json::json!([job_id_1.clone()]))
        .count();
    let inserted_count = [body1["reusedJobIds"].clone(), body2["reusedJobIds"].clone()]
        .into_iter()
        .filter(|value| value == &serde_json::json!([]))
        .count();
    assert_eq!(reused_count, 1);
    assert_eq!(inserted_count, 1);

    let job = wait_for_job_terminal(&state, &job_id_1).await;
    assert_eq!(job.reason, "webhook");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
}

#[tokio::test]
async fn merge_job_summary_fields_unions_webhook_arrays() {
    let state = test_state(":memory:").await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_check_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.summary_json = serde_json::json!({
        "source": "github_webhook",
        "matchedServiceIds": ["svc-a"],
        "reusedJobIds": ["chk-old"],
        "deliveryId": "delivery-1",
        "deliveryIds": ["delivery-1"],
        "repo": "ghcr.io/acme/web",
        "repos": ["ghcr.io/acme/web"]
    });
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .merge_job_summary_fields(
            &job_id,
            &serde_json::json!({
                "matchedServiceIds": ["svc-b", "svc-a"],
                "reusedJobIds": ["chk-old", "chk-new"],
                "deliveryId": "delivery-2",
                "deliveryIds": ["delivery-2"],
                "repo": "ghcr.io/acme/api",
                "repos": ["ghcr.io/acme/api"]
            }),
        )
        .await
        .unwrap();

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(
        job.summary_json["matchedServiceIds"],
        serde_json::json!(["svc-a", "svc-b"])
    );
    assert_eq!(
        job.summary_json["reusedJobIds"],
        serde_json::json!(["chk-old", "chk-new"])
    );
    assert_eq!(
        job.summary_json["deliveryId"],
        serde_json::json!("delivery-2")
    );
}

#[tokio::test]
async fn webhook_reused_ui_check_still_sends_new_version_notification() {
    let registry = Arc::new(CoalescingRegistry::new(Duration::from_millis(150)));
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry, runner).await;
    let app = api::router(state.clone());

    let compose_path = format!(
        "/tmp/dockrev-ghcr-webhook-reuse-notify-{}.yml",
        ulid::Ulid::new()
    );
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
    seed_discovered_project(&state, &stack_id, "demo-reuse-notify").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let ui_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "service",
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "reason": "ui"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ui_resp.status(), 200);
    let ui_body = response_json(ui_resp).await;
    let job_id = ui_body["checkId"].as_str().unwrap().to_string();

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let webhook_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "notify-reuse-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(webhook_resp.status(), 200);
    let webhook_body = response_json(webhook_resp).await;
    assert_eq!(webhook_body["jobId"], job_id);
    assert_eq!(webhook_body["jobIds"], serde_json::json!([job_id.clone()]));
    assert_eq!(
        webhook_body["reusedJobIds"],
        serde_json::json!([job_id.clone()])
    );

    let delivered = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        delivered["schema"].as_str(),
        Some("dockrev.notification.new_version_discovered.v2")
    );
    assert_eq!(delivered["kind"].as_str(), Some("new_version_discovered"));
    assert_eq!(delivered["channel"].as_str(), Some("webhook"));
    assert_eq!(delivered["check"]["jobId"].as_str(), Some(job_id.as_str()));

    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.reason, "ui");
    assert_eq!(job.summary_json["source"].as_str(), Some("github_webhook"));
    assert_eq!(
        job.summary_json["deliveryId"].as_str(),
        Some("notify-reuse-1")
    );
    wait_for_job_log_contains(&state, &job_id, "notify: webhook=ok").await;
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("github webhook reused check job"))
    );
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("notify: webhook=ok"))
    );
    server.abort();
}

#[tokio::test]
async fn service_scope_check_only_updates_target_service() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-check-service-scope-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  worker:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    seed_discovered_project(&state, &stack_id, "demo-service-scope").await;

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let target_service_id = services
        .iter()
        .find(|service| service.name == "web")
        .unwrap()
        .id
        .clone();
    let other_service_id = services
        .iter()
        .find(|service| service.name == "worker")
        .unwrap()
        .id
        .clone();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "service",
                        "stackId": stack_id,
                        "serviceId": target_service_id,
                        "reason": "ui"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["checkId"].as_str().unwrap().to_string();
    let job = wait_for_job_terminal(&state, &job_id).await;
    assert_eq!(job.summary_json["servicesChecked"].as_u64(), Some(1));

    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let target = stack
        .services
        .iter()
        .find(|service| service.id == target_service_id)
        .unwrap();
    let other = stack
        .services
        .iter()
        .find(|service| service.id == other_service_id)
        .unwrap();
    assert_eq!(
        target
            .candidate
            .as_ref()
            .map(|candidate| candidate.tag.as_str()),
        Some("5.2")
    );
    assert!(other.candidate.is_none());
}

#[tokio::test]
async fn webhook_reason_check_sends_new_version_notification() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ghcr-webhook-notify-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-notify").await;
    enable_github_packages_webhook(&state, "secret123", &[("acme", "web", true)]).await;
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let payload = serde_json::json!({
        "action": "published",
        "repository": { "full_name": "acme/web", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "notify-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["jobId"].as_str().unwrap().to_string();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["schema"].as_str(),
        Some("dockrev.notification.new_version_discovered.v2")
    );
    assert_eq!(payload["kind"].as_str(), Some("new_version_discovered"));
    assert_eq!(payload["channel"].as_str(), Some("webhook"));
    assert_eq!(payload["check"]["jobId"].as_str(), Some(job_id.as_str()));
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["currentDisplayTag"].as_str(),
        Some("5.2")
    );
    assert_eq!(
        payload["links"]["serviceUrls"][0]["candidateDisplayTag"].as_str(),
        Some("5.2")
    );

    let job = wait_for_job_terminal(&state, &job_id).await;
    wait_for_job_log_contains(&state, &job_id, "notify: webhook=ok").await;
    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(job.finished_at.is_some());
    assert!(
        logs.iter()
            .any(|line| line.msg.contains("notify: webhook=ok"))
    );
    server.abort();
}

#[tokio::test]
async fn schedule_new_version_notifications_are_deduped_by_active_record() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-schedule-notify-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let first_now = "2026-03-09T00:00:00Z";
    let first_job_id = insert_check_job(&state, "schedule", first_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &first_job_id,
        "schedule",
        first_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let first_payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        first_payload["check"]["jobId"].as_str(),
        Some(first_job_id.as_str())
    );
    let first_service = &first_payload["links"]["serviceUrls"][0];
    assert_eq!(first_service["currentDisplayTag"].as_str(), Some("1.0.0"));
    assert_eq!(first_service["candidateDisplayTag"].as_str(), Some("1.1.0"));
    let summary = first_payload["human"]["summary"]
        .as_str()
        .unwrap_or_default();
    assert!(summary.contains("1.0.0 -> 1.1.0"));
    assert!(!summary.contains("latest -> latest"));

    let second_now = "2026-03-09T00:01:00Z";
    let second_job_id = insert_check_job(&state, "schedule", second_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &second_job_id,
        "schedule",
        second_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "duplicate schedule notification should be skipped"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "sent");
    assert_eq!(rows[0].reason, "schedule");

    let logs = state.db.list_job_logs(&second_job_id).await.unwrap();
    assert!(logs.iter().any(|line| {
        line.msg.contains(
            "new-version notification skipped: all 1 services already have active records",
        )
    }));
    server.abort();
}

#[tokio::test]
async fn webhook_notifications_filter_to_matched_service_ids() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!(
        "/tmp/dockrev-webhook-filter-notify-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  worker:
    image: ghcr.io/acme/worker:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let web = services
        .iter()
        .find(|service| service.name == "web")
        .unwrap()
        .clone();
    let worker = services
        .iter()
        .find(|service| service.name == "worker")
        .unwrap()
        .clone();
    let now = "2026-03-09T00:00:00Z";
    for (service, digest, current_display, candidate_display) in [
        (&web, "sha256:web-new", "1.0.0", "1.1.0"),
        (&worker, "sha256:worker-new", "2.0.0", "2.1.0"),
    ] {
        state
            .db
            .update_service_check_result(
                &service.id,
                Some(format!("sha256:{}-old", service.name)),
                Some(current_display.to_string()),
                Some(format!("[\"{current_display}\"]")),
                Some(service.image_tag.clone()),
                Some(candidate_display.to_string()),
                Some(digest.to_string()),
                Some("match".to_string()),
                Some("[\"linux/amd64\"]".to_string()),
                None,
                None,
                now,
                now,
            )
            .await
            .unwrap();
    }
    let (mut rx, server) = configure_webhook_notifications(&state).await;
    let job_id = insert_check_job(&state, "webhook", now).await;
    let summary = serde_json::json!({
        "source": "github_webhook",
        "matchedServiceIds": [web.id.clone()],
        "servicesChecked": 2,
        "newVersions": {
            "services": [
                {
                    "stackId": stack_id.clone(),
                    "serviceId": web.id.clone(),
                    "imageRef": web.image_ref.clone(),
                    "currentTag": web.image_tag.clone(),
                    "currentDisplayTag": "1.0.0",
                    "candidateTag": "latest",
                    "candidateDisplayTag": "1.1.0",
                    "candidateDigest": "sha256:web-new"
                },
                {
                    "stackId": stack_id.clone(),
                    "serviceId": worker.id.clone(),
                    "imageRef": worker.image_ref.clone(),
                    "currentTag": worker.image_tag.clone(),
                    "currentDisplayTag": "2.0.0",
                    "candidateTag": "latest",
                    "candidateDisplayTag": "2.1.0",
                    "candidateDigest": "sha256:worker-new"
                }
            ]
        }
    });

    super::operations::maybe_notify_check_new_versions(&state, &job_id, "webhook", now, &summary)
        .await
        .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    let service_urls = payload["links"]["serviceUrls"].as_array().unwrap();
    assert_eq!(service_urls.len(), 1);
    assert!(
        service_urls[0]["url"]
            .as_str()
            .unwrap_or_default()
            .contains(&web.id)
    );

    let web_rows = state
        .db
        .list_new_version_notifications_for_service(&web.id)
        .await
        .unwrap();
    let worker_rows = state
        .db
        .list_new_version_notifications_for_service(&worker.id)
        .await
        .unwrap();
    assert_eq!(web_rows.len(), 1);
    assert!(worker_rows.is_empty());
    server.abort();
}

#[tokio::test]
async fn stale_new_version_notifications_are_skipped_when_candidate_was_cleared() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-stale-notify-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let active_now = "2026-03-09T00:00:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            active_now,
            active_now,
        )
        .await
        .unwrap();

    let cleared_now = "2026-03-09T00:01:00Z";
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            cleared_now,
            cleared_now,
        )
        .await
        .unwrap();

    let job_id = insert_check_job(&state, "schedule", cleared_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &job_id,
        "schedule",
        cleared_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "stale notification should be skipped after candidate clears"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert!(rows.is_empty());

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(logs.iter().any(|line| {
        line.msg.contains("new-version notification skipped: all 1 services no longer have matching active candidates")
    }));
    server.abort();
}

#[tokio::test]
async fn transient_runtime_unknown_does_not_reopen_same_digest_notification() {
    let state = test_state_with(
        ":memory:",
        Arc::new(DigestOnlyUpdateRegistry),
        Arc::new(FakeRunner),
    )
    .await;

    let compose_path = format!("/tmp/dockrev-transient-runtime-{}.yml", ulid::Ulid::new());
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
    let manifest_digest_cache = crate::service_check::new_manifest_digest_cache();
    let repo_tags_cache = crate::service_check::new_repo_tags_cache();

    let first_check_now = "2026-03-09T00:00:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-1",
        &service,
        Some("sha256:old".to_string()),
        "linux/amd64",
        first_check_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "5.2".to_string(),
        current_display_tag: "5.2".to_string(),
        candidate_tag: "5.2".to_string(),
        candidate_display_tag: "5.2".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let first_job_id = insert_check_job(&state, "schedule", first_check_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &first_job_id,
        "schedule",
        first_check_now,
        1,
        &discovered,
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("first webhook receive timeout")
        .expect("first notification payload missing");

    let uncertain_now = "2026-03-09T00:01:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-2",
        &service,
        None,
        "linux/amd64",
        uncertain_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let restored_now = "2026-03-09T00:02:00Z";
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    crate::service_check::check_service_and_persist(
        &state,
        "job-check-3",
        &service,
        Some("sha256:old".to_string()),
        "linux/amd64",
        restored_now,
        &manifest_digest_cache,
        &repo_tags_cache,
    )
    .await
    .unwrap();

    let second_job_id = insert_check_job(&state, "schedule", restored_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &second_job_id,
        "schedule",
        restored_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "same digest should remain deduped after an inconclusive runtime check"
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "sent");
    assert_eq!(rows[0].superseded_at, None);

    server.abort();
}

#[tokio::test]
async fn failed_new_version_notification_record_retries_after_all_channels_fail() {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-failed-notify-{}.yml", ulid::Ulid::new());
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
    let service = state.db.list_services_for_check(&stack_id).await.unwrap()[0].clone();
    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-09T00:00:00Z",
            "2026-03-09T00:00:00Z",
        )
        .await
        .unwrap();
    let discovered = vec![crate::notify::NewVersionDiscoveredService {
        stack_id: stack_id.clone(),
        service_id: service.id.clone(),
        image_ref: service.image_ref.clone(),
        current_tag: "latest".to_string(),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: "sha256:new".to_string(),
    }];

    let failing_app = Router::new().route(
        "/hook",
        post(|| async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let failing_server = tokio::spawn(async move {
        axum::serve(listener, failing_app).await.unwrap();
    });
    let fail_now = "2026-03-09T00:00:00Z";
    let mut notification = state.db.get_notification_settings().await.unwrap();
    notification.webhook_enabled = true;
    notification.webhook_url = Some(format!("http://{addr}/hook"));
    notification.event_new_version_enabled = true;
    state
        .db
        .put_notification_settings(&notification, fail_now)
        .await
        .unwrap();

    let failed_job_id = insert_check_job(&state, "schedule", fail_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &failed_job_id,
        "schedule",
        fail_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "failed");
    assert!(
        rows[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("webhook")
    );

    let (mut rx, success_server) = configure_webhook_notifications(&state).await;
    let retry_now = "2026-03-09T00:01:00Z";
    let retry_job_id = insert_check_job(&state, "schedule", retry_now).await;
    crate::notify::notify_new_versions_discovered(
        state.as_ref(),
        &retry_job_id,
        "schedule",
        retry_now,
        1,
        &discovered,
    )
    .await
    .unwrap();

    let payload = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("webhook receive timeout")
        .expect("notification payload missing");
    assert_eq!(
        payload["check"]["jobId"].as_str(),
        Some(retry_job_id.as_str())
    );

    let rows = state
        .db
        .list_new_version_notifications_for_service(&service.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "failed");
    assert_eq!(rows[1].status, "sent");

    success_server.abort();
    failing_server.abort();
}

#[tokio::test]
async fn ui_reason_check_does_not_send_new_version_notification() {
    let runner = Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-ui-check-notify-{}.yml", ulid::Ulid::new());
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
    seed_discovered_project(&state, &stack_id, "demo-ui-silent").await;
    let service_id = state.db.list_services_for_check(&stack_id).await.unwrap()[0]
        .id
        .clone();
    let (mut rx, server) = configure_webhook_notifications(&state).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "scope": "service",
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "reason": "ui"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    let job_id = body["checkId"].as_str().unwrap().to_string();
    let _job = wait_for_job_terminal(&state, &job_id).await;

    let received = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
    assert!(
        received.is_err(),
        "ui check should not emit new-version notifications"
    );

    let logs = state.db.list_job_logs(&job_id).await.unwrap();
    assert!(!logs.iter().any(|line| line.msg.contains("notify:")));
    server.abort();
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
async fn github_packages_repo_selected_enqueues_register_job_when_enabled() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("Acme", "Widgets", false, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/selected")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "fullName": "acme/widgets",
                        "selected": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(
        !job_id.is_empty(),
        "selected=true should enqueue a register job and return jobId"
    );

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.r#type.as_str(), "github_packages_webhook");
    assert_eq!(job.status, "queued");
    assert!(job.started_at.is_none());

    let repo = state
        .db
        .get_github_packages_repo("acme", "widgets")
        .await
        .unwrap()
        .unwrap();
    assert!(repo.selected);
    assert_eq!(repo.webhook_state, "queued");
    assert_eq!(repo.webhook_job_id.as_deref(), Some(job_id.as_str()));
    assert_eq!(repo.last_op.as_deref(), Some("register"));
}

#[tokio::test]
async fn github_packages_repo_delete_enqueues_unregister_job_and_keeps_row_until_worker_finishes() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
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
        .set_github_packages_repo_sync_result(
            "Acme",
            "Widgets",
            Some(12345),
            Some(&now),
            None,
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/delete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "fullName": "acme/widgets"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!job_id.is_empty());

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(job.r#type.as_str(), "github_packages_webhook");
    assert_eq!(job.status, "queued");

    let repo = state
        .db
        .get_github_packages_repo("acme", "widgets")
        .await
        .unwrap();
    let repo = repo.expect("row should remain until unregister worker succeeds");
    assert_eq!(repo.webhook_state, "queued");
    assert_eq!(repo.webhook_job_id.as_deref(), Some(job_id.as_str()));
    assert_eq!(repo.last_op.as_deref(), Some("unregister"));
}

#[tokio::test]
async fn github_packages_webhook_sync_all_enqueues_and_reuses_pending_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;
    assert_eq!(first_body["ok"], true);
    assert_eq!(first_body["reused"], false);
    assert_eq!(first_body["status"], "queued");
    let first_job_id = first_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!first_job_id.is_empty());

    let first_job = state.db.get_job(&first_job_id).await.unwrap().unwrap();
    assert_eq!(
        first_job.r#type.as_str(),
        "github_packages_webhook_sync_all"
    );
    assert_eq!(first_job.status, "queued");
    assert_eq!(first_job.summary_json["op"], "sync_all");
    assert_eq!(
        first_job.summary_json["repos"].as_array().map(|v| v.len()),
        Some(2)
    );

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;
    assert_eq!(second_body["ok"], true);
    assert_eq!(second_body["reused"], true);
    assert_eq!(second_body["jobId"], first_job_id);
    assert_eq!(second_body["status"], "queued");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_enqueues_and_dedupes_by_repo() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    let first_body = response_json(first).await;
    assert_eq!(first_body["ok"], true);
    assert_eq!(first_body["reused"], false);
    let first_job_id = first_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!first_job_id.is_empty());

    let first_job = state.db.get_job(&first_job_id).await.unwrap().unwrap();
    assert_eq!(
        first_job.r#type.as_str(),
        "github_packages_webhook_sync_repo"
    );
    assert_eq!(first_job.status, "queued");
    assert_eq!(first_job.service_id.as_deref(), Some("acme/widgets"));

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), 200);
    let second_body = response_json(second).await;
    assert_eq!(second_body["ok"], true);
    assert_eq!(second_body["reused"], true);
    assert_eq!(second_body["jobId"], first_job_id);

    let third = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/worker" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(third.status(), 200);
    let third_body = response_json(third).await;
    assert_eq!(third_body["ok"], true);
    assert_eq!(third_body["reused"], false);
    let third_job_id = third_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!third_job_id.is_empty());
    assert_ne!(third_job_id, first_job_id);
}

#[tokio::test]
async fn github_packages_webhook_sync_all_returns_400_when_no_selected_repos() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "invalid_argument");
    assert_eq!(body["error"]["message"], "no tracked repos selected");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_404_for_untracked_repo() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
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

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/worker" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "repo is not tracked");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_400_for_invalid_full_name() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "invalid-full-name" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "invalid_argument");
    assert_eq!(body["error"]["message"], "invalid fullName");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_returns_409_when_unregister_in_progress() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
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

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/delete")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(body["error"]["message"], "repo unregister in progress");
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_reuses_pending_legacy_register_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .upsert_github_packages_repo_selected("acme", "widgets", false, &now)
        .await
        .unwrap();

    let selected = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/repos/selected")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets", "selected": true }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(selected.status(), 200);
    let selected_body = response_json(selected).await;
    let legacy_job_id = selected_body["jobId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!legacy_job_id.is_empty());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["reused"], true);
    assert_eq!(body["jobId"], legacy_job_id);
    assert_eq!(body["status"], "queued");
}

#[tokio::test]
async fn github_packages_webhook_sync_all_ignores_repos_with_unregister_pending() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("widgets"), true),
                (String::from("acme"), String::from("worker"), true),
            ],
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_job_state(
            "acme",
            "widgets",
            "queued",
            Some("job_unregister_demo"),
            Some("unregister"),
            &now,
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["reused"], false);
    let job_id = body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!job_id.is_empty());

    let job = state.db.get_job(&job_id).await.unwrap().unwrap();
    let repos = job.summary_json["repos"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].as_str(), Some("acme/worker"));
}

#[tokio::test]
async fn github_packages_webhook_sync_repo_can_enqueue_while_sync_all_pending() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    state
        .db
        .put_github_packages_settings(&settings, &now)
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

    let full = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(full.status(), 200);
    let full_body = response_json(full).await;
    let full_job_id = full_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!full_job_id.is_empty());

    let repo = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/webhook/sync-repo")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "fullName": "acme/widgets" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repo.status(), 200);
    let repo_body = response_json(repo).await;
    assert_eq!(repo_body["reused"], false);
    let repo_job_id = repo_body["jobId"].as_str().unwrap_or_default().to_string();
    assert!(!repo_job_id.is_empty());
    assert_ne!(repo_job_id, full_job_id);
}

#[tokio::test]
async fn github_packages_webhook_overview_reports_repo_and_job_summary() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc();
    let now_s = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let audit_older = (now - time::Duration::hours(2))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let audit_newer = (now - time::Duration::hours(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    state
        .db
        .put_github_packages_repos(
            &[
                (String::from("acme"), String::from("ok-repo"), true),
                (String::from("acme"), String::from("missing-repo"), true),
                (String::from("acme"), String::from("error-repo"), true),
                (String::from("acme"), String::from("unselected-repo"), false),
            ],
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "ok-repo",
            "ok",
            Some(111),
            Some(&now_s),
            Some(&audit_older),
            None,
            None,
            Some("register"),
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "missing-repo",
            "missing",
            None,
            None,
            Some(&audit_newer),
            Some("webhook missing"),
            None,
            Some("audit_all"),
            &now_s,
        )
        .await
        .unwrap();
    state
        .db
        .set_github_packages_repo_webhook_result(
            "acme",
            "error-repo",
            "error",
            None,
            None,
            None,
            Some("permission denied"),
            None,
            Some("register"),
            &now_s,
        )
        .await
        .unwrap();

    let queued_job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: queued_job_id,
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now_s.clone(),
            created_by: "ivan".to_string(),
            reason: "ui".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({"op":"register","repos":["acme/ok-repo"]}),
        })
        .await
        .unwrap();

    let running_job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: running_job_id.clone(),
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "running".to_string(),
            created_at: (now + time::Duration::seconds(1))
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap(),
            created_by: "schedule".to_string(),
            reason: "schedule".to_string(),
            started_at: Some(now_s.clone()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({"op":"audit_all","repos":[]}),
        })
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/overview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;

    assert_eq!(body["summary"]["tracked"], 3);
    assert_eq!(body["summary"]["ok"], 1);
    assert_eq!(body["summary"]["missing"], 1);
    assert_eq!(body["summary"]["error"], 1);
    assert_eq!(body["summary"]["conflict"], 0);
    assert_eq!(body["jobsQueued"], 1);
    assert_eq!(body["jobsRunning"], 1);
    assert_eq!(body["runningJobId"].as_str(), Some(running_job_id.as_str()));
    assert_eq!(body["lastAuditAt"].as_str(), Some(audit_newer.as_str()));
}

#[tokio::test]
async fn github_packages_webhook_deliveries_lists_desc_and_paginates() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    state
        .db
        .insert_github_packages_delivery_if_new(
            "d1",
            "2026-03-01T00:00:00Z",
            Some("acme"),
            Some("alpha"),
        )
        .await
        .unwrap();
    state
        .db
        .insert_github_packages_delivery_if_new(
            "d2",
            "2026-03-01T00:00:00Z",
            Some("acme"),
            Some("beta"),
        )
        .await
        .unwrap();
    state
        .db
        .insert_github_packages_delivery_if_new(
            "d3",
            "2026-03-02T00:00:00Z",
            Some("acme"),
            Some("gamma"),
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?page=1&perPage=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["perPage"], 2);
    assert_eq!(body["total"], 3);
    assert_eq!(body["filteredTotal"], 3);
    assert_eq!(body["summary"]["processed"], 3);
    assert_eq!(body["summary"]["ignored"], 0);
    assert_eq!(body["summary"]["rejected"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(2));
    assert_eq!(body["deliveries"][0]["deliveryId"], "d3");
    assert_eq!(body["deliveries"][0]["fullName"], "acme/gamma");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
    assert_eq!(body["deliveries"][0]["attemptCount"], 1);
    assert_eq!(body["deliveries"][1]["deliveryId"], "d2");
    assert_eq!(body["deliveries"][1]["fullName"], "acme/beta");

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?page=2&perPage=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 3);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(1));
    assert_eq!(body["deliveries"][0]["deliveryId"], "d1");
    assert_eq!(body["deliveries"][0]["fullName"], "acme/alpha");
}

#[tokio::test]
async fn github_packages_webhook_deliveries_returns_empty_when_no_data() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["perPage"], 50);
    assert_eq!(body["total"], 0);
    assert_eq!(body["filteredTotal"], 0);
    assert_eq!(body["summary"]["processed"], 0);
    assert_eq!(body["summary"]["ignored"], 0);
    assert_eq!(body["summary"]["rejected"], 0);
    assert_eq!(body["deliveries"].as_array().map(|v| v.len()), Some(0));
}

#[tokio::test]
async fn github_packages_webhook_deliveries_supports_decision_and_query_filters() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-ok".to_string(),
            received_at: "2026-03-02T09:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("alpha".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "processed".to_string(),
            reason: None,
            response_status: Some(200),
            job_id: Some("dsc_ok".to_string()),
            job_ids: vec!["dsc_ok".to_string()],
        })
        .await
        .unwrap();
    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-ignore".to_string(),
            received_at: "2026-03-02T10:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("beta".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "ignored".to_string(),
            reason: Some("repo_not_selected".to_string()),
            response_status: Some(200),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();
    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "d-reject".to_string(),
            received_at: "2026-03-02T11:00:00Z".to_string(),
            owner: None,
            repo: None,
            event: Some("package".to_string()),
            action: None,
            decision: "rejected".to_string(),
            reason: Some("invalid_signature".to_string()),
            response_status: Some(401),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=processed&q=dsc_ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["summary"]["processed"], 1);
    assert_eq!(body["summary"]["ignored"], 1);
    assert_eq!(body["summary"]["rejected"], 1);
    assert_eq!(body["deliveries"][0]["jobId"], "dsc_ok");
    assert_eq!(
        body["deliveries"][0]["jobIds"],
        serde_json::json!(["dsc_ok"])
    );
    assert_eq!(body["deliveries"][0]["deliveryId"], "d-ok");
    assert_eq!(body["deliveries"][0]["decision"], "processed");
    assert_eq!(body["deliveries"][0]["reason"], serde_json::Value::Null);
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
}

#[tokio::test]
async fn github_packages_webhook_deliveries_requires_auth_when_anonymous_disabled() {
    let state = test_state_auth_required(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_emits_new_event() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );
    let mut body = resp.into_body();

    state
        .db
        .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
            delivery_id: "evt-new".to_string(),
            received_at: "2026-03-09T10:00:00Z".to_string(),
            owner: Some("acme".to_string()),
            repo: Some("widgets".to_string()),
            event: Some("package".to_string()),
            action: Some("published".to_string()),
            decision: "processed".to_string(),
            reason: None,
            response_status: Some(200),
            job_id: None,
            job_ids: Vec::new(),
        })
        .await
        .unwrap();
    let event_id = state
        .db
        .insert_github_packages_delivery_event(
            "evt-new",
            "2026-03-09T10:00:00Z",
            &github_delivery_event_payload("evt-new", "2026-03-09T10:00:00Z", "processed", 1)
                .to_string(),
        )
        .await
        .unwrap();

    let evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let event_id_s = event_id.to_string();
    assert_eq!(evt.id.as_deref(), Some(event_id_s.as_str()));
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["deliveryId"].as_str(), Some("evt-new"));
    assert_eq!(payload["attemptCount"].as_u64(), Some(1));
    assert_eq!(payload["decision"].as_str(), Some("processed"));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_honors_after_id_and_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    for (delivery_id, ts, attempt_count) in [
        ("evt-1", "2026-03-09T10:00:00Z", 1_u32),
        ("evt-2", "2026-03-09T10:05:00Z", 2_u32),
    ] {
        state
            .db
            .record_github_packages_delivery(crate::db::GitHubPackagesWebhookDeliveryRecordInput {
                delivery_id: delivery_id.to_string(),
                received_at: ts.to_string(),
                owner: Some("acme".to_string()),
                repo: Some("widgets".to_string()),
                event: Some("package".to_string()),
                action: Some("published".to_string()),
                decision: "processed".to_string(),
                reason: None,
                response_status: Some(200),
                job_id: None,
                job_ids: Vec::new(),
            })
            .await
            .unwrap();
        state
            .db
            .insert_github_packages_delivery_event(
                delivery_id,
                ts,
                &github_delivery_event_payload(delivery_id, ts, "processed", attempt_count)
                    .to_string(),
            )
            .await
            .unwrap();
    }

    let first_id = 1_i64;

    let resp_query = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/github-packages/webhook/deliveries/events?afterId={first_id}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_query.status(), 200);
    let mut body_query = resp_query.into_body();
    let evt_query = wait_for_sse_event(
        &mut body_query,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let payload_query: serde_json::Value = serde_json::from_str(&evt_query.data).unwrap();
    assert_eq!(payload_query["deliveryId"].as_str(), Some("evt-2"));
    assert_eq!(payload_query["attemptCount"].as_u64(), Some(2));

    let resp_header = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .header("Last-Event-ID", first_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_header.status(), 200);
    let mut body_header = resp_header.into_body();
    let evt_header = wait_for_sse_event(
        &mut body_header,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let payload_header: serde_json::Value = serde_json::from_str(&evt_header.data).unwrap();
    assert_eq!(payload_header["deliveryId"].as_str(), Some("evt-2"));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_stream_emits_processed_and_duplicate_attempt_updates()
 {
    let state = test_state_with(":memory:", Arc::new(FakeRegistry), Arc::new(FakeRunner)).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut settings = state.db.get_github_packages_settings().await.unwrap();
    settings.enabled = true;
    settings.callback_url = "https://dockrev.example.com/api/webhooks/github-packages".to_string();
    settings.pat = Some("ghp_example".to_string());
    settings.webhook_secret = Some("secret123".to_string());
    state
        .db
        .put_github_packages_settings(&settings, &now)
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
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let (payload_bytes, sig) = sign_github_package_payload("secret123", &payload);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "evt-dup")
                .header("X-Hub-Signature-256", sig.clone())
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let first_evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let first_payload: serde_json::Value = serde_json::from_str(&first_evt.data).unwrap();
    assert_eq!(first_payload["deliveryId"].as_str(), Some("evt-dup"));
    assert_eq!(first_payload["decision"].as_str(), Some("processed"));
    assert_eq!(first_payload["attemptCount"].as_u64(), Some(1));
    assert_eq!(first_payload["responseStatus"].as_u64(), Some(200));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "evt-dup")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let second_evt = wait_for_sse_event(
        &mut body,
        "github_packages_delivery_event",
        Duration::from_secs(3),
    )
    .await;
    let second_payload: serde_json::Value = serde_json::from_str(&second_evt.data).unwrap();
    assert_eq!(second_payload["deliveryId"].as_str(), Some("evt-dup"));
    assert_eq!(second_payload["decision"].as_str(), Some("processed"));
    assert_eq!(second_payload["attemptCount"].as_u64(), Some(2));
}

#[tokio::test]
async fn github_packages_webhook_delivery_events_requires_auth_when_anonymous_disabled() {
    let state = test_state_auth_required(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn settings_auth_reports_group_match_details() {
    let state = test_state_with_authz(":memory:", Some("alice"), Some("ops"), false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("X-Forwarded-User", "bob")
                .header("Remote-Groups", "dev, ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["forwardHeaderName"], "x-forwarded-user");
    assert_eq!(body["auth"]["groupHeaderName"], "remote-groups");
    assert_eq!(body["auth"]["authorizationMode"], "user_or_group");
    assert_eq!(body["auth"]["matchedBy"], "group");
    assert_eq!(body["auth"]["currentUser"], "bob");
    assert_eq!(
        body["auth"]["currentGroups"],
        serde_json::json!(["d**v", "o**s"])
    );
}

#[tokio::test]
async fn settings_auth_reports_group_only_mode() {
    let state = test_state_with_authz(":memory:", None, Some("ops"), false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("Remote-Groups", "dev, ops")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["authorizationMode"], "group_only");
    assert_eq!(body["auth"]["matchedBy"], "group");
}

#[tokio::test]
async fn settings_auth_serializes_empty_current_groups() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .header("X-Forwarded-User", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["auth"]["currentGroups"], serde_json::json!([]));
}

#[tokio::test]
async fn protected_endpoint_returns_authz_details_without_redirect_target() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
    assert_eq!(body["error"]["details"]["authorizationMode"], "user_only");
    assert_eq!(body["error"]["details"]["allowedUserMasked"], "al***ce");
    assert!(
        body["error"]["details"]
            .as_object()
            .is_some_and(|obj| !obj.contains_key("redirectTo"))
    );
}

#[tokio::test]
async fn protected_endpoint_does_not_allow_dev_bypass_when_allowlist_is_configured() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, true).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
    assert_eq!(body["error"]["details"]["authorizationMode"], "user_only");
}

#[tokio::test]
async fn deploy_check_report_requires_authorized_request() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
}

#[tokio::test]
async fn deploy_check_report_rejects_unauthorized_request_before_preflight() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    state
        .db
        .put_instance_public_base_url(
            Some("ftp://dockrev.example.com".to_string()),
            &super::now_rfc3339().unwrap(),
        )
        .await
        .unwrap();
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/deploy-check/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body = response_json(resp).await;
    assert_eq!(body["error"]["code"], "auth_required");
    assert_eq!(body["error"]["details"]["reason"], "identity_missing");
}

#[tokio::test]
async fn github_packages_webhook_persists_ignored_delivery_for_unselected_repo() {
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
        .clone()
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
        state
            .db
            .github_packages_delivery_exists("unselected-1")
            .await
            .unwrap()
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/github-packages/webhook/deliveries?decision=ignored")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["filteredTotal"], 1);
    assert_eq!(body["deliveries"][0]["deliveryId"], "unselected-1");
    assert_eq!(body["deliveries"][0]["decision"], "ignored");
    assert_eq!(body["deliveries"][0]["reason"], "repo_not_selected");
    assert_eq!(body["deliveries"][0]["responseStatus"], 200);
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

    let snapshot_checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state_a,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &snapshot_checked_at,
        vec!["5.2".to_string(), "5.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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

    let snapshot_checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state_b,
        "ghcr.io/acme/web",
        "sha256:new",
        "linux/amd64",
        &snapshot_checked_at,
        vec!["5.2".to_string(), "5.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

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
async fn runtime_scan_preserves_candidate_resolved_tag_when_candidate_digest_unchanged() {
    let compose_path = format!(
        "/tmp/dockrev-test-runtime-scan-preserve-{}.yml",
        ulid::Ulid::new()
    );
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", Arc::new(DigestOnlyUpdateRegistry), runner).await;
    let app = api::router(state.clone());
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

    let service = state
        .db
        .list_services_for_runtime_scan(&stack_id)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "web")
        .unwrap();

    state
        .db
        .update_service_check_result(
            &service.id,
            Some("sha256:older".to_string()),
            None,
            None,
            Some("5.2".to_string()),
            Some("5.2".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();

    let scan_payload = serde_json::json!({
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
    let candidate = &detail["stack"]["services"][0]["candidate"];
    assert_eq!(candidate["digest"].as_str(), Some("sha256:new"));
    assert_eq!(candidate["resolvedTag"].as_str(), Some("5.2"));
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

#[tokio::test]
async fn runtime_scan_candidate_change_for_strict_semver_does_not_enqueue_inference() {
    let registry = Arc::new(StrictSemverDriftRegistry::new(Duration::from_millis(400)));
    let runner: Arc<CheckAndRuntimeScanRunner> =
        Arc::new(CheckAndRuntimeScanRunner::new("sha256:old"));
    let state = test_state_with(":memory:", registry.clone(), runner).await;
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
            Some("sha256:older".to_string()),
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
                .uri(format!("/api/jobs/{job_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job = response_json(resp).await;
    assert_eq!(
        job["job"]["summary"]["servicesDrifted"]
            .as_u64()
            .unwrap_or_default(),
        1,
        "runtime scan summary: {job}"
    );
    assert_eq!(
        job["job"]["summary"]["servicesUpdated"]
            .as_u64()
            .unwrap_or_default(),
        1
    );

    let in_flight = state
        .snapshot_worker
        .in_flight_reason("ghcr.io/acme/web", "sha256:new", "linux/amd64")
        .await;
    assert!(
        in_flight.is_none(),
        "strict semver runtime-scan candidate changes should not enqueue version inference"
    );
}
