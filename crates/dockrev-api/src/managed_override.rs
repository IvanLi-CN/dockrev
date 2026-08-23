use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, MutexGuard},
};

use anyhow::Context as _;
use ring::digest::{SHA256, digest};

static MANAGED_OVERRIDE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub const STALE_TEMP_WARNING: &str = "warning:config_files_stale_dockrev_temp_override";

pub fn lock() -> MutexGuard<'static, ()> {
    MANAGED_OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn managed_override_path(root: &Path, stack_id: &str) -> PathBuf {
    let hash = hex::encode(digest(&SHA256, stack_id.as_bytes()).as_ref());
    root.join(format!("stack-{hash}.yml"))
}

pub fn configured_root(db_path: &Path) -> anyhow::Result<PathBuf> {
    #[cfg(test)]
    {
        let _ = db_path;
        let thread_name = std::thread::current()
            .name()
            .unwrap_or("test")
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>();
        Ok(std::env::temp_dir().join(format!(
            "dockrev-managed-{}-{thread_name}",
            std::process::id()
        )))
    }
    #[cfg(not(test))]
    configured_root_runtime(db_path)
}

#[cfg(not(test))]
fn configured_root_runtime(db_path: &Path) -> anyhow::Result<PathBuf> {
    if let Some(value) = std::env::var_os("DOCKREV_MANAGED_OVERRIDE_DIR") {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            anyhow::bail!("DOCKREV_MANAGED_OVERRIDE_DIR must be absolute");
        }
        return Ok(path);
    }
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let root = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        std::env::current_dir()?.join(parent)
    };
    Ok(root.join("managed-overrides"))
}

pub fn render_image_only_override(images: &[(String, String)]) -> anyhow::Result<String> {
    let mut output = String::from("services:\n");
    for (service, image) in images {
        let service = service.trim();
        let image = image.trim();
        if service.is_empty() || image.is_empty() {
            anyhow::bail!("managed override service and image must be non-empty");
        }
        if service.contains(['\n', '\r', ':']) || image.contains(['\n', '\r']) {
            anyhow::bail!("managed override contains unsupported control characters");
        }
        output.push_str("  ");
        output.push_str(service);
        output.push_str(":\n    image: ");
        output.push_str(image);
        output.push('\n');
    }
    Ok(output)
}

pub fn validate_image_only_yaml(
    contents: &str,
    allowed_services: &BTreeSet<String>,
) -> anyhow::Result<()> {
    validate_image_only_yaml_with_digest_policy(contents, allowed_services, true)
}

pub fn validate_image_only_yaml_relaxed(
    contents: &str,
    allowed_services: &BTreeSet<String>,
) -> anyhow::Result<()> {
    validate_image_only_yaml_with_digest_policy(contents, allowed_services, false)
}

fn validate_image_only_yaml_with_digest_policy(
    contents: &str,
    allowed_services: &BTreeSet<String>,
    strict_digest: bool,
) -> anyhow::Result<()> {
    use serde_yaml_ng::Value;

    let root: Value = serde_yaml_ng::from_str(contents).context("parse managed override yaml")?;
    let Some(root) = root.as_mapping() else {
        anyhow::bail!("managed override root must be a mapping");
    };
    for key in root.keys() {
        if key.as_str() != Some("services") {
            anyhow::bail!("managed override only permits the services key");
        }
    }
    let services = root
        .get(Value::String("services".to_string()))
        .ok_or_else(|| anyhow::anyhow!("managed override must contain services"))?
        .as_mapping()
        .ok_or_else(|| anyhow::anyhow!("managed override services must be a mapping"))?;
    for (service_key, service_value) in services {
        let service = service_key
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("managed override service name must be a string"))?;
        if !allowed_services.contains(service) {
            anyhow::bail!("managed override service is not part of the stack: {service}");
        }
        let service_map = service_value
            .as_mapping()
            .ok_or_else(|| anyhow::anyhow!("managed override service must be a mapping"))?;
        if service_map.len() != 1 || !service_map.contains_key(Value::String("image".to_string())) {
            anyhow::bail!("managed override service may only define image: {service}");
        }
        let image = service_map
            .get(Value::String("image".to_string()))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("managed override image must be a string: {service}"))?;
        if !image.contains("@sha256:")
            || image.split_once("@sha256:").is_none_or(|(_, digest)| {
                digest.is_empty()
                    || (strict_digest
                        && (digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit())))
            })
        {
            anyhow::bail!("managed override image must use a sha256 digest: {service}");
        }
    }
    Ok(())
}

