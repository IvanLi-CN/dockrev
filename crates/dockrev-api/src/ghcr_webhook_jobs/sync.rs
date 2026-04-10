use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
};

use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::api::types::{JobListItem, JobType};

use super::GhcrWebhookOp;

const GHCR_SYNC_REPO_LOCK_STRIPES: usize = 128;

static GHCR_SYNC_ENQUEUE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static GHCR_SYNC_REPO_LOCKS: OnceLock<Vec<Arc<Mutex<()>>>> = OnceLock::new();

pub(super) fn sync_enqueue_lock() -> &'static Mutex<()> {
    GHCR_SYNC_ENQUEUE_LOCK.get_or_init(|| Mutex::new(()))
}

fn repo_sync_locks() -> &'static [Arc<Mutex<()>>] {
    GHCR_SYNC_REPO_LOCKS
        .get_or_init(|| {
            (0..GHCR_SYNC_REPO_LOCK_STRIPES)
                .map(|_| Arc::new(Mutex::new(())))
                .collect()
        })
        .as_slice()
}

fn repo_sync_lock_index(key: &str) -> usize {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % GHCR_SYNC_REPO_LOCK_STRIPES
}

pub(super) async fn lock_repo_sync(owner: &str, repo: &str) -> OwnedMutexGuard<()> {
    let key = format!("{owner}/{repo}").to_ascii_lowercase();
    let repo_lock = repo_sync_locks()[repo_sync_lock_index(&key)].clone();
    repo_lock.lock_owned().await
}

pub(super) fn repo_unregistration_in_progress(webhook_state: &str, last_op: Option<&str>) -> bool {
    matches!(webhook_state, "queued" | "running")
        && last_op == Some(GhcrWebhookOp::Unregister.as_str())
}

pub(super) fn repo_registration_in_progress(webhook_state: &str, last_op: Option<&str>) -> bool {
    matches!(webhook_state, "queued" | "running")
        && last_op == Some(GhcrWebhookOp::Register.as_str())
}

pub(super) fn is_pending_status(status: &str) -> bool {
    status == "queued" || status == "running"
}

pub(super) fn is_legacy_register_job(job: &JobListItem) -> bool {
    job.r#type.as_str() == JobType::GitHubPackagesWebhook.as_str()
        && job.summary_json.get("op").and_then(|v| v.as_str())
            == Some(GhcrWebhookOp::Register.as_str())
}
