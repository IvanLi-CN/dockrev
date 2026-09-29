use super::*;

static EVIDENCE_RECOVERY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn cleanup_orphaned_spools(db: &crate::db::Db, db_path: &Path) {
    let root = spool_root(db_path);
    let _ = set_owner_only(&root);
    cleanup_committed_archive_only_files(db, &root).await;
    let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if file_type.is_file() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let job_id = name
                .strip_suffix(".tar.zst")
                .or_else(|| name.strip_suffix(".tar.zst.part"));
            if let Some(job_id) = job_id {
                let lookup = db.get_job(job_id).await.map(|job| job.map(|_| ()));
                cleanup_orphaned_entry(&path, false, lookup).await;
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let Some(job_id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let lookup = db.get_job(job_id).await.map(|job| job.map(|_| ()));
        cleanup_orphaned_entry(&path, true, lookup).await;
    }
}

pub(crate) async fn cleanup_orphaned_entry(
    path: &Path,
    is_directory: bool,
    job_lookup: anyhow::Result<Option<()>>,
) {
    match job_lookup {
        Ok(None) if is_directory => {
            let _ = tokio::fs::remove_dir_all(path).await;
            let _ = tokio::fs::remove_file(path.with_extension("tar.zst")).await;
        }
        Ok(None) => {
            let _ = tokio::fs::remove_file(path).await;
        }
        Ok(Some(())) => {
            if is_directory {
                let _ = set_owner_only(path);
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "preserving rollback evidence after job lookup failure");
            if is_directory {
                let _ = set_owner_only(path);
            }
        }
    }
}

pub(super) async fn recover_evidence(
    db: &crate::db::Db,
    db_path: &Path,
    allow_interrupted_nonterminal: bool,
) {
    let _guard = EVIDENCE_RECOVERY_LOCK.lock().await;
    let root = spool_root(db_path);
    cleanup_committed_archive_only_files(db, &root).await;
    let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
        return;
    };
    let _ = set_owner_only(&root);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let spool = entry.path();
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(job_id) = spool.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let _ = set_owner_only(&spool);
        let job = match db.get_job(job_id).await {
            Ok(Some(job)) => job,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(job_id = %job_id, error = %error, "preserving rollback evidence after job lookup failure");
                continue;
            }
        };
        let terminal = is_terminal_job(&job.status);
        if terminal {
            match cleanup_spool_for_committed_archive(db, job_id, &spool).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(job_id = %job_id, error = %error, "preserving committed rollback evidence after cleanup failure");
                    continue;
                }
            }
        }
        let manifest = spool.join("manifest.json");
        let manifest_bytes = match tokio::fs::read(&manifest).await {
            Ok(bytes) => bytes,
            Err(error) => {
                record_recovery_failure(db, job_id, &[], "read manifest", error).await;
                continue;
            }
        };
        let mut records = match serde_json::from_slice::<Vec<EvidenceMetadata>>(&manifest_bytes) {
            Ok(records) => records,
            Err(error) => {
                record_recovery_failure(db, job_id, &[], "parse manifest", error).await;
                continue;
            }
        };
        let capture_interrupted = records.iter().any(|record| {
            record
                .capture_errors
                .iter()
                .any(|error| error == CAPTURE_INTERRUPTED_REASON)
        });
        if !(terminal || allow_interrupted_nonterminal && capture_interrupted) {
            continue;
        }
        if let Err(error) = Box::pin(recover_interrupted_capture(&spool, &mut records)).await {
            record_recovery_failure(db, job_id, &records, "recover partial logs", error).await;
            continue;
        }
        match cleanup_spool_for_committed_archive(db, job_id, &spool).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                record_recovery_failure(db, job_id, &records, "check archive", error).await;
                continue;
            }
        }
        let archive_path = spool.with_extension("tar.zst");
        let part_path = spool.with_extension("tar.zst.part");
        if let Err(error) = archive_dir(&spool, &part_path, &archive_path).await {
            record_recovery_failure(db, job_id, &records, "build archive", error).await;
            continue;
        }
        let archive_size_bytes = match tokio::fs::metadata(&archive_path).await {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                record_recovery_failure(db, job_id, &records, "read archive size", error).await;
                continue;
            }
        };
        let (services, services_truncated) = bounded_summary_services(&records);
        let summary = EvidenceSummary {
            status: "available",
            failed_candidates: records.len(),
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: Some(archive_size_bytes),
            services,
            errors: bounded_summary_errors(Vec::new(), services_truncated),
        };
        let metadata = serde_json::to_value(&summary).unwrap_or_else(|_| serde_json::json!({}));
        match db
            .attach_rollback_evidence_archive_from_file(job_id, &archive_path, &metadata)
            .await
        {
            Ok(true) => match cleanup_spool_for_committed_archive(db, job_id, &spool).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(job_id = %job_id, "preserving local rollback evidence after committed archive was not found")
                }
                Err(error) => {
                    tracing::warn!(job_id = %job_id, error = %error, "preserving local rollback evidence after cleanup failure")
                }
            },
            Ok(false) => {
                tracing::warn!(job_id = %job_id, "preserving rollback evidence after archive was not attached")
            }
            Err(error) => {
                record_recovery_failure(db, job_id, &records, "attach archive", error).await;
            }
        }
    }
}

