use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLogMetaFormat {
    Json,
    Logfmt,
    Text,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogMeta {
    pub format: ServiceLogMetaFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogLine {
    pub ts: String,
    pub raw: String,
    pub plain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ServiceLogMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
