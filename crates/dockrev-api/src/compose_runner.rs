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
        // Keep Compose in terminal progress mode so layer updates overwrite their screen rows.
        // The command is piped rather than attached to a TTY, so ANSI must be explicit too.
        // `docker-compose` can be the standalone invocation for the same V2 implementation.
        cmd.env
            .push(("COMPOSE_PROGRESS".to_string(), "tty".to_string()));
        cmd.env
            .push(("COMPOSE_ANSI".to_string(), "always".to_string()));
        cmd
    }

    pub fn up_services(&self, cfg: &ComposeRunnerConfig, services: &[String]) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.extend(["up".to_string(), "-d".to_string()]);
        cmd.args.extend(services.iter().cloned());
        cmd
    }

    pub fn stop_services(&self, cfg: &ComposeRunnerConfig, services: &[String]) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.push("stop".to_string());
        cmd.args.extend(services.iter().cloned());
        cmd
    }

    pub fn start_stack_without_pull(&self, cfg: &ComposeRunnerConfig) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        if is_docker_plugin(&cfg.compose_bin) {
            cmd.args.extend([
                "up".to_string(),
                "-d".to_string(),
                "--pull".to_string(),
                "never".to_string(),
                "--no-recreate".to_string(),
            ]);
        } else {
            cmd.args.push("start".to_string());
        }
        cmd
    }

    pub fn stop_stack(&self, cfg: &ComposeRunnerConfig) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.push("stop".to_string());
        cmd
    }

    pub fn restart_stack(&self, cfg: &ComposeRunnerConfig) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.push("restart".to_string());
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

    pub fn start_service_without_pull(
        &self,
        cfg: &ComposeRunnerConfig,
        service: &str,
    ) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        if is_docker_plugin(&cfg.compose_bin) {
            cmd.args.extend([
                "up".to_string(),
                "-d".to_string(),
                "--pull".to_string(),
                "never".to_string(),
                "--no-recreate".to_string(),
                "--no-deps".to_string(),
            ]);
        } else {
            // Compose V1 has no `up --pull never`; `start` only starts an
            // existing container and fails safely when one does not exist.
            cmd.args.push("start".to_string());
        }
        cmd.args.push(service.to_string());
        cmd
    }

    pub fn ps_q_service(&self, cfg: &ComposeRunnerConfig, service: &str) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args
            .extend(["ps".to_string(), "-q".to_string(), service.to_string()]);
        cmd
    }

    pub fn ps_all_q_service(&self, cfg: &ComposeRunnerConfig, service: &str) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args.extend([
            "ps".to_string(),
            "-a".to_string(),
            "-q".to_string(),
            service.to_string(),
        ]);
        cmd
    }

    pub fn restart_service(&self, cfg: &ComposeRunnerConfig, service: &str) -> CommandSpec {
        let mut cmd = self.base_command(cfg);
        cmd.args
            .extend(["restart".to_string(), service.to_string()]);
        cmd
    }
}

pub(crate) fn is_docker_plugin(compose_bin: &str) -> bool {
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
    fn stack_lifecycle_commands_preserve_existing_containers() {
        let stack = ComposeStack {
            project_name: "myproj".to_string(),
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/app/docker-compose.yml".to_string()],
                env_file: None,
            },
        };
        let plugin = ComposeRunnerConfig {
            compose_bin: "docker".to_string(),
            env: Vec::new(),
        };
        let v1 = ComposeRunnerConfig {
            compose_bin: "docker-compose".to_string(),
            env: Vec::new(),
        };

        let plugin_start = stack.start_stack_without_pull(&plugin);
        assert_eq!(
            plugin_start.args[plugin_start.args.len() - 5..],
            ["up", "-d", "--pull", "never", "--no-recreate"]
        );
        assert_eq!(
            stack
                .start_stack_without_pull(&v1)
                .args
                .last()
                .map(String::as_str),
            Some("start")
        );
        assert_eq!(
            stack.stop_stack(&plugin).args.last().map(String::as_str),
            Some("stop")
        );
        assert_eq!(
            stack.restart_stack(&plugin).args.last().map(String::as_str),
            Some("restart")
        );
    }

    #[test]
    fn docker_compose_plugin_progress_env_keeps_auth_env_and_terminal_output() {
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
                ("COMPOSE_PROGRESS".to_string(), "tty".to_string()),
                ("COMPOSE_ANSI".to_string(), "always".to_string()),
            ]
        );
    }

    #[test]
    fn standalone_compose_progress_env_keeps_terminal_output() {
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

        let cmd = stack.pull_services_with_progress(&cfg, &["web".to_string()]);

        assert_eq!(
            cmd.env,
            vec![
                ("COMPOSE_PROGRESS".to_string(), "tty".to_string()),
                ("COMPOSE_ANSI".to_string(), "always".to_string()),
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
            vec![
                ("COMPOSE_PROGRESS".to_string(), "tty".to_string()),
                ("COMPOSE_ANSI".to_string(), "always".to_string()),
            ]
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

    #[test]
    fn lifecycle_commands_keep_compose_context_and_do_not_pull_on_start() {
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

        let start = stack.start_service_without_pull(&cfg, "web");
        assert_eq!(
            start.args[start.args.len() - 7..],
            [
                "up".to_string(),
                "-d".to_string(),
                "--pull".to_string(),
                "never".to_string(),
                "--no-recreate".to_string(),
                "--no-deps".to_string(),
                "web".to_string(),
            ]
        );

        let stop = stack.stop_services(&cfg, &["web".to_string()]);
        assert_eq!(
            stop.args[stop.args.len() - 2..],
            ["stop".to_string(), "web".to_string()]
        );

        let restart = stack.restart_service(&cfg, "web");
        assert_eq!(
            restart.args[restart.args.len() - 2..],
            ["restart".to_string(), "web".to_string()]
        );

        let all = stack.ps_all_q_service(&cfg, "web");
        assert_eq!(
            all.args[all.args.len() - 4..],
            [
                "ps".to_string(),
                "-a".to_string(),
                "-q".to_string(),
                "web".to_string()
            ]
        );

        let v1 = ComposeRunnerConfig {
            compose_bin: "docker-compose".to_string(),
            env: Vec::new(),
        };
        let v1_start = stack.start_service_without_pull(&v1, "web");
        assert_eq!(
            v1_start.args[v1_start.args.len() - 2..],
            ["start".to_string(), "web".to_string()]
        );
    }
}
