#[async_trait::async_trait]
impl CommandRunner for ScriptedRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;
        let (status, stdout) = if args.ends_with(&["version".to_string()]) {
            (0, "Docker Compose version v2.40.0\n".to_string())
        } else if args.ends_with(&["config".to_string(), "--services".to_string()]) {
            (0, "api\nweb\nworker\nactive\narchived\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, "cid1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.starts_with("{{.Image}}"))
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
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.starts_with("{{.Image}}"))
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
    runtime_started_ats: Vec<String>,
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl CheckAndRuntimeScanRunner {
    fn new(runtime_digest: &str) -> Self {
        Self::new_with_started_ats(runtime_digest, Vec::new())
    }

    fn new_with_started_at(runtime_digest: &str, runtime_started_at: Option<String>) -> Self {
        Self::new_with_started_ats(
            runtime_digest,
            runtime_started_at.into_iter().collect::<Vec<_>>(),
        )
    }

    fn new_with_started_ats(runtime_digest: &str, runtime_started_ats: Vec<String>) -> Self {
        Self {
            runtime_digest: runtime_digest.to_string(),
            runtime_started_ats,
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn runtime_container_ids(&self) -> Vec<String> {
        if self.runtime_started_ats.len() > 1 {
            (1..=self.runtime_started_ats.len())
                .map(|index| format!("cid{index}"))
                .collect()
        } else {
            vec!["cid1".to_string()]
        }
    }

    fn runtime_started_at_for_container(&self, container_id: &str) -> Option<&str> {
        if self.runtime_started_ats.is_empty() {
            return None;
        }
        if self.runtime_started_ats.len() == 1 {
            return self.runtime_started_ats.first().map(String::as_str);
        }
        container_id
            .strip_prefix("cid")
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .and_then(|index| self.runtime_started_ats.get(index.saturating_sub(1)))
            .map(String::as_str)
    }
}

#[derive(Clone)]
struct SharedMovingTagRunner {
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl SharedMovingTagRunner {
    fn new() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for SharedMovingTagRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;

        let (status, stdout) = if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            let filter = args.join(" ");
            if filter.contains("com.docker.compose.project=trtff") {
                (0, "cid_trtff\n".to_string())
            } else if filter.contains("com.docker.compose.project=ctp-recorder") {
                (0, "cid_ctp\n".to_string())
            } else {
                (0, String::new())
            }
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("com.docker.compose.service"))
        {
            let stdout = if args.iter().any(|arg| arg == "cid_trtff") {
                "trtff-api\tsha256:old-runtime\t2026-06-04T13:08:09Z\n"
            } else if args.iter().any(|arg| arg == "cid_ctp") {
                "ctp-recorder\tsha256:new-runtime\t2026-06-09T04:08:58Z\n"
            } else {
                ""
            };
            (0, stdout.to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.starts_with("{{.Image}}"))
        {
            let stdout = match args.get(3).map(String::as_str) {
                Some("cid_trtff") => "sha256:old-runtime\t2026-06-04T13:08:09Z\n",
                Some("cid_ctp") => "sha256:new-runtime\t2026-06-09T04:08:58Z\n",
                _ => "",
            };
            (0, stdout.to_string())
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.iter().any(|s| s.contains("RepoDigests"))
        {
            if args.iter().any(|s| s.contains("{{.Id}}")) {
                let mut lines = Vec::new();
                if args.iter().any(|arg| arg == "sha256:old-runtime") {
                    lines.push("sha256:old-runtime\t[]".to_string());
                }
                if args.iter().any(|arg| arg == "sha256:new-runtime") {
                    lines.push(
                        "sha256:new-runtime\t[\"ghcr.io/sequenxe/trtff@sha256:new-runtime\"]"
                            .to_string(),
                    );
                }
                (0, format!("{}\n", lines.join("\n")))
            } else if args.iter().any(|arg| arg == "sha256:old-runtime") {
                (0, "[]".to_string())
            } else if args.iter().any(|arg| arg == "sha256:new-runtime") {
                (
                    0,
                    "[\"ghcr.io/sequenxe/trtff@sha256:new-runtime\"]".to_string(),
                )
            } else {
                (0, "[]".to_string())
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

#[async_trait::async_trait]
impl CommandRunner for CheckAndRuntimeScanRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;

        let (status, stdout) = if args.ends_with(&["version".to_string()]) {
            (0, "Docker Compose version v2.40.0\n".to_string())
        } else if args.ends_with(&["config".to_string(), "--services".to_string()]) {
            (0, "web\napi\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, format!("{}\n", self.runtime_container_ids().join("\n")))
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.starts_with("{{.Image}}"))
        {
            let started_at = args
                .get(3)
                .and_then(|container_id| self.runtime_started_at_for_container(container_id));
            let stdout = started_at.map_or_else(
                || "img1\n".to_string(),
                |started_at| format!("img1\t{started_at}\n"),
            );
            (0, stdout)
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("com.docker.compose.service"))
        {
            let stdout = if self.runtime_started_ats.is_empty() {
                "web\timg1\n".to_string()
            } else {
                self.runtime_started_ats
                    .iter()
                    .map(|started_at| format!("web\timg1\t{started_at}"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            };
            (0, stdout)
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
    updated_started_ats: Vec<String>,
}

impl UpdateAndRuntimeScanRunner {
    fn new() -> Self {
        Self {
            updated: Arc::new(std::sync::Mutex::new(false)),
            updated_started_ats: Vec::new(),
        }
    }

    fn new_with_started_ats(updated_started_ats: Vec<String>) -> Self {
        Self {
            updated: Arc::new(std::sync::Mutex::new(false)),
            updated_started_ats,
        }
    }

    fn updated_container_ids(&self) -> Vec<String> {
        if self.updated_started_ats.len() > 1 {
            (1..=self.updated_started_ats.len())
                .map(|index| format!("container_new_{index}"))
                .collect()
        } else {
            vec!["container_new".to_string()]
        }
    }

    fn updated_started_at_for_container(&self, container_id: &str) -> Option<&str> {
        if self.updated_started_ats.is_empty() {
            return None;
        }
        if self.updated_started_ats.len() == 1 {
            return self.updated_started_ats.first().map(String::as_str);
        }
        container_id
            .strip_prefix("container_new_")
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .and_then(|index| self.updated_started_ats.get(index.saturating_sub(1)))
            .map(String::as_str)
    }
}

#[async_trait::async_trait]
impl CommandRunner for UpdateAndRuntimeScanRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let args = spec.args;
        if args.ends_with(&["version".to_string()]) {
            return Ok(CommandOutput {
                status: 0,
                stdout: "Docker Compose version v2.40.0\n".to_string(),
                stderr: String::new(),
            });
        }
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
                    format!("{}\n", self.updated_container_ids().join("\n"))
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
            && args
                .get(2)
                .map(|s| s.as_str())
                .is_some_and(|s| s.starts_with("{{.Image}}"))
        {
            let started_at = args
                .get(3)
                .and_then(|container_id| self.updated_started_at_for_container(container_id));
            match args.get(3).map(|s| s.as_str()) {
                Some("container_old") => (
                    0,
                    "img_old
"
                    .to_string(),
                ),
                Some(container_id) if container_id.starts_with("container_new") => {
                    let stdout = started_at.map_or_else(
                        || "img_new\n".to_string(),
                        |started_at| format!("img_new\t{started_at}\n"),
                    );
                    (0, stdout)
                }
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
            let stdout = if updated_now && !self.updated_started_ats.is_empty() {
                self.updated_started_ats
                    .iter()
                    .map(|started_at| format!("web\t{image}\t{started_at}"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            } else {
                format!("web\t{image}\n")
            };
            (0, stdout)
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

#[derive(Default)]
struct HealthRollbackUpdateRunner {
    step: std::sync::Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for HealthRollbackUpdateRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        if spec.args.ends_with(&["version".to_string()]) {
            return Ok(CommandOutput {
                status: 0,
                stdout: "Docker Compose version v2.40.0\n".to_string(),
                stderr: String::new(),
            });
        }
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 if spec
                .args
                .ends_with(&["ps".to_string(), "-q".to_string(), "web".to_string()]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "container_old\n".to_string(),
                    stderr: String::new(),
                }
            }
            1 if spec.args == vec!["inspect", "--format", "{{.Image}}", "container_old"] => {
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            2 if spec
                .args
                .ends_with(&["pull".to_string(), "web".to_string()]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            3 if spec
                .args
                .ends_with(&["up".to_string(), "-d".to_string(), "web".to_string()]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            4 if spec
                .args
                .ends_with(&["ps".to_string(), "-q".to_string(), "web".to_string()]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "container_new\n".to_string(),
                    stderr: String::new(),
                }
            }
            5 if spec.args
                == vec![
                    "inspect",
                    "--format",
                    "{{if .State.Health}}1{{else}}0{{end}}",
                    "container_new",
                ] =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "1\n".to_string(),
                    stderr: String::new(),
                }
            }
            6 if spec.args == vec!["inspect", "--format", "{{.Image}}", "container_new"] => {
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            }
            7 if spec.args
                == vec![
                    "inspect",
                    "--format",
                    "{{.State.Health.Status}}",
                    "container_new",
                ] =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "unhealthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            8 if spec.args == vec!["image", "tag", "sha256:old", "ghcr.io/acme/web:5.2"] => {
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            9 if spec.args.ends_with(&[
                "up".to_string(),
                "-d".to_string(),
                "--pull".to_string(),
                "never".to_string(),
                "web".to_string(),
            ]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 if spec
                .args
                .ends_with(&["ps".to_string(), "-q".to_string(), "web".to_string()]) =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "container_rollback\n".to_string(),
                    stderr: String::new(),
                }
            }
            11 if spec.args
                == vec![
                    "inspect",
                    "--format",
                    "{{.State.Health.Status}}",
                    "container_rollback",
                ] =>
            {
                CommandOutput {
                    status: 0,
                    stdout: "healthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            12 if spec.args == vec!["inspect", "--format", "{{.Image}}", "container_rollback"] => {
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "unexpected command at step {}: program={} args={:?}",
                *step, spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
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

#[derive(Clone, Default)]
struct SharedMovingTagRegistry;

#[async_trait::async_trait]
impl RegistryClient for SharedMovingTagRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["latest".to_string(), "0.28.0".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "latest" | "0.28.0" => "sha256:new-runtime",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
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
struct RepoLinkRegistry {
    oci_source: Option<String>,
    observed_references: Arc<std::sync::Mutex<Vec<String>>>,
}

impl RepoLinkRegistry {
    fn with_oci_source(source: Option<&str>) -> Self {
        Self {
            oci_source: source.map(ToString::to_string),
            observed_references: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn observed_references(&self) -> Vec<String> {
        self.observed_references.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl RegistryClient for RepoLinkRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["latest".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        Ok(ManifestInfo {
            digest: Some("sha256:latest".to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }

    async fn get_oci_source(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<Option<String>> {
        self.observed_references
            .lock()
            .unwrap()
            .push(reference.to_string());
        Ok(self.oci_source.clone())
    }
}

#[derive(Clone, Default)]
struct MixedRepoLinkRegistry {
    observed_references: Arc<std::sync::Mutex<Vec<String>>>,
}

impl MixedRepoLinkRegistry {
    fn observed_references(&self) -> Vec<String> {
        self.observed_references.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl RegistryClient for MixedRepoLinkRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["latest".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        _reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        Ok(ManifestInfo {
            digest: Some("sha256:latest".to_string()),
            platform_digest: None,
            arch: vec!["linux/amd64".to_string()],
        })
    }

    async fn get_oci_source(
        &self,
        image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<Option<String>> {
        self.observed_references
            .lock()
            .unwrap()
            .push(format!("{}/{}@{reference}", image.registry, image.name));
        if image.name.contains("/error") {
            anyhow::bail!("simulated oci source failure");
        }
        Ok(None)
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
struct BranchAliasRegistry {
    alias: String,
    list_tags_delay: Duration,
}

impl BranchAliasRegistry {
    fn new(alias: &str, list_tags_delay: Duration) -> Self {
        Self {
            alias: alias.to_string(),
            list_tags_delay,
        }
    }
}

#[async_trait::async_trait]
impl RegistryClient for BranchAliasRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        tokio::time::sleep(self.list_tags_delay).await;
        Ok(vec![
            self.alias.clone(),
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
            "5.2.0" => "sha256:old",
            "5.3.0" => "sha256:new",
            tag if tag == self.alias => "sha256:new",
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
