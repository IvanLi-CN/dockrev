use std::{collections::HashMap, path::Path, time::Duration};

use dockrev_common::normalized_semver_from_oci_version;
use serde::Deserialize;

use crate::{config::Config, state_store::now_rfc3339};

#[derive(Clone, Debug)]
pub struct TargetRuntime {
    pub container_ip: String,
    pub dockrev_http_port: u16,
    pub compose_project: String,
    pub compose_service: String,
    pub compose_files: Vec<String>,
    pub current_image_ref: String,
    pub current_image_id: String,
}

pub async fn resolve_target(cfg: &Config) -> anyhow::Result<TargetRuntime> {
    let container_id = if let Some(id) = cfg.target_container_id.as_deref() {
        id.to_string()
    } else {
        auto_match_container(cfg).await?
    };

    let inspect = docker_inspect(cfg, &container_id).await?;
    let labels = inspect.config.labels.unwrap_or_default();

    let compose_project = labels
        .get("com.docker.compose.project")
        .cloned()
        .or(cfg.target_compose_project.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "compose project not found; set DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT"
            )
        })?;

    let compose_service = cfg
        .target_compose_service
        .clone()
        .or_else(|| labels.get("com.docker.compose.service").cloned())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "compose service not found; set DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE"
            )
        })?;

    let mut compose_files: Vec<String> = Vec::new();
    let mut label_compose_files_err: Option<anyhow::Error> = None;

    if let Some(raw) = labels.get("com.docker.compose.project.config_files") {
        let label_files = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if !label_files.is_empty() {
            match ensure_all_readable(&label_files).await {
                Ok(()) => compose_files = label_files,
                Err(e) => label_compose_files_err = Some(e),
            }
        }
    }

    if compose_files.is_empty() {
        if !cfg.target_compose_files.is_empty() {
            ensure_all_readable(&cfg.target_compose_files).await?;
            compose_files = cfg.target_compose_files.clone();
        } else if let Some(e) = label_compose_files_err {
            return Err(e.context(
                "compose label config_files paths are not readable; mount them into supervisor or set DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES",
            ));
        } else {
            return Err(anyhow::anyhow!(
                "compose files not found; set DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES"
            ));
        }
    }

    let container_ip = pick_container_ip(&inspect.network_settings.networks, &compose_project)
        .ok_or_else(|| anyhow::anyhow!("container IP not found in docker inspect output"))?;

    let dockrev_http_port = inspect
        .config
        .env
        .as_deref()
        .and_then(parse_dockrev_http_port_from_env)
        .unwrap_or(50883);

    let current_image_ref = inspect
        .config
        .image
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| cfg.target_image_repo.clone());

    Ok(TargetRuntime {
        container_ip,
        dockrev_http_port,
        compose_project,
        compose_service,
        compose_files,
        current_image_ref,
        current_image_id: inspect.image,
    })
}

async fn ensure_readable(path: &Path) -> anyhow::Result<()> {
    tokio::fs::metadata(path).await.map_err(|e| {
        anyhow::anyhow!(
            "compose file not readable: {} ({e}); ensure it is mounted into supervisor at the same absolute path",
            path.display()
        )
    })?;
    Ok(())
}

async fn ensure_all_readable(paths: &[String]) -> anyhow::Result<()> {
    for p in paths {
        ensure_readable(Path::new(p)).await?;
    }
    Ok(())
}

