#[tokio::test]
async fn cleanup_once_keeps_latest_and_removes_due_backup_without_stack_health_gate() {
    let state = test_state(":memory:").await;
    let (stack_id, service_id, _compose_path) = seed_manual_rollback_service(&state).await;
    let now = test_now_rfc3339();
    let latest_created_at = test_offset_rfc3339(&now, time::Duration::hours(-1));
    let old_created_at = test_offset_rfc3339(&now, time::Duration::hours(-2));
    let cleanup_after = test_offset_rfc3339(&now, time::Duration::hours(-1));

    insert_update_job_with_summary(
        &state,
        "job-cleanup-latest",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        json!({ "targets": [{"serviceId": service_id}] }),
        &latest_created_at,
    )
    .await;
    insert_update_job_with_summary(
        &state,
        "job-cleanup-old",
        crate::api::types::JobScope::Service,
        Some(&stack_id),
        Some(&service_id),
        json!({ "targets": [{"serviceId": service_id}] }),
        &old_created_at,
    )
    .await;

    let storage_root = crate::backup_storage::logical_backup_root(&state.config.db_path).unwrap();
    let artifact_dir = storage_root.join(&stack_id);
    tokio::fs::create_dir_all(&artifact_dir).await.unwrap();
    let latest_path = artifact_dir.join("latest.tar.zst");
    let old_path = artifact_dir.join("old.tar.zst");
    tokio::fs::write(&latest_path, b"latest").await.unwrap();
    tokio::fs::write(&old_path, b"old").await.unwrap();

    insert_backup_record(
        &state,
        "backup-cleanup-latest",
        &stack_id,
        "job-cleanup-latest",
        &latest_created_at,
        "success",
        Some(latest_path.to_str().unwrap()),
        Some(6),
        None,
        Some(&cleanup_after),
        None,
    )
    .await;
    insert_backup_record(
        &state,
        "backup-cleanup-old",
        &stack_id,
        "job-cleanup-old",
        &old_created_at,
        "success",
        Some(old_path.to_str().unwrap()),
        Some(3),
        None,
        Some(&cleanup_after),
        None,
    )
    .await;

    crate::backup::cleanup_once(&state).await.unwrap();

    assert!(tokio::fs::try_exists(&latest_path).await.unwrap());
    assert!(!tokio::fs::try_exists(&old_path).await.unwrap());
    let records = state
        .db
        .list_service_backup_records(&stack_id, &service_id)
        .await
        .unwrap();
    let old_record = records
        .iter()
        .find(|record| record.backup_id == "backup-cleanup-old")
        .expect("cleaned backup record should remain auditable");
    assert!(old_record.deleted_at.is_some());

    tokio::fs::remove_dir_all(storage_root).await.unwrap();
}

