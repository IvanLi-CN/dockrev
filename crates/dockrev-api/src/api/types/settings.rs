use super::*;

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
    pub group_header_name: String,
    pub allow_anonymous_in_dev: bool,
    pub authorization_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_user_masked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_group_masked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_user: Option<String>,
    #[serde(default)]
    pub current_groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub matched_by: String,
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
