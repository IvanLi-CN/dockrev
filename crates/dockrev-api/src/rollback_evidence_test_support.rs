use super::*;
use crate::runner::{CommandOutput, CommandSpec, RawCommandOutput, RawFileCommandOutput};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn cleanup_preserves_evidence_when_job_lookup_fails() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-rollback-evidence-lookup-error-{}",
        ulid::Ulid::new()
    ));
    fs::create_dir_all(&root).expect("test root");
    let archive = root.join("job.tar.zst");
    tokio::fs::write(&archive, b"recoverable archive")
        .await
        .expect("archive");

    cleanup_orphaned_entry(
        &archive,
        false,
        Err(anyhow::anyhow!("temporary database read failure")),
    )
    .await;

    assert_eq!(
        tokio::fs::read(&archive).await.expect("preserved archive"),
        b"recoverable archive"
    );
    let spool = root.join("job-spool");
    tokio::fs::create_dir_all(&spool).await.expect("spool");
    let marker = spool.join("container.log.part");
    tokio::fs::write(&marker, b"partial raw logs")
        .await
        .expect("partial log");
    let sibling_archive = spool.with_extension("tar.zst");
    tokio::fs::write(&sibling_archive, b"recoverable spool archive")
        .await
        .expect("spool archive");

    cleanup_orphaned_entry(
        &spool,
        true,
        Err(anyhow::anyhow!("temporary database read failure")),
    )
    .await;

    assert_eq!(
        tokio::fs::read(&marker).await.expect("preserved spool"),
        b"partial raw logs"
    );
    assert_eq!(
        tokio::fs::read(&sibling_archive)
            .await
            .expect("preserved spool archive"),
        b"recoverable spool archive"
    );
    let _ = tokio::fs::remove_dir_all(root).await;
}

pub(super) async fn archive_member(archive: &Path, member: &str) -> Vec<u8> {
    let decompressed = tokio::process::Command::new("zstd")
        .arg("-dc")
        .arg(archive)
        .output()
        .await
        .expect("zstd should read evidence archive");
    assert!(decompressed.status.success());
    let mut tar = tokio::process::Command::new("tar")
        .args(["-xOf", "-", member])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("tar should read evidence archive");
    let mut tar_stdin = tar.stdin.take().expect("tar stdin");
    let tar_bytes = decompressed.stdout;
    let write_task = tokio::spawn(async move {
        tar_stdin.write_all(&tar_bytes).await?;
        anyhow::Ok(())
    });
    let extracted = tar
        .wait_with_output()
        .await
        .expect("tar extraction should complete");
    let write_result = write_task.await.expect("tar input task");
    assert!(extracted.status.success());
    if let Err(error) = write_result {
        assert!(
            error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::BrokenPipe),
            "tar input should be written unless extraction closes the pipe early: {error}"
        );
    }
    extracted.stdout
}

pub(super) struct FailedLogCommandRunner;

#[async_trait::async_trait]
impl CommandRunner for FailedLogCommandRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    async fn run_raw(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<RawCommandOutput> {
        Ok(RawCommandOutput {
            status: 0,
            stdout: br#"{"Status":"running","Health":{"Log":[]}}"#.to_vec(),
            stderr: Vec::new(),
        })
    }

    async fn run_raw_to_file(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
        output_path: &Path,
    ) -> anyhow::Result<RawFileCommandOutput> {
        let partial = b"partial raw output before nonzero exit\xff";
        tokio::fs::write(output_path, partial).await?;
        Ok(RawFileCommandOutput {
            status: 2,
            bytes_written: partial.len() as u64,
            stderr: Vec::new(),
            eof_reached: true,
            timed_out: false,
        })
    }
}

#[tokio::test]
async fn nonzero_log_exit_keeps_partial_file_and_records_incomplete_status() {
    let root =
        std::env::temp_dir().join(format!("dockrev-rollback-log-exit-{}", ulid::Ulid::new()));
    fs::create_dir_all(&root).expect("test root");
    let context =
        RollbackEvidenceContext::new("job-log-exit", &root.join("dockrev.sqlite")).expect("spool");

    let metadata = context
        .capture_failure(
            &FailedLogCommandRunner,
            &DockerRunnerConfig::default(),
            "service-a",
            "candidate-a",
            "unhealthy",
            None,
            None,
        )
        .await;

    assert!(metadata.logs_truncated);
    assert!(
        metadata
            .capture_errors
            .iter()
            .any(|error| error == "logs command exited with 2")
    );
    assert_eq!(
        tokio::fs::read(
            context
                .job_spool_path()
                .join("service-a/candidate-a/container.log")
        )
        .await
        .expect("partial log should be retained"),
        b"partial raw output before nonzero exit\xff"
    );

    let _ = tokio::fs::remove_dir_all(root).await;
}

