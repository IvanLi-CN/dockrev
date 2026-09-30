use super::*;

static EVIDENCE_RECOVERY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn cleanup_orphaned_spools(db: &crate::db::Db, db_path: &Path) {
    let root = spool_root(db_path);
    if let Err(error) = set_owner_only(&root)
        && error
            .downcast_ref::<std::io::Error>()
            .is_none_or(|error| error.kind() != std::io::ErrorKind::NotFound)
    {
        tracing::warn!(path = %root.display(), error = %error, "could not protect rollback evidence spool root");
    }
    cleanup_committed_archive_only_files(db, &root).await;
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(path = %root.display(), error = %error, "could not scan rollback evidence spools");
            return;
        }
    };
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(path = %root.display(), error = %error, "could not continue scanning rollback evidence spools");
                break;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type().await {
            Ok(file_type) => file_type,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "could not inspect rollback evidence entry");
                continue;
            }
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
                if let Err(error) = cleanup_orphaned_entry(&path, false, lookup).await {
                    tracing::warn!(path = %path.display(), error = %error, "could not remove orphaned rollback evidence file");
                }
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
        if let Err(error) = cleanup_orphaned_entry(&path, true, lookup).await {
            tracing::warn!(path = %path.display(), error = %error, "could not remove orphaned rollback evidence spool");
        }
    }
}

pub(crate) async fn cleanup_orphaned_entry(
    path: &Path,
    is_directory: bool,
    job_lookup: anyhow::Result<Option<()>>,
) -> anyhow::Result<()> {
    match job_lookup {
        Ok(None) if is_directory => {
            let mut errors = Vec::new();
            if let Err(error) = tokio::fs::remove_dir_all(path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(format!("remove spool {}: {error}", path.display()));
            }
            let archive = path.with_extension("tar.zst");
            if let Err(error) = tokio::fs::remove_file(&archive).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(format!("remove archive {}: {error}", archive.display()));
            }
            if !errors.is_empty() {
                return Err(anyhow::anyhow!(errors.join("; ")));
            }
        }
        Ok(None) => {
            if let Err(error) = tokio::fs::remove_file(path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(anyhow::anyhow!(
                    "remove evidence file {}: {error}",
                    path.display()
                ));
            }
        }
        Ok(Some(())) => {
            if is_directory {
                set_owner_only(path)?;
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "preserving rollback evidence after job lookup failure");
            if is_directory {
                set_owner_only(path)?;
            }
        }
    }
    Ok(())
}

