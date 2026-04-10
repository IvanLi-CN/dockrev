async fn response_json(resp: axum::response::Response) -> serde_json::Value {
    let payload = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&payload).unwrap()
}

fn format_test_rfc3339(ts: time::OffsetDateTime) -> String {
    ts.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

fn test_now_rfc3339() -> String {
    format_test_rfc3339(time::OffsetDateTime::now_utc())
}

fn test_offset_rfc3339(base: &str, delta: time::Duration) -> String {
    let parsed =
        time::OffsetDateTime::parse(base, &time::format_description::well_known::Rfc3339).unwrap();
    format_test_rfc3339(parsed + delta)
}

fn test_offset_from_now_rfc3339(delta: time::Duration) -> String {
    format_test_rfc3339(time::OffsetDateTime::now_utc() + delta)
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

#[derive(Clone)]
struct LatestTagUpdateRegistry;

#[async_trait::async_trait]
impl RegistryClient for LatestTagUpdateRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "latest".to_string(),
            "5.2".to_string(),
            "5.3".to_string(),
        ])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" => "sha256:new",
            "5.2" => "sha256:old",
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
struct AliasDriftRegistry {
    list_tags_delay: Duration,
}

impl AliasDriftRegistry {
    fn new(list_tags_delay: Duration) -> Self {
        Self { list_tags_delay }
    }
}

#[async_trait::async_trait]
impl RegistryClient for AliasDriftRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        tokio::time::sleep(self.list_tags_delay).await;
        Ok(vec![
            "5.2".to_string(),
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
            "5.2" | "5.3.0" => "sha256:new",
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
                assert_eq!(args.first().copied(), Some("inspect"));
                assert_eq!(args.get(1).copied(), Some("--format"));
                assert!(
                    args.get(2)
                        .copied()
                        .is_some_and(|value| value.starts_with("{{.Image}}"))
                );
                assert_eq!(args.get(3).copied(), Some("container_old"));
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
                assert_eq!(args.first().copied(), Some("inspect"));
                assert_eq!(args.get(1).copied(), Some("--format"));
                assert!(
                    args.get(2)
                        .copied()
                        .is_some_and(|value| value.starts_with("{{.Image}}"))
                );
                assert_eq!(args.get(3).copied(), Some("container_new"));
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            }
            7 => {
                assert_eq!(args, vec!["pull", "ghcr.io/acme/web:latest"]);
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            8 => {
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
            9..=11 => {
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

#[derive(Clone, Default)]
struct LatestOnlyRegistry;

#[async_trait::async_trait]
impl RegistryClient for LatestOnlyRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["latest".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" => "sha256:new",
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
struct ExplicitVersionFallbackRegistry {
    candidate_version: String,
}

impl ExplicitVersionFallbackRegistry {
    fn new(candidate_version: &str) -> Self {
        Self {
            candidate_version: candidate_version.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl RegistryClient for ExplicitVersionFallbackRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["latest".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" => "sha256:new",
            "0.29.12" => "sha256:old",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }

    async fn get_oci_version(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok((reference == "sha256:new").then(|| self.candidate_version.clone()))
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

