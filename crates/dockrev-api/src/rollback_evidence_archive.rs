use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use serde_json::Value;

use crate::backup_helper;

use super::{
    CAPTURE_INTERRUPTED_REASON, EvidenceMetadata, EvidenceSummary, RollbackEvidenceContext,
    bounded_summary_errors, bounded_summary_services,
};

pub(super) async fn finalize(context: &RollbackEvidenceContext) -> EvidenceSummary {
    let mut records = context.metadata();
    if records.is_empty() {
        return EvidenceSummary {
            status: "absent",
            failed_candidates: 0,
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: None,
            services: Vec::new(),
            errors: Vec::new(),
        };
    }
    let spool = context.job_spool_path();
    if let Err(error) = Box::pin(prepare_capture_archive(&spool, &mut records)).await {
        for record in &mut records {
            if record
                .capture_errors
                .iter()
                .any(|item| item == CAPTURE_INTERRUPTED_REASON)
            {
                record
                    .capture_errors
                    .push(format!("logs file promotion: {error}"));
            }
        }
        let _ = write_manifest(&spool, &records).await;
        let _ = tokio::fs::remove_file(spool.with_extension("tar.zst")).await;
        let _ = tokio::fs::remove_file(spool.with_extension("tar.zst.part")).await;
        let failed_candidates = records.len();
        let (services, services_truncated) = bounded_summary_services(&records);
        return EvidenceSummary {
            status: "incomplete",
            failed_candidates,
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: None,
            services,
            errors: bounded_summary_errors(
                vec![format!("logs file promotion: {error}")],
                services_truncated,
            ),
        };
    }
    let mut errors = records
        .iter()
        .flat_map(|record| record.capture_errors.iter().cloned())
        .collect::<Vec<_>>();
    let archive_path = spool.with_extension("tar.zst");
    let archive_part = spool.with_extension("tar.zst.part");
    if let Err(error) = write_manifest(&spool, &records).await {
        errors.insert(0, format!("manifest: {error}"));
        for (label, path) in [
            ("archive", &archive_path),
            ("partial archive", &archive_part),
        ] {
            if let Err(error) = tokio::fs::remove_file(path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(format!("remove {label}: {error}"));
            }
        }
        let failed_candidates = records.len();
        let (services, services_truncated) = bounded_summary_services(&records);
        return EvidenceSummary {
            status: "incomplete",
            failed_candidates,
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: None,
            services,
            errors: bounded_summary_errors(errors, services_truncated),
        };
    }
    let archive_size_bytes = match Box::pin(archive_dir(&spool, &archive_part, &archive_path)).await
    {
        Ok(()) => tokio::fs::metadata(&archive_path)
            .await
            .ok()
            .map(|m| m.len()),
        Err(error) => {
            errors.push(format!("archive: {error}"));
            None
        }
    };
    let failed_candidates = records.len();
    let (services, services_truncated) = bounded_summary_services(&records);
    EvidenceSummary {
        status: if archive_size_bytes.is_some() {
            "available"
        } else {
            "incomplete"
        },
        failed_candidates,
        archive_format: "tar",
        compression: "zstd",
        archive_size_bytes,
        services,
        errors: bounded_summary_errors(errors, services_truncated),
    }
}

