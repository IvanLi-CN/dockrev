use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLifecycleEvent {
    pub id: i64,
    pub service_id: String,
    pub stack_id: Option<String>,
    pub operation_group_id: String,
    pub job_id: Option<String>,
    pub origin: String,
    pub transition: String,
    pub observed_at: String,
    pub boundary_precision: String,
    pub evidence: Value,
    pub details: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleAvailabilityInterval {
    pub operation_group_id: String,
    pub started_at: String,
    pub stopped_at: String,
    pub start_event_id: i64,
    pub stop_event_id: i64,
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLifecycleProjection {
    pub events: Vec<ServiceLifecycleEvent>,
    pub availability_intervals: Vec<LifecycleAvailabilityInterval>,
    pub next_cursor: Option<i64>,
    pub last_event_id: Option<i64>,
    pub retention_since: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLifecycleSnapshotResponse {
    pub service_id: String,
    pub since: String,
    pub until: String,
    pub events: Vec<ServiceLifecycleEvent>,
    pub availability_intervals: Vec<LifecycleAvailabilityInterval>,
    pub next_cursor: Option<i64>,
    pub last_event_id: Option<i64>,
    pub retention_since: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ServiceLifecycleEventEnvelope {
    Event { event: ServiceLifecycleEvent },
    Reset { reason: String, cursor: i64 },
}
