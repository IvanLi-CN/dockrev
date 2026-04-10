use super::*;
use crate::{
    api::types::{
        ArchMatch, BackupTargetOverrides, Candidate, ComposeRef, Service, ServiceSettings,
        TernaryChoice,
    },
    runner::{CommandOutput, CommandRunner},
};
use std::{collections::BTreeMap, fs, sync::Mutex};

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
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

fn args_end_with(args: &[String], suffix: &[&str]) -> bool {
    if args.len() < suffix.len() {
        return false;
    }
    let start = args.len() - suffix.len();
    suffix
        .iter()
        .enumerate()
        .all(|(i, s)| args[start + i] == *s)
}

fn single_service_stack(image_reference: &str, candidate: Option<Candidate>) -> StackRecord {
    StackRecord {
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
                reference: image_reference.to_string(),
                tag: "1.0".to_string(),
                digest: None,
                resolved_tag: None,
                resolved_tags: None,
            },
            candidate,
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
    }
}

fn explicit_targets(
    service_id: &str,
    target_tag: &str,
    target_digest: &str,
    pull_tags: &[&str],
) -> Vec<UpdateServiceTarget> {
    vec![UpdateServiceTarget {
        service_id: service_id.to_string(),
        target_tag: target_tag.to_string(),
        target_digest: target_digest.to_string(),
        pull_tags: Some(pull_tags.iter().map(|tag| (*tag).to_string()).collect()),
        skip_tag_followups: false,
    }]
}

