use std::path::{Path, PathBuf};

use anyhow::Context as _;

pub(super) fn ensure_parent_dir(path: &Path) -> anyhow::Result<PathBuf> {
    let path = path.to_path_buf();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create dir {:?}", parent))?;
    }
    Ok(path)
}
