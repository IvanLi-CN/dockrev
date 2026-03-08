use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDigestTagsScanSummary {
    pub repo_tags_total: usize,
    pub repo_tags_considered: usize,
    pub manifests_ok: usize,
    pub manifests_timeout: usize,
    pub manifests_error: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDigestTagsResponse {
    pub digest: String,
    pub tags: Vec<String>,
    pub repo_tags: Vec<String>,
    pub scan: ServiceDigestTagsScanSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDigestTagsSnapshotResponse {
    pub digest: String,
    pub tags: Vec<String>,
    pub checked_at: String,
    pub scan: ServiceDigestTagsScanSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDigestTagsSnapshotPendingResponse {
    pub status: String,
    pub digest: String,
    pub retry_after_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerVersionInferenceRefreshResponse {
    pub status: String,
    pub service_id: String,
    pub image_repo: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceOverviewResponse {
    pub worker: VersionInferenceWorkerState,
    pub gc: VersionInferenceGcState,
    pub summary: VersionInferenceOverviewSummary,
    pub tasks: Vec<VersionInferenceTaskState>,
    pub rows: Vec<VersionInferenceOverviewRow>,
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceWorkerState {
    pub max_concurrency: u32,
    pub queued: u32,
    pub running: u32,
    pub in_flight: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceGcState {
    pub retention_days: i64,
    pub interval_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_deleted: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceOverviewSummary {
    pub snapshots_total: u32,
    pub queued: u32,
    pub running: u32,
    pub ready: u32,
    pub stale: u32,
    pub all_failed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceTaskProgressState {
    pub phase: String,
    pub message: String,
    pub current: u32,
    pub total: u32,
    pub percent: u32,
    pub assigned_current: u32,
    pub assigned_total: u32,
    pub assigned_percent: u32,
    pub result_current: u32,
    pub result_total: u32,
    pub result_percent: u32,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceTaskState {
    pub key: String,
    pub image_repo: String,
    pub host_platform: String,
    pub status: String,
    pub reason: String,
    pub enqueued_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<VersionInferenceTaskProgressState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceOverviewRow {
    pub key: String,
    pub image_repo: String,
    pub host_platform: String,
    pub status: String,
    pub service_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<VersionInferenceTaskProgressState>,
}
