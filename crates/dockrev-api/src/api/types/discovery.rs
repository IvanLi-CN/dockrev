use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveredProjectStatus {
    Active,
    Stopped,
    Missing,
    Invalid,
}

impl DiscoveredProjectStatus {
    pub fn from_str(input: &str) -> Self {
        match input {
            "active" => Self::Active,
            "stopped" => Self::Stopped,
            "missing" => Self::Missing,
            _ => Self::Invalid,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredProject {
    pub project: String,
    pub status: DiscoveredProjectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub archived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDiscoveredProjectsResponse {
    pub projects: Vec<DiscoveredProject>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryScanSummary {
    pub projects_seen: u32,
    pub stacks_created: u32,
    pub stacks_updated: u32,
    pub stacks_skipped: u32,
    pub stacks_failed: u32,
    pub stacks_stopped: u32,
    pub stacks_marked_missing: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryActionKind {
    Created,
    Updated,
    Skipped,
    Failed,
    MarkedStopped,
    MarkedMissing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryAction {
    pub project: String,
    pub action: DiscoveryActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDiscoveryScanResponse {
    pub started_at: String,
    pub duration_ms: u64,
    pub summary: DiscoveryScanSummary,
    pub actions: Vec<DiscoveryAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDiscoveryScanJobResponse {
    pub job_id: String,
}