fn write_test_docker_config() -> (PathBuf, TempDirCleanup) {
    let root = std::env::temp_dir().join(format!("dockrev-test-auth-{}", ulid::Ulid::new()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("docker-config.custom.json");
    fs::write(&path, r#"{"auths":{"ghcr.io":{"auth":"Zm9vOmJhcg=="}}}"#).unwrap();
    (path, TempDirCleanup(root))
}

fn write_test_default_named_docker_config() -> (PathBuf, TempDirCleanup) {
    let root =
        std::env::temp_dir().join(format!("dockrev-test-auth-default-{}", ulid::Ulid::new()));
    let contexts_dir = root.join("contexts/meta");
    let buildx_dir = root.join("buildx");
    fs::create_dir_all(&contexts_dir).unwrap();
    fs::create_dir_all(&buildx_dir).unwrap();
    let path = root.join("config.json");
    fs::write(&path, r#"{"auths":{"ghcr.io":{"auth":"Zm9vOmJhcg=="}}}"#).unwrap();
    fs::write(
        contexts_dir.join("state.json"),
        r#"{"currentContext":"desktop-linux"}"#,
    )
    .unwrap();
    fs::write(buildx_dir.join("state.json"), "cache-state").unwrap();
    fs::write(root.join("notes.txt"), "not-for-auth-bridge").unwrap();
    (path, TempDirCleanup(root))
}

#[test]
fn docker_cli_auth_bridge_stages_custom_config_as_config_json() {
    let (source_path, _source_cleanup) = write_test_docker_config();

    let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");
    let staged_path = bridge.docker_config_dir.join("config.json");

    assert_eq!(
        fs::read_to_string(&staged_path).unwrap(),
        fs::read_to_string(&source_path).unwrap()
    );
    assert_eq!(
        bridge.env(),
        vec![(
            "DOCKER_CONFIG".to_string(),
            bridge.docker_config_dir.to_string_lossy().to_string(),
        )]
    );
}

#[test]
fn docker_cli_auth_bridge_copies_context_metadata_for_real_config_json() {
    let (source_path, _source_cleanup) = write_test_default_named_docker_config();

    let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");
    let staged_path = bridge.docker_config_dir.join("config.json");
    let staged_context = bridge.docker_config_dir.join("contexts/meta/state.json");

    assert_eq!(
        fs::read_to_string(&staged_path).unwrap(),
        fs::read_to_string(&source_path).unwrap()
    );
    assert_eq!(
        fs::read_to_string(&staged_context).unwrap(),
        r#"{"currentContext":"desktop-linux"}"#
    );
    assert!(!bridge.docker_config_dir.join("buildx/state.json").exists());
    assert!(!bridge.docker_config_dir.join("notes.txt").exists());
}

#[cfg(unix)]
#[test]
fn docker_cli_auth_bridge_handles_read_only_real_config_json() {
    use std::os::unix::fs::PermissionsExt;

    let (source_path, _source_cleanup) = write_test_default_named_docker_config();
    let mut permissions = fs::metadata(&source_path).unwrap().permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&source_path, permissions).unwrap();

    let bridge = DockerCliAuthBridge::stage(&source_path).expect("auth bridge should stage");

    assert_eq!(
        fs::read_to_string(bridge.docker_config_dir.join("config.json")).unwrap(),
        fs::read_to_string(&source_path).unwrap()
    );
}

#[derive(Default)]
struct EnvCaptureUpdateRunner {
    step: Mutex<usize>,
    specs: Mutex<Vec<CommandSpec>>,
}

#[async_trait::async_trait]
impl CommandRunner for EnvCaptureUpdateRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.specs.lock().unwrap().push(spec.clone());
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            7 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

fn selection_test_service(id: &str, name: &str, image_reference: &str) -> Service {
    Service {
        id: id.to_string(),
        name: name.to_string(),
        image: ComposeRef {
            reference: image_reference.to_string(),
            tag: "0.29.3".to_string(),
            digest: None,
            resolved_tag: None,
            resolved_tags: None,
        },
        candidate: Some(Candidate {
            tag: "latest".to_string(),
            resolved_tag: Some("0.29.5".to_string()),
            digest: "sha256:candidate".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
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
    }
}

#[test]
fn aggregate_selection_excludes_dockrev_but_keeps_supervisor() {
    let stack = StackRecord {
        id: "stk_guard".to_string(),
        name: "dockrev-mod".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![
            selection_test_service("svc-dockrev", "dockrev", "ghcr.io/ivanli-cn/dockrev:0.29.3"),
            selection_test_service(
                "svc-supervisor",
                "dockrev-supervisor",
                "ghcr.io/ivanli-cn/dockrev-supervisor:0.29.3",
            ),
        ],
    };

    let selection = select_update_services(
        &stack,
        &JobScope::Stack,
        None,
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
    );
    let ids = selection
        .services
        .iter()
        .map(|svc| svc.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["svc-supervisor"]);
}

#[test]
fn service_scope_still_allows_dockrev_update_selection() {
    let stack = StackRecord {
        id: "stk_guard".to_string(),
        name: "dockrev-mod".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![selection_test_service(
            "svc-dockrev",
            "dockrev",
            "ghcr.io/ivanli-cn/dockrev:0.29.3",
        )],
    };

    let selection = select_update_services(
        &stack,
        &JobScope::Service,
        Some("svc-dockrev"),
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
    );

    assert_eq!(selection.services.len(), 1);
    assert_eq!(selection.services[0].id, "svc-dockrev");
}

#[test]
fn detect_semver_downgrade_ignores_opaque_hash_like_prerelease_versions() {
    let mut service = selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }

    assert_eq!(detect_semver_downgrade(&service), None);
}

#[test]
fn detect_semver_downgrade_does_not_fall_back_to_raw_tag_after_opaque_resolved_tag() {
    let mut service = selection_test_service(
        "svc-hash-tagged",
        "hash-build",
        "ghcr.io/acme/web:2026.3.28",
    );
    service.image.tag = "2026.3.28".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.tag = "2026.3.27".to_string();
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }

    assert_eq!(detect_semver_downgrade(&service), None);
}

#[test]
fn select_update_services_keeps_hash_like_prerelease_candidates_for_non_ui_runs() {
    let mut service = selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }
    let stack = StackRecord {
        id: "stk_hash".to_string(),
        name: "hash-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert_eq!(selection.services.len(), 1);
    assert!(selection.skipped_version_anomaly.is_empty());
}

#[test]
fn select_update_services_keeps_opaque_resolved_tags_even_when_raw_tags_look_semver_like() {
    let mut service = selection_test_service(
        "svc-hash-tagged",
        "hash-build",
        "ghcr.io/acme/web:2026.3.28",
    );
    service.image.tag = "2026.3.28".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.tag = "2026.3.27".to_string();
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }
    let stack = StackRecord {
        id: "stk_hash_tagged".to_string(),
        name: "hash-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert_eq!(selection.services.len(), 1);
    assert!(selection.skipped_version_anomaly.is_empty());
}

#[test]
fn select_update_services_still_skips_ordered_prerelease_downgrades() {
    let mut service = selection_test_service("svc-rc", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc.2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc.1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[test]
fn select_update_services_still_skips_single_token_prerelease_downgrades() {
    let mut service = selection_test_service("svc-rc1", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc1".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[test]
fn select_update_services_still_skips_hyphenated_prerelease_downgrades() {
    let mut service =
        selection_test_service("svc-rc-hyphen", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc-2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc-1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc_hyphen".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[tokio::test]
async fn aggregate_dockrev_only_update_becomes_noop() {
    let stack = single_service_stack(
        "ghcr.io/ivanli-cn/dockrev:0.29.3",
        Some(Candidate {
            tag: "latest".to_string(),
            resolved_tag: Some("0.29.5".to_string()),
            digest: "sha256:candidate".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let runner = FakeRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        None,
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["changedServices"].as_u64(), Some(0));
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn update_job_injects_docker_auth_env_into_compose_and_docker_commands() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = EnvCaptureUpdateRunner::default();
    let (docker_config_path, _docker_config_cleanup) = write_test_docker_config();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        Some(docker_config_path.as_path()),
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

    let specs = runner.specs.lock().unwrap();
    let compose_pull = specs
        .iter()
        .find(|spec| args_end_with(&spec.args, &["pull", "web"]))
        .expect("compose pull command should exist");
    let compose_up = specs
        .iter()
        .find(|spec| args_end_with(&spec.args, &["up", "-d", "web"]))
        .expect("compose up command should exist");
    let docker_tag = specs
        .iter()
        .find(|spec| spec.args == vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"])
        .expect("docker tag command should exist");

    for spec in [compose_pull, compose_up, docker_tag] {
        assert_eq!(spec.env.len(), 1);
        assert!(spec.env.iter().all(|(k, _)| k == "DOCKER_CONFIG"));
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "DOCKER_CONFIG" && v.ends_with("/.docker"))
        );
    }
}

#[tokio::test]
async fn noop_update_with_broken_docker_config_path_stays_noop() {
    let stack = single_service_stack("ghcr.io/ivanli-cn/dockrev:1.0", None);
    let runner = FakeRunner::default();
    let missing_path = std::env::temp_dir()
        .join(format!("dockrev-missing-config-{}", ulid::Ulid::new()))
        .join("config.json");

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        Some(missing_path.as_path()),
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        None,
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["changedServices"].as_u64(), Some(0));
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn update_job_without_docker_config_keeps_command_env_empty() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = EnvCaptureUpdateRunner::default();

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
    assert!(
        runner
            .specs
            .lock()
            .unwrap()
            .iter()
            .all(|spec| spec.env.is_empty())
    );
}

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

#[derive(Default)]
struct HealthRollbackRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for HealthRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "old_container\n".to_string(),
                    stderr: String::new(),
                }
            }
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
            2 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["pull", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            3 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["up", "-d", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            4 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "new_container\n".to_string(),
                    stderr: String::new(),
                }
            }
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
                    stdout: "1\n".to_string(),
                    stderr: String::new(),
                }
            }
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
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{.State.Health.Status}}",
                        "new_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "unhealthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
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
            9 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(
                    &spec.args,
                    &["up", "-d", "--pull", "never", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(&spec.args, &["ps", "-q", "web"]));
                CommandOutput {
                    status: 0,
                    stdout: "rollback_container\n".to_string(),
                    stderr: String::new(),
                }
            }
            11 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec![
                        "inspect",
                        "--format",
                        "{{.State.Health.Status}}",
                        "rollback_container"
                    ]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "healthy\n".to_string(),
                    stderr: String::new(),
                }
            }
            12 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
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
async fn healthcheck_failure_rolls_back_with_attempted_and_final_digests() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = HealthRollbackRunner::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UpdateProgressEvent>();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
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

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["failureStep"].as_str(),
        Some("healthcheck")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("healthcheck")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );

    let mut steps = Vec::new();
    let mut messages = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        steps.push(evt.step);
        messages.push(evt.message);
    }
    assert!(steps.contains(&UpdateProgressStep::HealthStart));
    assert!(steps.contains(&UpdateProgressStep::HealthFailed));
    assert!(!steps.contains(&UpdateProgressStep::HealthDone));
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("healthcheck failed"))
    );
    assert!(
        messages
            .iter()
            .any(|msg| msg.contains("rolled back after healthcheck failure"))
    );
    assert_eq!(*runner.step.lock().unwrap(), 13);
}

