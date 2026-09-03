use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use serde_json::Value;
use tokio::sync::{Mutex, Notify, Semaphore};

use crate::{
    api::{
        operations::lifecycle::{
            LIFECYCLE_STATUS_TIMEOUT_SECONDS, lifecycle_compose_config, lifecycle_compose_stack,
        },
        types::{Service, ServiceLifecycleState},
    },
    models::StackRecord,
    state::AppState,
};

#[derive(Clone, Debug)]
pub(crate) struct LifecycleSnapshot {
    pub states: BTreeMap<String, ServiceLifecycleState>,
    pub unavailable_reason: Option<String>,
}

struct InFlightSnapshot {
    result: Mutex<Option<LifecycleSnapshot>>,
    notify: Notify,
}

#[derive(Clone)]
pub(crate) struct LifecycleSnapshotCoordinator {
    gate: Arc<Semaphore>,
    in_flight: Arc<Mutex<HashMap<String, Arc<InFlightSnapshot>>>>,
    active_reads: Arc<AtomicUsize>,
    peak_reads: Arc<AtomicUsize>,
}

impl Default for LifecycleSnapshotCoordinator {
    fn default() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(4)),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            active_reads: Arc::new(AtomicUsize::new(0)),
            peak_reads: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl LifecycleSnapshotCoordinator {
    pub(crate) async fn read(
        &self,
        state: &Arc<AppState>,
        stack: &StackRecord,
        services: &[Service],
    ) -> LifecycleSnapshot {
        let (flight, leader) = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(flight) = in_flight.get(&stack.id) {
                (Arc::clone(flight), false)
            } else {
                let flight = Arc::new(InFlightSnapshot {
                    result: Mutex::new(None),
                    notify: Notify::new(),
                });
                in_flight.insert(stack.id.clone(), Arc::clone(&flight));
                (flight, true)
            }
        };

        if !leader {
            tracing::debug!(stack_id = %stack.id, in_flight_hit = true, "lifecycle snapshot joined in-flight read");
            flight.notify.notified().await;
            return flight
                .result
                .lock()
                .await
                .clone()
                .unwrap_or_else(|| unavailable_snapshot(services));
        }

        let started = Instant::now();
        let (snapshot, compose_commands, concurrent_peak) = {
            let _permit = self.gate.acquire().await.ok();
            let active = self.active_reads.fetch_add(1, Ordering::SeqCst) + 1;
            let concurrent_peak = self
                .peak_reads
                .fetch_max(active, Ordering::SeqCst)
                .max(active);
            let (snapshot, compose_commands) = read_snapshot(state, stack, services).await;
            self.active_reads.fetch_sub(1, Ordering::SeqCst);
            (snapshot, compose_commands, concurrent_peak)
        };
        tracing::debug!(
            stack_id = %stack.id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            compose_commands,
            in_flight_hit = false,
            concurrent_peak,
            "lifecycle snapshot read"
        );
        *flight.result.lock().await = Some(snapshot.clone());
        flight.notify.notify_waiters();
        self.in_flight.lock().await.remove(&stack.id);
        snapshot
    }
}

async fn read_snapshot(
    state: &Arc<AppState>,
    stack: &StackRecord,
    services: &[Service],
) -> (LifecycleSnapshot, u8) {
    let fallback = || unavailable_snapshot(services);
    let Ok((config, _auth_bridge)) = lifecycle_compose_config(state.as_ref()) else {
        return (fallback(), 0);
    };
    let compose = lifecycle_compose_stack(state.as_ref(), stack);
    let timeout = Duration::from_secs(LIFECYCLE_STATUS_TIMEOUT_SECONDS);
    let configured = match state
        .runner
        .run(compose.config_services(&config), timeout)
        .await
    {
        Ok(output) if output.status == 0 => output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
        _ => return (fallback(), 1),
    };
    let output = match state
        .runner
        .run(compose.ps_all_json(&config), timeout)
        .await
    {
        Ok(output) if output.status == 0 => output,
        _ => return (fallback(), 2),
    };
    let Some(rows) = parse_ps_rows(&output.stdout) else {
        return (fallback(), 2);
    };

    let mut counts = HashMap::<String, (usize, usize, bool)>::new();
    for row in rows {
        let Some(service_name) = value_string(&row, &["Service", "service"]) else {
            return (fallback(), 2);
        };
        let Some(status) = value_string(&row, &["State", "state"]) else {
            return (fallback(), 2);
        };
        let status = status.to_ascii_lowercase();
        let entry = counts.entry(service_name.to_string()).or_default();
        entry.0 += 1;
        if status == "running" {
            entry.1 += 1;
        } else if !matches!(
            status.as_str(),
            "created" | "exited" | "dead" | "paused" | "restarting" | "removing"
        ) {
            entry.2 = true;
        }
    }

    let mut states = BTreeMap::new();
    for service_name in configured {
        let state = match counts.get(&service_name) {
            None => ServiceLifecycleState::Stopped,
            Some((_, _, true)) => ServiceLifecycleState::Unknown,
            Some((all, running, false)) => {
                super::lifecycle::lifecycle_state_from_counts(*all, *running)
            }
        };
        states.insert(service_name, state);
    }
    let unavailable_reason = states
        .values()
        .any(|state| matches!(state, ServiceLifecycleState::Unknown))
        .then_some("lifecycle_status_unavailable".to_string());
    (
        LifecycleSnapshot {
            states,
            unavailable_reason,
        },
        2,
    )
}

fn unavailable_snapshot(services: &[Service]) -> LifecycleSnapshot {
    LifecycleSnapshot {
        states: services
            .iter()
            .map(|service| (service.name.clone(), ServiceLifecycleState::Unknown))
            .collect(),
        unavailable_reason: Some("lifecycle_status_unavailable".to_string()),
    }
}

fn parse_ps_rows(stdout: &str) -> Option<Vec<Value>> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return match value {
            Value::Array(rows) => Some(rows),
            Value::Object(_) => Some(vec![value]),
            _ => None,
        };
    }
    let mut rows = Vec::new();
    for line in trimmed.lines() {
        rows.push(serde_json::from_str::<Value>(line).ok()?);
    }
    Some(rows)
}

fn value_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_array_and_ndjson() {
        let array = parse_ps_rows(r#"[{"Service":"web","State":"running"}]"#).unwrap();
        assert_eq!(array.len(), 1);
        let ndjson = parse_ps_rows(
            r#"{"Service":"web","State":"running"}
{"Service":"api","State":"exited"}"#,
        )
        .unwrap();
        assert_eq!(ndjson.len(), 2);
    }
}
