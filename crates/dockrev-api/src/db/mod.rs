use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use tokio_rusqlite::Connection;

mod auto_update;
mod backups;
mod discovery;
mod github_packages;
mod jobs;
mod new_version_discoveries;
mod new_version_notifications;
mod repo_links;
mod resource_usage;
mod schema;
mod settings;
mod snapshots;
mod stacks;

pub(crate) use new_version_discoveries::{
    candidate_tag_allows_settled_fallback, canonical_candidate_identity_tag,
    canonical_visible_version_tag, collect_new_version_discovery_candidates_from_rows,
    count_new_version_discoveries_from_rows, infer_stable_candidate_display_tag_from_rows,
    new_version_discovery_notification_targets, normalize_discovery_key,
    stable_candidate_display_tag, stable_candidate_display_tag_from_tags,
};

use crate::api::types::{
    BackupSettings, ComposeConfig, ComposeRef, DeployWelcomeSettings, GitHubPackagesRepoDb,
    GitHubPackagesSettingsDb, GitHubPackagesTargetDb, GitHubPackagesWebhookDeliveryDb,
    GitHubPackagesWebhookDeliverySummary, IgnoreRule, IgnoreRuleMatch, IgnoreRuleScope,
    JobListItem, JobLogLine, JobScope, JobType, NotificationSettings, ResourceMonitorSettings,
    ScheduleItemSettings, SchedulesSettings, ServiceHomepage, ServiceResourceSample,
    ServiceSettings, ServiceUpdateGuard, StackListItem, StackRecord, StackStatus,
};

#[derive(Clone, Debug)]
pub struct BackupCleanupItem {
    pub id: String,
    pub stack_id: String,
    pub job_id: String,
    pub artifact_path: String,
}

#[derive(Clone, Debug)]
pub struct ComposeServiceSpec {
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub homepage: Option<ServiceHomepage>,
    pub update_guard: Option<ServiceUpdateGuard>,
}

