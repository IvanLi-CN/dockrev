use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CleanupPreset {
    Conservative,
    Balanced,
    ProjectDeepClean,
    Aggressive,
}

impl CleanupPreset {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Balanced => "balanced",
            Self::ProjectDeepClean => "project_deep_clean",
            Self::Aggressive => "aggressive",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupScope {
    Service,
    Stack,
    All,
}

impl CleanupScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Stack => "stack",
            Self::All => "all",
        }
    }
}

fn default_cleanup_scope_all() -> CleanupScope {
    CleanupScope::All
}

fn default_cleanup_scan_refresh() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CleanupResourceKind {
    Image,
    Container,
    Network,
    Volume,
    BuilderCache,
}

impl CleanupResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Container => "container",
            Self::Network => "network",
            Self::Volume => "volume",
            Self::BuilderCache => "builder_cache",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupScanReason {
    Page,
    Confirm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupApplyReason {
    Ui,
}

impl CleanupApplyReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ui => "ui",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupScanRequest {
    pub reason: CleanupScanReason,
    pub preset: CleanupPreset,
    #[serde(default = "default_cleanup_scan_refresh")]
    pub refresh: bool,
    #[serde(default = "default_cleanup_scope_all")]
    pub scope: CleanupScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupApplyRequest {
    pub reason: CleanupApplyReason,
    pub preset: CleanupPreset,
    pub scope: CleanupScope,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    pub confirmation_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupApplyResponse {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupScanStatus {
    Pending,
    Ready,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupServerDiskUsage {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

impl From<(u64, u64)> for CleanupServerDiskUsage {
    fn from((used_bytes, total_bytes): (u64, u64)) -> Self {
        Self {
            used_bytes,
            total_bytes,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResourceItem {
    pub resource_id: String,
    pub kind: CleanupResourceKind,
    pub label: String,
    pub reason: String,
    pub min_preset: CleanupPreset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_reclaimable_bytes: Option<u64>,
    #[serde(default)]
    pub estimate_unknown: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupServiceGroup {
    pub service_id: String,
    pub service_name: String,
    pub estimated_reclaimable_bytes: u64,
    #[serde(default)]
    pub has_unknown_size: bool,
    pub resources: Vec<CleanupResourceItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStackGroup {
    pub stack_id: String,
    pub stack_name: String,
    pub estimated_reclaimable_bytes: u64,
    #[serde(default)]
    pub has_unknown_size: bool,
    pub stack_orphans: Vec<CleanupResourceItem>,
    pub services: Vec<CleanupServiceGroup>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupUnownedGroup {
    pub title: String,
    pub estimated_reclaimable_bytes: u64,
    #[serde(default)]
    pub has_unknown_size: bool,
    pub resources: Vec<CleanupResourceItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupScanResponse {
    pub status: CleanupScanStatus,
    pub reason: CleanupScanReason,
    pub preset: CleanupPreset,
    pub scope: CleanupScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,
    #[serde(default)]
    pub refreshing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_reclaimable_bytes: Option<u64>,
    #[serde(default)]
    pub has_unknown_size: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_disk_usage: Option<CleanupServerDiskUsage>,
    #[serde(default)]
    pub stack_groups: Vec<CleanupStackGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unowned_group: Option<CleanupUnownedGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupInventorySnapshot {
    pub scanned_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_disk_usage: Option<CleanupServerDiskUsage>,
    pub candidates: Vec<CleanupInventoryCandidate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupInventoryCandidate {
    pub key: String,
    pub resource_id: String,
    pub kind: CleanupResourceKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_reclaimable_bytes: Option<u64>,
    #[serde(default)]
    pub estimate_unknown: bool,
    #[serde(default)]
    pub requires_ephemeral_confirmation: bool,
    pub ownership: CleanupInventoryOwnership,
    pub category: CleanupInventoryCategory,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupInventoryOwnershipType {
    Service,
    StackOrphan,
    Unowned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupInventoryOwnership {
    pub kind: CleanupInventoryOwnershipType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CleanupInventoryCategory {
    StoppedContainer,
    DanglingImage,
    ManagedUnusedImage,
    UnusedNetwork,
    ManagedUnusedVolume,
    GlobalUnusedImage,
    GlobalUnusedVolume,
    BuilderCache,
}
