use std::{collections::HashMap, path::Path, time::Duration};

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
    let out = run_docker_lines(
        cfg,
        &["ps", "--format", "{{json .}}"],
        Duration::from_secs(10),
    )
    .await?;
    let mut matches: Vec<String> = Vec::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parsed: DockerPsLine = serde_json::from_str(line)?;
        if image_ref_matches_repo(&parsed.image, &cfg.target_image_repo) {
            matches.push(parsed.id);
        }
    }
    if matches.is_empty() {
        return Err(anyhow::anyhow!(
            "no running container matched image repo {}; set DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID",
            cfg.target_image_repo
        ));
    }
    if matches.len() > 1 {
        let desired = cfg.target_compose_service.as_deref().unwrap_or("dockrev");
        let mut candidates: Vec<ComposeCandidate> = Vec::new();
        for id in matches {
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
            return Ok(id);
        }

        return Err(anyhow::anyhow!(
            "multiple running containers matched image repo {}; set DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID or DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE",
            cfg.target_image_repo
        ));
    }
    Ok(matches.remove(0))
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

async fn run_docker_lines(
    cfg: &Config,
    args: &[&str],
    timeout: Duration,
) -> anyhow::Result<String> {
    let mut a = Vec::with_capacity(args.len());
    for s in args {
        a.push((*s).to_string());
    }
    run_cmd_lines(cfg, "docker", &a, timeout).await
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
}
