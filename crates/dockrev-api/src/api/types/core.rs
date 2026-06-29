use super::*;

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

pub use crate::models::StackRecord;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub name: String,
    pub image: ComposeRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<ServiceHomepage>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub update_guard: Option<ServiceUpdateGuard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<IgnoreMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_inference: Option<VersionInferenceState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version_discovery_count: Option<u32>,
    pub settings: ServiceSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpdateGuard {
    pub blocked: bool,
    pub code: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHomepage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ServiceHomepage {
    pub fn is_empty(&self) -> bool {
        self.group.is_none()
            && self.name.is_none()
            && self.icon.is_none()
            && self.href.is_none()
            && self.description.is_none()
    }
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
    pub repo_url: Option<String>,
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupTargetPolicy {
    Disabled,
    StopRelatedServices,
    LiveBackup,
}

impl BackupTargetPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::StopRelatedServices => "stop_related_services",
            Self::LiveBackup => "live_backup",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "stop_related_services" => Self::StopRelatedServices,
            "live_backup" => Self::LiveBackup,
            _ => Self::Disabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdatePolicyMode {
    #[default]
    Inherit,
    Override,
    Disabled,
}

impl AutoUpdatePolicyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Override => "override",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_str(input: &str) -> Self {
        match input {
            "override" => Self::Override,
            "disabled" => Self::Disabled,
            _ => Self::Inherit,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdateMatcherType {
    Semver,
    Regex,
    Glob,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoUpdateMatcher {
    #[serde(rename = "type")]
    pub kind: AutoUpdateMatcherType,
    pub pattern: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdateRuleAction {
    Immediate,
    Delayed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoUpdateDelay {
    pub min_age_seconds: u32,
    pub min_version_lag: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoUpdateRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub matcher: AutoUpdateMatcher,
    pub action: AutoUpdateRuleAction,
    #[serde(default)]
    pub delay: AutoUpdateDelay,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutoUpdatePolicy {
    #[serde(default)]
    pub mode: AutoUpdatePolicyMode,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<AutoUpdateRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

pub use crate::models::ServiceSeed;

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

impl BackupTarget {
    #[cfg(test)]
    pub fn key(&self) -> &str {
        match self {
            Self::DockerVolume { name } => name,
            Self::BindMount { path } => path,
        }
    }
}