#[tokio::test]
async fn service_backup_records_report_stack_wide_retention_metadata() {
    let state = test_state(":memory:").await;
    let compose_path = format!("/tmp/dockrev-backup-retention-{}.yml", ulid::Ulid::new());
    tokio::fs::write(&compose_path, "services:\n  api:\n    image: example/api\n  web:\n    image: example/web\n")
        .await
        .unwrap();
    let stack_id = seed_stack_from_compose(&state, "retention", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let api_id = stack.services.iter().find(|svc| svc.name == "api").unwrap().id.clone();
    let web_id = stack.services.iter().find(|svc| svc.name == "web").unwrap().id.clone();
    let now = test_now_rfc3339();
    let newer = test_offset_rfc3339(&now, time::Duration::seconds(1));
    insert_update_job_with_summary(&state, "job-retention-api", crate::api::types::JobScope::Service, Some(&stack_id), Some(&api_id), json!({"targets": [{"serviceId": api_id}]}), &now).await;
    insert_update_job_with_summary(&state, "job-retention-web", crate::api::types::JobScope::Service, Some(&stack_id), Some(&web_id), json!({"targets": [{"serviceId": web_id}]}), &newer).await;
    insert_backup_record(&state, "bkp-retention-api", &stack_id, "job-retention-api", &now, "success", Some("/tmp/retention-api.tar.gz"), Some(1), None, None, None).await;
    insert_backup_record(&state, "bkp-retention-web", &stack_id, "job-retention-web", &newer, "success", Some("/tmp/retention-web.tar.gz"), Some(1), None, None, None).await;

    let response = api::router(state)
        .oneshot(Request::builder().uri(format!("/api/services/{api_id}/backup-records")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let records = response_json(response).await["records"].clone();
    assert_eq!(records.as_array().unwrap().len(), 1);
    assert_eq!(records[0]["backupId"].as_str(), Some("bkp-retention-api"));
    assert_eq!(records[0]["retained"].as_bool(), Some(false));
    tokio::fs::remove_file(compose_path).await.unwrap();
}

#[tokio::test]
async fn rollback_evidence_api_download_preserves_raw_candidate_logs_end_to_end() {
    let state = test_state_with_authz(":memory:", Some("alice"), None, false).await;
    let job_id = ids::new_job_id();
    let job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::Service,
        None,
        None,
        "2026-08-28T00:00:00Z",
    )
    .to_db();
    state.db.insert_job(job).await.unwrap();
    let evidence = crate::rollback_evidence::RollbackEvidenceContext::new(
        job_id.clone(),
        &state.config.db_path,
    )
    .expect("evidence spool");
    let evidence_spool_root = crate::rollback_evidence::spool_root(&state.config.db_path);
    let mut random_state = 0x9e37_79b9_u32;
    let mut expected_logs = (0..(2 * 1024 * 1024 + 173))
        .map(|_| {
            random_state ^= random_state << 13;
            random_state ^= random_state >> 17;
            random_state ^= random_state << 5;
            random_state as u8
        })
        .collect::<Vec<_>>();
    let private_marker = b"candidate-log-private-marker\n";
    expected_logs[..private_marker.len()].copy_from_slice(private_marker);
    let capture = evidence
        .capture_failure(
            &CandidateLogEvidenceRunner {
                log_bytes: expected_logs.clone(),
            },
            &crate::docker_runner::DockerRunnerConfig::default(),
            "service-a",
            "candidate-a",
            "unhealthy",
            None,
            None,
        )
        .await;
    assert_eq!(capture.logs_bytes, expected_logs.len() as u64);
    assert!(!capture.logs_truncated);
    assert_eq!(
        tokio::fs::read(
            evidence
                .job_spool_path()
                .join("service-a/candidate-a/container.log")
        )
        .await
        .unwrap(),
        expected_logs
    );
    let evidence_summary = evidence.finalize().await;
    assert_eq!(evidence_summary.status, "available");
    let archive_path = evidence.archive_path();
    let archive_bytes = tokio::fs::read(&archive_path).await.unwrap();
    assert!(archive_bytes.len() > 64 * 1024);
    let summary = json!({"rollbackEvidence": evidence_summary});
    state
        .db
        .finish_job_with_archive_file_and_settlement_and_notification(
            &job_id,
            "rolled_back",
            "2026-08-28T00:01:00Z",
            &summary,
            Some(archive_path.clone()),
            None,
            None,
        )
        .await
        .unwrap();
    evidence.cleanup_after_commit().await;
    assert!(!evidence.job_spool_path().exists());
    assert!(!archive_path.exists());

    let app = api::router(state);
    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}/rollback-evidence"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}/rollback-evidence"))
                .header("X-Forwarded-User", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/zstd"
    );
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "private, no-store"
    );
    assert_eq!(
        response.headers().get("content-length").unwrap(),
        archive_bytes.len().to_string().as_str()
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), archive_bytes);
    assert!(!bytes.is_empty());
    let downloaded_archive = std::env::temp_dir().join(format!(
        "dockrev-api-downloaded-evidence-{}.tar.zst",
        ulid::Ulid::new()
    ));
    tokio::fs::write(&downloaded_archive, &bytes).await.unwrap();
    assert_eq!(
        extract_rollback_evidence_member(
            &downloaded_archive,
            "./service-a/candidate-a/container.log"
        ),
        expected_logs
    );

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{job_id}"))
                .header("X-Forwarded-User", "alice")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_json = response_json(detail).await;
    assert_eq!(detail_json["job"]["summary"]["rollbackEvidence"]["status"], "available");
    assert_eq!(
        detail_json["job"]["summary"]["rollbackEvidence"]["services"][0]["logsTruncated"],
        false
    );
    assert!(!detail_json
        .to_string()
        .contains("candidate-log-private-marker"));
    tokio::fs::remove_file(downloaded_archive).await.unwrap();
    let _ = tokio::fs::remove_dir(evidence_spool_root).await;
}

struct CandidateLogEvidenceRunner {
    log_bytes: Vec<u8>,
}

#[async_trait::async_trait]
impl CommandRunner for CandidateLogEvidenceRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        timeout: Duration,
    ) -> anyhow::Result<crate::runner::CommandOutput> {
        let output = self.run_raw(spec, timeout).await?;
        Ok(crate::runner::CommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn run_raw(
        &self,
        spec: CommandSpec,
        _timeout: Duration,
    ) -> anyhow::Result<crate::runner::RawCommandOutput> {
        let stdout = if spec.args.iter().any(|arg| arg.contains("logs --timestamps")) {
            self.log_bytes.clone()
        } else {
            br#"{"Status":"exited","Error":"","ExitCode":1,"RestartCount":0,"Health":{"Log":[]}}"#.to_vec()
        };
        Ok(crate::runner::RawCommandOutput {
            status: 0,
            stdout,
            stderr: Vec::new(),
        })
    }
}

fn extract_rollback_evidence_member(archive: &std::path::Path, member: &str) -> Vec<u8> {
    let tar_path = archive.with_extension("tar");
    let decompressed = std::process::Command::new("zstd")
        .arg("-d")
        .arg("-f")
        .arg(archive)
        .arg("-o")
        .arg(&tar_path)
        .output()
        .expect("zstd should read downloaded evidence archive");
    assert!(decompressed.status.success());
    let extracted = std::process::Command::new("tar")
        .args(["-xOf"])
        .arg(&tar_path)
        .arg(member)
        .output()
        .expect("tar should read downloaded evidence archive");
    assert!(extracted.status.success());
    let _ = std::fs::remove_file(tar_path);
    extracted.stdout
}