#[derive(Default)]
struct SyncTagRollbackRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for SyncTagRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
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
                    status: 1,
                    stdout: String::new(),
                    stderr: "cannot sync tag".to_string(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
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
            9 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(
                    &spec.args,
                    &["up", "-d", "--pull", "never", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => CommandOutput {
                status: 0,
                stdout: "rollback_container\n".to_string(),
                stderr: String::new(),
            },
            11 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
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
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

#[derive(Default)]
struct DigestPinnedRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for DigestPinnedRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            _ => panic!(
                "unexpected extra command: program={} args={:?}",
                spec.program, spec.args
            ),
        };
        *step += 1;
        Ok(out)
    }
}

#[derive(Default)]
struct ExplicitTargetDigestSyncRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for ExplicitTargetDigestSyncRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:1.0"]
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
            8 => {
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

#[derive(Default)]
struct SyncBeforeSemverRunner {
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait::async_trait]
impl CommandRunner for SyncBeforeSemverRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls
            .lock()
            .unwrap()
            .push((spec.program.clone(), spec.args.clone()));
        let args = spec.args.iter().map(String::as_str).collect::<Vec<_>>();
        let out = if spec.program == "docker-compose"
            && args_end_with(&spec.args, &["ps", "-q", "web"])
        {
            CommandOutput {
                status: 0,
                stdout: "new_container\n".to_string(),
                stderr: String::new(),
            }
        } else if spec.program == "docker"
            && args == vec!["inspect", "--format", "{{.Image}}", "new_container"]
        {
            CommandOutput {
                status: 0,
                stdout: "sha256:new\n".to_string(),
                stderr: String::new(),
            }
        } else if spec.program == "docker"
            && args
                == vec![
                    "inspect",
                    "--format",
                    "{{if .State.Health}}1{{else}}0{{end}}",
                    "new_container",
                ]
        {
            CommandOutput {
                status: 0,
                stdout: "0\n".to_string(),
                stderr: String::new(),
            }
        } else {
            CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        };
        Ok(out)
    }
}

