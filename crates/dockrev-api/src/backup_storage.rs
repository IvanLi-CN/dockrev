use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::runner::{CommandRunner, CommandSpec};

pub const DOCKREV_ROLE_LABEL: &str = "cc.ivanli.dockrev.role=api";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStorageInfo {
    pub mode: String,
    pub logical_path: String,
    pub resolved_location: String,
    pub writable: bool,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupStorage {
    Local {
        logical_root: PathBuf,
    },
    Docker {
        logical_root: PathBuf,
        mount_source: String,
        mount_relative: PathBuf,
        mount_type: String,
        helper_image: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerInspect {
    image: String,
    #[serde(default)]
    mounts: Vec<ContainerMount>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerMount {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    name: String,
    destination: String,
    #[serde(rename = "RW", default)]
    writable: bool,
}

impl BackupStorage {
    pub fn logical_root(&self) -> &Path {
        match self {
            Self::Local { logical_root } | Self::Docker { logical_root, .. } => logical_root,
        }
    }

    pub fn artifact_key(&self, stack_id: &str, file_name: &str) -> PathBuf {
        PathBuf::from(stack_id).join(file_name)
    }

    pub fn logical_artifact_path(&self, key: &Path) -> PathBuf {
        self.logical_root().join(key)
    }

    pub fn helper_output_mount(&self) -> (String, PathBuf) {
        match self {
            Self::Local { logical_root } => {
                (logical_root.to_string_lossy().to_string(), PathBuf::new())
            }
            Self::Docker {
                mount_source,
                mount_relative,
                ..
            } => (mount_source.clone(), mount_relative.clone()),
        }
    }

    pub fn helper_image<'a>(&'a self, fallback: &'a str) -> &'a str {
        match self {
            Self::Docker { helper_image, .. } => helper_image,
            Self::Local { .. } => fallback,
        }
    }

    pub fn info(&self) -> BackupStorageInfo {
        match self {
            Self::Local { logical_root } => BackupStorageInfo {
                mode: "local".to_string(),
                logical_path: logical_root.to_string_lossy().to_string(),
                resolved_location: logical_root.to_string_lossy().to_string(),
                writable: true,
                diagnostic: None,
            },
            Self::Docker {
                logical_root,
                mount_source,
                mount_relative,
                mount_type,
                ..
            } => {
                let suffix = mount_relative.to_string_lossy();
                BackupStorageInfo {
                    mode: format!("docker_{mount_type}"),
                    logical_path: logical_root.to_string_lossy().to_string(),
                    resolved_location: if suffix.is_empty() {
                        mount_source.clone()
                    } else {
                        format!("{}:/{}", mount_source, suffix.trim_start_matches('/'))
                    },
                    writable: true,
                    diagnostic: None,
                }
            }
        }
    }
}

pub fn logical_backup_root(db_path: &Path) -> anyhow::Result<PathBuf> {
    let parent = db_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("DOCKREV_DB_PATH has no parent directory"))?;
    let root = normalize_path(&parent.join("backups"));
    if root.is_absolute() {
        Ok(root)
    } else {
        Ok(normalize_path(&std::env::current_dir()?.join(root)))
    }
}

pub async fn resolve_backup_storage(
    runner: &dyn CommandRunner,
    db_path: &Path,
) -> anyhow::Result<BackupStorage> {
    let logical_root = logical_backup_root(db_path)?;
    if !is_containerized() {
        return Ok(BackupStorage::Local { logical_root });
    }

    let container_id = resolve_self_container_id(runner).await?;
    let inspect = inspect_container(runner, &container_id).await?;
    resolve_from_inspect(logical_root, inspect)
}

fn is_containerized() -> bool {
    Path::new("/.dockerenv").exists()
        || std::env::var("DOCKREV_CONTAINERIZED")
            .is_ok_and(|value| value.eq_ignore_ascii_case("true"))
}

async fn resolve_self_container_id(runner: &dyn CommandRunner) -> anyhow::Result<String> {
    let by_role = run_docker(
        runner,
        vec![
            "ps".to_string(),
            "-q".to_string(),
            "--filter".to_string(),
            format!("label={DOCKREV_ROLE_LABEL}"),
        ],
    )
    .await?;
    let role_candidates = non_empty_lines(&by_role);
    match role_candidates.as_slice() {
        [only] => return Ok(only.clone()),
        [] => {}
        _ => {
            return Err(anyhow::anyhow!(
                "multiple Dockrev API containers match {DOCKREV_ROLE_LABEL}"
            ));
        }
    }

    if let Ok(hostname) = std::env::var("HOSTNAME")
        && let Ok(inspected) = run_docker(
            runner,
            vec![
                "inspect".to_string(),
                "--format={{.Id}}".to_string(),
                hostname,
            ],
        )
        .await
    {
        let candidates = non_empty_lines(&inspected);
        if let [only] = candidates.as_slice() {
            return Ok(only.clone());
        }
    }

    if let Ok(image) = std::env::var("DOCKREV_IMAGE_REPO") {
        let candidates = non_empty_lines(
            &run_docker(
                runner,
                vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("ancestor={image}"),
                ],
            )
            .await?,
        );
        if let [only] = candidates.as_slice() {
            return Ok(only.clone());
        }
    }

    for compose_service in ["dockrev", "api"] {
        let candidates = non_empty_lines(
            &run_docker(
                runner,
                vec![
                    "ps".to_string(),
                    "-q".to_string(),
                    "--filter".to_string(),
                    format!("label=com.docker.compose.service={compose_service}"),
                ],
            )
            .await?,
        );
        match candidates.as_slice() {
            [only] => return Ok(only.clone()),
            [] => {}
            _ => return Err(anyhow::anyhow!("Dockrev Compose identity is ambiguous")),
        }
    }
    Err(anyhow::anyhow!(
        "Dockrev container identity cannot be resolved"
    ))
}

async fn inspect_container(
    runner: &dyn CommandRunner,
    container_id: &str,
) -> anyhow::Result<ContainerInspect> {
    let raw = run_docker(
        runner,
        vec![
            "inspect".to_string(),
            "--format={{json .}}".to_string(),
            container_id.to_string(),
        ],
    )
    .await?;
    serde_json::from_str(raw.trim()).map_err(Into::into)
}

async fn run_docker(runner: &dyn CommandRunner, args: Vec<String>) -> anyhow::Result<String> {
    let out = runner
        .run(
            CommandSpec {
                program: "docker".to_string(),
                args,
                env: Vec::new(),
            },
            Duration::from_secs(15),
        )
        .await?;
    if out.status != 0 {
        return Err(anyhow::anyhow!(
            "Docker inspection failed: {}",
            out.stderr.trim()
        ));
    }
    Ok(out.stdout)
}

fn non_empty_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn resolve_from_inspect(
    logical_root: PathBuf,
    inspect: ContainerInspect,
) -> anyhow::Result<BackupStorage> {
    let mut matches = inspect
        .mounts
        .into_iter()
        .filter(|mount| mount.writable)
        .filter_map(|mount| {
            let destination = normalize_path(Path::new(&mount.destination));
            logical_root
                .strip_prefix(&destination)
                .ok()
                .map(|relative| (destination, relative.to_path_buf(), mount))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(destination, _, _)| std::cmp::Reverse(destination.components().count()));
    if matches.len() > 1
        && matches[0].0.components().count() == matches[1].0.components().count()
        && matches[0].0 == matches[1].0
    {
        return Err(anyhow::anyhow!(
            "logical backup directory {} has ambiguous writable mounts",
            logical_root.display()
        ));
    }
    let Some((_best_destination, relative, mount)) = matches.into_iter().next() else {
        return Err(anyhow::anyhow!(
            "logical backup directory {} is not covered by a writable Dockrev mount",
            logical_root.display()
        ));
    };
    let source = match mount.kind.as_str() {
        "bind" if !mount.source.is_empty() => mount.source,
        "volume" if !mount.name.is_empty() => mount.name,
        other => return Err(anyhow::anyhow!("unsupported backup mount type: {other}")),
    };
    Ok(BackupStorage::Docker {
        logical_root,
        mount_source: source,
        mount_relative: relative,
        mount_type: mount.kind,
        helper_image: inspect.image,
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let ends_with_parent = result
                    .components()
                    .next_back()
                    .is_some_and(|last| last == Component::ParentDir);
                if path.is_absolute() {
                    result.pop();
                } else if result.as_os_str().is_empty() || ends_with_parent {
                    result.push(Component::ParentDir.as_os_str());
                } else {
                    result.pop();
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inspect(mounts: serde_json::Value) -> ContainerInspect {
        serde_json::from_value(serde_json::json!({
            "Image": "sha256:dockrev",
            "Mounts": mounts,
        }))
        .unwrap()
    }

    #[test]
    fn chooses_longest_writable_mount() {
        let storage = resolve_from_inspect(
            PathBuf::from("/data/state/backups"),
            inspect(serde_json::json!([
                {"Type":"bind","Source":"/host/data","Destination":"/data","RW":true},
                {"Type":"volume","Name":"state","Destination":"/data/state","RW":true}
            ])),
        )
        .unwrap();
        assert_eq!(
            storage,
            BackupStorage::Docker {
                logical_root: PathBuf::from("/data/state/backups"),
                mount_source: "state".to_string(),
                mount_relative: PathBuf::from("backups"),
                mount_type: "volume".to_string(),
                helper_image: "sha256:dockrev".to_string(),
            }
        );
    }

    #[test]
    fn rejects_read_only_or_uncovered_mounts() {
        let error = resolve_from_inspect(
            PathBuf::from("/data/backups"),
            inspect(serde_json::json!([
                {"Type":"bind","Source":"/host/data","Destination":"/data","RW":false}
            ])),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not covered by a writable"));
    }

    #[test]
    fn rejects_ambiguous_equal_destination_mounts() {
        let error = resolve_from_inspect(
            PathBuf::from("/data/backups"),
            inspect(serde_json::json!([
                {"Type":"bind","Source":"/host/a","Destination":"/data","RW":true},
                {"Type":"volume","Name":"data-b","Destination":"/data","RW":true}
            ])),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ambiguous writable mounts"));
    }

    #[test]
    fn preserves_relative_parent_components() {
        assert_eq!(
            normalize_path(Path::new("../state/backups")),
            PathBuf::from("../state/backups")
        );
        assert_eq!(
            normalize_path(Path::new("../../state")),
            PathBuf::from("../../state")
        );
        assert_eq!(
            normalize_path(Path::new("state/../backups")),
            PathBuf::from("backups")
        );
    }
}
