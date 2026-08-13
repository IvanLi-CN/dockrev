use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::{Mutex, Notify};

pub const MANAGEMENT_EVENT_BUFFER_MAX_AGE: Duration = Duration::from_secs(60);
pub const MANAGEMENT_EVENT_BUFFER_MAX_EVENTS: usize = 1024;
pub const MANAGEMENT_EVENT_COALESCE_WINDOW: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementEventEntity {
    pub entity_type: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub domain: String,
    pub entities: Vec<ManagementEventEntity>,
    pub version: u64,
    pub summary: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ManagementEventRecord {
    pub cursor: String,
    pub event: ManagementEvent,
}

#[derive(Clone, Debug)]
pub enum ManagementEventReplay {
    Events {
        cursor: u64,
        events: Vec<ManagementEventRecord>,
    },
    ResyncRequired {
        cursor: u64,
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementEventMetrics {
    pub generation: String,
    pub active_connections: usize,
    pub buffered_events: usize,
    pub published_events: u64,
    pub coalesced_events: u64,
    pub evicted_events: u64,
    pub resync_required_events: u64,
    pub reconnects: u64,
    pub publish_failures: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CoalesceKey {
    domain: String,
    entity_type: String,
    id: String,
}

#[derive(Clone, Debug)]
struct BufferedEvent {
    id: u64,
    created_at: Instant,
    event: ManagementEvent,
}

#[derive(Debug)]
struct Runtime {
    events: VecDeque<BufferedEvent>,
    pending: BTreeMap<CoalesceKey, ManagementEvent>,
    next_id: u64,
    flush_scheduled: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            pending: BTreeMap::new(),
            next_id: 1,
            flush_scheduled: false,
        }
    }
}

#[derive(Default)]
struct Counters {
    active_connections: AtomicUsize,
    published_events: AtomicU64,
    coalesced_events: AtomicU64,
    evicted_events: AtomicU64,
    resync_required_events: AtomicU64,
    reconnects: AtomicU64,
    publish_failures: AtomicU64,
}

#[derive(Clone)]
pub struct ManagementEventHub {
    generation: Arc<String>,
    runtime: Arc<Mutex<Runtime>>,
    notify: Arc<Notify>,
    counters: Arc<Counters>,
}

pub struct ManagementEventConnection {
    counters: Arc<Counters>,
}

impl Drop for ManagementEventConnection {
    fn drop(&mut self) {
        self.counters
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    }
}

impl ManagementEventHub {
    pub fn new() -> Self {
        Self {
            generation: Arc::new(ulid::Ulid::new().to_string()),
            runtime: Arc::new(Mutex::new(Runtime::default())),
            notify: Arc::new(Notify::new()),
            counters: Arc::new(Counters::default()),
        }
    }

    pub fn generation(&self) -> &str {
        self.generation.as_str()
    }

    pub fn register_connection(&self, reconnect: bool) -> ManagementEventConnection {
        self.counters
            .active_connections
            .fetch_add(1, Ordering::Relaxed);
        if reconnect {
            self.counters.reconnects.fetch_add(1, Ordering::Relaxed);
        }
        ManagementEventConnection {
            counters: self.counters.clone(),
        }
    }

    pub fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.notify.notified()
    }

    pub fn record_publish_failure(&self) {
        self.counters
            .publish_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    pub async fn publish_change(
        &self,
        domain: impl Into<String>,
        entity_type: impl Into<String>,
        id: impl Into<String>,
        summary: serde_json::Value,
    ) {
        let key = CoalesceKey {
            domain: domain.into(),
            entity_type: entity_type.into(),
            id: id.into(),
        };
        let event = ManagementEvent {
            event_type: "entities_changed".to_string(),
            domain: key.domain.clone(),
            entities: vec![ManagementEventEntity {
                entity_type: key.entity_type.clone(),
                id: key.id.clone(),
            }],
            version: 0,
            summary,
        };

        let should_schedule = {
            let mut runtime = self.runtime.lock().await;
            if runtime.pending.insert(key, event).is_some() {
                self.counters
                    .coalesced_events
                    .fetch_add(1, Ordering::Relaxed);
            }
            if runtime.flush_scheduled {
                false
            } else {
                runtime.flush_scheduled = true;
                true
            }
        };

        if should_schedule {
            let hub = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(MANAGEMENT_EVENT_COALESCE_WINDOW).await;
                hub.flush_pending().await;
            });
        }
    }

    pub async fn publish_immediate(
        &self,
        domain: impl Into<String>,
        entities: Vec<ManagementEventEntity>,
        summary: serde_json::Value,
    ) {
        let domain = domain.into();
        let mut runtime = self.runtime.lock().await;
        for entity in &entities {
            runtime.pending.remove(&CoalesceKey {
                domain: domain.clone(),
                entity_type: entity.entity_type.clone(),
                id: entity.id.clone(),
            });
        }
        let event = ManagementEvent {
            event_type: "entities_changed".to_string(),
            domain,
            entities,
            version: 0,
            summary,
        };
        self.append_locked(&mut runtime, event);
        drop(runtime);
        self.counters
            .published_events
            .fetch_add(1, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    async fn flush_pending(&self) {
        let mut runtime = self.runtime.lock().await;
        runtime.flush_scheduled = false;
        let pending = std::mem::take(&mut runtime.pending);
        let published = pending.len() as u64;
        for (_, event) in pending {
            self.append_locked(&mut runtime, event);
        }
        drop(runtime);
        if published > 0 {
            self.counters
                .published_events
                .fetch_add(published, Ordering::Relaxed);
            self.notify.notify_waiters();
        }
    }

    fn append_locked(&self, runtime: &mut Runtime, mut event: ManagementEvent) {
        let id = runtime.next_id;
        runtime.next_id = runtime.next_id.saturating_add(1);
        event.version = id;
        runtime.events.push_back(BufferedEvent {
            id,
            created_at: Instant::now(),
            event,
        });
        self.prune_locked(runtime);
    }

    pub async fn replay_after(&self, last_event_id: Option<&str>) -> ManagementEventReplay {
        let mut runtime = self.runtime.lock().await;
        self.prune_locked(&mut runtime);
        let latest = runtime.next_id.saturating_sub(1);
        let Some(last_event_id) = last_event_id.filter(|value| !value.is_empty()) else {
            return ManagementEventReplay::Events {
                cursor: latest,
                events: Vec::new(),
            };
        };
        let last_id = match self.parse_cursor(last_event_id) {
            Ok(last_id) => last_id,
            Err(reason) => return self.resync_required(latest, reason),
        };
        let replay_gap = match runtime.events.front() {
            Some(oldest) => last_id < oldest.id.saturating_sub(1),
            None => last_id < latest,
        };
        if replay_gap || last_id > latest {
            return self.resync_required(latest, "cursor_expired");
        }
        let events = runtime
            .events
            .iter()
            .filter(|event| event.id > last_id)
            .map(|event| ManagementEventRecord {
                cursor: self.format_cursor(event.id),
                event: event.event.clone(),
            })
            .collect();
        ManagementEventReplay::Events {
            cursor: latest.max(last_id),
            events,
        }
    }

    pub async fn metrics(&self) -> ManagementEventMetrics {
        let mut runtime = self.runtime.lock().await;
        self.prune_locked(&mut runtime);
        ManagementEventMetrics {
            generation: self.generation().to_string(),
            active_connections: self.counters.active_connections.load(Ordering::Relaxed),
            buffered_events: runtime.events.len(),
            published_events: self.counters.published_events.load(Ordering::Relaxed),
            coalesced_events: self.counters.coalesced_events.load(Ordering::Relaxed),
            evicted_events: self.counters.evicted_events.load(Ordering::Relaxed),
            resync_required_events: self.counters.resync_required_events.load(Ordering::Relaxed),
            reconnects: self.counters.reconnects.load(Ordering::Relaxed),
            publish_failures: self.counters.publish_failures.load(Ordering::Relaxed),
        }
    }

    fn parse_cursor(&self, value: &str) -> Result<u64, &'static str> {
        let (generation, id) = value.split_once(':').ok_or("invalid_cursor")?;
        if generation != self.generation() {
            return Err("generation_changed");
        }
        id.parse().map_err(|_| "invalid_cursor")
    }

    fn format_cursor(&self, id: u64) -> String {
        format!("{}:{id}", self.generation())
    }

    fn resync_required(&self, cursor: u64, reason: &'static str) -> ManagementEventReplay {
        self.counters
            .resync_required_events
            .fetch_add(1, Ordering::Relaxed);
        ManagementEventReplay::ResyncRequired { cursor, reason }
    }

    fn prune_locked(&self, runtime: &mut Runtime) {
        let cutoff = Instant::now() - MANAGEMENT_EVENT_BUFFER_MAX_AGE;
        while runtime
            .events
            .front()
            .is_some_and(|event| event.created_at < cutoff)
            || runtime.events.len() > MANAGEMENT_EVENT_BUFFER_MAX_EVENTS
        {
            runtime.events.pop_front();
            self.counters.evicted_events.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Default for ManagementEventHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coalesces_same_entity_and_replays_by_cursor() {
        let hub = ManagementEventHub::new();
        hub.publish_change("stacks", "stack", "one", serde_json::json!({ "phase": 1 }))
            .await;
        hub.publish_change("stacks", "stack", "one", serde_json::json!({ "phase": 2 }))
            .await;
        tokio::time::sleep(MANAGEMENT_EVENT_COALESCE_WINDOW + Duration::from_millis(20)).await;

        let ManagementEventReplay::Events { events, .. } = hub
            .replay_after(Some(&format!("{}:0", hub.generation())))
            .await
        else {
            panic!("expected replay");
        };
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.summary["phase"], 2);
        assert_eq!(hub.metrics().await.coalesced_events, 1);
    }

    #[tokio::test]
    async fn rejects_cursor_from_another_generation() {
        let hub = ManagementEventHub::new();
        let ManagementEventReplay::ResyncRequired { reason, .. } =
            hub.replay_after(Some("previous:1")).await
        else {
            panic!("expected resync");
        };
        assert_eq!(reason, "generation_changed");
    }

    #[tokio::test]
    async fn evicts_oldest_events_when_ring_reaches_capacity() {
        let hub = ManagementEventHub::new();
        for index in 0..=MANAGEMENT_EVENT_BUFFER_MAX_EVENTS {
            hub.publish_immediate(
                "stacks",
                vec![ManagementEventEntity {
                    entity_type: "stack".to_string(),
                    id: index.to_string(),
                }],
                serde_json::json!({ "operation": "updated" }),
            )
            .await;
        }

        let metrics = hub.metrics().await;
        assert_eq!(metrics.buffered_events, MANAGEMENT_EVENT_BUFFER_MAX_EVENTS);
        assert_eq!(metrics.evicted_events, 1);

        let ManagementEventReplay::ResyncRequired { reason, .. } = hub
            .replay_after(Some(&format!("{}:0", hub.generation())))
            .await
        else {
            panic!("expected a resync after cursor eviction");
        };
        assert_eq!(reason, "cursor_expired");
    }

    #[tokio::test]
    async fn requires_resync_when_every_event_after_cursor_has_expired() {
        let hub = ManagementEventHub::new();
        hub.publish_immediate(
            "stacks",
            vec![ManagementEventEntity {
                entity_type: "stack".to_string(),
                id: "one".to_string(),
            }],
            serde_json::json!({ "operation": "updated" }),
        )
        .await;

        hub.runtime.lock().await.events.clear();
        let ManagementEventReplay::ResyncRequired { reason, .. } = hub
            .replay_after(Some(&format!("{}:0", hub.generation())))
            .await
        else {
            panic!("expected a resync after buffer expiry");
        };
        assert_eq!(reason, "cursor_expired");
    }
}
