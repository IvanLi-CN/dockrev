use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerCheckRequest {
    pub scope: JobScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    pub reason: CheckReason,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckReason {
    Ui,
    Webhook,
    Schedule,
}

impl CheckReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Webhook => "webhook",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerCheckResponse {
    pub check_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRuntimeScanRequest {
    pub scope: JobScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    pub reason: RuntimeScanReason,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeScanReason {
    Ui,
    Schedule,
}

impl RuntimeScanReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRuntimeScanResponse {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateServiceTarget {
    pub service_id: String,
    pub target_tag: String,
    pub target_digest: String,
    #[serde(default)]
    pub pull_tags: Option<Vec<String>>,
    #[serde(default)]
    pub skip_tag_followups: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerUpdateRequest {
    pub scope: JobScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub target_tag: Option<String>,
    #[serde(default)]
    pub target_digest: Option<String>,
    #[serde(default)]
    pub pull_tags: Option<Vec<String>>,
    #[serde(default)]
    pub targets: Option<Vec<UpdateServiceTarget>>,
    pub mode: UpdateMode,
    pub allow_arch_mismatch: bool,
    pub backup_mode: BackupMode,
    pub reason: UpdateReason,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateMode {
    Apply,
    DryRun,
}

impl UpdateMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::DryRun => "dry-run",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupMode {
    Inherit,
    Skip,
    Force,
}

impl BackupMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Skip => "skip",
            Self::Force => "force",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateReason {
    Ui,
    Webhook,
    Schedule,
    AutoPolicy,
}

impl UpdateReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Webhook => "webhook",
            Self::Schedule => "schedule",
            Self::AutoPolicy => "auto_policy",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerUpdateResponse {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRollbackTargetResponse {
    pub available: bool,
    pub current_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_display_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_display_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_update_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_job_status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRollbackResponse {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobScope {
    Service,
    Stack,
    All,
}

impl JobScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Stack => "stack",
            Self::All => "all",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "service" => Self::Service,
            "stack" => Self::Stack,
            _ => Self::All,
        }
    }
}

#[derive(Clone, Debug)]
pub enum JobType {
    Check,
    CleanupApply,
    Discovery,
    RuntimeScan,
    GitHubPackagesWebhook,
    GitHubPackagesWebhookSyncAll,
    GitHubPackagesWebhookSyncRepo,
    RepoLinkBackfill,
    Update,
    Rollback,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::CleanupApply => "cleanup_apply",
            Self::Discovery => "discovery",
            Self::RuntimeScan => "runtime_scan",
            Self::GitHubPackagesWebhook => "github_packages_webhook",
            Self::GitHubPackagesWebhookSyncAll => "github_packages_webhook_sync_all",
            Self::GitHubPackagesWebhookSyncRepo => "github_packages_webhook_sync_repo",
            Self::RepoLinkBackfill => "repo_link_backfill",
            Self::Update => "update",
            Self::Rollback => "rollback",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "check" => Self::Check,
            "cleanup_apply" => Self::CleanupApply,
            "discovery" => Self::Discovery,
            "runtime_scan" => Self::RuntimeScan,
            "github_packages_webhook" => Self::GitHubPackagesWebhook,
            "github_packages_webhook_sync_all" => Self::GitHubPackagesWebhookSyncAll,
            "github_packages_webhook_sync_repo" => Self::GitHubPackagesWebhookSyncRepo,
            "repo_link_backfill" => Self::RepoLinkBackfill,
            "rollback" => Self::Rollback,
            _ => Self::Update,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JobListItem {
    pub id: String,
    pub r#type: JobType,
    pub scope: JobScope,
    pub stack_id: Option<String>,
    pub service_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub created_by: String,
    pub reason: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: String,
    pub summary_json: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    /// Progress phase label (e.g. prepare/scan/apply/done).
    pub phase: String,
    /// Human-readable status message for current phase.
    pub message: String,
    /// Completed units in current phase.
    pub current: u32,
    /// Total units in current phase. `0` means unknown total (indeterminate).
    pub total: u32,
    /// Percent provided by backend. Frontend should not derive/override this value.
    pub percent: u32,
    /// Planned/scheduled units. Defaults to completed units when omitted by old producers.
    pub planned_current: Option<u32>,
    /// Planned/scheduled total units. Defaults to total when omitted by old producers.
    pub planned_total: Option<u32>,
    /// Planned/scheduled percent. Defaults to percent when omitted by old producers.
    pub planned_percent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
    pub updated_at: String,
}

fn progress_from_summary(summary: &Value) -> Option<JobProgress> {
    let progress = summary.as_object()?.get("progress")?.clone();
    serde_json::from_value::<JobProgress>(progress).ok()
}

impl JobListItem {
    pub fn into_api(self) -> JobApiListItem {
        let progress = progress_from_summary(&self.summary_json);
        JobApiListItem {
            id: self.id,
            r#type: self.r#type.as_str().to_string(),
            scope: self.scope.as_str().to_string(),
            stack_id: self.stack_id,
            service_id: self.service_id,
            status: self.status,
            created_by: self.created_by,
            reason: self.reason,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
            allow_arch_mismatch: self.allow_arch_mismatch,
            backup_mode: self.backup_mode,
            summary: self.summary_json,
            progress,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListJobsResponse {
    pub jobs: Vec<JobApiListItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobApiListItem {
    pub id: String,
    pub r#type: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    pub status: String,
    pub created_by: String,
    pub reason: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: String,
    pub summary: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<JobProgress>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetJobResponse {
    pub job: JobDetail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDetail {
    pub id: String,
    pub r#type: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    pub status: String,
    pub created_by: String,
    pub reason: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: String,
    pub summary: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<JobProgress>,
    pub logs: Vec<JobLogLine>,
    pub logs_last_id: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLogLine {
    pub ts: String,
    pub level: String,
    pub msg: String,
}

pub use crate::models::JobRecord;
