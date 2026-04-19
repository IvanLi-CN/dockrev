use crate::{api::types::ComposeConfig, runner::CommandSpec};

#[derive(Clone, Debug)]
pub struct ComposeRunnerConfig {
    pub compose_bin: String,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct ComposeStack {
    pub project_name: String,
    pub compose: ComposeConfig,
}

impl ComposeStack {
    pub fn base_command(&self, cfg: &ComposeRunnerConfig) -> CommandSpec {
        let mut args: Vec<String> = Vec::new();

        if is_docker_plugin(&cfg.compose_bin) {
            args.push("compose".to_string());
        }

        for f in &self.compose.compose_files {
            args.push("-f".to_string());
            args.push(f.clone());
        }

        if let Some(env_file) = self.compose.env_file.as_deref() {
            args.push("--env-file".to_string());
            args.push(env_file.to_string());
        }

        args.push("--project-name".to_string());
        args.push(self.project_name.clone());

        CommandSpec {
            program: cfg.compose_bin.clone(),
            args,
            env: cfg.env.clone(),
        }
    }

    pub fn pull_services(&self, cfg: &ComposeRunnerConfig, services: &[String]) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.push("pull".to_string());
        cmd.args.extend(services.iter().cloned());
        cmd
    }

    pub fn pull_services_with_progress(
        &self,
        cfg: &ComposeRunnerConfig,
        services: &[String],
    ) -> CommandSpec {
        let mut cmd = self.pull_services(cfg, services);
        // Docker Compose v2 supports COMPOSE_PROGRESS=plain for stable machine-friendly output.
        if is_docker_plugin(&cfg.compose_bin) {
            cmd.env
                .push(("COMPOSE_PROGRESS".to_string(), "plain".to_string()));
        }
        cmd
    }

    pub fn up_services(&self, cfg: &ComposeRunnerConfig, services: &[String]) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.extend(["up".to_string(), "-d".to_string()]);
        cmd.args.extend(services.iter().cloned());
        cmd
    }

    pub fn up_service_no_pull(&self, cfg: &ComposeRunnerConfig, service: &str) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.extend([
            "up".to_string(),
            "-d".to_string(),
            "--pull".to_string(),
            "never".to_string(),
            service.to_string(),
        ]);
        cmd
    }

    pub fn ps_q_service(&self, cfg: &ComposeRunnerConfig, service: &str) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args
            .extend(["ps".to_string(), "-q".to_string(), service.to_string()]);
        cmd
    }
}

fn is_docker_plugin(compose_bin: &str) -> bool {
    let bin = compose_bin.to_ascii_lowercase();
    bin == "docker" || bin.ends_with("/docker") || bin.ends_with("\\docker")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_compose_plugin_builds_args() {
        let stack = ComposeStack {
            project_name: "myproj".to_string(),
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/app/docker-compose.yml".to_string()],
                env_file: Some("/srv/app/.env".to_string()),
            },
        };
        let cfg = ComposeRunnerConfig {
            compose_bin: "docker".to_string(),
            env: vec![("DOCKER_CONFIG".to_string(), "/tmp/auth/.docker".to_string())],
        };
        let cmd = stack.pull_services(&cfg, &["web".to_string()]);
        assert_eq!(cmd.program, "docker");
        assert_eq!(cmd.args[0], "compose");
        assert!(cmd.args.iter().any(|a| a == "--project-name"));
        assert_eq!(
            cmd.env,
            vec![("DOCKER_CONFIG".to_string(), "/tmp/auth/.docker".to_string())]
        );
    }

    #[test]
    fn docker_compose_v1_builds_args() {
        let stack = ComposeStack {
            project_name: "myproj".to_string(),
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/app/docker-compose.yml".to_string()],
                env_file: None,
            },
        };
        let cfg = ComposeRunnerConfig {
            compose_bin: "docker-compose".to_string(),
            env: Vec::new(),
        };
        let cmd = stack.ps_q_service(&cfg, "web");
        assert_eq!(cmd.program, "docker-compose");
        assert_ne!(cmd.args[0], "compose");
    }

    #[test]
    fn docker_compose_plugin_progress_env_keeps_auth_env() {
        let stack = ComposeStack {
            project_name: "myproj".to_string(),
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/app/docker-compose.yml".to_string()],
                env_file: None,
            },
        };
        let cfg = ComposeRunnerConfig {
            compose_bin: "docker".to_string(),
            env: vec![(
                "DOCKER_CONFIG".to_string(),
                "/tmp/dockrev-auth-config/.docker".to_string(),
            )],
        };

        let cmd = stack.pull_services_with_progress(&cfg, &["web".to_string()]);

        assert_eq!(
            cmd.env,
            vec![
                (
                    "DOCKER_CONFIG".to_string(),
                    "/tmp/dockrev-auth-config/.docker".to_string()
                ),
                ("COMPOSE_PROGRESS".to_string(), "plain".to_string()),
            ]
        );
    }

    #[test]
    fn docker_compose_plugin_batches_pull_and_up_for_multiple_services() {
        let stack = ComposeStack {
            project_name: "myproj".to_string(),
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/app/docker-compose.yml".to_string()],
                env_file: None,
            },
        };
        let cfg = ComposeRunnerConfig {
            compose_bin: "docker".to_string(),
            env: Vec::new(),
        };
        let services = vec!["web".to_string(), "worker".to_string()];

        let pull_cmd = stack.pull_services_with_progress(&cfg, &services);
        assert_eq!(pull_cmd.program, "docker");
        assert_eq!(
            pull_cmd.args[pull_cmd.args.len() - 3..],
            ["pull".to_string(), "web".to_string(), "worker".to_string()]
        );
        assert_eq!(
            pull_cmd.env,
            vec![("COMPOSE_PROGRESS".to_string(), "plain".to_string())]
        );

        let up_cmd = stack.up_services(&cfg, &services);
        assert_eq!(up_cmd.program, "docker");
        assert_eq!(
            up_cmd.args[up_cmd.args.len() - 4..],
            [
                "up".to_string(),
                "-d".to_string(),
                "web".to_string(),
                "worker".to_string()
            ]
        );
    }
}
