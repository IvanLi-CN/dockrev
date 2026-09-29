use super::*;

static EVIDENCE_RECOVERY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn cleanup_orphaned_spools(db: &crate::db::Db, db_path: &Path) {
    let root = spool_root(db_path);
    let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
        return;
    };
    let _ = set_owner_only(&root);
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
        let terminal = matches!(
            job.status.as_str(),
            "success" | "failed" | "rolled_back" | "cancelled"
        );
        if terminal {
            match cleanup_spool_for_committed_archive(db, job_id, &spool).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    record_recovery_failure(db, job_id, &[], "check archive", error).await;
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
            Ok(true) => {
                let _ = tokio::fs::remove_dir_all(&spool).await;
                let _ = tokio::fs::remove_file(archive_path).await;
                let _ = tokio::fs::remove_file(part_path).await;
            }
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
    let _ = tokio::fs::remove_dir_all(spool).await;
    let _ = tokio::fs::remove_file(spool.with_extension("tar.zst")).await;
    let _ = tokio::fs::remove_file(spool.with_extension("tar.zst.part")).await;
    Ok(true)
}

async fn record_recovery_failure(
    db: &crate::db::Db,
    job_id: &str,
    records: &[EvidenceMetadata],
    stage: &str,
    error: impl std::fmt::Display,
) {
    let full_message = format!("{stage}: {error}");
    let mut chars = full_message.chars();
    let mut message = chars.by_ref().take(512).collect::<String>();
    if chars.next().is_some() {
        message.push_str("...");
    }
    let summary = EvidenceSummary {
        status: "incomplete",
        failed_candidates: records.len(),
        archive_format: "tar",
        compression: "zstd",
        archive_size_bytes: None,
        services: records.to_vec(),
        errors: vec![message],
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
