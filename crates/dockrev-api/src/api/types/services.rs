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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    pub digest: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceRepoLinkInferenceStrategy {
    OciSource,
    GhcrExact,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRepoLinkInferenceResponse {
    pub repo_url: Option<String>,
    pub strategy: ServiceRepoLinkInferenceStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTagSuggestionItem {
    pub tag: String,
    pub last_used_at: String,
    pub source: String,
    pub use_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTagSuggestionsResponse {
    pub items: Vec<ServiceTagSuggestionItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceComposeTagRequest {
    pub tag: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceComposeTagResponse {
    pub ok: bool,
    pub tag: String,
    pub image_ref: String,
    pub compose_file: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceBackupTargetItem {
    pub key: String,
    pub policy: BackupTargetPolicy,
    pub related_service_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_service_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceBackupStorageInfo {
    pub base_dir: String,
    pub artifact_pattern: String,
    pub compression: String,
    pub keep_last: u32,
    pub delete_after_stable_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceBackupTargetsResponse {
    pub bind_paths: Vec<ServiceBackupTargetItem>,
    pub volume_names: Vec<ServiceBackupTargetItem>,
    pub storage: ServiceBackupStorageInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceBackupTargetItem {
    pub key: String,
    pub policy: BackupTargetPolicy,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceBackupTargetsRequest {
    #[serde(default)]
    pub bind_paths: Vec<PutServiceBackupTargetItem>,
    #[serde(default)]
    pub volume_names: Vec<PutServiceBackupTargetItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceBackupTargetsResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewVersionDiscoveryTimelineItemKind {
    CurrentCandidate,
    HistoricalCandidate,
    CurrentRunning,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVersionDiscoveryTimelineItem {
    pub kind: NewVersionDiscoveryTimelineItemKind,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVersionDiscoveryTimelineResponse {
    pub items: Vec<NewVersionDiscoveryTimelineItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitHubReleaseAuthMode {
    Pat,
    Anonymous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceGitHubReleasesStatus {
    Ready,
    UnsupportedRepo,
    PermissionDenied,
    RateLimited,
    UpstreamError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceGitHubRepoRef {
    pub full_name: String,
    pub html_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceGitHubReleaseItem {
    pub id: i64,
    pub tag_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceGitHubReleasesResponse {
    pub status: ServiceGitHubReleasesStatus,
    pub auth_mode: GitHubReleaseAuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<ServiceGitHubRepoRef>,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
    pub items: Vec<ServiceGitHubReleaseItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceGitHubReleaseLocateStatus {
    Found,
    OutsideWindow,
    NotFound,
    UnsupportedRepo,
    PermissionDenied,
    RateLimited,
    UpstreamError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceGitHubReleaseLocateResponse {
    pub status: ServiceGitHubReleaseLocateStatus,
    pub auth_mode: GitHubReleaseAuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<ServiceGitHubRepoRef>,
    pub version: String,
    pub searched_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_within_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