#[tokio::test]
async fn sync_tag_failure_rolls_back_instead_of_reporting_success() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = SyncTagRollbackRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
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

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["failureStep"].as_str(),
        Some("sync_configured_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("sync_configured_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(*runner.step.lock().unwrap(), 12);
}

#[tokio::test]
async fn explicit_target_digest_still_syncs_tag_based_service() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = ExplicitTargetDigestSyncRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:explicit", &[]);

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(
        outcome.summary_json["targetTagsPulled"],
        json!(["ghcr.io/org/web:1.0"])
    );
    assert_eq!(*runner.step.lock().unwrap(), 9);
}

#[tokio::test]
async fn digest_pinned_service_skips_local_tag_sync() {
    let stack = single_service_stack("ghcr.io/org/web@sha256:old", None);
    let runner = DigestPinnedRunner::default();

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
    assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
    assert_eq!(*runner.step.lock().unwrap(), 7);
}

#[tokio::test]
async fn explicit_targets_must_cover_selected_services_at_execution_time() {
    let mut stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("1.0".to_string()),
            digest: "sha256:new1".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let mut worker = stack.services[0].clone();
    worker.id = "svc_2".to_string();
    worker.name = "worker".to_string();
    worker.image.reference = "ghcr.io/org/worker:2.0".to_string();
    worker.image.tag = "2.0".to_string();
    worker.candidate = Some(Candidate {
        tag: "2.0".to_string(),
        resolved_tag: Some("2.0".to_string()),
        digest: "sha256:new2".to_string(),
        arch_match: ArchMatch::Match,
        arch: vec!["linux/amd64".to_string()],
    });
    stack.services.push(worker);

    let runner = FakeRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new1", &[]);

    let err = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .expect_err("missing explicit target should fail before executing update commands");

    assert!(
        err.to_string()
            .contains("explicit update targets no longer cover selected services: svc_2")
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stack_update_pulls_target_tag_before_sync_and_compatibility_tags_afterwards() {
    let stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("0.7.7".to_string()),
            digest: "sha256:candidate".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let runner = SyncBeforeSemverRunner::default();
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:candidate", &["v0.7.7"]);

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(
        outcome.summary_json["targetTagsPulled"],
        json!(["ghcr.io/org/web:1.0"])
    );
    assert_eq!(
        outcome.summary_json["pullTagsPulled"],
        json!(["ghcr.io/org/web:v0.7.7"])
    );
    let calls = runner.calls.lock().unwrap();
    let target_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args == &vec!["pull".to_string(), "ghcr.io/org/web:1.0".to_string()]
        })
        .expect("target tag pull should exist");
    let sync_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args
                    == &vec![
                        "image".to_string(),
                        "tag".to_string(),
                        "sha256:new".to_string(),
                        "ghcr.io/org/web:1.0".to_string(),
                    ]
        })
        .expect("sync tag command should exist");
    let compat_idx = calls
        .iter()
        .position(|(program, args)| {
            program == "docker"
                && args == &vec!["pull".to_string(), "ghcr.io/org/web:v0.7.7".to_string()]
        })
        .expect("compatibility tag pull should exist");
    assert!(target_idx < sync_idx);
    assert!(sync_idx < compat_idx);
}

