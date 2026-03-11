use crate::runner::CommandSpec;

#[derive(Clone, Debug)]
pub struct DockerRunnerConfig {
    pub docker_bin: String,
    pub env: Vec<(String, String)>,
}

impl Default for DockerRunnerConfig {
    fn default() -> Self {
        Self {
            docker_bin: "docker".to_string(),
            env: Vec::new(),
        }
    }
}

pub fn inspect_health_status(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.State.Health.Status}}".to_string(),
            container_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn inspect_has_healthcheck(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{if .State.Health}}1{{else}}0{{end}}".to_string(),
            container_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn inspect_image_id(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.Image}}".to_string(),
            container_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn tag_image(cfg: &DockerRunnerConfig, image_id: &str, image_ref: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "image".to_string(),
            "tag".to_string(),
            image_id.to_string(),
            image_ref.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn image_inspect_oci_version(cfg: &DockerRunnerConfig, image_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "image".to_string(),
            "inspect".to_string(),
            "--format".to_string(),
            r#"{{ index .Config.Labels "org.opencontainers.image.version" }}"#.to_string(),
            image_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn image_inspect_repo_tags(cfg: &DockerRunnerConfig, image_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "image".to_string(),
            "inspect".to_string(),
            "--format".to_string(),
            "{{json .RepoTags}}".to_string(),
            image_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn pull_image(cfg: &DockerRunnerConfig, image_ref: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec!["pull".to_string(), image_ref.to_string()],
        env: cfg.env.clone(),
    }
}
