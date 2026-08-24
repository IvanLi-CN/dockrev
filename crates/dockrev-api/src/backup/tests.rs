use super::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FakeRecoveryStore;

#[async_trait::async_trait]
impl BackupRecoveryStore for FakeRecoveryStore {
    async fn save(&self, _snapshot: &BackupRecoverySnapshot) -> anyhow::Result<()> {
        Ok(())
    }

    async fn clear(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakeRunner {
    sizes: BTreeMap<String, u64>,
    calls: Arc<Mutex<Vec<CommandSpec>>>,
}

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        self.calls.lock().unwrap().push(spec.clone());
        if (spec.program == "docker-compose" || spec.program == "docker")
            && spec
                .args
                .iter()
                .any(|arg| arg == "stop" || arg == "up" || arg == "ps")
        {
            return Ok(crate::runner::CommandOutput {
                status: 0,
                stdout: "container-id\n".to_string(),
                stderr: String::new(),
            });
        }
        if spec.program == "docker" && spec.args.first().is_some_and(|a| a == "run") {
            if spec.args.iter().any(|a| a.contains("du -sb /data")) {
                let mount = spec
                    .args
                    .windows(2)
                    .find(|w| w[0] == "-v")
                    .map(|w| w[1].clone())
                    .unwrap_or_default();
                let key = mount.split(':').next().unwrap_or_default().to_string();
                let bytes = self.sizes.get(&key).copied().unwrap_or(0);
                return Ok(crate::runner::CommandOutput {
                    status: 0,
                    stdout: format!("{bytes}\n"),
                    stderr: String::new(),
                });
            }

            if spec.args.iter().any(|arg| arg == "backup-helper")
                && let Some(out_mount) = spec
                    .args
                    .windows(2)
                    .find(|w| w[0] == "-v" && w[1].ends_with(":/out-root"))
                    .map(|w| w[1].clone())
            {
                let host_dir = out_mount.split(':').next().unwrap_or_default();
                let output_final = spec
                    .args
                    .windows(2)
                    .find(|w| w[0] == "--output-final")
                    .map(|w| w[1].trim_start_matches("/out-root/"))
                    .unwrap();
                let path = PathBuf::from(host_dir).join(output_final);
                tokio::fs::create_dir_all(path.parent().unwrap()).await?;
                tokio::fs::write(&path, vec![0u8; 10]).await?;
                return Ok(crate::runner::CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
        }

        Ok(crate::runner::CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn test_stack(targets: Vec<BackupTarget>) -> StackRecord {
    StackRecord {
        id: "stk_test".to_string(),
        name: "demo".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/tmp/compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig {
            targets,
            retention: Default::default(),
        },
        services: vec![crate::api::types::Service {
            id: "svc_test".to_string(),
            name: "web".to_string(),
            image: crate::api::types::ComposeRef {
                reference: "ghcr.io/acme/web:5.2".to_string(),
                tag: "5.2".to_string(),
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
            settings: crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    }
}

#[tokio::test]
async fn failed_apply_recovery_restores_managed_override_before_up() {
    let root = std::env::temp_dir().join(format!("dockrev-backup-recovery-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&root).unwrap();
    let path = crate::managed_override::managed_override_path(&root, "stk_test");
    let old = "services:\n  web:\n    image: ghcr.io/acme/web@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n";
    let next = "services:\n  web:\n    image: ghcr.io/acme/web@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
    crate::managed_override::commit_with_snapshot(&path, old).unwrap();
    crate::managed_override::commit_with_snapshot(&path, next).unwrap();

    let runner = FakeRunner::default();
    restore_services_after_failed_apply(
        &runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root,
        &["web".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
    assert!(
        runner
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|spec| spec.args.iter().any(|arg| arg == "up"))
    );
    assert!(!crate::managed_override::has_pending_snapshot(&path));

    let root_without_pending = std::env::temp_dir().join(format!(
        "dockrev-backup-recovery-no-pending-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root_without_pending).unwrap();
    let stale_path =
        crate::managed_override::managed_override_path(&root_without_pending, "stk_test");
    crate::managed_override::atomic_commit(&stale_path, next).unwrap();
    crate::managed_override::atomic_commit(
        &PathBuf::from(format!("{}.previous", stale_path.display())),
        old,
    )
    .unwrap();
    restore_services_after_failed_apply(
        &runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root_without_pending,
        &["web".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read_to_string(&stale_path).unwrap(), next);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(root_without_pending).unwrap();

    let root_without_active = std::env::temp_dir().join(format!(
        "dockrev-backup-recovery-missing-active-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root_without_active).unwrap();
    let missing_active_path =
        crate::managed_override::managed_override_path(&root_without_active, "stk_test");
    crate::managed_override::commit_with_snapshot(&missing_active_path, old).unwrap();
    crate::managed_override::commit_with_snapshot(&missing_active_path, next).unwrap();
    std::fs::remove_file(&missing_active_path).unwrap();
    restore_services_after_failed_apply(
        &runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root_without_active,
        &["web".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read_to_string(&missing_active_path).unwrap(), old);
    assert!(!crate::managed_override::has_pending_snapshot(
        &missing_active_path
    ));
    std::fs::remove_dir_all(root_without_active).unwrap();

    let root_with_applied = std::env::temp_dir().join(format!(
        "dockrev-backup-recovery-applied-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root_with_applied).unwrap();
    let applied_path =
        crate::managed_override::managed_override_path(&root_with_applied, "stk_test");
    crate::managed_override::commit_with_snapshot(&applied_path, old).unwrap();
    crate::managed_override::commit_with_snapshot(&applied_path, next).unwrap();
    crate::managed_override::mark_snapshot_applied(&applied_path).unwrap();
    let applied_runner = FakeRunner::default();
    restore_services_after_failed_apply(
        &applied_runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root_with_applied,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read_to_string(&applied_path).unwrap(), next);
    assert!(!crate::managed_override::has_pending_snapshot(
        &applied_path
    ));
    assert!(applied_runner.calls.lock().unwrap().is_empty());
    std::fs::remove_dir_all(root_with_applied).unwrap();

    let root_with_applied_services = std::env::temp_dir().join(format!(
        "dockrev-backup-recovery-applied-services-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root_with_applied_services).unwrap();
    let applied_services_path =
        crate::managed_override::managed_override_path(&root_with_applied_services, "stk_test");
    crate::managed_override::commit_with_snapshot(&applied_services_path, old).unwrap();
    crate::managed_override::commit_with_snapshot(&applied_services_path, next).unwrap();
    crate::managed_override::mark_snapshot_applied(&applied_services_path).unwrap();
    let applied_services_runner = FakeRunner::default();
    restore_services_after_failed_apply(
        &applied_services_runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root_with_applied_services,
        &["web".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&applied_services_path).unwrap(),
        next
    );
    assert!(!crate::managed_override::has_pending_snapshot(
        &applied_services_path
    ));
    {
        let calls = applied_services_runner.calls.lock().unwrap();
        let up_call = calls
            .iter()
            .find(|spec| spec.args.iter().any(|arg| arg == "up"))
            .expect("applied recovery should restart services");
        assert!(
            up_call
                .args
                .iter()
                .any(|arg| arg == &applied_services_path.to_string_lossy())
        );
        assert!(up_call.args.iter().any(|arg| arg == "--pull"));
        assert!(up_call.args.iter().any(|arg| arg == "never"));
        assert!(up_call.args.iter().any(|arg| arg == "--no-deps"));
        assert!(up_call.args.iter().any(|arg| arg == "--force-recreate"));
    }
    std::fs::remove_dir_all(root_with_applied_services).unwrap();

    let root_with_missing_applied = std::env::temp_dir().join(format!(
        "dockrev-backup-recovery-missing-applied-{}",
        ulid::Ulid::new()
    ));
    std::fs::create_dir_all(&root_with_missing_applied).unwrap();
    let missing_applied_path =
        crate::managed_override::managed_override_path(&root_with_missing_applied, "stk_test");
    crate::managed_override::commit_with_snapshot(&missing_applied_path, old).unwrap();
    crate::managed_override::commit_with_snapshot(&missing_applied_path, next).unwrap();
    crate::managed_override::mark_snapshot_applied(&missing_applied_path).unwrap();
    std::fs::remove_file(&missing_applied_path).unwrap();
    let missing_applied_runner = FakeRunner::default();
    let error = restore_services_after_failed_apply(
        &missing_applied_runner,
        "docker-compose",
        None,
        &test_stack(Vec::new()),
        &root_with_missing_applied,
        &["web".to_string()],
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("no active override"));
    assert!(crate::managed_override::has_pending_snapshot(
        &missing_applied_path
    ));
    assert!(missing_applied_runner.calls.lock().unwrap().is_empty());
    std::fs::remove_dir_all(root_with_missing_applied).unwrap();
}

#[tokio::test]
async fn backup_skips_over_threshold_for_inherit() {
    let tmp = std::env::temp_dir()
        .join(format!("dockrev-backup-test-{}", ulid::Ulid::new()))
        .to_string_lossy()
        .to_string();
    let settings = BackupSettings {
        enabled: true,
        require_success: true,
        base_dir: tmp.clone(),
        skip_targets_over_bytes: 100,
    };

    let runner = FakeRunner {
        sizes: BTreeMap::from([("big".to_string(), 1000)]),
        ..Default::default()
    };

    let stack = test_stack(vec![BackupTarget::DockerVolume {
        name: "big".to_string(),
    }]);

    let out = run_pre_update_backup(
        &runner,
        &runner,
        &runner,
        &FakeRecoveryStore,
        &settings,
        Path::new(&tmp).join("dockrev.sqlite").as_path(),
        "ghcr.io/ivanli-cn/dockrev:latest",
        "backup-test",
        "job-test",
        "docker-compose",
        None,
        &stack,
        &JobScope::Stack,
        None,
        &[],
        "2026-01-19T00:00:00Z",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.status, "skipped");
}

#[tokio::test]
async fn backup_includes_force_over_threshold() {
    let tmp = std::env::temp_dir()
        .join(format!("dockrev-backup-test-{}", ulid::Ulid::new()))
        .to_string_lossy()
        .to_string();
    let settings = BackupSettings {
        enabled: true,
        require_success: true,
        base_dir: tmp.clone(),
        skip_targets_over_bytes: 100,
    };

    let runner = FakeRunner {
        sizes: BTreeMap::from([("big".to_string(), 1000)]),
        ..Default::default()
    };

    let mut stack = test_stack(vec![BackupTarget::DockerVolume {
        name: "big".to_string(),
    }]);
    stack.services[0]
        .settings
        .backup_targets
        .volume_names
        .insert("big".to_string(), crate::api::types::TernaryChoice::Force);

    let out = run_pre_update_backup(
        &runner,
        &runner,
        &runner,
        &FakeRecoveryStore,
        &settings,
        Path::new(&tmp).join("dockrev.sqlite").as_path(),
        "ghcr.io/ivanli-cn/dockrev:latest",
        "backup-test",
        "job-test",
        "docker-compose",
        None,
        &stack,
        &JobScope::Stack,
        None,
        &["web".to_string()],
        "2026-01-19T00:00:00Z",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.status, "success");
    assert!(out.artifact_path.as_deref().unwrap().ends_with(".tar.zst"));
    assert_eq!(out.services_kept_stopped, ["web"]);
    let calls = runner.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|spec| spec.args.iter().any(|arg| arg == "stop"))
    );
    assert!(
        !calls
            .iter()
            .any(|spec| spec.args.iter().any(|arg| arg == "up"))
    );
    assert_eq!(out.size_bytes, Some(10));
}

#[test]
fn legacy_artifact_key_rejects_parent_traversal() {
    let storage = crate::backup_storage::BackupStorage::Local {
        logical_root: PathBuf::from("/data/backups"),
    };
    assert_eq!(
        legacy_artifact_key(&storage, "/data/backups/stack/archive.tar.gz"),
        Some(PathBuf::from("stack/archive.tar.gz"))
    );
    assert_eq!(
        legacy_artifact_key(&storage, "/data/backups/../important/file"),
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_delete_rejects_symlink_escape() {
    let root = std::env::temp_dir().join(format!("dockrev-cleanup-test-{}", ulid::Ulid::new()));
    let managed = root.join("backups");
    let outside = root.join("outside");
    tokio::fs::create_dir_all(&managed).await.unwrap();
    tokio::fs::create_dir_all(&outside).await.unwrap();
    tokio::fs::write(outside.join("keep.tar.gz"), b"keep")
        .await
        .unwrap();
    std::os::unix::fs::symlink(&outside, managed.join("escaped")).unwrap();

    let storage = crate::backup_storage::BackupStorage::Local {
        logical_root: managed,
    };
    let error = delete_artifact(
        &FakeRunner::default(),
        &storage,
        "unused",
        Path::new("escaped/keep.tar.gz"),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("outside managed storage"));
    assert!(outside.join("keep.tar.gz").exists());
    std::fs::remove_dir_all(root).unwrap();
}
