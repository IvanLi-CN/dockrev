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
    pub settings: ServiceSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub tag: String,
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
    Update,
    Rollback,
}

impl JobType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Discovery => "discovery",
            Self::Update => "update",
            Self::Rollback => "rollback",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "check" => Self::Check,
            "discovery" => Self::Discovery,
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

impl JobListItem {
    pub fn into_api(self) -> JobApiListItem {
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
    pub logs: Vec<JobLogLine>,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    pub email: EmailNotification,
    pub webhook: WebhookNotification,
    pub telegram: TelegramNotification,
    pub web_push: WebPushNotification,
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
                bot_token: mask_if_some(db.telegram_bot_token),
                chat_id: mask_if_some(db.telegram_chat_id),
            },
            web_push: WebPushNotification {
                enabled: db.webpush_enabled,
                vapid_public_key: db.webpush_vapid_public_key,
                vapid_private_key: mask_if_some(db.webpush_vapid_private_key),
                vapid_subject: db.webpush_vapid_subject,
            },
        }
    }

    pub fn into_db(self) -> NotificationSettings {
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
        }
    }
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationsResponse {
    pub ok: bool,
    pub results: Value,
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
    pub auth: AuthSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSettings {
    pub forward_header_name: String,
    pub allow_anonymous_in_dev: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutSettingsRequest {
    pub backup: BackupSettings,
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
pub struct PutSettingsResponse {
    pub ok: bool,
}

fn mask_if_some(input: Option<String>) -> Option<String> {
    input.map(|_| "******".to_string())
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
pub struct ServiceCandidatesResponse {
    pub candidates: Vec<ServiceCandidateOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCandidateOption {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub arch_match: ArchMatch,
    #[serde(default)]
    pub arch: Vec<String>,
    pub ignored: bool,
}