#[derive(Clone, Debug)]
pub struct ServiceForCheck {
    pub id: String,
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub current_digest: Option<String>,
    pub current_runtime_started_at: Option<String>,
    pub current_resolved_tag: Option<String>,
    pub current_resolved_tags_json: Option<String>,
    pub candidate_digest: Option<String>,
    pub candidate_resolved_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceForRuntimeScan {
    pub id: String,
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub current_digest: Option<String>,
    pub current_runtime_started_at: Option<String>,
    pub current_resolved_tag: Option<String>,
    pub current_resolved_tags_json: Option<String>,
    pub candidate_digest: Option<String>,
    pub candidate_resolved_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceSnapshotTarget {
    pub image_ref: String,
    pub current_tag: String,
    pub current_digest: Option<String>,
    pub candidate_digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StoredServiceSettings {
    pub settings: ServiceSettings,
    pub repo_url_auto_disabled: bool,
    pub auto_update_policy: crate::api::types::AutoUpdatePolicy,
}

#[derive(Clone, Debug)]
pub struct AutoUpdatePendingInput {
    pub id: String,
    pub policy_scope_type: String,
    pub policy_scope_id: String,
    pub rule_id: String,
    pub stack_id: String,
    pub service_id: String,
    pub source_check_job_id: String,
    pub candidate_tag: String,
    pub candidate_display_tag: String,
    pub candidate_digest: String,
    pub current_display_tag: String,
    pub first_seen_at: String,
    pub due_at: String,
    pub min_age_seconds: u32,
    pub min_version_lag: u32,
    pub summary_json: serde_json::Value,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct AutoUpdatePendingRow {
    pub id: String,
    pub policy_scope_type: String,
    pub policy_scope_id: String,
    pub rule_id: String,
    pub stack_id: String,
    pub service_id: String,
    pub source_check_job_id: String,
    pub candidate_tag: String,
    pub candidate_display_tag: String,
    pub candidate_digest: String,
    pub current_display_tag: String,
    pub first_seen_at: String,
    pub due_at: String,
    pub min_age_seconds: u32,
    pub min_version_lag: u32,
    pub status: String,
    pub update_job_id: Option<String>,
    pub summary_json: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct RepoLinkBackfillTarget {
    pub service_id: String,
    #[allow(dead_code)]
    pub stack_id: String,
    pub stack_name: String,
    pub service_name: String,
    pub snapshot_target: ServiceSnapshotTarget,
    pub repo_url_auto_disabled: bool,
}

#[derive(Clone, Debug)]
pub struct VersionInferenceServiceTargetRow {
    pub image_ref: String,
    pub image_tag: String,
    pub candidate_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ImageDigestTagsSnapshotRow {
    pub image_repo: String,
    #[cfg_attr(not(test), allow(dead_code))]
    pub digest: String,
    pub host_platform: String,
    pub snapshot_json: String,
    pub checked_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewVersionDiscoveryRow {
    pub service_id: String,
    pub image_ref: String,
    pub discovered_at: String,
    pub current_digest: String,
    pub current_display_tag: String,
    pub current_tag: String,
    pub candidate_tag: String,
    pub candidate_digest: String,
    pub candidate_display_tag: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewVersionDiscoveryCandidate {
    pub identity_key: String,
    pub version: String,
    pub first_discovered_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceNewVersionTimelineContext {
    pub image_ref: String,
    pub current_digest: Option<String>,
    pub current_runtime_started_at: Option<String>,
    pub current_resolved_tag: Option<String>,
    pub current_tag: String,
    pub candidate_tag: Option<String>,
    pub candidate_resolved_tag: Option<String>,
    pub candidate_digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceResourceSampleInput {
    pub service_id: String,
    pub sampled_at: String,
    pub cpu_percent: f64,
    pub mem_used_bytes: Option<u64>,
    pub mem_limit_bytes: Option<u64>,
    pub net_rx_bytes: Option<u64>,
    pub net_tx_bytes: Option<u64>,
    pub block_read_bytes: Option<u64>,
    pub block_write_bytes: Option<u64>,
    pub pids: Option<u64>,
    pub container_count: u32,
}

#[derive(Clone, Debug)]
pub struct ServiceResourceTarget {
    pub service_id: String,
    pub service_name: String,
    pub compose_project: String,
}

#[derive(Clone, Debug)]
pub struct ServiceResourceOverviewSamples {
    pub service_id: String,
    pub samples: Vec<ServiceResourceSample>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewVersionNotificationRecord {
    pub id: String,
    pub service_id: String,
    pub job_id: String,
    pub reason: String,
    pub image_ref: String,
    pub image_tag: String,
    pub current_tag: String,
    pub current_display_tag: String,
    pub candidate_tag: String,
    pub candidate_display_tag: String,
    pub candidate_digest: String,
    pub status: String,
    pub sent_channels: Vec<String>,
    pub created_at: String,
    pub sent_at: Option<String>,
    pub superseded_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewVersionNotificationPending {
    pub id: String,
    pub service_id: String,
    pub job_id: String,
    pub reason: String,
    pub image_ref: String,
    pub image_tag: String,
    pub current_tag: String,
    pub current_display_tag: String,
    pub candidate_tag: String,
    pub candidate_display_tag: String,
    pub candidate_digest: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewVersionNotificationReserveResult {
    Reserved(String),
    SkippedDuplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentNewVersionNotificationTarget {
    pub service_id: String,
    pub image_ref: String,
    pub image_tag: String,
    pub candidate_digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GithubWebhookServiceTarget {
    pub stack_id: String,
    pub service_id: String,
    pub image_ref: String,
}

#[derive(Clone, Debug)]
pub struct DiscoveredComposeProjectRecord {
    pub stack_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DiscoveredComposeProjectUpsert {
    pub project: String,
    pub stack_id: Option<String>,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub last_scan_at: String,
    pub last_error: Option<String>,
    pub last_config_files: Option<Vec<String>>,
    pub unarchive_if_active: bool,
}

#[derive(Clone)]
pub struct Db {
    conn: Connection,
}

#[derive(Clone, Debug)]
pub struct JobLogRow {
    pub id: i64,
    pub ts: String,
    pub level: String,
    pub msg: String,
}

#[derive(Clone, Debug)]
pub struct JobEventLogRow {
    pub id: i64,
    pub job_id: String,
    pub ts: String,
    pub msg: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct GitHubPackagesDeliveryEventRow {
    pub id: i64,
    pub payload_json: String,
}

#[derive(Clone, Debug)]
pub struct GitHubPackagesWebhookDeliveryRecordInput {
    pub delivery_id: String,
    pub received_at: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub event: Option<String>,
    pub action: Option<String>,
    pub decision: String,
    pub reason: Option<String>,
    pub response_status: Option<u16>,
    pub job_id: Option<String>,
    pub job_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchivedFilter {
    Exclude,
    Include,
    Only,
}

fn parse_github_packages_delivery_job_ids(
    job_id: Option<&str>,
    job_ids_json: Option<&str>,
) -> Vec<String> {
    let mut job_ids = job_ids_json
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default();
    if job_ids.is_empty()
        && let Some(job_id) = job_id.filter(|value| !value.is_empty())
    {
        job_ids.push(job_id.to_string());
    }
    job_ids
}

#[derive(Clone, Debug)]
pub enum PendingJobUpsert {
    Inserted,
    Reused(Box<JobListItem>),
}

fn map_job_list_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobListItem> {
    let summary_json: String = row.get(13)?;
    let summary: serde_json::Value = serde_json::from_str(&summary_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(JobListItem {
        id: row.get(0)?,
        r#type: JobType::from_str(&row.get::<_, String>(1)?),
        scope: JobScope::from_str(&row.get::<_, String>(2)?),
        stack_id: row.get(3)?,
        service_id: row.get(4)?,
        status: row.get(5)?,
        created_by: row.get(6)?,
        reason: row.get(7)?,
        created_at: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
        backup_mode: row.get(12)?,
        summary_json: summary,
    })
}

fn job_is_stale(existing: &JobListItem, now: &str, stale_threshold: time::Duration) -> bool {
    let started_at = existing
        .started_at
        .as_deref()
        .unwrap_or(existing.created_at.as_str());
    time::OffsetDateTime::parse(started_at, &time::format_description::well_known::Rfc3339)
        .ok()
        .and_then(|started| {
            time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)
                .ok()
                .map(|cur| cur - started)
        })
        .is_some_and(|age| age > stale_threshold)
}

fn terminate_job_as_failed_tx(
    tx: &rusqlite::Transaction<'_>,
    job: &JobListItem,
    now: &str,
    reason: &str,
) -> anyhow::Result<()> {
    tx.execute(
        r#"
INSERT INTO job_logs (job_id, ts, level, msg)
VALUES (?1, ?2, 'warn', ?3)
"#,
        params![
            &job.id,
            now,
            format!(
                "job terminated: reason={reason} (previous_status={})",
                job.status
            )
        ],
    )?;

    let mut summary = job.summary_json.clone();
    if !summary.is_object() {
        summary = serde_json::json!({});
    }
    if let Some(obj) = summary.as_object_mut() {
        obj.insert(
            "terminated".to_string(),
            serde_json::json!({
                "reason": reason,
                "at": now,
            }),
        );
    }

    let summary_json = serde_json::to_string(&summary)?;
    let is_terminal = matches!(job.status.as_str(), "success" | "failed" | "rolled_back");
    if is_terminal {
        tx.execute(
            r#"
UPDATE jobs
SET finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
            params![&job.id, now, summary_json],
        )?;
    } else {
        tx.execute(
            r#"
UPDATE jobs
SET status = 'failed', finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
            params![&job.id, now, summary_json],
        )?;
    }
    Ok(())
}

fn merge_summary_string_array(existing: &mut serde_json::Value, value: &serde_json::Value) -> bool {
    let Some(existing_items) = existing.as_array_mut() else {
        return false;
    };
    let Some(new_items) = value.as_array() else {
        return false;
    };

    let mut seen = existing_items
        .iter()
        .filter_map(|item| item.as_str().map(ToString::to_string))
        .collect::<std::collections::HashSet<_>>();
    for item in new_items {
        if let Some(item) = item.as_str() {
            if seen.insert(item.to_string()) {
                existing_items.push(serde_json::Value::String(item.to_string()));
            }
        } else if !existing_items.contains(item) {
            existing_items.push(item.clone());
        }
    }
    true
}

fn merge_job_summary_value(summary: &mut serde_json::Value, fields: &serde_json::Value) {
    if !summary.is_object() {
        *summary = serde_json::Value::Object(Default::default());
    }

    if let Some(summary_obj) = summary.as_object_mut()
        && let Some(fields_obj) = fields.as_object()
    {
        for (key, value) in fields_obj {
            if matches!(
                key.as_str(),
                "matchedServiceIds" | "reusedJobIds" | "deliveryIds" | "repos"
            ) && let Some(existing) = summary_obj.get_mut(key)
                && merge_summary_string_array(existing, value)
            {
                continue;
            }
            summary_obj.insert(key.clone(), value.clone());
        }
    }
}

fn insert_job_tx(tx: &rusqlite::Transaction<'_>, job: &JobListItem) -> anyhow::Result<()> {
    tx.execute(
        r#"
INSERT INTO jobs (
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  allow_arch_mismatch,
  backup_mode,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  summary_json
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
"#,
        params![
            &job.id,
            job.r#type.as_str(),
            job.scope.as_str(),
            &job.stack_id,
            &job.service_id,
            &job.status,
            job.allow_arch_mismatch as i64,
            &job.backup_mode,
            &job.created_by,
            &job.reason,
            &job.created_at,
            &job.started_at,
            &job.finished_at,
            serde_json::to_string(&job.summary_json)?
        ],
    )?;
    Ok(())
}

impl ArchivedFilter {
    fn where_clause(self, column: &str) -> String {
        match self {
            Self::Exclude => format!("AND {column} = 0"),
            Self::Include => String::new(),
            Self::Only => format!("AND {column} = 1"),
        }
    }
}

impl Db {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let path = schema::ensure_parent_dir(path)?;
        let conn = Connection::open(path).await?;

        let db = Self { conn };
        db.init().await?;
        db.ensure_defaults().await?;
        Ok(db)
    }

    async fn call<R, F>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        self.conn
            .call(f)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    async fn init(&self) -> anyhow::Result<()> {
        self.call(|conn| {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            conn.execute_batch(schema::SCHEMA)?;
            Ok(())
        })
        .await?;
        self.migrate().await?;
        Ok(())
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        self.call(|conn| {
            schema::migrate(conn)?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn ensure_defaults(&self) -> anyhow::Result<()> {
        self.call(schema::ensure_defaults).await?;
        Ok(())
    }
}
