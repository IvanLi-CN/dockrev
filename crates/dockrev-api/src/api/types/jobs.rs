use std::fmt;

use super::*;

fn deserialize_planned_percent<'de, D>(deserializer: D) -> Result<Option<Option<u32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct PlannedPercentVisitor;

    impl<'de> serde::de::Visitor<'de> for PlannedPercentVisitor {
        type Value = Option<Option<u32>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a planned percent number or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(None))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(None))
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            u32::deserialize(deserializer).map(|value| Some(Some(value)))
        }
    }

    deserializer.deserialize_option(PlannedPercentVisitor)
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_current: Option<u32>,
    /// Planned/scheduled total units. Defaults to total when omitted by old producers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_total: Option<u32>,
    /// Planned/scheduled percent. `None` keeps legacy records omitted; `Some(None)` emits explicit null.
    #[serde(
        default,
        deserialize_with = "deserialize_planned_percent",
        skip_serializing_if = "Option::is_none"
    )]
    pub planned_percent: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
    pub updated_at: String,
}

fn progress_from_summary(summary: &Value) -> Option<JobProgress> {
    let progress = summary.as_object()?.get("progress")?.clone();
    serde_json::from_value::<JobProgress>(progress).ok()
}

fn value_as_non_empty_str(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn object_field_as_non_empty_str<'a>(
    object: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a str> {
    value_as_non_empty_str(object.and_then(|obj| obj.get(key)))
}

fn first_stack_transition_summary<'a>(
    summary: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    summary
        .get("stacks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|stack| {
            stack
                .as_object()
                .and_then(|stack_obj| stack_obj.get(key).and_then(Value::as_object))
        })
}

fn transition_summary_has_failure_fields(summary: &serde_json::Map<String, Value>) -> bool {
    if object_field_as_non_empty_str(Some(summary), "failureStep").is_some()
        || object_field_as_non_empty_str(Some(summary), "lastError").is_some()
    {
        return true;
    }

    summary
        .get("pullTagWarnings")
        .and_then(Value::as_array)
        .is_some_and(|warnings| {
            warnings.iter().any(|warning| {
                warning.as_object().is_some_and(|warning_obj| {
                    object_field_as_non_empty_str(Some(warning_obj), "lastError").is_some()
                        || object_field_as_non_empty_str(Some(warning_obj), "error").is_some()
                })
            })
        })
}

fn stack_transition_summary_for_status<'a>(
    summary: &'a serde_json::Map<String, Value>,
    key: &str,
    status: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    let stack_summaries = summary
        .get("stacks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|stack| {
            stack
                .as_object()
                .and_then(|stack_obj| stack_obj.get(key).and_then(Value::as_object))
        })
        .collect::<Vec<_>>();

    if matches!(status, "failed" | "rolled_back")
        && let Some(failed_summary) = stack_summaries
            .iter()
            .copied()
            .find(|transition_summary| transition_summary_has_failure_fields(transition_summary))
    {
        return Some(failed_summary);
    }

    stack_summaries
        .first()
        .copied()
        .or_else(|| first_stack_transition_summary(summary, key))
}

fn normalize_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn humanize_failure_step(step: &str) -> &'static str {
    match step {
        "healthcheck" => "健康检查失败",
        "pull_target_tag" => "镜像拉取失败",
        "sync_configured_tag" => "Compose tag 同步失败",
        _ => "任务失败",
    }
}

fn humanize_failure_detail(step: &str) -> &'static str {
    match step {
        "healthcheck" => "健康检查未通过，已停止本次变更并恢复到回滚前状态。",
        "pull_target_tag" => "目标镜像拉取失败，已停止本次变更并恢复到回滚前状态。",
        "sync_configured_tag" => "Compose tag 同步失败，已停止本次变更并恢复到回滚前状态。",
        _ => "任务执行失败，详情请参考原始输出。",
    }
}