#[test]
fn parse_pull_fraction_supports_size_ratio_tokens() {
    let line = "d2cad1f9f7c9 Downloading [==================> ] 3.146MB/5.89MB";
    let frac = parse_pull_fraction_from_line(line).unwrap();
    assert!(frac > 0.50 && frac < 0.60);

    let full =
        "9b4e5f7f3558 Downloading [==================================================>] 443B/443B";
    let full_frac = parse_pull_fraction_from_line(full).unwrap();
    assert!((full_frac - 1.0).abs() < f64::EPSILON);
}

#[derive(Default)]
struct FlakyInspectRunner {
    calls: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for FlakyInspectRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls < 3 {
            Ok(CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "transient".to_string(),
            })
        } else {
            Ok(CommandOutput {
                status: 0,
                stdout: "ok\n".to_string(),
                stderr: String::new(),
            })
        }
    }
}

#[tokio::test]
async fn run_to_string_with_retry_succeeds_after_transient_failures() {
    let runner = FlakyInspectRunner::default();
    let got = run_to_string_with_retry(
        &runner,
        CommandSpec {
            program: "docker".to_string(),
            args: vec!["inspect".to_string()],
            env: Vec::new(),
        },
        Duration::from_millis(100),
        "inspect_image_id",
        IdempotentRetryPolicy {
            max_attempts: 3,
            base_ms: 1,
            max_ms: 2,
        },
    )
    .await
    .expect("third attempt should succeed");
    assert_eq!(got.trim(), "ok");
    assert_eq!(*runner.calls.lock().unwrap(), 3);
}

#[derive(Default)]
struct FailUpRunner {
    up_calls: Mutex<usize>,
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for FailUpRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "c_before\n".to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old\n".to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => {
                if args_end_with(&spec.args, &["up", "-d", "web"]) {
                    *self.up_calls.lock().unwrap() += 1;
                }
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "up failed".to_string(),
                }
            }
            _ => CommandOutput {
                status: 1,
                stdout: String::new(),
                stderr: "unexpected extra command".to_string(),
            },
        };
        *step += 1;
        Ok(out)
    }
}

#[tokio::test]
async fn up_command_is_not_retried_when_it_fails() {
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

    let runner = FailUpRunner::default();
    let err = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 5,
            base_ms: 1,
            max_ms: 2,
        },
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
    .expect_err("up -d failure should abort immediately without retries");
    assert!(err.to_string().contains("command failed"));
    assert_eq!(*runner.up_calls.lock().unwrap(), 1);
}

#[derive(Default)]
struct CompatibilityTagWarningRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for CompatibilityTagWarningRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container
"
                .to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old
"
                .to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container
"
                .to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0
"
                .to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new