pub fn atomic_commit(path: &Path, contents: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("managed override path has no parent"))?;
    if !parent.is_absolute() {
        anyhow::bail!("managed override path must be absolute");
    }
    fs::create_dir_all(parent).context("create managed override directory")?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        ulid::Ulid::new()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)
        .with_context(|| format!("replace managed override {}", path.display()))?;
    sync_directory(parent);
    Ok(())
}

pub fn commit_with_snapshot(path: &Path, contents: &str) -> anyhow::Result<Option<String>> {
    let previous = if path.exists() {
        let snapshot = format!("{}.previous", path.display());
        atomic_commit(Path::new(&snapshot), &fs::read_to_string(path)?)?;
        Some(snapshot)
    } else {
        let snapshot = format!("{}.previous", path.display());
        atomic_commit(Path::new(&snapshot), "services:\n")?;
        Some(snapshot)
    };
    atomic_commit(path, contents)?;
    Ok(previous)
}

pub fn restore_snapshot(path: &Path, snapshot: Option<&str>) -> anyhow::Result<()> {
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    let contents = fs::read_to_string(snapshot)?;
    atomic_commit(path, &contents)
}

pub fn recover_interrupted(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("managed override path has no parent"))?;
    fs::create_dir_all(parent)?;
    if !path.exists() {
        let snapshot = PathBuf::from(format!("{}.previous", path.display()));
        if snapshot.exists() {
            atomic_commit(path, &fs::read_to_string(snapshot)?)?;
        }
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&format!(
            ".{}.",
            path.file_name().unwrap_or_default().to_string_lossy()
        )) {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = OpenOptions::new().read(true).open(path) {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn path_is_absolute_and_stable_for_stack_id() {
        let root = Path::new("/var/lib/dockrev/managed-overrides");
        let first = managed_override_path(root, "stk_01HABC");
        assert!(first.is_absolute());
        assert_eq!(first, managed_override_path(root, "stk_01HABC"));
        assert_ne!(first, managed_override_path(root, "stk_01HXYZ"));
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".yml")
        );
    }

    #[test]
    fn rendered_override_is_image_only_and_rejects_unsafe_yaml() {
        let yaml = render_image_only_override(&[(
            "web".to_string(),
            format!("ghcr.io/acme/web@sha256:{}", "a".repeat(64)),
        )])
        .unwrap();
        let allowed = BTreeSet::from(["web".to_string()]);
        validate_image_only_yaml(&yaml, &allowed).unwrap();
        assert!(validate_image_only_yaml("services:\n  web:\n    volumes: [x]", &allowed).is_err());
    }

    #[test]
    fn atomic_commit_replaces_previous_contents_without_deleting_referenced_file() {
        let root = std::env::temp_dir().join(format!("dockrev-managed-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("stack.yml");
        atomic_commit(&path, "services:\n  web:\n    image: old@sha256:1\n").unwrap();
        atomic_commit(&path, "services:\n  web:\n    image: new@sha256:2\n").unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("new@sha256:2"),
        );
        assert!(path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_and_recovery_restore_last_committed_override() {
        let root =
            std::env::temp_dir().join(format!("dockrev-managed-recovery-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("stack.yml");
        commit_with_snapshot(&path, "services:\n  web:\n    image: old@sha256:1\n").unwrap();
        commit_with_snapshot(&path, "services:\n  web:\n    image: new@sha256:2\n").unwrap();
        std::fs::remove_file(&path).unwrap();
        recover_interrupted(&path).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("old@sha256:1")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