pub(super) async fn recover_interrupted_capture(
    spool: &Path,
    records: &mut [EvidenceMetadata],
) -> anyhow::Result<()> {
    for record in records.iter_mut() {
        if !record
            .capture_errors
            .iter()
            .any(|error| error == CAPTURE_INTERRUPTED_REASON)
        {
            continue;
        }

        let candidate_dir = spool
            .join(path_component(&record.service_id))
            .join(path_component(&record.candidate_id));
        let partial_path = candidate_dir.join("container.log.part");
        let log_path = candidate_dir.join("container.log");
        let promotion_path = candidate_dir.join("container.log.promote.part");
        match tokio::fs::remove_file(&promotion_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match tokio::fs::metadata(&partial_path).await {
            Ok(_) => match tokio::fs::metadata(&log_path).await {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    promote_log_file(&partial_path, &log_path).await?;
                }
                Ok(_) => {
                    if files_equal(&partial_path, &log_path).await? {
                        tokio::fs::remove_file(&partial_path).await?;
                    } else {
                        record.capture_errors.push(
                            "recovery found different complete and partial log files".to_string(),
                        );
                    }
                }
                Err(error) => return Err(error.into()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if tokio::fs::metadata(&log_path).await.is_err()
            && tokio::fs::metadata(&partial_path).await.is_err()
        {
            let log = create_private_file(&log_path).await?;
            log.sync_all().await?;
        }
        let stored_metadata = match tokio::fs::metadata(&log_path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::metadata(&partial_path).await?
            }
            Err(error) => return Err(error.into()),
        };
        record.logs_bytes = stored_metadata.len();
        record.logs_truncated = true;
    }
    write_manifest(spool, records).await
}

pub(super) async fn create_private_file(path: &Path) -> std::io::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options.open(path).await
}

pub(super) async fn prepare_capture_archive(
    spool: &Path,
    records: &mut [EvidenceMetadata],
) -> anyhow::Result<()> {
    let has_interrupted_capture = records.iter().any(|record| {
        record
            .capture_errors
            .iter()
            .any(|error| error == CAPTURE_INTERRUPTED_REASON)
    });
    if has_interrupted_capture {
        recover_interrupted_capture(spool, records).await?;
    }
    Ok(())
}

pub(super) async fn promote_log_file(partial_path: &Path, log_path: &Path) -> anyhow::Result<()> {
    let rename_result = tokio::fs::rename(partial_path, log_path).await;
    promote_log_file_after_rename(partial_path, log_path, rename_result).await
}

pub(super) async fn promote_log_file_after_rename(
    partial_path: &Path,
    log_path: &Path,
    rename_result: std::io::Result<()>,
) -> anyhow::Result<()> {
    match rename_result {
        Ok(()) => {
            set_owner_only(log_path)?;
            Ok(())
        }
        Err(rename_error) => {
            match tokio::fs::metadata(log_path).await {
                Ok(_) => return Err(rename_error.into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            copy_log_file_atomically(partial_path, log_path)
                .await
                .with_context(|| format!("rename failed: {rename_error}"))?;
            set_owner_only(log_path)?;
            tokio::fs::remove_file(partial_path).await?;
            Ok(())
        }
    }
}

async fn copy_log_file_atomically(partial_path: &Path, log_path: &Path) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let promotion_path = log_path.with_file_name("container.log.promote.part");
    match tokio::fs::remove_file(&promotion_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let result = async {
        let mut source = tokio::fs::File::open(partial_path).await?;
        let mut destination = create_private_file(&promotion_path).await?;
        tokio::io::copy(&mut source, &mut destination).await?;
        destination.flush().await?;
        destination.sync_all().await?;
        drop(destination);
        set_owner_only(&promotion_path)?;
        tokio::fs::rename(&promotion_path, log_path).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&promotion_path).await;
    }
    result
}

async fn files_equal(left: &Path, right: &Path) -> anyhow::Result<bool> {
    use tokio::io::AsyncReadExt as _;

    let mut left = tokio::fs::File::open(left).await?;
    let mut right = tokio::fs::File::open(right).await?;
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_read = left.read(&mut left_buffer).await?;
        let right_read = right.read(&mut right_buffer).await?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

pub(super) async fn write_capture(
    dir: &Path,
    state: &Value,
    health_log: &Value,
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    set_owner_only(dir)?;
    atomic_write(dir.join("state.json"), serde_json::to_vec(state)?).await?;
    atomic_write(dir.join("health.log"), serde_json::to_vec(health_log)?).await?;
    Ok(())
}

pub(super) async fn write_manifest(dir: &Path, records: &[EvidenceMetadata]) -> anyhow::Result<()> {
    atomic_write(dir.join("manifest.json"), serde_json::to_vec(records)?).await
}

async fn atomic_write(path: PathBuf, bytes: Vec<u8>) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let tmp = path.with_extension("tmp");
    match tokio::fs::remove_file(&tmp).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut file = create_private_file(&tmp).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    set_owner_only(&tmp)?;
    tokio::fs::rename(tmp, path).await?;
    Ok(())
}

pub(super) async fn archive_dir(
    source: &Path,
    part: &Path,
    final_path: &Path,
) -> anyhow::Result<()> {
    if !source.is_dir() {
        anyhow::bail!("evidence spool missing: {}", source.display());
    }
    let _ = tokio::fs::remove_file(part).await;
    let _ = tokio::fs::remove_file(final_path).await;
    let total_bytes = directory_size(source).await.unwrap_or(0);
    backup_helper::archive_directory(source, part, final_path, total_bytes).await
}

async fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut entries = tokio::fs::read_dir(current).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

pub(super) fn path_component(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty() || matches!(result.as_str(), "." | "..") {
        result.clear();
        result.push('_');
    }
    result
}

pub(super) fn set_owner_only(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(if path.is_dir() { 0o700 } else { 0o600 });
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn recovered_log_file_is_owner_only_when_created() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("dockrev-private-log-{}", ulid::Ulid::new()));
        tokio::fs::create_dir_all(&root).await.expect("test root");
        let spool = root.join("job");
        let candidate_dir = spool.join("service").join("candidate");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        let log_path = candidate_dir.join("container.log");
        let mut records = [EvidenceMetadata {
            service_id: "service".to_string(),
            candidate_id: "candidate".to_string(),
            capture_errors: vec![CAPTURE_INTERRUPTED_REASON.to_string()],
            ..Default::default()
        }];
        recover_interrupted_capture(&spool, &mut records)
            .await
            .expect("recover interrupted capture");
        let file = tokio::fs::File::open(&log_path)
            .await
            .expect("recovered log");

        assert_eq!(
            file.metadata()
                .await
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        drop(file);
        tokio::fs::remove_dir_all(root)
            .await
            .expect("remove test root");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn evidence_archive_is_owner_only_after_creation() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("dockrev-private-archive-{}", ulid::Ulid::new()));
        let spool = root.join("job");
        tokio::fs::create_dir_all(&spool).await.expect("spool");
        tokio::fs::write(spool.join("container.log"), b"private candidate output")
            .await
            .expect("candidate log");
        let part = root.join("job.tar.zst.part");
        let archive = root.join("job.tar.zst");

        archive_dir(&spool, &part, &archive)
            .await
            .expect("build archive");

        assert_eq!(
            tokio::fs::metadata(&archive)
                .await
                .expect("archive metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        tokio::fs::remove_dir_all(root)
            .await
            .expect("remove test root");
    }
}
