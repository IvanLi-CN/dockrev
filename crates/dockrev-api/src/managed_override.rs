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
static MANAGED_OVERRIDE_OPERATION_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

pub const STALE_TEMP_WARNING: &str = "warning:config_files_stale_dockrev_temp_override";

pub fn lock() -> MutexGuard<'static, ()> {
    MANAGED_OVERRIDE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn try_operation_lock() -> Option<tokio::sync::MutexGuard<'static, ()>> {
    MANAGED_OVERRIDE_OPERATION_LOCK.try_lock().ok()
}

pub async fn operation_lock() -> tokio::sync::MutexGuard<'static, ()> {
    MANAGED_OVERRIDE_OPERATION_LOCK.lock().await
}

pub fn managed_override_path(root: &Path, stack_id: &str) -> PathBuf {
    let hash = hex::encode(digest(&SHA256, stack_id.as_bytes()).as_ref());
    root.join(format!("stack-{hash}.yml"))
}

pub fn configured_root(db_path: &Path) -> anyhow::Result<PathBuf> {
    #[cfg(test)]
    {
        if let Some(value) = std::env::var_os("DOCKREV_MANAGED_OVERRIDE_DIR") {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                anyhow::bail!("DOCKREV_MANAGED_OVERRIDE_DIR must be absolute");
            }
            return Ok(path);
        }
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
    let services_value = root
        .get(Value::String("services".to_string()))
        .ok_or_else(|| anyhow::anyhow!("managed override must contain services"))?;
    let Some(services) = services_value.as_mapping() else {
        // Older snapshots used `services:` for an empty override. Treat that representation as
        // an empty, safe image-only document while keeping all populated documents strict.
        if services_value.is_null() {
            return Ok(());
        }
        anyhow::bail!("managed override services must be a mapping");
    };
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
                    || digest.len() != 64
                    || !digest.chars().all(|c| c.is_ascii_hexdigit())
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

#[cfg(test)]
pub fn commit_with_snapshot(path: &Path, contents: &str) -> anyhow::Result<Option<String>> {
    let services = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(contents)
        .ok()
        .and_then(|value| {
            value
                .get("services")
                .and_then(serde_yaml_ng::Value::as_mapping)
                .map(|services| {
                    services
                        .keys()
                        .filter_map(serde_yaml_ng::Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();
    commit_with_snapshot_for_services(path, contents, &services)
}

pub fn commit_with_snapshot_for_services(
    path: &Path,
    contents: &str,
    services: &[String],
) -> anyhow::Result<Option<String>> {
    let previous = if path.exists() {
        let snapshot = format!("{}.previous", path.display());
        atomic_commit(Path::new(&snapshot), &fs::read_to_string(path)?)?;
        Some(snapshot)
    } else {
        let snapshot = format!("{}.previous", path.display());
        atomic_commit(Path::new(&snapshot), "services: {}\n")?;
        Some(snapshot)
    };
    atomic_commit(
        &PathBuf::from(format!("{}.pending", path.display())),
        &serde_json::json!({"phase": "prepared", "services": services}).to_string(),
    )?;
    // Publish the recovery marker before replacing the active file. A crash at any point after
    // this marker is therefore recoverable to the last committed snapshot.
    atomic_commit(path, contents)?;
    Ok(previous)
}

pub fn has_pending_snapshot(path: &Path) -> bool {
    Path::new(&format!("{}.pending", path.display())).is_file()
}

fn read_pending_marker(path: &Path) -> anyhow::Result<Option<(String, Vec<String>)>> {
    if !has_pending_snapshot(path) {
        return Ok(None);
    }
    let marker = fs::read_to_string(format!("{}.pending", path.display()))?;
    let trimmed = marker.trim();
    if trimmed.eq_ignore_ascii_case("pending") {
        return Ok(Some(("prepared".to_string(), Vec::new())));
    }
    if trimmed.eq_ignore_ascii_case("applied") {
        return Ok(Some(("applied".to_string(), Vec::new())));
    }
    let value: serde_json::Value =
        serde_json::from_str(trimmed).context("parse managed override pending marker")?;
    let phase = value
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("managed override pending marker has no phase"))?;
    let services = value
        .get("services")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("managed override pending marker has no services"))?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    Ok(Some((phase.to_string(), services)))
}

pub fn pending_snapshot_services(path: &Path) -> anyhow::Result<Vec<String>> {
    let Some((phase, services)) = read_pending_marker(path)? else {
        return Ok(Vec::new());
    };
    if phase == "prepared" && legacy_pending_marker(path)? {
        return infer_legacy_pending_services(path);
    }
    Ok(services)
}

fn legacy_pending_marker(path: &Path) -> anyhow::Result<bool> {
    if !has_pending_snapshot(path) {
        return Ok(false);
    }
    let marker = fs::read_to_string(format!("{}.pending", path.display()))?;
    let trimmed = marker.trim();
    Ok(trimmed.eq_ignore_ascii_case("pending"))
}

pub fn pending_snapshot_is_legacy(path: &Path) -> anyhow::Result<bool> {
    legacy_pending_marker(path)
}

fn infer_legacy_pending_services(path: &Path) -> anyhow::Result<Vec<String>> {
    let active = read_override_images(path)?;
    let previous = read_override_images(&PathBuf::from(format!("{}.previous", path.display())))?;
    let names = active
        .keys()
        .chain(previous.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    Ok(names
        .into_iter()
        .filter(|name| active.get(name) != previous.get(name))
        .collect())
}

fn read_override_images(path: &Path) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    if !path.is_file() {
        return Ok(std::collections::BTreeMap::new());
    }
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&fs::read_to_string(path)?)?;
    let mut images = std::collections::BTreeMap::new();
    if let Some(services) = value
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping)
    {
        for (service, config) in services {
            if let (Some(service), Some(image)) = (
                service.as_str(),
                config.get("image").and_then(serde_yaml_ng::Value::as_str),
            ) {
                images.insert(service.to_string(), image.to_string());
            }
        }
    }
    Ok(images)
}

pub fn pending_snapshot_is_applied(path: &Path) -> anyhow::Result<bool> {
    Ok(read_pending_marker(path)?.is_some_and(|(phase, _)| phase == "applied"))
}

pub fn mark_snapshot_applied(path: &Path) -> anyhow::Result<()> {
    if has_pending_snapshot(path) {
        let services = read_pending_marker(path)?.map_or_else(Vec::new, |(_, services)| services);
        atomic_commit(
            &PathBuf::from(format!("{}.pending", path.display())),
            &serde_json::json!({"phase": "applied", "services": services}).to_string(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub fn recover_pending_snapshot(path: &Path) -> anyhow::Result<bool> {
    let _guard = lock();
    recover_pending_snapshot_unlocked(path)
}

fn recover_pending_snapshot_unlocked(path: &Path) -> anyhow::Result<bool> {
    if !has_pending_snapshot(path) {
        return Ok(false);
    }
    if pending_snapshot_is_applied(path)? {
        discard_snapshot(path)?;
        return Ok(false);
    }
    let snapshot = PathBuf::from(format!("{}.previous", path.display()));
    if !snapshot.is_file() {
        anyhow::bail!(
            "managed override pending marker has no previous snapshot: {}",
            path.display()
        );
    }
    restore_snapshot(path, Some(snapshot.to_string_lossy().as_ref()))?;
    discard_snapshot(path)?;
    Ok(true)
}

pub fn discard_snapshot(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("managed override path has no parent"))?;
    for sidecar in [
        PathBuf::from(format!("{}.previous", path.display())),
        PathBuf::from(format!("{}.pending", path.display())),
    ] {
        if sidecar.exists() {
            fs::remove_file(sidecar)?;
        }
    }
    sync_directory(parent);
    Ok(())
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
    if has_pending_snapshot(path) {
        recover_pending_snapshot_unlocked(path)?;
    } else if !path.exists() {
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

    #[tokio::test]
    async fn operation_lock_serializes_competing_lifecycle_work() {
        let first = operation_lock().await;
        assert!(try_operation_lock().is_none());
        drop(first);

        let second = try_operation_lock().expect("released operation lock should be available");
        drop(second);
        assert!(try_operation_lock().is_some());
        drop(try_operation_lock());
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

    #[test]
    fn pending_snapshot_recovery_restores_and_clears_transaction_state() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-managed-pending-recovery-{}",
            ulid::Ulid::new()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("stack.yml");
        commit_with_snapshot(&path, "services:\n  web:\n    image: old@sha256:1\n").unwrap();
        commit_with_snapshot(&path, "services:\n  web:\n    image: new@sha256:2\n").unwrap();

        assert!(has_pending_snapshot(&path));
        assert_eq!(pending_snapshot_services(&path).unwrap(), vec!["web"]);
        assert!(recover_pending_snapshot(&path).unwrap());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("old@sha256:1")
        );
        assert!(!has_pending_snapshot(&path));
        assert!(!Path::new(&format!("{}.previous", path.display())).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_pending_marker_infers_changed_services_from_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-managed-legacy-pending-{}",
            ulid::Ulid::new()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("stack.yml");
        atomic_commit(
            &path,
            "services:\n  web:\n    image: web@sha256:old\n  worker:\n    image: worker@sha256:same\n",
        )
        .unwrap();
        atomic_commit(
            &PathBuf::from(format!("{}.previous", path.display())),
            "services:\n  web:\n    image: web@sha256:new\n  worker:\n    image: worker@sha256:same\n",
        )
        .unwrap();
        atomic_commit(
            &PathBuf::from(format!("{}.pending", path.display())),
            "pending\n",
        )
        .unwrap();

        assert_eq!(pending_snapshot_services(&path).unwrap(), vec!["web"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn applied_snapshot_recovery_keeps_active_override_and_only_clears_sidecars() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-managed-applied-recovery-{}",
            ulid::Ulid::new()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("stack.yml");
        commit_with_snapshot(&path, "services:\n  web:\n    image: old@sha256:1\n").unwrap();
        commit_with_snapshot(&path, "services:\n  web:\n    image: new@sha256:2\n").unwrap();
        mark_snapshot_applied(&path).unwrap();

        assert!(pending_snapshot_is_applied(&path).unwrap());
        assert!(!recover_pending_snapshot(&path).unwrap());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("new@sha256:2")
        );
        assert!(!has_pending_snapshot(&path));
        std::fs::remove_dir_all(root).unwrap();
    }
}
