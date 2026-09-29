use super::*;
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
    write_task
        .await
        .expect("tar input task")
        .expect("tar input should be written");
    assert!(extracted.status.success());
    extracted.stdout
}
