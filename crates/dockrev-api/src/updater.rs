use std::time::Duration;

use serde_json::json;

use crate::{
    api::types::{JobScope, StackRecord},
    compose_runner::{ComposeRunnerConfig, ComposeStack},
    docker_runner,
    runner::{CommandRunner, CommandSpec},
};

#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub status: String,
    pub summary_json: serde_json::Value,
}

pub async fn run_update_job(
    runner: &dyn CommandRunner,
    compose_bin: &str,
    stack: &StackRecord,
    scope: &JobScope,
    service_id: Option<&str>,
    mode: &str,
) -> anyhow::Result<UpdateOutcome> {
    let compose_cfg = ComposeRunnerConfig {
        compose_bin: compose_bin.to_string(),
    };
    let compose_stack = ComposeStack {
        project_name: sanitize_project_name(&stack.name),
        compose: stack.compose.clone(),
    };

    let services = match scope {
        JobScope::All => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Stack => stack.services.iter().collect::<Vec<_>>(),
        JobScope::Service => stack
            .services
            .iter()
            .filter(|s| service_id.is_some_and(|id| id == s.id))
            .collect::<Vec<_>>(),
    };

    if mode == "dry-run" {
        return Ok(UpdateOutcome {
            status: "success".to_string(),
            summary_json: json!({
                "mode": "dry-run",
                "changedServices": services.len(),
            }),
        });
    }

    let docker_cfg = docker_runner::DockerRunnerConfig::default();

    let mut changed = 0u32;
    let mut old_images = serde_json::Map::new();
    let mut new_images = serde_json::Map::new();

    for svc in services {
        let container_id = run_to_string(
            runner,
            compose_stack.ps_q_service(&compose_cfg, &svc.name),
            Duration::from_secs(30),
        )
        .await?;
        let container_id = container_id.trim().to_string();
        if container_id.is_empty() {
            continue;
        }

        let old_image_id = run_to_string(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &container_id),
            Duration::from_secs(10),
        )
        .await?;
        let old_image_id = old_image_id.trim().to_string();
        old_images.insert(svc.id.clone(), json!(old_image_id));

        run_checked(
            runner,
            compose_stack.pull_service(&compose_cfg, &svc.name),
            Duration::from_secs(300),
        )
        .await?;
        run_checked(
            runner,
            compose_stack.up_service(&compose_cfg, &svc.name),
            Duration::from_secs(300),
        )
        .await?;

        let has_health = run_to_string(
            runner,
            docker_runner::inspect_has_healthcheck(&docker_cfg, &container_id),
            Duration::from_secs(10),
        )
        .await?;

        let has_health = has_health.trim() == "1";
        let mut rolled_back = false;
        if has_health {
            let ok =
                wait_healthy(runner, &docker_cfg, &container_id, Duration::from_secs(90)).await?;
            if !ok {
                run_checked(
                    runner,
                    docker_runner::tag_image(&docker_cfg, &old_image_id, &svc.image.reference),
                    Duration::from_secs(30),
                )
                .await?;
                run_checked(
                    runner,
                    compose_stack.up_service_no_pull(&compose_cfg, &svc.name),
                    Duration::from_secs(300),
                )
                .await?;
                let ok2 = wait_healthy(runner, &docker_cfg, &container_id, Duration::from_secs(90))
                    .await?;
                if !ok2 {
                    return Ok(UpdateOutcome {
                        status: "failed".to_string(),
                        summary_json: json!({"reason":"rollback_failed"}),
                    });
                }
                rolled_back = true;
            }
        }

        let new_image_id = run_to_string(
            runner,
            docker_runner::inspect_image_id(&docker_cfg, &container_id),
            Duration::from_secs(10),
        )
        .await?;
        new_images.insert(svc.id.clone(), json!(new_image_id.trim()));
        changed += 1;

        if rolled_back {
            return Ok(UpdateOutcome {
                status: "rolled_back".to_string(),
                summary_json: json!({
                    "changedServices": changed,
                    "oldDigests": old_images,
                    "newDigests": new_images,
                }),
            });
        }
    }

    Ok(UpdateOutcome {
        status: "success".to_string(),
        summary_json: json!({
            "changedServices": changed,
            "oldDigests": old_images,
            "newDigests": new_images,
        }),
    })
}

async fn wait_healthy(
    runner: &dyn CommandRunner,
    docker_cfg: &docker_runner::DockerRunnerConfig,
    container_id: &str,
    timeout: Duration,
) -> anyhow::Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = run_to_string(
            runner,
            docker_runner::inspect_health_status(docker_cfg, container_id),
            Duration::from_secs(10),
        )
        .await?;

        match status.trim() {
            "healthy" => return Ok(true),
            "unhealthy" => return Ok(false),
            _ => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_checked(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
) -> anyhow::Result<()> {
    let out = runner.run(spec, timeout).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "command failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(())
}

async fn run_to_string(
    runner: &dyn CommandRunner,
    spec: CommandSpec,
    timeout: Duration,
) -> anyhow::Result<String> {
    let out = runner.run(spec, timeout).await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "command failed: status={} stderr={}",
            out.status,
            out.stderr
        ));
    }
    Ok(out.stdout)
}

fn sanitize_project_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "dockrev".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::types::{BackupTargetOverrides, ComposeRef, Service, ServiceSettings, TernaryChoice},
        runner::{CommandOutput, CommandRunner},
    };
    use std::{collections::BTreeMap, sync::Mutex};

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for FakeRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((spec.program, spec.args.clone()));
            Ok(CommandOutput {
                status: 0,
                stdout: "\n".to_string(),
                stderr: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn dry_run_does_not_execute() {
        let stack = StackRecord {
            id: "stk_1".to_string(),
            name: "App".to_string(),
            compose: crate::api::types::ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/docker-compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: vec![Service {
                id: "svc_1".to_string(),
                name: "web".to_string(),
                image: ComposeRef {
                    reference: "ghcr.io/org/web:1.0".to_string(),
                    tag: "1.0".to_string(),
                    digest: None,
                },
                candidate: None,
                ignore: None,
                settings: ServiceSettings {
                    auto_rollback: true,
                    backup_targets: BackupTargetOverrides {
                        bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                        volume_names: BTreeMap::<String, TernaryChoice>::new(),
                    },
                },
            }],
        };

        let runner = FakeRunner::default();
        let outcome = run_update_job(
            &runner,
            "docker-compose",
            &stack,
            &JobScope::Stack,
            None,
            "dry-run",
        )
        .await
        .unwrap();
        assert_eq!(outcome.status, "success");
        assert_eq!(runner.calls.lock().unwrap().len(), 0);
    }
}