fn lower_contains_any(haystack: &str, needles: &[&str]) -> bool {
    let lowercase = haystack.to_ascii_lowercase();
    needles
        .iter()
        .any(|needle| lowercase.contains(&needle.to_ascii_lowercase()))
}

fn detect_registry_rate_limit(raw: &str) -> bool {
    lower_contains_any(
        raw,
        &[
            "toomanyrequests",
            "too many requests",
            "rate limit",
            "ratelimit",
            "429",
            "docker hub",
        ],
    )
}

fn friendly_result_reason_from_transition_summary(
    job_type: &str,
    status: &str,
    summary: &serde_json::Map<String, Value>,
    progress: Option<&JobProgress>,
) -> Option<JobResultReason> {
    let transition_key = match job_type {
        "update" => "update",
        "rollback" => "rollback",
        _ => return None,
    };
    let update_summary = stack_transition_summary_for_status(summary, transition_key, status)
        .or_else(|| Some(summary))?;

    let failure_step = object_field_as_non_empty_str(Some(update_summary), "failureStep");
    let raw_last_error = object_field_as_non_empty_str(Some(update_summary), "lastError");
    let pull_tag_warning = update_summary
        .get("pullTagWarnings")
        .and_then(Value::as_array)
        .and_then(|warnings| warnings.first())
        .and_then(Value::as_object);
    let warning_last_error = object_field_as_non_empty_str(pull_tag_warning, "lastError")
        .or_else(|| object_field_as_non_empty_str(pull_tag_warning, "error"));
    let raw_error = raw_last_error
        .or(warning_last_error)
        .map(normalize_message)
        .filter(|value| !value.is_empty());
    let terminal_message = progress
        .map(|item| normalize_message(&item.message))
        .filter(|value| !value.is_empty());

    if status == "success" && job_type == "update" {
        let detail = progress
            .map(|item| normalize_message(&item.message))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "更新已完成，目标版本已应用。".to_string());
        return Some(JobResultReason {
            summary: "更新完成".to_string(),
            detail,
            raw: None,
        });
    }

    match status {
        "rolled_back" => {
            if job_type == "rollback" {
                let detail = progress
                    .map(|item| normalize_message(&item.message))
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "回滚已完成，目标服务已恢复到指定版本。".to_string());
                return Some(JobResultReason {
                    summary: "回滚完成".to_string(),
                    raw: None,
                    detail,
                });
            }
            if let Some(step) = failure_step {
                let summary = if step == "pull_target_tag"
                    && raw_error.as_deref().is_some_and(detect_registry_rate_limit)
                {
                    "镜像拉取失败（Registry / Docker Hub 限流），已回滚".to_string()
                } else {
                    format!("{}，已回滚", humanize_failure_step(step))
                };
                let detail = match step {
                    "pull_target_tag" if raw_error.as_deref().is_some_and(detect_registry_rate_limit) => {
                        "镜像拉取命中 Registry / Docker Hub 限流，Dockrev 已终止更新并自动回滚到升级前版本。"
                            .to_string()
                    }
                    _ => humanize_failure_detail(step).to_string(),
                };
                return Some(JobResultReason {
                    summary,
                    detail: detail.clone(),
                    raw: raw_error.filter(|value| value != &detail),
                });
            }
            let detail =
                terminal_message.unwrap_or_else(|| "更新未完成，Dockrev 已执行回滚。".to_string());
            return Some(JobResultReason {
                summary: "更新未完成，已回滚".to_string(),
                raw: None,
                detail,
            });
        }
        "failed" => {
            if let Some(step) = failure_step {
                let summary = if step == "pull_target_tag"
                    && raw_error.as_deref().is_some_and(detect_registry_rate_limit)
                {
                    if job_type == "rollback" {
                        "回滚镜像拉取失败（Registry / Docker Hub 限流）".to_string()
                    } else {
                        "镜像拉取失败（Registry / Docker Hub 限流）".to_string()
                    }
                } else {
                    match job_type {
                        "rollback" => format!("回滚失败（{}）", humanize_failure_step(step)),
                        _ => humanize_failure_step(step).to_string(),
                    }
                };
                let detail = match step {
                    "pull_target_tag"
                        if raw_error.as_deref().is_some_and(detect_registry_rate_limit) =>
                    {
                        if job_type == "rollback" {
                            "回滚目标镜像拉取命中 Registry / Docker Hub 限流，本次回滚未能继续执行。".to_string()
                        } else {
                            "镜像拉取命中 Registry / Docker Hub 限流，本次任务未能继续执行。"
                                .to_string()
                        }
                    }
                    _ => raw_error
                        .clone()
                        .unwrap_or_else(|| humanize_failure_detail(step).to_string()),
                };
                return Some(JobResultReason {
                    summary,
                    detail: detail.clone(),
                    raw: raw_error.filter(|value| value != &detail),
                });
            }
        }
        _ => {}
    }

    None
}

