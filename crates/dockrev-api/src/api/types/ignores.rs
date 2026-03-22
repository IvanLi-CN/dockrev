use super::*;

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
    pub repo_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettingsRequest {
    pub auto_rollback: bool,
    pub backup_targets: BackupTargetOverrides,
    pub repo_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutServiceSettingsResponse {
    pub ok: bool,
}
