use std::path::{Path, PathBuf};

use anyhow::Context;
use ulid::Ulid;

#[derive(Clone, Debug)]
pub(crate) struct TempDirCleanup(pub(crate) PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DockerCliAuthBridge {
    pub(crate) docker_config_dir: PathBuf,
    _cleanup: TempDirCleanup,
}

impl DockerCliAuthBridge {
    pub(crate) fn stage(docker_config_path: &Path) -> anyhow::Result<Self> {
        let temp_root = std::env::temp_dir().join(format!("dockrev-auth-config-{}", Ulid::new()));
        let docker_config_dir = temp_root.join(".docker");
        let source_dir = docker_config_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let source_file_name = docker_config_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        std::fs::create_dir_all(&docker_config_dir).with_context(|| {
            format!(
                "create docker auth workspace {}",
                docker_config_dir.display()
            )
        })?;
        if source_file_name == "config.json" {
            copy_selected_docker_config_metadata(source_dir, &docker_config_dir)?;
        }

        let staged_config_path = docker_config_dir.join("config.json");
        std::fs::copy(docker_config_path, &staged_config_path).with_context(|| {
            format!(
                "stage docker config {} -> {}",
                docker_config_path.display(),
                staged_config_path.display()
            )
        })?;

        Ok(Self {
            docker_config_dir,
            _cleanup: TempDirCleanup(temp_root),
        })
    }

    pub(crate) fn env(&self) -> Vec<(String, String)> {
        vec![(
            "DOCKER_CONFIG".to_string(),
            self.docker_config_dir.to_string_lossy().to_string(),
        )]
    }
}

fn copy_selected_docker_config_metadata(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let contexts_src = src.join("contexts");
    if contexts_src.is_dir() {
        copy_dir_recursively(&contexts_src, &dest.join("contexts")).with_context(|| {
            format!(
                "stage docker config contexts {} -> {}",
                contexts_src.display(),
                dest.join("contexts").display()
            )
        })?;
    }
    Ok(())
}

fn copy_dir_recursively(src: &Path, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursively(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}
