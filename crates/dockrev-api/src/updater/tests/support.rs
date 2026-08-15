use super::*;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct FakeRunner {
    pub(super) calls: Mutex<Vec<(String, Vec<String>)>>,
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

pub(super) fn args_end_with(args: &[String], suffix: &[&str]) -> bool {
    if args.len() < suffix.len() {
        return false;
    }
    let start = args.len() - suffix.len();
    suffix
        .iter()
        .enumerate()
        .all(|(i, s)| args[start + i] == *s)
}

pub(super) fn single_service_stack(
    image_reference: &str,
    candidate: Option<Candidate>,
) -> StackRecord {
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
            homepage: None,
            update_guard: None,
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

pub(super) fn explicit_targets(
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

pub(super) fn write_test_docker_config() -> (PathBuf, TempDirCleanup) {
    let root = std::env::temp_dir().join(format!("dockrev-test-auth-{}", ulid::Ulid::new()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("docker-config.custom.json");
    fs::write(&path, r#"{"auths":{"ghcr.io":{"auth":"Zm9vOmJhcg=="}}}"#).unwrap();
    (path, TempDirCleanup(root))
}

pub(super) fn write_test_default_named_docker_config() -> (PathBuf, TempDirCleanup) {
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
pub(super) fn docker_cli_auth_bridge_stages_custom_config_as_config_json() {
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
pub(super) fn docker_cli_auth_bridge_copies_context_metadata_for_real_config_json() {
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
pub(super) fn docker_cli_auth_bridge_handles_read_only_real_config_json() {
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
pub(super) struct EnvCaptureUpdateRunner {
    pub(super) step: Mutex<usize>,
    pub(super) specs: Mutex<Vec<CommandSpec>>,
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

pub(super) fn selection_test_service(id: &str, name: &str, image_reference: &str) -> Service {
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
        homepage: None,
        update_guard: None,
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
