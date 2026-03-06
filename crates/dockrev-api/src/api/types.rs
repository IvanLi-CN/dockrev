use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListStacksResponse {
    pub stacks: Vec<StackListItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackListItem {
    pub id: String,
    pub name: String,
    pub status: StackStatus,
    pub services: u32,
    pub updates: u32,
    pub last_check_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_services: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackStatus {
    Healthy,
    Degraded,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStackResponse {
    pub stack: StackResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackResponse {
    pub id: String,
    pub name: String,
    pub compose: ComposeConfig,
    pub services: Vec<Service>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct StackRecord {
    pub id: String,
    pub name: String,
    pub archived: bool,
    pub compose: ComposeConfig,
    pub backup: StackBackupConfig,
    pub services: Vec<Service>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub name: String,
    pub image: ComposeRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<IgnoreMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_inference: Option<VersionInferenceState>,
    pub settings: ServiceSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInferenceState {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_tag: Option<String>,
    pub digest: String,
    pub arch_match: ArchMatch,
    pub arch: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchMatch {
    Match,
    Mismatch,
    Unknown,
}

impl ArchMatch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Mismatch => "mismatch",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "match" => Self::Match,
            "mismatch" => Self::Mismatch,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreMatch {
    pub matched: bool,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettings {
    pub auto_rollback: bool,
    pub backup_targets: BackupTargetOverrides,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTargetOverrides {
    pub bind_paths: BTreeMap<String, TernaryChoice>,
    pub volume_names: BTreeMap<String, TernaryChoice>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TernaryChoice {
    Inherit,
    Skip,
    Force,
}

#[derive(Clone, Debug)]
pub struct ServiceSeed {
    pub id: String,
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub auto_rollback: bool,
    pub backup_bind_paths: BTreeMap<String, TernaryChoice>,
    pub backup_volume_names: BTreeMap<String, TernaryChoice>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub compose_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StackBackupConfig {
    pub targets: Vec<BackupTarget>,
    pub retention: BackupRetention,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRetention {
    pub keep_last: u32,
    pub delete_after_stable_seconds: u32,
}

impl Default for BackupRetention {
    fn default() -> Self {
        Self {
            keep_last: 1,
            delete_after_stable_seconds: 3600,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BackupTarget {
    #[serde(rename_all = "camelCase")]
    DockerVolume { name: String },
    #[serde(rename_all = "camelCase")]
    BindMount { path: String },
}

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
}

impl UpdateReason {
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
pub struct TriggerUpdateResponse {
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
    Discovery,
    RuntimeScan,
    GitHubPackagesWebhook,
    GitHubPackagesWebhookSyncAll,
    GitHubPackagesWebhookSyncRepo,
    Update,
    Rollback,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Discovery => "discovery",
            Self::RuntimeScan => "runtime_scan",
            Self::GitHubPackagesWebhook => "github_packages_webhook",
            Self::GitHubPackagesWebhookSyncAll => "github_packages_webhook_sync_all",
            Self::GitHubPackagesWebhookSyncRepo => "github_packages_webhook_sync_repo",
            Self::Update => "update",
            Self::Rollback => "rollback",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "check" => Self::Check,
            "discovery" => Self::Discovery,
            "runtime_scan" => Self::RuntimeScan,
            "github_packages_webhook" => Self::GitHubPackagesWebhook,
            "github_packages_webhook_sync_all" => Self::GitHubPackagesWebhookSyncAll,
            "github_packages_webhook_sync_repo" => Self::GitHubPackagesWebhookSyncRepo,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_current: Option<u32>,
    /// Planned/scheduled total units. Defaults to total when omitted by old producers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_total: Option<u32>,
    /// Planned/scheduled percent. Defaults to percent when omitted by old producers.
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Clone, Debug)]
pub struct JobRecord {
    pub id: String,
    pub r#type: JobType,
    pub scope: JobScope,
    pub stack_id: Option<String>,
    pub service_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub allow_arch_mismatch: bool,
    pub backup_mode: String,
    pub summary_json: Value,
}

impl JobRecord {
    pub fn new_running(
        id: String,
        r#type: JobType,
        scope: JobScope,
        stack_id: Option<String>,
        service_id: Option<String>,
        now: &str,
    ) -> Self {
        Self {
            id,
            r#type,
            scope,
            stack_id,
            service_id,
            status: "running".to_string(),
            created_at: now.to_string(),
            started_at: Some(now.to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: Value::Object(Default::default()),
        }
    }

    pub fn to_db(&self) -> JobListItem {
        JobListItem {
            id: self.id.clone(),
            r#type: self.r#type.clone(),
            scope: self.scope.clone(),
            stack_id: self.stack_id.clone(),
            service_id: self.service_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at.clone(),
            created_by: "unknown".to_string(),
            reason: "unknown".to_string(),
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            allow_arch_mismatch: self.allow_arch_mismatch,
            backup_mode: self.backup_mode.clone(),
            summary_json: self.summary_json.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIgnoresResponse {
    pub rules: Vec<IgnoreRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreRule {
    pub id: String,
    pub enabled: bool,
    pub scope: IgnoreRuleScope,
    #[serde(rename = "match")]
    pub matcher: IgnoreRuleMatch,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreRuleScope {
    #[serde(rename = "type")]
    pub kind: String,
    pub service_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreRuleMatch {
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIgnoreRequest {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub scope: IgnoreRuleScope,
    #[serde(rename = "match")]
    pub matcher: IgnoreRuleMatch,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIgnoreResponse {
    pub rule_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteIgnoreRequest {
    pub rule_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteIgnoreResponse {
    pub deleted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettingsResponse {
    pub auto_rollback: bool,
    pub backup_targets: BackupTargetOverrides,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettingsRequest {
    pub auto_rollback: bool,
    pub backup_targets: BackupTargetOverrides,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceSettingsResponse {
    pub ok: bool,
}

#[derive(Clone, Debug)]
pub struct NotificationSettings {
    pub email_enabled: bool,
    pub email_smtp_url: Option<String>,
    pub webhook_enabled: bool,
    pub webhook_url: Option<String>,
    pub telegram_enabled: bool,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub webpush_enabled: bool,
    pub webpush_vapid_public_key: Option<String>,
    pub webpush_vapid_private_key: Option<String>,
    pub webpush_vapid_subject: Option<String>,
    pub event_update_enabled: bool,
    pub event_new_version_enabled: bool,
    pub event_ghcr_webhook_anomaly_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    pub email: EmailNotification,
    pub webhook: WebhookNotification,
    pub telegram: TelegramNotification,
    pub web_push: WebPushNotification,
    #[serde(default)]
    pub events: Option<NotificationEventsConfig>,
}

impl NotificationConfig {
    pub fn from_db(db: NotificationSettings) -> Self {
        Self {
            email: EmailNotification {
                enabled: db.email_enabled,
                smtp_url: mask_if_some(db.email_smtp_url),
            },
            webhook: WebhookNotification {
                enabled: db.webhook_enabled,
                url: mask_if_some(db.webhook_url),
            },
            telegram: TelegramNotification {
                enabled: db.telegram_enabled,
                bot_token: None,
                bot_token_configured: is_non_empty(db.telegram_bot_token.as_deref()),
                chat_id: db.telegram_chat_id,
            },
            web_push: WebPushNotification {
                enabled: db.webpush_enabled,
                vapid_public_key: db.webpush_vapid_public_key,
                vapid_private_key: mask_if_some(db.webpush_vapid_private_key),
                vapid_subject: db.webpush_vapid_subject,
            },
            events: Some(NotificationEventsConfig {
                update: db.event_update_enabled,
                new_version: db.event_new_version_enabled,
                ghcr_webhook_anomaly: db.event_ghcr_webhook_anomaly_enabled,
            }),
        }
    }

    pub fn into_db(self) -> NotificationSettings {
        let events = self.events.unwrap_or_default();
        NotificationSettings {
            email_enabled: self.email.enabled,
            email_smtp_url: self.email.smtp_url,
            webhook_enabled: self.webhook.enabled,
            webhook_url: self.webhook.url,
            telegram_enabled: self.telegram.enabled,
            telegram_bot_token: self.telegram.bot_token,
            telegram_chat_id: self.telegram.chat_id,
            webpush_enabled: self.web_push.enabled,
            webpush_vapid_public_key: self.web_push.vapid_public_key,
            webpush_vapid_private_key: self.web_push.vapid_private_key,
            webpush_vapid_subject: self.web_push.vapid_subject,
            event_update_enabled: events.update,
            event_new_version_enabled: events.new_version,
            event_ghcr_webhook_anomaly_enabled: events.ghcr_webhook_anomaly,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationEventsConfig {
    #[serde(default = "notification_event_default_true")]
    pub update: bool,
    #[serde(default = "notification_event_default_true")]
    pub new_version: bool,
    #[serde(default = "notification_event_default_true")]
    pub ghcr_webhook_anomaly: bool,
}

impl Default for NotificationEventsConfig {
    fn default() -> Self {
        Self {
            update: true,
            new_version: true,
            ghcr_webhook_anomaly: true,
        }
    }
}

fn notification_event_default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smtp_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub bot_token_configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_private_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_subject: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutNotificationsResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationsRequest {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub channel: Option<NotificationTestChannel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationsResponse {
    pub ok: bool,
    pub results: Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationTestChannel {
    Email,
    Webhook,
    Telegram,
    WebPush,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscriptionRequest {
    pub endpoint: String,
    pub keys: WebPushKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushKeys {
    pub p256dh: String,
    pub auth: String,
}

// --- GitHub Packages (GHCR) webhook integration ---

#[derive(Clone, Debug)]
pub struct GitHubPackagesSettingsDb {
    pub enabled: bool,
    pub callback_url: String,
    pub pat: Option<String>,
    pub webhook_secret: Option<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesTargetDb {
    pub id: String,
    pub input: String,
    pub kind: String,
    pub owner: String,
    pub warnings: Vec<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesRepoDb {
    pub owner: String,
    pub repo: String,
    pub selected: bool,
    pub webhook_state: String,
    pub webhook_job_id: Option<String>,
    pub hook_id: Option<i64>,
    pub last_sync_at: Option<String>,
    pub last_audit_at: Option<String>,
    pub last_op: Option<String>,
    pub last_error: Option<String>,
    #[allow(dead_code)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesWebhookDeliveryDb {
    pub delivery_id: String,
    pub received_at: String,
    pub first_received_at: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub event: Option<String>,
    pub action: Option<String>,
    pub decision: String,
    pub reason: Option<String>,
    pub response_status: Option<u16>,
    pub job_id: Option<String>,
    pub attempt_count: u32,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    pub backup: BackupSettings,
    pub resource_monitor: ResourceMonitorSettings,
    pub schedules: SchedulesSettings,
    pub auth: AuthSettings,
    pub instance: InstanceSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettings {
    #[serde(default)]
    pub public_base_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSettings {
    pub forward_header_name: String,
    pub allow_anonymous_in_dev: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleItemSettings {
    pub enabled: bool,
    pub cron: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulesSettings {
    pub update_check: ScheduleItemSettings,
    pub ghcr_webhook_audit: ScheduleItemSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutSchedulesSettings {
    #[serde(default)]
    pub update_check: Option<ScheduleItemSettings>,
    #[serde(default)]
    pub ghcr_webhook_audit: Option<ScheduleItemSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutSettingsRequest {
    pub backup: BackupSettings,
    #[serde(default)]
    pub resource_monitor: Option<PutResourceMonitorSettings>,
    #[serde(default)]
    pub schedules: Option<PutSchedulesSettings>,
    pub instance: Option<PutInstanceSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutInstanceSettings {
    /// When present, updates the stored public base url. `null` (or empty string) clears it.
    #[serde(default)]
    pub public_base_url: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    pub enabled: bool,
    pub require_success: bool,
    pub base_dir: String,
    pub skip_targets_over_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMonitorSettings {
    pub enabled: bool,
    pub sample_interval_seconds: u64,
    pub retention_days: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutResourceMonitorSettings {
    pub enabled: bool,
    pub sample_interval_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutSettingsResponse {
    pub ok: bool,
}

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
pub struct DeployCheckReportResponse {
    pub overall: DeployCheckOverall,
    pub generated_at: String,
    pub checks: Vec<DeployCheckItem>,
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

fn mask_if_some(input: Option<String>) -> Option<String> {
    input.map(|_| "******".to_string())
}

fn is_non_empty(input: Option<&str>) -> bool {
    input.map(|value| !value.trim().is_empty()).unwrap_or(false)
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveredProjectStatus {
    Active,
    Missing,
    Invalid,
}

impl DiscoveredProjectStatus {
    pub fn from_str(input: &str) -> Self {
        match input {
            "active" => Self::Active,
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
    pub stacks_marked_missing: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryActionKind {
    Created,
    Updated,
    Skipped,
    Failed,
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
