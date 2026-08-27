use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::runner::{CommandRunner, CommandSpec};

pub(crate) fn legacy_artifact_key(
    storage: &crate::backup_storage::BackupStorage,
    artifact_path: &str,
) -> Option<PathBuf> {
    let key = Path::new(artifact_path)
        .strip_prefix(storage.logical_root())
        .ok()
        .filter(|key| !key.as_os_str().is_empty())?;
    key.components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
        .then(|| key.to_path_buf())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactCleanupOutcome {
    Deleted,
    Missing,
}

#[cfg(test)]
pub(crate) async fn reconcile_artifact(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<ArtifactCleanupOutcome> {
    if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
        return Ok(ArtifactCleanupOutcome::Missing);
    }

    match delete_artifact(runner, storage, helper_image_fallback, artifact_key).await {
        Ok(()) => Ok(ArtifactCleanupOutcome::Deleted),
        Err(error) => {
            if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
                Ok(ArtifactCleanupOutcome::Missing)
            } else {
                Err(error)
            }
        }
    }
}

async fn ensure_local_path_within_root(
    logical_root: &Path,
    root: &Path,
    artifact_key: &Path,
) -> anyhow::Result<()> {
    let mut component_path = logical_root.to_path_buf();
    let components = artifact_key.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        if index + 1 == components.len() {
            break;
        }
        component_path.push(component.as_os_str());
        match tokio::fs::symlink_metadata(&component_path).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if let Ok(resolved) = tokio::fs::canonicalize(&component_path).await
                    && !resolved.starts_with(root)
                {
                    return Err(anyhow::anyhow!(
                        "backup artifact path resolves outside managed storage: {}",
                        component_path.display()
                    ));
                }
                return Err(anyhow::anyhow!(
                    "backup artifact path contains a symlink: {}",
                    component_path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    let artifact_path = logical_root.join(artifact_key);
    let mut parent = artifact_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("backup artifact has no parent path"))?
        .to_path_buf();
    loop {
        match tokio::fs::symlink_metadata(&parent).await {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !parent.pop() {
                    return Err(anyhow::anyhow!(
                        "backup artifact parent path is unavailable"
                    ));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let resolved_parent = tokio::fs::canonicalize(&parent).await?;
    if !resolved_parent.starts_with(root) {
        return Err(anyhow::anyhow!(
            "backup artifact parent resolves outside managed storage: {}",
            artifact_path.display()
        ));
    }
    Ok(())
}

pub(crate) async fn artifact_exists(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<bool> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let root = tokio::fs::canonicalize(logical_root).await?;
            let artifact_path = logical_root.join(artifact_key);
            ensure_local_path_within_root(logical_root, &root, artifact_key).await?;
            let final_is_symlink = tokio::fs::symlink_metadata(&artifact_path)
                .await
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
            let resolved_artifact = match tokio::fs::canonicalize(&artifact_path).await {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if final_is_symlink {
                        return Err(anyhow::anyhow!(
                            "backup artifact is a symlink: {}",
                            artifact_path.display()
                        ));
                    }
                    return Ok(false);
                }
                Err(error) => return Err(error.into()),
            };
            if !resolved_artifact.starts_with(&root) {
                return Err(anyhow::anyhow!(
                    "backup artifact resolves outside managed storage: {}",
                    artifact_path.display()
                ));
            }
            if final_is_symlink {
                return Err(anyhow::anyhow!(
                    "backup artifact is a symlink: {}",
                    artifact_path.display()
                ));
            }
            Ok(resolved_artifact.is_file())
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root:ro"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "sh".to_string(),
                            "-ec".to_string(),
                            r#"managed=$(readlink -f -- "$2") || exit 2
case "$managed" in
  /out-root|/out-root/*) ;;
  *) printf 'managed backup root resolves outside mounted storage: %s\n' "$managed" >&2; exit 2 ;;
esac
path_without_root=${1#/out-root/}
old_ifs=$IFS
IFS=/
current=/out-root
for component in $path_without_root; do
  [ -n "$component" ] || continue
  current="$current/$component"
  if [ -L "$current" ]; then printf 'backup artifact path contains a symlink: %s\n' "$current" >&2; exit 2; fi
done
IFS=$old_ifs
parent=$(dirname "$1")
while [ ! -e "$parent" ] && [ ! -L "$parent" ]; do
  next=$(dirname "$parent")
  [ "$next" = "$parent" ] && break
  parent="$next"
done
resolved_parent=$(readlink -f -- "$parent") || exit 2
case "$resolved_parent" in
  "$managed"|"$managed"/*) ;;
  *) printf 'backup artifact parent resolves outside managed storage: %s\n' "$resolved_parent" >&2; exit 2 ;;
esac
if [ -L "$1" ]; then printf 'backup artifact must not be a symlink: %s\n' "$1" >&2; exit 2; fi
if [ ! -e "$1" ] && [ ! -L "$1" ]; then exit 1; fi
resolved=$(readlink -f -- "$1") || exit 3
case "$resolved" in
  "$managed"/*) [ -f "$resolved" ] && exit 0 || exit 1 ;;
  *) printf 'backup artifact resolves outside managed storage: %s\n' "$resolved" >&2; exit 2 ;;
esac"#
                                .to_string(),
                            "dockrev-backup-check".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                            format!("/out-root/{}", relative.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            match out.status {
                0 => Ok(true),
                1 => Ok(false),
                3 => Err(anyhow::anyhow!(
                    "backup artifact existence resolution failed: {}",
                    out.stderr.trim()
                )),
                status => Err(anyhow::anyhow!(
                    "backup artifact existence check failed: status={} stderr={}",
                    status,
                    out.stderr.trim()
                )),
            }
        }
    }
}

pub(crate) async fn delete_artifact(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<()> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let root = tokio::fs::canonicalize(logical_root).await?;
            let artifact_path = logical_root.join(artifact_key);
            ensure_local_path_within_root(logical_root, &root, artifact_key).await?;
            let resolved_artifact = tokio::fs::canonicalize(&artifact_path).await?;
            if !resolved_artifact.starts_with(&root) {
                return Err(anyhow::anyhow!(
                    "backup artifact resolves outside managed storage: {}",
                    artifact_path.display()
                ));
            }
            if tokio::fs::symlink_metadata(&artifact_path)
                .await
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(anyhow::anyhow!(
                    "backup artifact is a symlink: {}",
                    artifact_path.display()
                ));
            }
            tokio::fs::remove_file(resolved_artifact).await?;
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "sh".to_string(),
                            "-ec".to_string(),
                            r#"managed=$(readlink -f -- "$2") || exit 2
case "$managed" in
  /out-root|/out-root/*) ;;
  *) printf 'managed backup root resolves outside mounted storage: %s\n' "$managed" >&2; exit 2 ;;
esac
path_without_root=${1#/out-root/}
old_ifs=$IFS
IFS=/
current=/out-root
for component in $path_without_root; do
  [ -n "$component" ] || continue
  current="$current/$component"
  if [ -L "$current" ]; then printf 'backup artifact path contains a symlink: %s\n' "$current" >&2; exit 2; fi
done
IFS=$old_ifs
parent=$(dirname "$1")
while [ ! -e "$parent" ] && [ ! -L "$parent" ]; do
  next=$(dirname "$parent")
  [ "$next" = "$parent" ] && break
  parent="$next"
done
resolved_parent=$(readlink -f -- "$parent") || exit 2
case "$resolved_parent" in
  "$managed"|"$managed"/*) ;;
  *) printf 'backup artifact parent resolves outside managed storage: %s\n' "$resolved_parent" >&2; exit 2 ;;
esac
if [ -L "$1" ]; then printf 'backup artifact must not be a symlink: %s\n' "$1" >&2; exit 2; fi
if [ ! -e "$1" ] && [ ! -L "$1" ]; then exit 1; fi
resolved=$(readlink -f -- "$1") || exit 3
case "$resolved" in
  "$managed"/*) rm -- "$resolved" ;;
  *) printf 'backup artifact resolves outside managed storage: %s\n' "$resolved" >&2; exit 2 ;;
esac"#
                                .to_string(),
                            "dockrev-backup-delete".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                            format!("/out-root/{}", relative.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            if out.status != 0 {
                return Err(anyhow::anyhow!(
                    "backup artifact delete failed: {}",
                    out.stderr
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn move_artifact_to_tombstone(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
    tombstone_key: &Path,
) -> anyhow::Result<()> {
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let root = tokio::fs::canonicalize(logical_root).await?;
            let artifact_path = logical_root.join(artifact_key);
            let tombstone_path = logical_root.join(tombstone_key);
            ensure_local_path_within_root(logical_root, &root, artifact_key).await?;
            ensure_local_path_within_root(logical_root, &root, tombstone_key).await?;
            let resolved_artifact = tokio::fs::canonicalize(&artifact_path).await?;
            let resolved_tombstone_parent = tokio::fs::canonicalize(
                tombstone_path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("backup artifact tombstone has no parent"))?,
            )
            .await?;
            let resolved_tombstone =
                resolved_tombstone_parent.join(tombstone_path.file_name().ok_or_else(|| {
                    anyhow::anyhow!("backup artifact tombstone has no file name")
                })?);
            if !resolved_artifact.starts_with(&root) || !resolved_tombstone.starts_with(&root) {
                return Err(anyhow::anyhow!(
                    "backup artifact tombstone is outside managed storage: {}",
                    tombstone_path.display()
                ));
            }
            if tokio::fs::symlink_metadata(&artifact_path)
                .await
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(anyhow::anyhow!(
                    "backup artifact is a symlink: {}",
                    artifact_path.display()
                ));
            }
            tokio::fs::rename(resolved_artifact, resolved_tombstone).await?;
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let tombstone = relative.join(tombstone_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "sh".to_string(),
                            "-ec".to_string(),
                            r#"managed=$(readlink -f -- "$2") || exit 2
case "$managed" in
  /out-root|/out-root/*) ;;
  *) printf 'managed backup root resolves outside mounted storage: %s\n' "$managed" >&2; exit 2 ;;
esac
path_without_root=${1#/out-root/}
old_ifs=$IFS
IFS=/
current=/out-root
for component in $path_without_root; do
  [ -n "$component" ] || continue
  current="$current/$component"
  if [ -L "$current" ]; then printf 'backup artifact path contains a symlink: %s\n' "$current" >&2; exit 2; fi
done
IFS=$old_ifs
parent=$(dirname "$1")
while [ ! -e "$parent" ] && [ ! -L "$parent" ]; do
  next=$(dirname "$parent")
  [ "$next" = "$parent" ] && break
  parent="$next"
done
resolved_parent=$(readlink -f -- "$parent") || exit 2
case "$resolved_parent" in
  "$managed"|"$managed"/*) ;;
  *) printf 'backup artifact parent resolves outside managed storage: %s\n' "$resolved_parent" >&2; exit 2 ;;
esac
if [ -L "$1" ]; then printf 'backup artifact must not be a symlink: %s\n' "$1" >&2; exit 2; fi
if [ ! -e "$1" ] && [ ! -L "$1" ]; then exit 1; fi
resolved=$(readlink -f -- "$1") || exit 3
tombstone="$3"
case "$resolved" in
  "$managed"/*) ;;
  *) printf 'backup artifact resolves outside managed storage: %s\n' "$resolved" >&2; exit 2 ;;
esac
case "$tombstone" in
  "$managed"/*) mv -- "$resolved" "$tombstone" ;;
  *) printf 'backup artifact tombstone resolves outside managed storage: %s\n' "$tombstone" >&2; exit 2 ;;
esac"#
                                .to_string(),
                            "dockrev-backup-tombstone".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                            format!("/out-root/{}", relative.to_string_lossy()),
                            format!("/out-root/{}", tombstone.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            if out.status != 0 {
                return Err(anyhow::anyhow!(
                    "backup artifact tombstone move failed: {}",
                    out.stderr
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn delete_artifact_if_present(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<()> {
    if !artifact_exists(runner, storage, helper_image_fallback, artifact_key).await? {
        return Ok(());
    }
    delete_artifact(runner, storage, helper_image_fallback, artifact_key).await
}

pub(crate) async fn find_artifact_tombstone(
    runner: &dyn CommandRunner,
    storage: &crate::backup_storage::BackupStorage,
    helper_image_fallback: &str,
    artifact_key: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    let artifact_name = artifact_key
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("backup artifact has no file name"))?
        .to_string_lossy()
        .to_string();
    match storage {
        crate::backup_storage::BackupStorage::Local { logical_root } => {
            let parent = artifact_key.parent().unwrap_or_else(|| Path::new(""));
            let mut entries = tokio::fs::read_dir(logical_root.join(parent)).await?;
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with(".dockrev-delete-")
                    || !name.ends_with(&format!("-{artifact_name}"))
                {
                    continue;
                }
                let candidate = parent.join(entry.file_name());
                if artifact_exists(runner, storage, helper_image_fallback, &candidate).await? {
                    return Ok(Some(candidate));
                }
            }
            Ok(None)
        }
        _ => {
            let (source, relative) = storage.helper_output_mount();
            let path = relative.join(artifact_key);
            let out = runner
                .run(
                    CommandSpec {
                        program: "docker".to_string(),
                        args: vec![
                            "run".to_string(),
                            "--rm".to_string(),
                            "-v".to_string(),
                            format!("{source}:/out-root:ro"),
                            storage.helper_image(helper_image_fallback).to_string(),
                            "sh".to_string(),
                            "-ec".to_string(),
                            r#"managed=$(readlink -f -- "$2") || exit 2
case "$managed" in
  /out-root|/out-root/*) ;;
  *) printf 'managed backup root resolves outside mounted storage: %s\n' "$managed" >&2; exit 2 ;;
esac
parent=$(dirname "$1")
resolved_parent=$(readlink -f -- "$parent") || exit 2
case "$resolved_parent" in
  "$managed"|"$managed"/*) ;;
  *) printf 'backup artifact parent resolves outside managed storage: %s\n' "$resolved_parent" >&2; exit 2 ;;
esac
artifact_name=${1##*/}
for candidate in "$parent"/.dockrev-delete-*-"$artifact_name"; do
  if [ -f "$candidate" ] && [ ! -L "$candidate" ]; then
    printf '%s\n' "$candidate"
    exit 0
  fi
done
exit 1"#
                                .to_string(),
                            "dockrev-backup-tombstone-find".to_string(),
                            format!("/out-root/{}", path.to_string_lossy()),
                            format!("/out-root/{}", relative.to_string_lossy()),
                        ],
                        env: Vec::new(),
                    },
                    Duration::from_secs(20),
                )
                .await?;
            match out.status {
                0 => out
                    .stdout
                    .trim()
                    .strip_prefix("/out-root/")
                    .map(PathBuf::from)
                    .ok_or_else(|| {
                        anyhow::anyhow!("backup tombstone helper returned an invalid path")
                    })
                    .map(Some),
                1 => Ok(None),
                status => Err(anyhow::anyhow!(
                    "backup tombstone discovery failed: status={} stderr={}",
                    status,
                    out.stderr.trim()
                )),
            }
        }
    }
}
