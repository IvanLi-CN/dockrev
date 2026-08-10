use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResourceSample {
    pub sampled_at: String,
    pub cpu_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_rx_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_tx_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_read_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_write_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
    pub container_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResourceHistoryResponse {
    pub service_id: String,
    pub window: String,
    pub samples: Vec<ServiceResourceSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResourceOverviewItem {
    pub service_id: String,
    pub sampled_at: Option<String>,
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_rx_rate_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_tx_rate_bps: Option<f64>,
    pub stale: bool,
    pub sample_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResourceOverviewResponse {
    pub enabled: bool,
    pub window: String,
    pub generated_at: String,
    pub stale_after_seconds: u64,
    pub services: Vec<ServiceResourceOverviewItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomepageNavItem {
    pub stack_id: String,
    pub stack_name: String,
    pub service_id: String,
    pub service_name: String,
    pub image_ref: String,
    pub image_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_resolved_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_resolved_tags: Option<Vec<String>>,
    pub is_dockrev: bool,
    pub homepage: ServiceHomepage,
    pub candidate: Option<Candidate>,
    pub ignore: Option<IgnoreMatch>,
    pub version_inference: Option<VersionInferenceState>,
    pub new_version_discovery_count: Option<u32>,
    pub settings: ServiceSettings,
    pub archived: Option<bool>,
    pub resource: ServiceResourceOverviewItem,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomepageNavResponse {
    pub generated_at: String,
    pub last_check_at: Option<String>,
    pub resource_summary: ServiceResourceOverviewResponse,
    pub items: Vec<HomepageNavItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckReportResponse {
    pub overall: DeployCheckOverall,
    pub generated_at: String,
    pub checks: Vec<DeployCheckItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployCheckReportStatus {
    Pending,
    Ready,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckReportEnvelope {
    pub status: DeployCheckReportStatus,
    #[serde(default)]
    pub refreshing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<DeployCheckReportResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckOverall {
    pub result: DeployCheckResult,
    pub blocking_check_ids: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployCheckResult {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployCheckStatus {
    Pass,
    Fail,
    Na,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployCheckNaReason {
    DisabledBySwitch,
    MissingPrerequisite,
    NotApplicable,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployCheckGroup {
    Core,
    Feature,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployCheckItem {
    pub id: String,
    pub title: String,
    pub group: DeployCheckGroup,
    pub required: bool,
    pub status: DeployCheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub na_reason: Option<DeployCheckNaReason>,
    pub summary: String,
    pub impact: String,
    pub evidence: String,
    pub recommendation: String,
}

#[derive(Clone, Debug)]
pub struct DeployWelcomeSettings {
    pub never_auto_open: bool,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployWelcomeResponse {
    pub never_auto_open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl From<DeployWelcomeSettings> for DeployWelcomeResponse {
    fn from(input: DeployWelcomeSettings) -> Self {
        Self {
            never_auto_open: input.never_auto_open,
            updated_at: input.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutDeployWelcomeRequest {
    pub never_auto_open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutDeployWelcomeResponse {
    pub ok: bool,
    pub never_auto_open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