struct ManifestCheckpointFailureRunner {
    raw_command_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl CommandRunner for ManifestCheckpointFailureRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.raw_command_calls.fetch_add(1, Ordering::SeqCst);
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    async fn run_raw(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<RawCommandOutput> {
        self.raw_command_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RawCommandOutput {
            status: 0,
            stdout: br#"{"Status":"running","Health":{"Log":[]}}"#.to_vec(),
            stderr: Vec::new(),
        })
    }
}

#[tokio::test]
async fn manifest_checkpoint_failure_skips_raw_candidate_log_command() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-rollback-checkpoint-failure-{}",
        ulid::Ulid::new()
    ));
    fs::create_dir_all(&root).expect("test root");
    let context = RollbackEvidenceContext::new("job-checkpoint", &root.join("dockrev.sqlite"))
        .expect("spool");
    tokio::fs::create_dir_all(context.job_spool_path())
        .await
        .expect("job spool");
    tokio::fs::create_dir(context.job_spool_path().join("manifest.json"))
        .await
        .expect("block manifest rename");
    let runner = ManifestCheckpointFailureRunner {
        raw_command_calls: AtomicUsize::new(0),
    };

    let metadata = context
        .capture_failure(
            &runner,
            &DockerRunnerConfig::default(),
            "service-a",
            "candidate-a",
            "unhealthy",
            None,
            None,
        )
        .await;

    assert!(metadata.logs_truncated);
    assert!(
        metadata
            .capture_errors
            .iter()
            .any(|error| error.starts_with("manifest checkpoint:"))
    );
    assert_eq!(runner.raw_command_calls.load(Ordering::SeqCst), 0);
    assert!(
        !context
            .job_spool_path()
            .join("service-a/candidate-a/container.log.part")
            .exists()
    );

    let _ = tokio::fs::remove_dir_all(root).await;
}

struct FinalManifestFailureRunner {
    manifest_tmp_path: PathBuf,
}

#[async_trait::async_trait]
impl CommandRunner for FinalManifestFailureRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    async fn run_raw(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<RawCommandOutput> {
        Ok(RawCommandOutput {
            status: 0,
            stdout: br#"{"Status":"running","Health":{"Log":[]}}"#.to_vec(),
            stderr: Vec::new(),
        })
    }

    async fn run_raw_to_file(
        &self,
        _spec: CommandSpec,
        _timeout: Duration,
        output_path: &Path,
    ) -> anyhow::Result<RawFileCommandOutput> {
        let log = b"complete candidate log";
        tokio::fs::write(output_path, log).await?;
        tokio::fs::create_dir(&self.manifest_tmp_path).await?;
        Ok(RawFileCommandOutput {
            status: 0,
            bytes_written: log.len() as u64,
            stderr: Vec::new(),
            eof_reached: true,
            timed_out: false,
        })
    }
}

