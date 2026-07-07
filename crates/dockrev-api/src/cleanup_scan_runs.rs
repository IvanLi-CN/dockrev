use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, Notify};

use crate::{
    api::types::{CleanupScanResponse, CleanupScanRunEvent, CleanupScanRunPhase},
    ids,
};

const RETAIN_FINISHED_FOR: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct CleanupScanRunStoredEvent {
    pub id: u64,
    pub event_name: String,
    pub payload: CleanupScanRunEvent,
}

struct CleanupScanRunRecord {
    events: Vec<CleanupScanRunStoredEvent>,
    next_id: u64,
    finished: bool,
    updated_at: Instant,
    notify: Arc<Notify>,
}

#[derive(Default)]
pub struct CleanupScanRunHub {
    runs: Mutex<BTreeMap<String, CleanupScanRunRecord>>,
    active_driver: Mutex<Option<String>>,
    active_run_ids: Mutex<BTreeSet<String>>,
}

impl CleanupScanRunHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create_or_join_active(&self) -> (String, bool) {
        let scan_id = ids::new_cleanup_scan_id();
        let mut runs = self.runs.lock().await;
        let mut active_driver = self.active_driver.lock().await;
        let mut active_run_ids = self.active_run_ids.lock().await;
        prune_finished(&mut runs);
        let has_active_driver = active_driver
            .as_deref()
            .and_then(|driver_id| runs.get(driver_id))
            .is_some_and(|record| !record.finished);
        if !has_active_driver {
            *active_driver = None;
            active_run_ids.clear();
        }
        let is_driver = active_driver.is_none();
        if is_driver {
            *active_driver = Some(scan_id.clone());
        }
        active_run_ids.insert(scan_id.clone());
        runs.insert(scan_id.clone(), new_record());
        (scan_id, is_driver)
    }

    pub async fn append(&self, scan_id: &str, event: CleanupScanRunEvent) {
        self.append_inner(scan_id, event, false).await;
    }

    pub async fn append_to_active(
        &self,
        driver_id: &str,
        phase: CleanupScanRunPhase,
        response: Option<CleanupScanResponse>,
        message: Option<String>,
    ) {
        let mut runs = self.runs.lock().await;
        let mut active_driver = self.active_driver.lock().await;
        let mut active_run_ids = self.active_run_ids.lock().await;
        let is_active_driver = active_driver.as_deref() == Some(driver_id);
        let target_ids = if is_active_driver {
            active_run_ids.iter().cloned().collect::<Vec<_>>()
        } else {
            vec![driver_id.to_string()]
        };
        let finished = matches!(
            phase,
            CleanupScanRunPhase::Ready | CleanupScanRunPhase::Failed
        );
        let mut notifies = Vec::new();
        for scan_id in target_ids {
            let Some(record) = runs.get_mut(&scan_id) else {
                continue;
            };
            append_event_to_record(
                record,
                CleanupScanRunEvent {
                    scan_id: scan_id.clone(),
                    phase: phase.clone(),
                    response: response.clone(),
                    message: message.clone(),
                },
                finished,
            );
            notifies.push(record.notify.clone());
        }
        if finished && is_active_driver {
            *active_driver = None;
            active_run_ids.clear();
        }
        drop(active_run_ids);
        drop(active_driver);
        drop(runs);
        for notify in notifies {
            notify.notify_waiters();
        }
    }

    pub async fn snapshot_after(
        &self,
        scan_id: &str,
        after_id: u64,
    ) -> Option<(Vec<CleanupScanRunStoredEvent>, bool, Arc<Notify>)> {
        let runs = self.runs.lock().await;
        let record = runs.get(scan_id)?;
        let events = record
            .events
            .iter()
            .filter(|event| event.id > after_id)
            .cloned()
            .collect::<Vec<_>>();
        Some((events, record.finished, record.notify.clone()))
    }

    async fn append_inner(&self, scan_id: &str, event: CleanupScanRunEvent, finished: bool) {
        let mut runs = self.runs.lock().await;
        let Some(record) = runs.get_mut(scan_id) else {
            return;
        };
        append_event_to_record(record, event, finished);
        let notify = record.notify.clone();
        drop(runs);
        notify.notify_waiters();
    }
}

fn new_record() -> CleanupScanRunRecord {
    CleanupScanRunRecord {
        events: Vec::new(),
        next_id: 1,
        finished: false,
        updated_at: Instant::now(),
        notify: Arc::new(Notify::new()),
    }
}

fn append_event_to_record(
    record: &mut CleanupScanRunRecord,
    event: CleanupScanRunEvent,
    finished: bool,
) {
    let event_name = event.phase.event_name().to_string();
    let stored = CleanupScanRunStoredEvent {
        id: record.next_id,
        event_name,
        payload: event,
    };
    record.next_id = record.next_id.saturating_add(1);
    record.events.push(stored);
    record.finished |= finished
        || matches!(
            record.events.last().map(|event| &event.payload.phase),
            Some(CleanupScanRunPhase::Ready | CleanupScanRunPhase::Failed)
        );
    record.updated_at = Instant::now();
}

fn prune_finished(runs: &mut BTreeMap<String, CleanupScanRunRecord>) {
    let now = Instant::now();
    runs.retain(|_, record| {
        !record.finished || now.duration_since(record.updated_at) <= RETAIN_FINISHED_FOR
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_driver_fans_out_terminal_events_to_joined_runs() {
        let hub = CleanupScanRunHub::new();
        let (driver_id, driver_should_start) = hub.create_or_join_active().await;
        let (follower_id, follower_should_start) = hub.create_or_join_active().await;

        assert!(driver_should_start);
        assert!(!follower_should_start);

        hub.append_to_active(&driver_id, CleanupScanRunPhase::Ready, None, None)
            .await;

        let (driver_events, driver_finished, _) = hub.snapshot_after(&driver_id, 0).await.unwrap();
        let (follower_events, follower_finished, _) =
            hub.snapshot_after(&follower_id, 0).await.unwrap();

        assert!(driver_finished);
        assert!(follower_finished);
        assert_eq!(driver_events.len(), 1);
        assert_eq!(follower_events.len(), 1);
        assert_eq!(driver_events[0].event_name, "scan_ready");
        assert_eq!(follower_events[0].event_name, "scan_ready");
        assert_eq!(driver_events[0].payload.scan_id, driver_id);
        assert_eq!(follower_events[0].payload.scan_id, follower_id);

        let (_next_id, next_should_start) = hub.create_or_join_active().await;
        assert!(next_should_start);
    }
}