fn generic_result_reason(
    status: &str,
    summary: &serde_json::Map<String, Value>,
    progress: Option<&JobProgress>,
) -> Option<JobResultReason> {
    let progress_message = progress
        .map(|item| normalize_message(&item.message))
        .filter(|value| !value.is_empty());
    let summary_error = object_field_as_non_empty_str(Some(summary), "error")
        .map(normalize_message)
        .filter(|value| !value.is_empty());

    match status {
        "success" => {
            let detail = progress_message?;
            let lowered = detail.to_ascii_lowercase();
            let low_signal = [
                "finished",
                "done",
                "completed",
                "success",
                "rollback finished",
                "update finished",
            ]
            .iter()
            .any(|needle| lowered == *needle || lowered == format!("job {needle}"));
            if low_signal {
                return None;
            }
            Some(JobResultReason {
                summary: "任务已完成".to_string(),
                detail,
                raw: None,
            })
        }
        "failed" => {
            let detail = summary_error.or(progress_message)?;
            Some(JobResultReason {
                summary: "任务执行失败".to_string(),
                raw: None,
                detail,
            })
        }
        "rolled_back" => {
            let detail = progress_message?;
            Some(JobResultReason {
                summary: "任务已回滚".to_string(),
                raw: None,
                detail,
            })
        }
        _ => None,
    }
}

pub(crate) fn result_reason_from_summary(
    job_type: &str,
    status: &str,
    summary: &Value,
    progress: Option<&JobProgress>,
) -> Option<JobResultReason> {
    if !matches!(status, "success" | "failed" | "rolled_back") {
        return None;
    }
    let summary_object = summary.as_object()?;
    friendly_result_reason_from_transition_summary(job_type, status, summary_object, progress)
        .or_else(|| generic_result_reason(status, summary_object, progress))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResultReason {
    pub summary: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn result_reason_prefers_failed_stack_summary_over_first_stack() {
        let summary = json!({
            "stacks": [
                {
                    "stackId": "stk_ok",
                    "update": {
                        "changedServices": 1
                    }
                },
                {
                    "stackId": "stk_failed",
                    "update": {
                        "failureStep": "healthcheck",
                        "lastError": "container api never became healthy"
                    }
                }
            ]
        });

        let reason = result_reason_from_summary("update", "rolled_back", &summary, None)
            .expect("result reason should be derived");

        assert_eq!(reason.summary, "健康检查失败，已回滚");
        assert_eq!(
            reason.detail,
            "健康检查未通过，已停止本次变更并恢复到回滚前状态。"
        );
        assert_eq!(
            reason.raw.as_deref(),
            Some("container api never became healthy")
        );
    }
}

impl JobListItem {
    pub fn into_api(self) -> JobApiListItem {
        let progress = progress_from_summary(&self.summary_json);
        let job_type = self.r#type.as_str().to_string();
        let result_reason = result_reason_from_summary(
            &job_type,
            &self.status,
            &self.summary_json,
            progress.as_ref(),
        );
        JobApiListItem {
            id: self.id,
            r#type: job_type,
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
            result_reason,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_reason: Option<JobResultReason>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_reason: Option<JobResultReason>,
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
