use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogLine {
    pub ts: String,
    pub raw: String,
    pub plain: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServiceLogEventEnvelope {
    Line {
        id: u64,
        service_id: String,
        line: ServiceLogLine,
    },
    Reset {
        id: u64,
        service_id: String,
        reason: String,
    },
}

impl ServiceLogEventEnvelope {
    pub fn id(&self) -> u64 {
        match self {
            Self::Line { id, .. } | Self::Reset { id, .. } => *id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogSnapshotResponse {
    pub service_id: String,
    pub lines: Vec<ServiceLogLine>,
    pub last_event_id: u64,
    pub buffer_limit: usize,
}
