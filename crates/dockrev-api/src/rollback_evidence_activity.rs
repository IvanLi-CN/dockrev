use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

static ACTIVE_SPOOLS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

pub(super) struct ActiveSpool {
    path: PathBuf,
    released: AtomicBool,
}

impl ActiveSpool {
    pub(super) fn register(path: PathBuf) -> Self {
        let mut active = active_spools()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active.entry(path.clone()).or_default() += 1;
        Self {
            path,
            released: AtomicBool::new(false),
        }
    }

    pub(super) fn release(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut active = active_spools()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remove = if let Some(count) = active.get_mut(&self.path) {
            *count -= 1;
            *count == 0
        } else {
            false
        };
        if remove {
            active.remove(&self.path);
        }
    }
}

impl super::RollbackEvidenceContext {
    pub(crate) fn protect_active_spool_from_recovery(&mut self) {
        if self.recovery_activity.is_none() {
            self.recovery_activity = Some(Arc::new(ActiveSpool::register(self.job_spool_path())));
        }
    }

    pub(crate) fn release_active_spool_from_recovery(&self) {
        if let Some(activity) = &self.recovery_activity {
            activity.release();
        }
    }
}

impl Drop for ActiveSpool {
    fn drop(&mut self) {
        self.release();
    }
}

pub(super) fn is_active(path: &Path) -> bool {
    active_spools()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(path)
}

fn active_spools() -> &'static Mutex<HashMap<PathBuf, usize>> {
    ACTIVE_SPOOLS.get_or_init(|| Mutex::new(HashMap::new()))
}
