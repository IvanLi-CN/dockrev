use super::*;

static EVIDENCE_RECOVERY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const MAX_INCOMPLETE_SERVICES: usize = 32;
const MAX_INCOMPLETE_CAPTURE_ERRORS: usize = 4;
const MAX_INCOMPLETE_FIELD_CHARS: usize = 256;
const MAX_RECOVERY_ERROR_CHARS: usize = 512;

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
        let summary = EvidenceSummary {
            status: "available",
            failed_candidates: records.len(),
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: Some(archive_size_bytes),
            services: records.clone(),
            errors: Vec::new(),
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
    let mut metadata_truncated = truncate_text(&mut message, MAX_RECOVERY_ERROR_CHARS);
    let (services, services_truncated) = bounded_incomplete_services(records);
    metadata_truncated |= services_truncated;
    let mut errors = vec![message];
    if metadata_truncated {
        errors.push("recovery metadata was truncated to keep the job summary bounded".to_string());
    }
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

fn bounded_incomplete_services(records: &[EvidenceMetadata]) -> (Vec<EvidenceMetadata>, bool) {
    let mut truncated = records.len() > MAX_INCOMPLETE_SERVICES;
    let mut services = records
        .iter()
        .take(MAX_INCOMPLETE_SERVICES)
        .cloned()
        .collect::<Vec<_>>();
    for service in &mut services {
        truncated |= truncate_text(&mut service.service_id, MAX_INCOMPLETE_FIELD_CHARS);
        truncated |= truncate_text(&mut service.candidate_id, MAX_INCOMPLETE_FIELD_CHARS);
        truncated |= truncate_text(&mut service.health_status, MAX_INCOMPLETE_FIELD_CHARS);
        if let Some(value) = service.state_status.as_mut() {
            truncated |= truncate_text(value, MAX_INCOMPLETE_FIELD_CHARS);
        }
        if let Some(value) = service.state_error.as_mut() {
            truncated |= truncate_text(value, MAX_INCOMPLETE_FIELD_CHARS);
        }
        if service.capture_errors.len() > MAX_INCOMPLETE_CAPTURE_ERRORS {
            service
                .capture_errors
                .truncate(MAX_INCOMPLETE_CAPTURE_ERRORS);
            truncated = true;
        }
        for error in &mut service.capture_errors {
            truncated |= truncate_text(error, MAX_INCOMPLETE_FIELD_CHARS);
        }
    }
    (services, truncated)
}

fn truncate_text(value: &mut String, max_chars: usize) -> bool {
    if value.chars().count() <= max_chars {
        return false;
    }
    let prefix_chars = max_chars.saturating_sub(3);
    let prefix = value.chars().take(prefix_chars).collect::<String>();
    *value = format!("{prefix}...");
    true
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
        assert_eq!(services.len(), MAX_INCOMPLETE_SERVICES);
        for service in services {
            for field in [
                "serviceId",
                "candidateId",
                "healthStatus",
                "stateStatus",
                "stateError",
            ] {
                if let Some(value) = service[field].as_str() {
                    assert!(value.chars().count() <= MAX_INCOMPLETE_FIELD_CHARS);
                }
            }
            let errors = service["captureErrors"].as_array().expect("capture errors");
            assert_eq!(errors.len(), MAX_INCOMPLETE_CAPTURE_ERRORS);
            assert!(errors.iter().all(|error| {
                error.as_str().expect("capture error").chars().count() <= MAX_INCOMPLETE_FIELD_CHARS
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
            MAX_RECOVERY_ERROR_CHARS
        );
        assert!(summary["errors"].as_array().expect("errors").len() > 1);
    }
}
