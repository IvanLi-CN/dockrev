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

pub fn inspect_health_policy(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{json .Config.Healthcheck}}".to_string(),
            container_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn inspect_candidate_state(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{json .State}}".to_string(),
            container_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

pub fn logs_with_timestamps(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "exec \"$1\" logs --timestamps \"$2\" 2>&1".to_string(),
            "dockrev-docker-logs".to_string(),
            cfg.docker_bin.clone(),
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

pub fn inspect_repo_digests(cfg: &DockerRunnerConfig, image_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "image".to_string(),
            "inspect".to_string(),
            "--format".to_string(),
            "{{join .RepoDigests \",\"}}".to_string(),
            image_id.to_string(),
        ],
        env: cfg.env.clone(),
    }
}

#[allow(dead_code)]
pub fn inspect_is_running(cfg: &DockerRunnerConfig, container_id: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{if .State.Running}}1{{else}}0{{end}}".to_string(),
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

pub fn pull_image(cfg: &DockerRunnerConfig, image_ref: &str) -> CommandSpec {
    CommandSpec {
        program: cfg.docker_bin.clone(),
        args: vec!["pull".to_string(), image_ref.to_string()],
        env: cfg.env.clone(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use super::*;
    use crate::runner::{CommandRunner, TokioCommandRunner};

    #[tokio::test]
    async fn logs_with_timestamps_captures_both_cli_streams_in_order() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-docker-logs-{}-{}",
            std::process::id(),
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let docker_bin = root.join("docker");
        tokio::fs::write(
            &docker_bin,
            "#!/bin/sh\nprintf 'container stdout\\n'\nprintf 'container stderr\\n' >&2\n",
        )
        .await
        .unwrap();
        let mut permissions = std::fs::metadata(&docker_bin).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&docker_bin, permissions).unwrap();

        let output_path = root.join("container.log");
        let output = TokioCommandRunner
            .run_raw_to_file(
                logs_with_timestamps(
                    &DockerRunnerConfig {
                        docker_bin: docker_bin.to_string_lossy().into_owned(),
                        env: Vec::new(),
                    },
                    "candidate-id",
                ),
                Duration::from_secs(2),
                &output_path,
            )
            .await
            .unwrap();

        let expected = b"container stdout\ncontainer stderr\n";
        assert_eq!(output.status, 0);
        assert_eq!(output.bytes_written, expected.len() as u64);
        assert!(output.eof_reached);
        assert!(!output.timed_out);
        assert!(output.stderr.is_empty());
        assert_eq!(tokio::fs::read(&output_path).await.unwrap(), expected);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
