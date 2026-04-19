use super::*;

#[derive(Default)]
struct RefreshContainerIdRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for RefreshContainerIdRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            // ps -q (pre-update)
            0 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            // docker inspect image id (pre-update)
            1 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "old_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old\n".to_string(),
                    stderr: String::new(),
                }
            }
            // docker-compose pull
            2 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["pull", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            // docker-compose up -d
            3 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["up", "-d", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            // ps -q (post-update; container recreated)
            4 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            // docker inspect has healthcheck (MUST use post-update id)
            5 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{if .State.Health}}1{{else}}0{{end}}",
                        "new_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "0\n".to_string(),
                    stderr: String::new(),
                }
            }
            // docker inspect image id (post-update; MUST use post-update id)
            6 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "new_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:new\n".to_string(),
                    stderr: String::new(),
                }
            }
            // docker image tag after successful update
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };

        *step += 1;
        Ok(out)
    }
}

#[tokio::test]
async fn dry_run_does_not_execute() {
    let stack = StackRecord {
        id: "stk_1".to_string(),
        name: "App".to_string(),
        archived: false,
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
                resolved_tag: None,
                resolved_tags: None,
            },
            homepage: None,
            update_guard: None,
            candidate: None,
            ignore: None,
            version_inference: None,
            new_version_discovery_count: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    };

    let runner = FakeRunner::default();
    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "dry-run",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, "success");
    assert_eq!(runner.calls.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn update_job_refreshes_container_id_after_up() {
    let stack = StackRecord {
        id: "stk_1".to_string(),
        name: "App".to_string(),
        archived: false,
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
                resolved_tag: None,
                resolved_tags: None,
            },
            homepage: None,
            update_guard: None,
            candidate: None,
            ignore: None,
            version_inference: None,
            new_version_discovery_count: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    };

    let runner = RefreshContainerIdRunner::default();
    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["changedServices"].as_u64().unwrap(), 1);
    assert_eq!(*runner.step.lock().unwrap(), 8);
}

#[tokio::test]
async fn update_job_emits_service_progress_events() {
    let stack = StackRecord {
        id: "stk_1".to_string(),
        name: "App".to_string(),
        archived: false,
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
                resolved_tag: None,
                resolved_tags: None,
            },
            homepage: None,
            update_guard: None,
            candidate: None,
            ignore: None,
            version_inference: None,
            new_version_discovery_count: None,
            settings: ServiceSettings {
                auto_rollback: true,
                backup_targets: BackupTargetOverrides {
                    bind_paths: BTreeMap::<String, TernaryChoice>::new(),
                    volume_names: BTreeMap::<String, TernaryChoice>::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    };

    let runner = RefreshContainerIdRunner::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UpdateProgressEvent>();
    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        Some(tx),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    let mut steps = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        steps.push(evt.step);
    }
    assert!(steps.contains(&UpdateProgressStep::ServiceStart));
    assert!(steps.contains(&UpdateProgressStep::PullStart));
    assert!(steps.contains(&UpdateProgressStep::PullDone));
    assert!(steps.contains(&UpdateProgressStep::UpDone));
    assert!(steps.contains(&UpdateProgressStep::SyncTagStart));
    assert!(steps.contains(&UpdateProgressStep::SyncTagDone));
    assert!(steps.contains(&UpdateProgressStep::ServiceDone));
}
