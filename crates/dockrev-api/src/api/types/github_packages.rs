use super::*;

#[allow(unused_imports)]
pub use crate::models::{
    GitHubPackagesRepoDb, GitHubPackagesSettingsDb, GitHubPackagesTargetDb,
    GitHubPackagesWebhookDeliveryDb,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGitHubPackagesTargetRequest {
    pub input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesRepoSelection {
    pub full_name: String,
    pub selected: bool,
    pub visibility: Option<String>,
    pub last_activity_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGitHubPackagesTargetResponse {
    pub kind: String, // "repo" | "owner"
    pub owner: String,
    pub repos: Vec<GitHubPackagesRepoSelection>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesTarget {
    pub input: String,
    pub kind: String,
    pub owner: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesRepo {
    pub full_name: String,
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_audit_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_op: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesSettingsResponse {
    pub enabled: bool,
    pub callback_url: String,
    pub targets: Vec<GitHubPackagesTarget>,
    pub repos_total: u32,
    pub repos_selected_total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pat_masked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_masked: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutGitHubPackagesSettingsRequest {
    pub enabled: bool,
    pub callback_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<GitHubPackagesTargetInput>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<Vec<GitHubPackagesRepoSelection>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pat: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesTargetInput {
    pub input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGitHubPackagesReposResponse {
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
    pub filtered_total: u32,
    pub selected_total: u32,
    pub repos: Vec<GitHubPackagesRepo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetGitHubPackagesRepoSelectedRequest {
    pub full_name: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetGitHubPackagesRepoSelectedResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSetGitHubPackagesReposSelectedRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_filter: Option<String>, // all|selected|unselected
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSetGitHubPackagesReposSelectedResponse {
    pub ok: bool,
    pub affected: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddGitHubPackagesTargetRequest {
    pub input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddGitHubPackagesTargetResponse {
    pub ok: bool,
    pub kind: String,  // repo|owner
    pub owner: String, // resolved owner
    pub repos_added: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveGitHubPackagesTargetRequest {
    pub input: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveGitHubPackagesTargetResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutGitHubPackagesSettingsResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGitHubPackagesConflicts {
    pub repo: String,
    pub keep_hook_id: i64,
    pub delete_hook_ids: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGitHubPackagesWebhooksRequest {
    #[serde(default)]
    pub dry_run: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_conflicts: Option<Vec<ResolveGitHubPackagesConflicts>>,
    /// If provided, only sync these repos (fullName: "owner/repo").
    /// Otherwise, sync all selected repos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesConflictHook {
    pub id: i64,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGitHubPackagesWebhookResult {
    pub repo: String,
    pub action: String, // noop|created|updated|conflict|error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_hooks: Option<Vec<GitHubPackagesConflictHook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGitHubPackagesWebhooksResponse {
    pub ok: bool,
    pub results: Vec<SyncGitHubPackagesWebhookResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerGitHubPackagesWebhookSyncAllResponse {
    pub ok: bool,
    pub job_id: String,
    pub status: String,
    pub reused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerGitHubPackagesWebhookSyncRepoRequest {
    pub full_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerGitHubPackagesWebhookSyncRepoResponse {
    pub ok: bool,
    pub job_id: String,
    pub status: String,
    pub reused: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGitHubPackagesRepoRequest {
    pub full_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGitHubPackagesRepoResponse {
    pub ok: bool,
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookOverviewSummary {
    pub tracked: u32,
    pub ok: u32,
    pub missing: u32,
    pub error: u32,
    pub conflict: u32,
    pub queued: u32,
    pub running: u32,
    pub unknown: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookOverviewResponse {
    pub summary: GitHubPackagesWebhookOverviewSummary,
    pub jobs_queued: u32,
    pub jobs_running: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_audit_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookDelivery {
    pub delivery_id: String,
    pub received_at: String,
    pub first_received_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub job_ids: Vec<String>,
    pub attempt_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookDeliverySummary {
    pub processed: u32,
    pub ignored: u32,
    pub rejected: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGitHubPackagesWebhookDeliveriesResponse {
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
    pub filtered_total: u32,
    pub summary: GitHubPackagesWebhookDeliverySummary,
    pub deliveries: Vec<GitHubPackagesWebhookDelivery>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookDeliveryEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub delivery: GitHubPackagesWebhookDelivery,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPackagesWebhookDeliveryEventsErrorPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub error: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWebPushSubscriptionRequest {
    pub endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscriptionResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookTriggerRequest {
    pub action: WebhookAction,
    pub scope: JobScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: BackupMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookAction {
    Check,
    Update,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookTriggerResponse {
    pub job_id: String,
}