"
                .to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:1.0"]
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
            8 => {
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
            9 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:v0.7.7"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "compat tag missing".to_string(),
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
async fn compatibility_tag_pull_failures_only_record_warnings() {
    let stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("1.0".to_string()),
            digest: "sha256:new".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &["v0.7.7"]);
    let runner = CompatibilityTagWarningRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(
        outcome.summary_json["targetTagsPulled"],
        json!(["ghcr.io/org/web:1.0"])
    );
    assert_eq!(outcome.summary_json["pullTagsPulled"], json!([]));
    assert_eq!(outcome.summary_json["semverPulled"], json!([]));
    let warnings = outcome.summary_json["pullTagWarnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["tagRef"], json!("ghcr.io/org/web:v0.7.7"));
    assert_eq!(warnings[0]["step"], json!("pull_tag"));
}

#[derive(Default)]
struct TargetTagPullRollbackRunner {
    step: Mutex<usize>,
}

#[async_trait::async_trait]
impl CommandRunner for TargetTagPullRollbackRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        let mut step = self.step.lock().unwrap();
        let out = match *step {
            0 => CommandOutput {
                status: 0,
                stdout: "old_container
"
                .to_string(),
                stderr: String::new(),
            },
            1 => CommandOutput {
                status: 0,
                stdout: "sha256:old
"
                .to_string(),
                stderr: String::new(),
            },
            2 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            3 => CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            4 => CommandOutput {
                status: 0,
                stdout: "new_container
"
                .to_string(),
                stderr: String::new(),
            },
            5 => CommandOutput {
                status: 0,
                stdout: "0
"
                .to_string(),
                stderr: String::new(),
            },
            6 => CommandOutput {
                status: 0,
                stdout: "sha256:new
"
                .to_string(),
                stderr: String::new(),
            },
            7 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["pull", "ghcr.io/org/web:1.0"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: "target tag missing".to_string(),
                }
            }
            8 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["image", "tag", "sha256:old", "ghcr.io/org/web:1.0"]
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
            9 => {
                assert_eq!(spec.program, "docker-compose");
                assert!(args_end_with(
                    &spec.args,
                    &["up", "-d", "--pull", "never", "web"]
                ));
                CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            }
            10 => CommandOutput {
                status: 0,
                stdout: "rollback_container
"
                .to_string(),
                stderr: String::new(),
            },
            11 => {
                assert_eq!(spec.program, "docker");
                assert_eq!(
                    spec.args,
                    vec!["inspect", "--format", "{{.Image}}", "rollback_container"]
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                );
                CommandOutput {
                    status: 0,
                    stdout: "sha256:old
"
                    .to_string(),
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
async fn target_tag_pull_failure_rolls_back_with_explicit_failure_step() {
    let stack = single_service_stack(
        "ghcr.io/org/web:1.0",
        Some(Candidate {
            tag: "1.0".to_string(),
            resolved_tag: Some("1.0".to_string()),
            digest: "sha256:new".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let explicit_targets = explicit_targets("svc_1", "1.0", "sha256:new", &[]);
    let runner = TargetTagPullRollbackRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy {
            max_attempts: 1,
            base_ms: 1,
            max_ms: 2,
        },
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        Some(explicit_targets.as_slice()),
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "rolled_back");
    assert_eq!(
        outcome.summary_json["failureStep"],
        json!("pull_target_tag")
    );
    assert_eq!(outcome.summary_json["targetTagsPulled"], json!([]));
    assert_eq!(
        outcome.summary_json["newDigests"]["svc_1"],
        json!("sha256:new")
    );
    assert_eq!(
        outcome.summary_json["finalDigests"]["svc_1"],
        json!("sha256:old")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["trigger"],
        json!("pull_target_tag")
    );
    assert_eq!(
        outcome.summary_json["rollback"]["toDigests"]["svc_1"],
        json!("sha256:old")
    );
}

#[test]
fn strip_tag_and_digest_handles_digest_only_refs() {
    assert_eq!(
        strip_tag_and_digest("alpine@sha256:deadbeef"),
        Some("alpine".to_string())
    );
    assert_eq!(
        strip_tag_and_digest("ghcr.io/org/web@sha256:deadbeef"),
        Some("ghcr.io/org/web".to_string())
    );
}