async fn cleanup_spool_for_committed_archive(
    db: &crate::db::Db,
    job_id: &str,
    spool: &Path,
) -> anyhow::Result<bool> {
    if db.rollback_evidence_archive_size(job_id).await?.is_none() {
        return Ok(false);
    }
    let archive = spool.with_extension("tar.zst");
    let part = spool.with_extension("tar.zst.part");
    let mut errors = Vec::new();
    if let Err(error) = tokio::fs::remove_dir_all(spool).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        errors.push(format!("remove spool {}: {error}", spool.display()));
    }
    for path in [&archive, &part] {
        if let Err(error) = tokio::fs::remove_file(path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(format!("remove archive file {}: {error}", path.display()));
        }
    }
    if !errors.is_empty() {
        let message = errors.join("; ");
        return Err(anyhow::anyhow!(message));
    }
    Ok(true)
}

async fn cleanup_committed_archive_only_files(db: &crate::db::Db, root: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(job_id) = name
            .strip_suffix(".tar.zst")
            .or_else(|| name.strip_suffix(".tar.zst.part"))
        else {
            continue;
        };
        let job = match db.get_job(job_id).await {
            Ok(Some(job)) if is_terminal_job(&job.status) => job,
            Ok(_) => continue,
            Err(error) => {
                tracing::warn!(job_id = %job_id, error = %error, "preserving archive-only rollback evidence after job lookup failure");
                continue;
            }
        };
        match cleanup_spool_for_committed_archive(db, job_id, &root.join(&job.id)).await {
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(job_id = %job_id, error = %error, "preserving archive-only rollback evidence after cleanup failure")
            }
        }
    }
}

fn is_terminal_job(status: &str) -> bool {
    matches!(status, "success" | "failed" | "rolled_back" | "cancelled")
}