#[tokio::test]
async fn final_manifest_write_failure_does_not_attach_an_archive_with_stale_metadata() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-rollback-final-manifest-failure-{}",
        ulid::Ulid::new()
    ));
    fs::create_dir_all(&root).expect("test root");
    let db_path = root.join("dockrev.sqlite");
    let db = crate::db::Db::open(&db_path).await.expect("db");
    let job_id = "job-final-manifest-failure";
    db.insert_job(
        crate::api::types::JobRecord::new_running(
            job_id.to_string(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            None,
            None,
            "2026-08-28T00:00:00Z",
        )
        .to_db(),
    )
    .await
    .expect("insert job");
    let evidence = RollbackEvidenceContext::new(job_id, &db_path).expect("evidence spool");
    let manifest_tmp_path = evidence.job_spool_path().join("manifest.tmp");
    let metadata = evidence
        .capture_failure(
            &FinalManifestFailureRunner { manifest_tmp_path },
            &DockerRunnerConfig::default(),
            "service-a",
            "candidate-a",
            "unhealthy",
            None,
            None,
        )
        .await;
    assert!(!metadata.logs_truncated);
    assert!(
        metadata
            .capture_errors
            .iter()
            .any(|error| error.starts_with("manifest update:"))
    );

    let evidence_summary = evidence.finalize().await;
    assert_eq!(evidence_summary.status, "incomplete");
    assert_eq!(evidence_summary.archive_size_bytes, None);
    assert!(
        evidence_summary
            .errors
            .iter()
            .any(|error| error.starts_with("manifest:"))
    );
    let archive_path = (evidence_summary.status == "available").then(|| evidence.archive_path());
    let mut summary = serde_json::json!({"rollbackEvidence": evidence_summary});
    crate::rollback_evidence_finalize::finish_job_with_evidence_archive(
        &db,
        job_id,
        "rolled_back",
        "2026-08-28T00:05:00Z",
        &mut summary,
        archive_path,
        Some(&evidence),
        None,
        None,
    )
    .await
    .expect("terminal job should finish without attaching stale evidence");

    let job = db.get_job(job_id).await.expect("load job").expect("job");
    assert_eq!(job.status, "rolled_back");
    assert_eq!(job.summary_json["rollbackEvidence"]["status"], "incomplete");
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load archive")
            .is_none()
    );
    assert!(!evidence.archive_path().exists());

    tokio::fs::remove_dir(evidence.job_spool_path().join("manifest.tmp"))
        .await
        .expect("remove manifest failure trigger before simulated restart");
    crate::rollback_evidence::recover_orphaned_evidence(&db, &db_path).await;

    let recovered_job = db.get_job(job_id).await.expect("reload job").expect("job");
    assert_eq!(
        recovered_job.summary_json["rollbackEvidence"]["status"],
        "incomplete"
    );
    assert!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load recovered archive")
            .is_none()
    );

    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn recovery_cleans_spool_without_overwriting_an_existing_archive() {
    let root = std::env::temp_dir().join(format!(
        "dockrev-rollback-preserve-archive-{}",
        ulid::Ulid::new()
    ));
    fs::create_dir_all(&root).expect("test root");
    let db_path = root.join("dockrev.sqlite");
    let db = crate::db::Db::open(&db_path).await.expect("db");
    let job_id = "job-existing-archive";
    db.insert_job(
        crate::api::types::JobRecord::new_running(
            job_id.to_string(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            None,
            None,
            "2026-08-28T00:00:00Z",
        )
        .to_db(),
    )
    .await
    .expect("insert job");
    let original_archive = b"previously committed archive bytes";
    db.finish_job_with_archive(
        job_id,
        "rolled_back",
        "2026-08-28T00:05:00Z",
        &serde_json::json!({
            "rollbackEvidence": {
                "status": "available",
                "archiveSizeBytes": original_archive.len()
            }
        }),
        Some(original_archive.to_vec()),
    )
    .await
    .expect("commit archive");

    let spool = spool_root(&db_path).join(job_id);
    tokio::fs::create_dir_all(spool.join("service-a/candidate-a"))
        .await
        .expect("spool");
    write_manifest(
        &spool,
        &[EvidenceMetadata {
            service_id: "service-a".to_string(),
            candidate_id: "candidate-a".to_string(),
            health_status: "unhealthy".to_string(),
            ..Default::default()
        }],
    )
    .await
    .expect("manifest");

    recover_orphaned_evidence(&db, &db_path).await;

    assert_eq!(
        db.get_rollback_evidence_archive(job_id)
            .await
            .expect("load archive")
            .expect("archive exists"),
        original_archive
    );
    assert!(!spool.exists());

    for (suffix, manifest) in [
        ("missing-manifest", None),
        ("corrupt-manifest", Some(&b"{ invalid"[..])),
    ] {
        let job_id = format!("job-existing-archive-{suffix}");
        db.insert_job(
            crate::api::types::JobRecord::new_running(
                job_id.clone(),
                crate::api::types::JobType::Update,
                crate::api::types::JobScope::Service,
                None,
                None,
                "2026-08-28T00:00:00Z",
            )
            .to_db(),
        )
        .await
        .expect("insert job");
        let original_archive = format!("committed archive {suffix}").into_bytes();
        db.finish_job_with_archive(
            &job_id,
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "available",
                    "archiveSizeBytes": original_archive.len()
                }
            }),
            Some(original_archive.clone()),
        )
        .await
        .expect("commit archive");

        let spool = spool_root(&db_path).join(&job_id);
        let candidate = spool.join("service-a/candidate-a");
        tokio::fs::create_dir_all(&candidate).await.expect("spool");
        tokio::fs::write(candidate.join("container.log"), b"raw candidate logs")
            .await
            .expect("candidate logs");
        if let Some(manifest) = manifest {
            tokio::fs::write(spool.join("manifest.json"), manifest)
                .await
                .expect("manifest");
        }
        let archive_path = spool.with_extension("tar.zst");
        let partial_archive_path = spool.with_extension("tar.zst.part");
        tokio::fs::write(&archive_path, b"local archive copy")
            .await
            .expect("local archive");
        tokio::fs::write(&partial_archive_path, b"partial archive")
            .await
            .expect("partial archive");

        recover_orphaned_evidence(&db, &db_path).await;

        assert_eq!(
            db.get_rollback_evidence_archive(&job_id)
                .await
                .expect("load archive")
                .expect("archive exists"),
            original_archive
        );
        let recovered_job = db
            .get_job(&job_id)
            .await
            .expect("load job")
            .expect("job exists");
        assert_eq!(
            recovered_job.summary_json["rollbackEvidence"]["status"],
            "available"
        );
        assert!(!spool.exists());
        assert!(!archive_path.exists());
        assert!(!partial_archive_path.exists());
    }

    let _ = tokio::fs::remove_dir_all(root).await;
}