pub(super) async fn recover_evidence(
    db: &crate::db::Db,
    db_path: &Path,
    allow_interrupted_nonterminal: bool,
) {
    let _guard = EVIDENCE_RECOVERY_LOCK.lock().await;
    let root = spool_root(db_path);
    match tokio::fs::metadata(&root).await {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            tracing::warn!(path = %root.display(), "rollback evidence recovery root is not a directory");
            return;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(path = %root.display(), error = %error, "could not inspect rollback evidence recovery root");
            return;
        }
    }
    if let Err(error) = set_owner_only(&root) {
        tracing::warn!(path = %root.display(), error = %error, "could not protect rollback evidence recovery root");
        return;
    }
    cleanup_committed_archive_only_files(db, &root).await;
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(path = %root.display(), error = %error, "could not scan rollback evidence for recovery");
            return;
        }
    };
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(path = %root.display(), error = %error, "could not continue scanning rollback evidence for recovery");
                break;
            }
        };
        let spool = entry.path();
        let file_type = match entry.file_type().await {
            Ok(file_type) => file_type,
            Err(error) => {
                tracing::warn!(path = %spool.display(), error = %error, "could not inspect rollback evidence recovery entry");
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(job_id) = spool.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Err(error) = set_owner_only(&spool) {
            tracing::warn!(path = %spool.display(), error = %error, "could not protect rollback evidence recovery spool");
            continue;
        }
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
        let retry_final_manifest = terminal && has_final_manifest_write_failure(&job.summary_json);
        let manifest = spool.join("manifest.json");
        let manifest_bytes = match tokio::fs::read(&manifest).await {
            Ok(bytes) => bytes,
            Err(error) => {
                record_recovery_failure_without_candidate_count(db, job_id, "read manifest", error)
                    .await;
                continue;
            }
        };
        let mut records = match serde_json::from_slice::<Vec<EvidenceMetadata>>(&manifest_bytes) {
            Ok(records) => records,
            Err(error) => {
                record_recovery_failure_without_candidate_count(
                    db,
                    job_id,
                    "parse manifest",
                    error,
                )
                .await;
                continue;
            }
        };
        let evidence_summary = &job.summary_json["rollbackEvidence"];
        let recorded_candidates = evidence_summary["failedCandidates"]
            .as_u64()
            .and_then(|count| usize::try_from(count).ok());
        let checkpoint_count_required = retry_final_manifest
            || evidence_summary["status"] == "incomplete"
                && evidence_summary.get("failedCandidates").is_some();
        if checkpoint_count_required && recorded_candidates != Some(records.len()) {
            let expected = recorded_candidates
                .map(|count| count.to_string())
                .unwrap_or_else(|| "a recorded candidate count".to_string());
            record_recovery_failure_with_candidate_count(
                db,
                job_id,
                recorded_candidates,
                &[],
                "validate checkpoint",
                format!(
                    "checkpoint contains {} candidate records; expected {expected}",
                    records.len()
                ),
            )
            .await;
            continue;
        }
        if retry_final_manifest {
            if records.is_empty() {
                record_recovery_failure(
                    db,
                    job_id,
                    &records,
                    "rebuild final manifest",
                    "checkpoint contains no candidate records",
                )
                .await;
                continue;
            }
            for record in &mut records {
                record.logs_truncated = true;
                let reason = "final manifest write failed; capture completeness is uncertain";
                if !record.capture_errors.iter().any(|error| error == reason) {
                    record.capture_errors.push(reason.to_string());
                }
            }
        }
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
        if retry_final_manifest && let Err(error) = write_manifest(&spool, &records).await {
            record_recovery_failure(db, job_id, &records, "rebuild final manifest", error).await;
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
        let capture_errors = ordered_capture_errors(&records);
        let summary = EvidenceSummary {
            status: "available",
            failed_candidates: records.len(),
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: Some(archive_size_bytes),
            services,
            errors: bounded_summary_errors(capture_errors, services_truncated),
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

fn has_final_manifest_write_failure(summary: &Value) -> bool {
    summary["rollbackEvidence"]["status"] == "incomplete"
        && summary["rollbackEvidence"]["errors"]
            .as_array()
            .is_some_and(|errors| {
                errors.iter().any(|error| {
                    error
                        .as_str()
                        .is_some_and(|error| error.starts_with("manifest:"))
                })
            })
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
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(path = %root.display(), error = %error, "could not scan committed rollback archives");
            return;
        }
    };
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(path = %root.display(), error = %error, "could not continue scanning committed rollback archives");
                break;
            }
        };
        let file_type = match entry.file_type().await {
            Ok(file_type) => file_type,
            Err(error) => {
                tracing::warn!(path = %entry.path().display(), error = %error, "could not inspect committed rollback archive entry");
                continue;
            }
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
    record_recovery_failure_with_candidate_count(
        db,
        job_id,
        Some(records.len()),
        records,
        stage,
        error,
    )
    .await;
}

async fn record_recovery_failure_without_candidate_count(
    db: &crate::db::Db,
    job_id: &str,
    stage: &str,
    error: impl std::fmt::Display,
) {
    record_recovery_failure_with_candidate_count(db, job_id, None, &[], stage, error).await;
}

async fn record_recovery_failure_with_candidate_count(
    db: &crate::db::Db,
    job_id: &str,
    candidate_count: Option<usize>,
    records: &[EvidenceMetadata],
    stage: &str,
    error: impl std::fmt::Display,
) {
    let mut message = format!("{stage}: {error}");
    let mut metadata_truncated = truncate_summary_text(&mut message, MAX_SUMMARY_ERROR_CHARS);
    let (services, services_truncated) = bounded_summary_services(records);
    metadata_truncated |= services_truncated;
    let mut errors = vec![message];
    errors.extend(ordered_capture_errors(records));
    let errors = bounded_summary_errors(errors, metadata_truncated);
    let summary = EvidenceSummary {
        status: "incomplete",
        failed_candidates: candidate_count.unwrap_or_default(),
        archive_format: "tar",
        compression: "zstd",
        archive_size_bytes: None,
        services,
        errors,
    };
    let metadata = serde_json::to_value(summary).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "incomplete",
            "failedCandidates": candidate_count.unwrap_or_default(),
            "archiveFormat": "tar",
            "compression": "zstd",
            "archiveSizeBytes": null,
            "services": [],
            "errors": ["evidence recovery failed"]
        })
    });
    let mut metadata = metadata;
    if candidate_count.is_none()
        && let Some(metadata) = metadata.as_object_mut()
    {
        metadata.remove("failedCandidates");
    }
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
    async fn orphan_spool_cleanup_surfaces_removal_failure() {
        let root =
            std::env::temp_dir().join(format!("dockrev-orphan-cleanup-{}", ulid::Ulid::new()));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let spool = root.join("orphan");
        tokio::fs::write(&spool, b"not a directory")
            .await
            .expect("spool blocker");

        let result = cleanup_orphaned_entry(&spool, true, Ok(None)).await;

        assert!(result.is_err());
        assert!(spool.is_file());
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
        let raw_state_error = "docker state error: credential=private-marker";
        let records = (0..96)
            .map(|_| EvidenceMetadata {
                service_id: long.clone(),
                candidate_id: long.clone(),
                health_status: long.clone(),
                state_status: Some(long.clone()),
                state_error: Some(raw_state_error.to_string()),
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
            assert!(service["stateError"].is_null());
            for field in ["serviceId", "candidateId", "healthStatus", "stateStatus"] {
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
        assert!(
            !serde_json::to_string(summary)
                .expect("serialized summary")
                .contains(raw_state_error)
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
    async fn unreadable_manifest_preserves_previously_recorded_candidate_total() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-recovery-manifest-count-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-manifest-count";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 3,
                    "services": [{
                        "serviceId": "service-retained",
                        "candidateId": "candidate-retained",
                        "healthStatus": "unhealthy",
                        "logsTruncated": true,
                        "captureErrors": ["candidate logs were incomplete"]
                    }],
                    "errors": ["archive persistence: injected failure"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        tokio::fs::write(spool.join("manifest.json"), b"not valid json")
            .await
            .expect("invalid manifest fixture");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db.get_job(job_id).await.expect("load job").expect("job");
        let summary = &job.summary_json["rollbackEvidence"];
        assert_eq!(summary["status"], "incomplete");
        assert_eq!(summary["failedCandidates"], 3);
        assert_eq!(summary["services"][0]["serviceId"], "service-retained");
        assert!(
            summary["errors"][0]
                .as_str()
                .unwrap_or_default()
                .starts_with("parse manifest:")
        );
        assert!(
            summary["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| { error.as_str() == Some("archive persistence: injected failure") })
        );
        assert!(
            db.get_rollback_evidence_archive(job_id)
                .await
                .expect("read archive")
                .is_none()
        );

        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn final_manifest_retry_marker_survives_summary_error_limit() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-final-manifest-retry-marker-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-final-manifest-retry-marker";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 1,
                    "errors": ["manifest: final manifest write failed"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        let record = EvidenceMetadata {
            service_id: "service-a".to_string(),
            candidate_id: "candidate-a".to_string(),
            health_status: "unhealthy".to_string(),
            capture_errors: (0..24)
                .map(|index| format!("capture failure {index}"))
                .collect(),
            ..Default::default()
        };
        write_manifest(&spool, &[record]).await.expect("checkpoint");
        tokio::fs::create_dir(spool.join("manifest.tmp"))
            .await
            .expect("block manifest replacement");

        recover_orphaned_evidence(&db, &db_path).await;
        let first_job = db
            .get_job(job_id)
            .await
            .expect("load first job")
            .expect("job");
        let marker_survived = first_job.summary_json["rollbackEvidence"]["errors"]
            .as_array()
            .is_some_and(|errors| {
                errors.iter().any(|error| {
                    error
                        .as_str()
                        .is_some_and(|error| error.starts_with("manifest:"))
                })
            });

        tokio::fs::remove_dir(spool.join("manifest.tmp"))
            .await
            .expect("remove manifest failure trigger");
        recover_orphaned_evidence(&db, &db_path).await;
        let recovered_job = db
            .get_job(job_id)
            .await
            .expect("load recovered job")
            .expect("job");
        let recovered_truncated =
            recovered_job.summary_json["rollbackEvidence"]["services"][0]["logsTruncated"] == true;
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();

        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert!(
            marker_survived,
            "manifest retry marker must not be truncated"
        );
        assert!(
            archive_exists,
            "recovery should attach the archive after retry"
        );
        assert!(
            recovered_truncated,
            "recovered logs must remain marked incomplete after a manifest retry"
        );
    }

    #[tokio::test]
    async fn final_manifest_retry_rejects_checkpoint_with_missing_candidates() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-final-manifest-count-mismatch-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-final-manifest-count-mismatch";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 2,
                    "services": [{
                        "serviceId": "service-retained",
                        "candidateId": "candidate-retained",
                        "healthStatus": "unhealthy",
                        "logsTruncated": true,
                        "captureErrors": ["retained candidate error"]
                    }],
                    "errors": ["manifest: final manifest write failed"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        write_manifest(
            &spool,
            &[EvidenceMetadata {
                service_id: "service-a".to_string(),
                candidate_id: "candidate-a".to_string(),
                health_status: "unhealthy".to_string(),
                logs_truncated: false,
                ..Default::default()
            }],
        )
        .await
        .expect("short checkpoint");

        recover_orphaned_evidence(&db, &db_path).await;
        let job = db.get_job(job_id).await.expect("load job").expect("job");
        let summary = &job.summary_json["rollbackEvidence"];
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();
        let spool_exists = spool.exists();

        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert!(
            !archive_exists,
            "mismatched checkpoint must not be downloadable"
        );
        assert!(
            spool_exists,
            "mismatched checkpoint must remain available for recovery"
        );
        assert_eq!(summary["status"], "incomplete");
        assert_eq!(summary["failedCandidates"], 2);
        assert_eq!(summary["services"][0]["serviceId"], "service-retained");
        assert!(
            summary["errors"].as_array().is_some_and(|errors| {
                errors.iter().any(|error| {
                    error
                        .as_str()
                        .is_some_and(|error| error.starts_with("validate checkpoint:"))
                })
            }),
            "the mismatch must be reported in bounded evidence errors"
        );
    }

    #[tokio::test]
    async fn terminal_recovery_rejects_stale_checkpoint_after_archive_failure() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-stale-checkpoint-after-archive-failure-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-stale-checkpoint-after-archive-failure";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 2,
                    "services": [{
                        "serviceId": "service-retained",
                        "candidateId": "candidate-retained",
                        "healthStatus": "unhealthy",
                        "logsTruncated": true,
                        "captureErrors": ["retained candidate error"]
                    }],
                    "errors": ["archive persistence: injected failure"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
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
        .expect("older checkpoint");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db.get_job(job_id).await.expect("load job").expect("job");
        let summary = &job.summary_json["rollbackEvidence"];
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();
        let spool_exists = spool.exists();

        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert!(
            !archive_exists,
            "stale checkpoint must not become a downloadable archive"
        );
        assert!(spool_exists, "stale checkpoint must remain for recovery");
        assert_eq!(summary["status"], "incomplete");
        assert_eq!(summary["failedCandidates"], 2);
        assert!(summary["errors"].as_array().is_some_and(|errors| {
            errors.iter().any(|error| {
                error
                    .as_str()
                    .is_some_and(|error| error.starts_with("validate checkpoint:"))
            })
        }));
    }

    #[tokio::test]
    async fn manifest_read_failure_without_candidate_count_can_recover_later() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-manifest-retry-without-count-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-manifest-retry-without-count";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "errors": ["archive persistence: injected failure"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");

        recover_orphaned_evidence(&db, &db_path).await;

        let first_job = db.get_job(job_id).await.expect("load first job").unwrap();
        assert!(
            first_job.summary_json["rollbackEvidence"]
                .get("failedCandidates")
                .is_none()
        );
        let records = [EvidenceMetadata {
            service_id: "service-a".to_string(),
            candidate_id: "candidate-a".to_string(),
            health_status: "unhealthy".to_string(),
            ..Default::default()
        }];
        let candidate_dir = spool.join("service-a").join("candidate-a");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        tokio::fs::write(
            candidate_dir.join("container.log"),
            b"recovered candidate logs",
        )
        .await
        .expect("candidate log");
        write_manifest(&spool, &records)
            .await
            .expect("recovered manifest");

        recover_orphaned_evidence(&db, &db_path).await;

        let recovered_job = db
            .get_job(job_id)
            .await
            .expect("load recovered job")
            .unwrap();
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();
        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert_eq!(
            recovered_job.summary_json["rollbackEvidence"]["status"],
            "available"
        );
        assert_eq!(
            recovered_job.summary_json["rollbackEvidence"]["failedCandidates"],
            1
        );
        assert!(archive_exists, "recovered manifest should attach evidence");
    }

    #[tokio::test]
    async fn manifest_read_failure_preserves_known_zero_candidate_count() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-manifest-retry-known-zero-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-manifest-retry-known-zero";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 0,
                    "errors": ["archive persistence: injected failure"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");

        recover_orphaned_evidence(&db, &db_path).await;

        let first_job = db.get_job(job_id).await.expect("load first job").unwrap();
        assert_eq!(
            first_job.summary_json["rollbackEvidence"]["failedCandidates"],
            0
        );
        let records = [EvidenceMetadata {
            service_id: "service-a".to_string(),
            candidate_id: "candidate-a".to_string(),
            health_status: "unhealthy".to_string(),
            ..Default::default()
        }];
        let candidate_dir = spool.join("service-a").join("candidate-a");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        tokio::fs::write(candidate_dir.join("container.log"), b"candidate logs")
            .await
            .expect("candidate log");
        write_manifest(&spool, &records)
            .await
            .expect("recovered manifest");

        recover_orphaned_evidence(&db, &db_path).await;

        let recovered_job = db
            .get_job(job_id)
            .await
            .expect("load recovered job")
            .unwrap();
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();
        let spool_exists = spool.exists();
        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert_eq!(
            recovered_job.summary_json["rollbackEvidence"]["status"],
            "incomplete"
        );
        assert_eq!(
            recovered_job.summary_json["rollbackEvidence"]["failedCandidates"],
            0
        );
        assert!(!archive_exists, "mismatched checkpoint must not attach");
        assert!(
            spool_exists,
            "mismatched checkpoint must remain for recovery"
        );
    }

    #[tokio::test]
    async fn archive_persistence_retry_preserves_complete_candidate_logs() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-archive-retry-preserves-complete-logs-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-archive-retry-preserves-complete-logs";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "incomplete",
                    "failedCandidates": 1,
                    "errors": ["archive persistence: injected failure"]
                }
            }),
        )
        .await
        .expect("finish job");
        let spool = spool_root(&db_path).join(job_id);
        let candidate_dir = spool.join("service-a").join("candidate-a");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        tokio::fs::write(
            candidate_dir.join("container.log"),
            b"complete candidate output\n",
        )
        .await
        .expect("candidate log");
        write_manifest(
            &spool,
            &[EvidenceMetadata {
                service_id: "service-a".to_string(),
                candidate_id: "candidate-a".to_string(),
                health_status: "unhealthy".to_string(),
                logs_truncated: false,
                ..Default::default()
            }],
        )
        .await
        .expect("checkpoint");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db.get_job(job_id).await.expect("load job").expect("job");
        let summary = &job.summary_json["rollbackEvidence"];
        let archive_exists = db
            .get_rollback_evidence_archive(job_id)
            .await
            .expect("read archive")
            .is_some();

        drop(db);
        let _ = tokio::fs::remove_dir_all(root).await;
        assert!(
            archive_exists,
            "archive persistence retry should attach evidence"
        );
        assert_eq!(summary["status"], "available");
        assert_eq!(summary["services"][0]["logsTruncated"], false);
        assert!(
            summary["services"][0]["captureErrors"]
                .as_array()
                .is_some_and(|errors| {
                    !errors.iter().any(|error| {
                        error
                            .as_str()
                            .is_some_and(|error| error.contains("final manifest write failed"))
                    })
                })
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
            assert!(service["stateError"].is_null());
            for field in ["serviceId", "candidateId", "healthStatus", "stateStatus"] {
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

    #[tokio::test]
    async fn recovered_archive_keeps_capture_errors_from_services_beyond_summary_limit() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-recovered-summary-late-error-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job_id = "job-recovered-summary-late-error";
        insert_running_job(&db, job_id).await;
        db.finish_job(
            job_id,
            "failed",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({}),
        )
        .await
        .expect("finish job");

        let mut records = (0..MAX_SUMMARY_SERVICES + 1)
            .map(|index| EvidenceMetadata {
                service_id: format!("service-{index:02}"),
                candidate_id: format!("candidate-{index:02}"),
                health_status: "unhealthy".to_string(),
                capture_errors: vec![format!("candidate {index} capture failed")],
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let late_error = "candidate 33 logs capture failed";
        records[MAX_SUMMARY_SERVICES]
            .capture_errors
            .push(late_error.to_string());
        let spool = spool_root(&db_path).join(job_id);
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        write_manifest(&spool, &records)
            .await
            .expect("recovery manifest");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db.get_job(job_id).await.expect("load job").expect("job");
        assert_eq!(job.summary_json["rollbackEvidence"]["status"], "available");
        assert_eq!(
            job.summary_json["rollbackEvidence"]["services"]
                .as_array()
                .unwrap()
                .len(),
            MAX_SUMMARY_SERVICES
        );
        assert!(
            job.summary_json["rollbackEvidence"]["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error.as_str() == Some(late_error))
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn startup_evidence_recovery_returns_while_archive_scan_is_blocked() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-startup-recovery-background-{}",
            ulid::Ulid::new()
        ));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");

        let recovery_guard = EVIDENCE_RECOVERY_LOCK.lock().await;
        let recovery = super::spawn_startup_interrupted_evidence_recovery(db, db_path);
        assert!(!recovery.is_finished());
        drop(recovery_guard);
        tokio::time::timeout(Duration::from_secs(2), recovery)
            .await
            .expect("background recovery should finish")
            .expect("recovery task should not panic");

        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