async fn record_recovery_failure(
    db: &crate::db::Db,
    job_id: &str,
    records: &[EvidenceMetadata],
    stage: &str,
    error: impl std::fmt::Display,
) {
    let mut message = format!("{stage}: {error}");
    let mut metadata_truncated = truncate_summary_text(&mut message, MAX_SUMMARY_ERROR_CHARS);
    let (services, services_truncated) = bounded_summary_services(records);
    metadata_truncated |= services_truncated;
    let errors = bounded_summary_errors(vec![message], metadata_truncated);
    let summary = EvidenceSummary {
        status: "incomplete",
        failed_candidates: records.len(),
        archive_format: "tar",
        compression: "zstd",
        archive_size_bytes: None,
        services,
        errors,
    };
    let metadata = serde_json::to_value(summary).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "incomplete",
            "failedCandidates": records.len(),
            "archiveFormat": "tar",
            "compression": "zstd",
            "archiveSizeBytes": null,
            "services": [],
            "errors": ["evidence recovery failed"]
        })
    });
    if let Err(error) = db
        .mark_rollback_evidence_incomplete_if_archive_absent(job_id, &metadata)
        .await
    {
        tracing::warn!(job_id = %job_id, error = %error, "could not record incomplete rollback evidence recovery");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_running_job(db: &crate::db::Db, job_id: &str) {
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
    }

    #[tokio::test]
    async fn committed_archive_cleanup_reports_a_spool_removal_failure() {
        let db = crate::db::Db::open(Path::new(":memory:"))
            .await
            .expect("db");
        insert_running_job(&db, "job-cleanup-error").await;
        db.finish_job_with_archive(
            "job-cleanup-error",
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {"status": "available"}
            }),
            Some(b"committed archive".to_vec()),
        )
        .await
        .expect("commit archive");

        let root =
            std::env::temp_dir().join(format!("dockrev-cleanup-error-{}", ulid::Ulid::new()));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let spool = root.join("job-cleanup-error");
        tokio::fs::write(&spool, b"not a spool directory")
            .await
            .expect("spool blocker");
        let archive_path = spool.with_extension("tar.zst");
        let part_path = spool.with_extension("tar.zst.part");
        tokio::fs::write(&archive_path, b"local archive")
            .await
            .expect("archive");
        tokio::fs::write(&part_path, b"partial archive")
            .await
            .expect("part");

        let result = cleanup_spool_for_committed_archive(&db, "job-cleanup-error", &spool).await;

        assert!(result.is_err());
        assert!(spool.is_file());
        assert!(!archive_path.exists());
        assert!(!part_path.exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn incomplete_recovery_summary_bounds_records_and_metadata_fields() {
        let db = crate::db::Db::open(Path::new(":memory:"))
            .await
            .expect("db");
        insert_running_job(&db, "job-large-recovery-summary").await;
        db.finish_job(
            "job-large-recovery-summary",
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({}),
        )
        .await
        .expect("finish job");

        let long = "x".repeat(2_000);
        let records = (0..96)
            .map(|_| EvidenceMetadata {
                service_id: long.clone(),
                candidate_id: long.clone(),
                health_status: long.clone(),
                state_status: Some(long.clone()),
                state_error: Some(long.clone()),
                capture_errors: vec![long.clone(); 12],
                logs_truncated: true,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        record_recovery_failure(
            &db,
            "job-large-recovery-summary",
            &records,
            "build archive",
            &long,
        )
        .await;

        let job = db
            .get_job("job-large-recovery-summary")
            .await
            .expect("load job")
            .expect("job exists");
        let summary = &job.summary_json["rollbackEvidence"];
        assert_eq!(summary["status"], "incomplete");
        assert_eq!(summary["failedCandidates"], records.len());
        let services = summary["services"].as_array().expect("services");
        assert_eq!(services.len(), MAX_SUMMARY_SERVICES);
        for service in services {
            for field in [
                "serviceId",
                "candidateId",
                "healthStatus",
                "stateStatus",
                "stateError",
            ] {
                if let Some(value) = service[field].as_str() {
                    assert!(value.chars().count() <= MAX_SUMMARY_FIELD_CHARS);
                }
            }
            let errors = service["captureErrors"].as_array().expect("capture errors");
            assert_eq!(errors.len(), MAX_SUMMARY_CAPTURE_ERRORS);
            assert!(errors.iter().all(|error| {
                error.as_str().expect("capture error").chars().count() <= MAX_SUMMARY_FIELD_CHARS
            }));
            assert_eq!(service["logsTruncated"], true);
        }
        assert!(
            serde_json::to_vec(summary)
                .expect("serialized summary")
                .len()
                < 1024 * 1024
        );
        assert_eq!(
            summary["errors"][0]
                .as_str()
                .expect("recovery error")
                .chars()
                .count(),
            MAX_SUMMARY_ERROR_CHARS
        );
        let errors = summary["errors"].as_array().expect("errors");
        assert!(errors.len() <= MAX_SUMMARY_ERRORS);
        assert!(
            errors
                .iter()
                .any(|error| { error.as_str() == Some(SUMMARY_TRUNCATION_NOTE) })
        );
    }

    #[tokio::test]
    async fn recovered_archive_summary_bounds_metadata_and_preserves_archive_contents() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-recovered-summary-bounds-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-recovered-summary-bounds";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({}),
        )
        .await
        .expect("finish job");

        let long = "x".repeat(300);
        let records = (0..34)
            .map(|index| {
                let (service_id, candidate_id) = if index == 0 {
                    ("service-00".to_string(), "candidate-00".to_string())
                } else {
                    (
                        format!("service-{index:02}-{long}"),
                        format!("candidate-{index:02}-{long}"),
                    )
                };
                EvidenceMetadata {
                    service_id,
                    candidate_id,
                    health_status: long.clone(),
                    state_status: Some(long.clone()),
                    state_error: Some(long.clone()),
                    capture_errors: vec![long.clone(); 5],
                    logs_truncated: false,
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        write_manifest(&spool, &records)
            .await
            .expect("full recovery manifest");
        let first = &records[0];
        let candidate_dir = spool
            .join(path_component(&first.service_id))
            .join(path_component(&first.candidate_id));
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        let expected_logs = b"recovered raw log\0\xff\n";
        tokio::fs::write(candidate_dir.join("container.log"), expected_logs)
            .await
            .expect("candidate log");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db.get_job(job_id).await.expect("load job").expect("job");
        let summary = &job.summary_json["rollbackEvidence"];
        assert_eq!(summary["status"], "available");
        assert_eq!(summary["failedCandidates"], records.len());
        let services = summary["services"].as_array().expect("services");
        assert_eq!(services.len(), 32);
        for service in services {
            for field in [
                "serviceId",
                "candidateId",
                "healthStatus",
                "stateStatus",
                "stateError",
            ] {
                if let Some(value) = service[field].as_str() {
                    assert!(value.chars().count() <= 256);
                }
            }
            let errors = service["captureErrors"].as_array().expect("capture errors");
            assert_eq!(errors.len(), 4);
            assert!(
                errors
                    .iter()
                    .all(|error| { error.as_str().expect("capture error").chars().count() <= 256 })
            );
        }
        let errors = summary["errors"].as_array().expect("summary errors");
        assert!(errors.len() <= 16);
        assert!(
            errors
                .iter()
                .all(|error| { error.as_str().expect("summary error").chars().count() <= 512 })
        );
        assert!(
            errors
                .iter()
                .any(|error| { error.as_str() == Some(SUMMARY_TRUNCATION_NOTE) })
        );

        let archive = root.join("recovered.tar.zst");
        tokio::fs::write(
            &archive,
            db.get_rollback_evidence_archive(job_id)
                .await
                .expect("read archive")
                .expect("archive exists"),
        )
        .await
        .expect("write extracted archive fixture");
        let manifest: Vec<serde_json::Value> = serde_json::from_slice(
            &super::test_support::archive_member(&archive, "./manifest.json").await,
        )
        .expect("full archive manifest");
        assert_eq!(manifest.len(), records.len());
        assert_eq!(manifest[0], serde_json::to_value(first).unwrap());
        assert_eq!(
            super::test_support::archive_member(
                &archive,
                &format!(
                    "./{}/{}/container.log",
                    path_component(&first.service_id),
                    path_component(&first.candidate_id)
                )
            )
            .await,
            expected_logs
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