fn non_empty(v: &str) -> Option<String> {
    if v.trim().is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn non_empty_opt(v: Option<&str>) -> Option<String> {
    v.and_then(non_empty)
}

fn pick_container_ip(
    networks: &HashMap<String, DockerNetwork>,
    compose_project: &str,
) -> Option<String> {
    let preferred = format!("{compose_project}_default");
    if let Some(ip) = networks
        .get(&preferred)
        .and_then(|n| non_empty_opt(n.ip_address.as_deref()))
    {
        return Some(ip);
    }

    let mut entries: Vec<(&String, &DockerNetwork)> = networks.iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    entries
        .into_iter()
        .find_map(|(_, n)| non_empty_opt(n.ip_address.as_deref()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerPsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Image")]
    image: String,
}

async fn auto_match_container(cfg: &Config) -> anyhow::Result<String> {
    // 1) labels-first when explicit compose selectors are configured. This works even if the image is
    // dangling (docker ps shows short image id), because labels live on the container.
    if let Some(id) = auto_match_container_by_compose_labels(cfg).await? {
        return Ok(id);
    }

    // 2) legacy behavior: match by `docker ps` Image string (repo/repo:tag/repo@digest).
    if let Some(id) = auto_match_container_by_ps_image_repo(cfg).await? {
        return Ok(id);
    }

    // 3) fallback for dangling images: inspect each container and match against Config.Image.
    if let Some(id) = auto_match_container_by_inspect_config_image(cfg).await? {
        return Ok(id);
    }

    Err(anyhow::anyhow!(
        "no running container matched image repo {}; set DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID",
        cfg.target_image_repo
    ))
}

async fn docker_ps(cfg: &Config, filters: &[String]) -> anyhow::Result<Vec<DockerPsLine>> {
    let mut args: Vec<String> = Vec::new();
    args.push("ps".to_string());
    args.push("--format".to_string());
    args.push("{{json .}}".to_string());
    for f in filters {
        args.push("--filter".to_string());
        args.push(f.clone());
    }

    let out = run_cmd_lines(cfg, &cfg.docker_bin, &args, Duration::from_secs(10)).await?;
    let mut lines: Vec<DockerPsLine> = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        lines.push(serde_json::from_str::<DockerPsLine>(line)?);
    }
    Ok(lines)
}

async fn resolve_unique_candidate(
    cfg: &Config,
    matched_ids: Vec<String>,
    error_context: &str,
) -> anyhow::Result<Option<String>> {
    if matched_ids.is_empty() {
        return Ok(None);
    }
    if matched_ids.len() == 1 {
        return Ok(Some(matched_ids.into_iter().next().unwrap()));
    }

    let desired = cfg.target_compose_service.as_deref().unwrap_or("dockrev");
    let mut candidates: Vec<ComposeCandidate> = Vec::new();
    for id in matched_ids {
        let inspect = docker_inspect(cfg, &id).await?;
        let labels = inspect.config.labels.unwrap_or_default();
        let compose_service = labels.get("com.docker.compose.service").cloned();
        let compose_project = labels.get("com.docker.compose.project").cloned();
        candidates.push(ComposeCandidate {
            id,
            compose_service,
            compose_project,
        });
    }

    if let Some(id) = pick_compose_candidate(cfg, desired, &candidates) {
        return Ok(Some(id));
    }

    Err(anyhow::anyhow!(
        "multiple running containers matched {}; set DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID or DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE",
        error_context
    ))
}

async fn auto_match_container_by_compose_labels(cfg: &Config) -> anyhow::Result<Option<String>> {
    let mut filters: Vec<String> = Vec::new();
    if let Some(project) = cfg.target_compose_project.as_deref() {
        filters.push(format!("label=com.docker.compose.project={project}"));
    }
    if let Some(service) = cfg.target_compose_service.as_deref() {
        filters.push(format!("label=com.docker.compose.service={service}"));
    }
    if filters.is_empty() {
        return Ok(None);
    }

    let lines = docker_ps(cfg, &filters).await?;

    // Validate repo using inspect.Config.Image (handles dangling cases).
    let mut matches: Vec<String> = Vec::new();
    for line in lines {
        let inspect = docker_inspect(cfg, &line.id).await?;
        if inspect
            .config
            .image
            .as_deref()
            .is_some_and(|img| image_ref_matches_repo(img, &cfg.target_image_repo))
        {
            matches.push(line.id);
        }
    }

    resolve_unique_candidate(
        cfg,
        matches,
        "compose labels (com.docker.compose.project/service)",
    )
    .await
}

async fn auto_match_container_by_ps_image_repo(cfg: &Config) -> anyhow::Result<Option<String>> {
    let lines = docker_ps(cfg, &[]).await?;
    let mut matches: Vec<String> = Vec::new();
    for line in lines {
        if image_ref_matches_repo(&line.image, &cfg.target_image_repo) {
            matches.push(line.id);
        }
    }
    resolve_unique_candidate(
        cfg,
        matches,
        &format!("image repo {}", cfg.target_image_repo),
    )
    .await
}

async fn auto_match_container_by_inspect_config_image(
    cfg: &Config,
) -> anyhow::Result<Option<String>> {
    let lines = docker_ps(cfg, &[]).await?;
    let mut matches: Vec<String> = Vec::new();
    for line in lines {
        let inspect = docker_inspect(cfg, &line.id).await?;
        if inspect
            .config
            .image
            .as_deref()
            .is_some_and(|img| image_ref_matches_repo(img, &cfg.target_image_repo))
        {
            matches.push(line.id);
        }
    }
    resolve_unique_candidate(
        cfg,
        matches,
        &format!(
            "image repo {} (inspect Config.Image)",
            cfg.target_image_repo
        ),
    )
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerInspect {
    image: String,
    config: DockerInspectConfig,
    #[serde(rename = "NetworkSettings")]
    network_settings: DockerNetworkSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerInspectConfig {
    #[serde(default)]
    labels: Option<HashMap<String, String>>,
    #[serde(default)]
    env: Option<Vec<String>>,
    #[serde(default)]
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerNetworkSettings {
    #[serde(default)]
    networks: HashMap<String, DockerNetwork>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerNetwork {
    #[serde(default, rename = "IPAddress", alias = "IpAddress")]
    ip_address: Option<String>,
}

#[cfg(test)]
mod docker_inspect_tests {
    use super::*;

    #[test]
    fn docker_network_parses_ip_address() {
        let json = r#"
        {
          "Image": "sha256:deadbeef",
          "Config": { "Labels": {}, "Env": [], "Image": "dockrev:latest" },
          "NetworkSettings": {
            "Networks": {
              "dockrev_default": { "IPAddress": "172.18.0.2" }
            }
          }
        }
        "#;
        let parsed: DockerInspect = serde_json::from_str(json).unwrap();
        let ip = pick_container_ip(&parsed.network_settings.networks, "dockrev").unwrap();
        assert_eq!(ip, "172.18.0.2");
    }

    #[test]
    fn pick_container_ip_prefers_compose_default_network() {
        let json = r#"
        {
          "Image": "sha256:deadbeef",
          "Config": { "Labels": {}, "Env": [], "Image": "dockrev:latest" },
          "NetworkSettings": {
            "Networks": {
              "traefik": { "IPAddress": "10.0.0.2" },
              "dockrev_default": { "IPAddress": "172.18.0.2" }
            }
          }
        }
        "#;
        let parsed: DockerInspect = serde_json::from_str(json).unwrap();
        let ip = pick_container_ip(&parsed.network_settings.networks, "dockrev").unwrap();
        assert_eq!(ip, "172.18.0.2");
    }

    #[test]
    fn docker_network_accepts_legacy_ipaddress_key() {
        let json = r#"
        {
          "Image": "sha256:deadbeef",
          "Config": { "Labels": {}, "Env": [], "Image": "dockrev:latest" },
          "NetworkSettings": {
            "Networks": {
              "dockrev_default": { "IpAddress": "172.18.0.3" }
            }
          }
        }
        "#;
        let parsed: DockerInspect = serde_json::from_str(json).unwrap();
        let ip = pick_container_ip(&parsed.network_settings.networks, "dockrev").unwrap();
        assert_eq!(ip, "172.18.0.3");
    }
}

async fn docker_inspect(cfg: &Config, container_id: &str) -> anyhow::Result<DockerInspect> {
    let out = run_docker_lines(
        cfg,
        &["inspect", container_id, "--format", "{{json .}}"],
        Duration::from_secs(10),
    )
    .await?;
    Ok(serde_json::from_str::<DockerInspect>(out.trim())?)
}

pub async fn docker_pull(cfg: &Config, image_ref: &str, timeout: Duration) -> anyhow::Result<()> {
    let _ts = now_rfc3339()?;
    let _ = run_docker_lines(cfg, &["pull", image_ref], timeout).await?;
    Ok(())
}

pub async fn docker_tag(
    cfg: &Config,
    image_id: &str,
    image_ref: &str,
    timeout: Duration,
) -> anyhow::Result<()> {
    let _ts = now_rfc3339()?;
    let _ = run_docker_lines(cfg, &["image", "tag", image_id, image_ref], timeout).await?;
    Ok(())
}

pub async fn compose_up(
    cfg: &Config,
    target: &TargetRuntime,
    override_path: &Path,
    timeout: Duration,
) -> anyhow::Result<()> {
    let mut args: Vec<String> = Vec::new();

    let compose_bin = cfg.compose_bin.trim();
    if is_docker_cli(compose_bin) {
        args.push("compose".to_string());
    }
    args.push("-p".to_string());
    args.push(target.compose_project.clone());
    for f in &target.compose_files {
        args.push("-f".to_string());
        args.push(f.clone());
    }
    args.push("-f".to_string());
    args.push(override_path.display().to_string());
    args.push("up".to_string());
    args.push("-d".to_string());
    args.push("--no-deps".to_string());
    args.push("--pull".to_string());
    args.push("always".to_string());
    args.push(target.compose_service.clone());

    let _ = run_cmd_lines(cfg, compose_bin, &args, timeout).await?;
    Ok(())
}

#[derive(Clone, Debug)]
struct ComposeCandidate {
    id: String,
    compose_service: Option<String>,
    compose_project: Option<String>,
}

fn pick_compose_candidate(
    cfg: &Config,
    desired_service: &str,
    candidates: &[ComposeCandidate],
) -> Option<String> {
    let by_service: Vec<&ComposeCandidate> = candidates
        .iter()
        .filter(|c| c.compose_service.as_deref() == Some(desired_service))
        .collect();
    if by_service.len() == 1 {
        return Some(by_service[0].id.clone());
    }

    // If the caller didn't explicitly choose a service, try best-effort to exclude "supervisor".
    if cfg.target_compose_service.is_none() {
        let non_supervisor: Vec<&ComposeCandidate> = candidates
            .iter()
            .filter(|c| c.compose_service.as_deref() != Some("supervisor"))
            .collect();
        if non_supervisor.len() == 1 {
            return Some(non_supervisor[0].id.clone());
        }
    }

    // If project is explicitly configured, prefer candidates within that project.
    if let Some(project) = cfg.target_compose_project.as_deref() {
        let by_project: Vec<&ComposeCandidate> = candidates
            .iter()
            .filter(|c| c.compose_project.as_deref() == Some(project))
            .collect();
        if by_project.len() == 1 {
            return Some(by_project[0].id.clone());
        }
    }

    None
}

fn parse_dockrev_http_port_from_env(env: &[String]) -> Option<u16> {
    let mut http_addr = None;
    for e in env {
        if let Some(v) = e.strip_prefix("DOCKREV_HTTP_ADDR=") {
            http_addr = Some(v);
            break;
        }
    }
    let http_addr = http_addr?;
    parse_port_from_http_addr(http_addr)
}

fn image_ref_matches_repo(image_ref: &str, repo: &str) -> bool {
    image_ref == repo
        || image_ref.starts_with(&format!("{repo}:"))
        || image_ref.starts_with(&format!("{repo}@"))
}

fn parse_port_from_http_addr(addr: &str) -> Option<u16> {
    let t = addr.trim();
    if t.is_empty() {
        return None;
    }

    // common forms: "0.0.0.0:50883", ":50883", "[::]:50883"
    let last_colon = t.rfind(':')?;
    let port_str = &t[(last_colon + 1)..];
    port_str.trim().parse::<u16>().ok()
}

fn is_docker_cli(program: &str) -> bool {
    let t = program.trim();
    if t.is_empty() {
        return false;
    }
    let name = t.rsplit(['/', '\\']).next().unwrap_or(t);
    name == "docker" || name == "docker.exe"
}

pub async fn docker_image_repo_digest(
    cfg: &Config,
    image_id: &str,
    repo: &str,
) -> anyhow::Result<Option<String>> {
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct ImgInspect {
        #[serde(default)]
        repo_digests: Option<Vec<String>>,
    }
    let out = run_docker_lines(
        cfg,
        &["image", "inspect", image_id, "--format", "{{json .}}"],
        Duration::from_secs(10),
    )
    .await?;
    let parsed: ImgInspect = serde_json::from_str(out.trim())?;
    let digs = parsed.repo_digests.unwrap_or_default();
    for d in digs {
        if let Some(rest) = d.strip_prefix(&format!("{repo}@")) {
            return Ok(Some(rest.to_string()));
        }
    }
    Ok(None)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerImageInspect {
    #[serde(default)]
    repo_tags: Option<Vec<String>>,
    #[serde(default)]
    config: Option<DockerImageInspectConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerImageInspectConfig {
    #[serde(default)]
    labels: Option<HashMap<String, String>>,
}

async fn docker_image_inspect(
    cfg: &Config,
    image_ref_or_id: &str,
) -> anyhow::Result<DockerImageInspect> {
    let out = run_docker_lines(
        cfg,
        &[
            "image",
            "inspect",
            image_ref_or_id,
            "--format",
            "{{json .}}",
        ],
        Duration::from_secs(10),
    )
    .await?;
    Ok(serde_json::from_str::<DockerImageInspect>(out.trim())?)
}

pub async fn docker_image_semver_tag_ref_to_pull(
    cfg: &Config,
    image_ref_or_id: &str,
    repo: &str,
) -> anyhow::Result<Option<String>> {
    let inspected = docker_image_inspect(cfg, image_ref_or_id).await?;
    let labels = inspected.config.and_then(|c| c.labels).unwrap_or_default();
    let Some(raw_version) = labels.get("org.opencontainers.image.version") else {
        return Ok(None);
    };
    let Some(tag) = normalized_semver_from_oci_version(raw_version) else {
        return Ok(None);
    };
    let desired = format!("{repo}:{tag}");
    let repo_tags = inspected.repo_tags.unwrap_or_default();
    if repo_tags.iter().any(|t| t == &desired) {
        return Ok(None);
    }
    Ok(Some(desired))
}

async fn run_docker_lines(
    cfg: &Config,
    args: &[&str],
    timeout: Duration,
) -> anyhow::Result<String> {
    let mut a = Vec::with_capacity(args.len());
    for s in args {
        a.push((*s).to_string());
    }
    run_cmd_lines(cfg, &cfg.docker_bin, &a, timeout).await
}

async fn run_cmd_lines(
    cfg: &Config,
    program: &str,
    args: &[String],
    timeout: Duration,
) -> anyhow::Result<String> {
    use tokio::process::Command;

    let mut cmd = Command::new(program);
    cmd.kill_on_drop(true);
    cmd.args(args);
    if let Some(h) = cfg.docker_host.as_deref() {
        cmd.env("DOCKER_HOST", h);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = cmd.spawn()?;
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(v) => v?,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "command timed out: {} {:?} timeout={:?}",
                program,
                args,
                timeout
            ));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "command failed: {} {:?} stderr={}",
            program,
            args,
            stderr
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parse_port_from_http_addr_parses_common_forms() {
        assert_eq!(parse_port_from_http_addr("0.0.0.0:50883"), Some(50883));
        assert_eq!(parse_port_from_http_addr(":50883"), Some(50883));
        assert_eq!(parse_port_from_http_addr("[::]:50883"), Some(50883));
    }

    #[test]
    fn parse_dockrev_http_port_from_env_extracts_var() {
        let env = vec![
            "FOO=1".to_string(),
            "DOCKREV_HTTP_ADDR=0.0.0.0:1234".to_string(),
        ];
        assert_eq!(parse_dockrev_http_port_from_env(&env), Some(1234));
    }

    #[test]
    fn is_docker_cli_accepts_absolute_paths() {
        assert!(is_docker_cli("docker"));
        assert!(is_docker_cli("/usr/bin/docker"));
        assert!(is_docker_cli("C:\\Program Files\\Docker\\docker.exe"));
        assert!(!is_docker_cli("docker-compose"));
    }

    #[test]
    fn image_ref_matches_repo_handles_tag_and_digest() {
        assert!(image_ref_matches_repo(
            "ghcr.io/ivanli-cn/dockrev:latest",
            "ghcr.io/ivanli-cn/dockrev"
        ));
        assert!(image_ref_matches_repo(
            "ghcr.io/ivanli-cn/dockrev@sha256:abc",
            "ghcr.io/ivanli-cn/dockrev"
        ));
        assert!(!image_ref_matches_repo(
            "ghcr.io/ivanli-cn/dockrev-supervisor:latest",
            "ghcr.io/ivanli-cn/dockrev"
        ));
    }

    #[test]
    fn semver_tag_from_oci_version_parses_v_prefix_and_rejects_build() {
        assert_eq!(
            normalized_semver_from_oci_version("v0.7.7"),
            Some("0.7.7".to_string())
        );
        assert_eq!(
            normalized_semver_from_oci_version("0.7.7+build.1"),
            None,
            "docker tags cannot include '+' build metadata"
        );
    }

    #[cfg(unix)]
    struct TempDir {
        path: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl TempDir {
        fn new(prefix: &str) -> anyhow::Result<Self> {
            static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{seq}", std::process::id()));
            std::fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    #[cfg(unix)]
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(unix)]
    fn install_fake_docker(script_body: &str) -> anyhow::Result<(TempDir, String)> {
        let dir = TempDir::new("dockrev-supervisor-fake-docker")?;
        let docker = dir.path().join("docker");
        let docker_tmp = dir.path().join("docker.tmp");

        {
            use std::io::Write as _;
            let mut file = std::fs::File::create(&docker_tmp)?;
            file.write_all(script_body.as_bytes())?;
            file.sync_all()?;
        }

        let mut perms = std::fs::metadata(&docker_tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&docker_tmp, perms)?;
        std::fs::rename(&docker_tmp, &docker)?;

        // GH Actions can intermittently report ETXTBSY when executing a just-written script.
        // Probe the command once and retry briefly on that specific transient.
        const ETXTBSY: i32 = 26;
        let mut last_err: Option<std::io::Error> = None;
        for _ in 0..10 {
            match std::process::Command::new(&docker)
                .arg("dockrev-fake-probe")
                .output()
            {
                Ok(_) => {
                    last_err = None;
                    break;
                }
                Err(err) if err.raw_os_error() == Some(ETXTBSY) => {
                    last_err = Some(err);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(err) => return Err(err.into()),
            }
        }
        if let Some(err) = last_err {
            return Err(err.into());
        }

        Ok((dir, docker.to_string_lossy().to_string()))
    }

    #[cfg(unix)]
    fn test_cfg() -> Config {
        Config {
            http_addr: "127.0.0.1:0".to_string(),
            base_path: "/supervisor".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: None,
            auth_allowed_group: None,
            auth_allow_anonymous_in_dev: true,
            target_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            target_container_id: None,
            target_compose_project: None,
            target_compose_service: None,
            target_compose_files: Vec::new(),
            docker_bin: "docker".to_string(),
            docker_host: None,
            compose_bin: "docker-compose".to_string(),
            state_path: std::path::PathBuf::from("/tmp/dockrev-supervisor-test.json"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn auto_match_container_handles_dangling_images_via_inspect_config_image() {
        let script = r#"#!/usr/bin/env bash
set -euo pipefail
cmd="${1:-}"
shift || true

if [[ "$cmd" == "ps" ]]; then
  echo '{"ID":"ctr_dangling","Image":"c85819d0c6dd"}'
  exit 0
fi

if [[ "$cmd" == "inspect" ]]; then
  # docker inspect <id> --format {{json .}}
  cat <<'JSON'
{"Image":"sha256:deadbeef","Config":{"Labels":{"com.docker.compose.service":"dockrev","com.docker.compose.project":"dockrev"},"Env":[],"Image":"ghcr.io/ivanli-cn/dockrev:latest"},"NetworkSettings":{"Networks":{}}}
JSON
  exit 0
fi

echo "unexpected args: $cmd $*" >&2
exit 1
"#;

        let (_dir, docker_bin) = install_fake_docker(script).unwrap();

        let mut cfg = test_cfg();
        cfg.docker_bin = docker_bin;
        let got = auto_match_container(&cfg).await.unwrap();
        assert_eq!(got, "ctr_dangling");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn docker_tag_invokes_docker_image_tag() {
        let script = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "dockrev-fake-probe" ]]; then
  exit 0
fi
printf '%s
' "$*" >"$(dirname "$0")/last-args.txt"
exit 0
"#;

        let (dir, docker_bin) = install_fake_docker(script).unwrap();
        let mut cfg = test_cfg();
        cfg.docker_bin = docker_bin;

        docker_tag(
            &cfg,
            "sha256:new-image",
            "ghcr.io/ivanli-cn/dockrev:latest",
            Duration::from_secs(1),
        )
        .await
        .unwrap();

        let recorded = std::fs::read_to_string(dir.path().join("last-args.txt")).unwrap();
        assert_eq!(
            recorded.trim(),
            "image tag sha256:new-image ghcr.io/ivanli-cn/dockrev:latest"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn auto_match_container_prefers_compose_labels_when_configured() {
        let script = r#"#!/usr/bin/env bash
set -euo pipefail
cmd="${1:-}"
shift || true

if [[ "$cmd" == "ps" ]]; then
  args="$*"
  if [[ "$args" == *"label=com.docker.compose.project=dockrev"* && "$args" == *"label=com.docker.compose.service=dockrev"* ]]; then
    echo '{"ID":"ctr_labels","Image":"c85819d0c6dd"}'
    exit 0
  fi
  # Unfiltered ps should not be used when labels-first succeeds, but keep it deterministic.
  echo '{"ID":"ctr_other","Image":"ghcr.io/ivanli-cn/dockrev:latest"}'
  exit 0
fi

if [[ "$cmd" == "inspect" ]]; then
  cat <<'JSON'
{"Image":"sha256:deadbeef","Config":{"Labels":{"com.docker.compose.service":"dockrev","com.docker.compose.project":"dockrev"},"Env":[],"Image":"ghcr.io/ivanli-cn/dockrev:latest"},"NetworkSettings":{"Networks":{}}}
JSON
  exit 0
fi

echo "unexpected args: $cmd $*" >&2
exit 1
"#;

        let (_dir, docker_bin) = install_fake_docker(script).unwrap();

        let mut cfg = test_cfg();
        cfg.target_compose_project = Some("dockrev".to_string());
        cfg.target_compose_service = Some("dockrev".to_string());
        cfg.docker_bin = docker_bin;

        let got = auto_match_container(&cfg).await.unwrap();
        assert_eq!(got, "ctr_labels");
    }
}
