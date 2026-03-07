use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use tokio_rusqlite::Connection;

use crate::api::types::{
    BackupSettings, ComposeConfig, ComposeRef, DeployWelcomeSettings, GitHubPackagesRepoDb,
    GitHubPackagesSettingsDb, GitHubPackagesTargetDb, GitHubPackagesWebhookDeliveryDb,
    GitHubPackagesWebhookDeliverySummary, IgnoreRule, IgnoreRuleMatch, IgnoreRuleScope,
    JobListItem, JobLogLine, JobScope, JobType, NotificationSettings, ResourceMonitorSettings,
    ScheduleItemSettings, SchedulesSettings, ServiceResourceSample, ServiceSettings, StackListItem,
    StackRecord, StackStatus,
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
}

#[derive(Clone, Debug)]
pub struct ServiceForCheck {
    pub id: String,
    pub name: String,
    pub image_ref: String,
    pub image_tag: String,
    pub current_digest: Option<String>,
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
    pub current_resolved_tag: Option<String>,
    pub current_resolved_tags_json: Option<String>,
    pub candidate_digest: Option<String>,
    pub candidate_resolved_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceSnapshotTarget {
    pub image_ref: String,
    pub current_digest: Option<String>,
    pub candidate_digest: Option<String>,
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
    pub host_platform: String,
    pub snapshot_json: String,
    pub checked_at: String,
    pub updated_at: String,
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
        let path = ensure_parent_dir(path)?;
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
            conn.execute_batch(SCHEMA)?;
            Ok(())
        })
        .await?;
        self.migrate().await?;
        Ok(())
    }

    async fn migrate(&self) -> anyhow::Result<()> {
        self.call(|conn| {
            ensure_service_columns(conn)?;
            ensure_notification_columns(conn)?;
            ensure_settings_deploy_welcome_columns(conn)?;
            ensure_settings_resource_monitor_columns(conn)?;
            ensure_settings_schedule_columns(conn)?;
            ensure_settings_public_base_url_columns(conn)?;
            ensure_stack_archive_columns(conn)?;
            ensure_service_archive_columns(conn)?;
            ensure_discovery_schema(conn)?;
            ensure_github_packages_repos_webhook_columns(conn)?;
            ensure_github_packages_deliveries_columns(conn)?;
            ensure_schema_migrations_table(conn)?;
            apply_migration_0007_remove_manual_stacks(conn)?;
            apply_migration_0008_drop_version_inference_snapshots(conn)?;
            auto_archive_missing_discovery_projects_on_startup(conn)?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn ensure_defaults(&self) -> anyhow::Result<()> {
        self.call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            tx.execute(
                r#"
INSERT OR IGNORE INTO settings (
  id,
  backup_enabled,
  backup_require_success,
  backup_base_dir,
  backup_skip_targets_over_bytes,
  resource_monitor_enabled,
  resource_sample_interval_seconds,
  schedule_update_check_enabled,
  schedule_update_check_cron,
  schedule_ghcr_webhook_audit_enabled,
  schedule_ghcr_webhook_audit_cron,
  deploy_welcome_never_auto_open,
  deploy_welcome_updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
"#,
                params![
                    "default",
                    1i64,
                    1i64,
                    "/data/backups",
                    104857600i64,
                    1i64,
                    30i64,
                    0i64,
                    "*/30 * * * *",
                    1i64,
                    "0 3 * * *",
                    0i64,
                    Option::<String>::None
                ],
            )?;

            tx.execute(
                r#"
INSERT OR IGNORE INTO notification_settings (
  id,
  email_enabled,
  email_smtp_url,
  webhook_enabled,
  webhook_url,
  telegram_enabled,
  telegram_bot_token,
  telegram_chat_id,
  webpush_enabled,
  webpush_vapid_public_key,
  webpush_vapid_private_key,
  webpush_vapid_subject,
  event_update_enabled,
  event_new_version_enabled,
  event_ghcr_webhook_anomaly_enabled
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
"#,
                params![
                    "default",
                    0i64,
                    Option::<String>::None,
                    0i64,
                    Option::<String>::None,
                    0i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    0i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    1i64,
                    1i64,
                    1i64
                ],
            )?;

            tx.execute(
                r#"
INSERT OR IGNORE INTO github_packages_settings (
  id,
  enabled,
  callback_url,
  pat,
  webhook_secret
) VALUES (?1, ?2, ?3, ?4, ?5)
"#,
                params![
                    "default",
                    0i64,
                    "",
                    Option::<String>::None,
                    Option::<String>::None
                ],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn list_stacks(
        &self,
        archived: ArchivedFilter,
    ) -> anyhow::Result<Vec<StackListItem>> {
        self.call(move |conn| {
            let filter_clause = archived.where_clause("s.archived");
            let sql = format!(
                r#"
SELECT
  s.id,
  s.name,
  s.last_check_at,
  s.archived,
  (SELECT COUNT(1) FROM services sv WHERE sv.stack_id = s.id) AS services,
  (SELECT COUNT(1) FROM services sv WHERE sv.stack_id = s.id AND sv.archived = 1) AS archived_services,
  (
    SELECT COUNT(1)
    FROM services sv
    WHERE
      sv.stack_id = s.id
      AND sv.candidate_tag IS NOT NULL
      AND sv.ignore_rule_id IS NULL
      AND sv.candidate_arch_match = 'match'
  ) AS updates
FROM stacks s
WHERE 1=1
{filter_clause}
ORDER BY s.created_at DESC
"#,
            );
            let mut stmt = conn.prepare(&sql)?;

            let rows = stmt.query_map([], |row| {
                Ok(StackListItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: StackStatus::Unknown,
                    last_check_at: row.get(2)?,
                    archived: Some(row.get::<_, i64>(3)? != 0),
                    services: row.get::<_, i64>(4)? as u32,
                    archived_services: Some(row.get::<_, i64>(5)? as u32),
                    updates: row.get::<_, i64>(6)? as u32,
                })
            })?;

            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list stacks")
    }

    pub async fn get_stack(&self, stack_id: &str) -> anyhow::Result<Option<StackRecord>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let stack = conn
                .query_row(
                    r#"
SELECT
  id,
  name,
  compose_type,
  compose_files_json,
  env_file,
  backup_targets_json,
  backup_retention_keep_last,
  backup_retention_delete_after_stable_seconds,
  archived
FROM stacks
WHERE id = ?1
"#,
                    params![stack_id],
                    |row| {
                        let compose_files_json: String = row.get(3)?;
                        let backup_targets_json: String = row.get(5)?;

                        let compose_files: Vec<String> = serde_json::from_str(&compose_files_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;

                        let backup_targets: Vec<crate::api::types::BackupTarget> =
                            serde_json::from_str(&backup_targets_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;

                        Ok(StackRecord {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            archived: row.get::<_, i64>(8)? != 0,
                            compose: ComposeConfig {
                                kind: row.get(2)?,
                                compose_files,
                                env_file: row.get(4)?,
                            },
                            backup: crate::api::types::StackBackupConfig {
                                targets: backup_targets,
                                retention: crate::api::types::BackupRetention {
                                    keep_last: row.get::<_, i64>(6)? as u32,
                                    delete_after_stable_seconds: row.get::<_, i64>(7)? as u32,
                                },
                            },
                            services: Vec::new(),
                        })
                    },
                )
                .optional()?;

            let Some(mut stack) = stack else {
                return Ok(None);
            };

            let mut stmt = conn.prepare(
                r#"
	SELECT
	  id,
	  name,
	  image_ref,
	  image_tag,
	  current_digest,
	  current_resolved_tag,
	  current_resolved_tags_json,
	  candidate_tag,
	  candidate_resolved_tag,
	  candidate_digest,
	  candidate_arch_match,
	  candidate_arch_json,
	  ignore_rule_id,
	  ignore_reason,
	  auto_rollback,
	  archived,
	  backup_targets_bind_paths_json,
	  backup_targets_volume_names_json
	FROM services
	WHERE stack_id = ?1
	ORDER BY name ASC
"#,
            )?;
            let mut rows = stmt.query(params![stack.id.clone()])?;

            while let Some(row) = rows.next()? {
                let bind_paths_json: String = row.get(16)?;
                let volume_names_json: String = row.get(17)?;
                let bind_paths: BTreeMap<String, crate::api::types::TernaryChoice> =
                    serde_json::from_str(&bind_paths_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let volume_names: BTreeMap<String, crate::api::types::TernaryChoice> =
                    serde_json::from_str(&volume_names_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                let current_resolved_tag: Option<String> = row.get(5)?;
                let current_resolved_tags_json: Option<String> = row.get(6)?;

                let candidate_tag: Option<String> = row.get(7)?;
                let candidate_resolved_tag: Option<String> = row.get(8)?;
                let candidate_digest: Option<String> = row.get(9)?;
                let candidate_arch_match: Option<String> = row.get(10)?;
                let candidate_arch_json: Option<String> = row.get(11)?;
                let ignore_rule_id: Option<String> = row.get(12)?;
                let ignore_reason: Option<String> = row.get(13)?;

                let current_resolved_tags: Option<Vec<String>> = current_resolved_tags_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .and_then(|v| if v.is_empty() { None } else { Some(v) });

                let candidate_arch: Vec<String> = candidate_arch_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .unwrap_or_default();

                let candidate = match (candidate_tag, candidate_digest) {
                    (Some(tag), Some(digest)) => Some(crate::api::types::Candidate {
                        tag,
                        resolved_tag: candidate_resolved_tag,
                        digest,
                        arch_match: crate::api::types::ArchMatch::from_str(
                            candidate_arch_match.as_deref().unwrap_or("unknown"),
                        ),
                        arch: candidate_arch,
                    }),
                    _ => None,
                };

                let ignore = match (ignore_rule_id, ignore_reason) {
                    (Some(rule_id), Some(reason)) => Some(crate::api::types::IgnoreMatch {
                        matched: true,
                        rule_id,
                        reason,
                    }),
                    _ => None,
                };

                stack.services.push(crate::api::types::Service {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image: ComposeRef {
                        reference: row.get(2)?,
                        tag: row.get(3)?,
                        digest: row.get(4)?,
                        resolved_tag: current_resolved_tag,
                        resolved_tags: current_resolved_tags,
                    },
                    candidate,
                    ignore,
                    version_inference: None,
                    settings: ServiceSettings {
                        auto_rollback: row.get::<_, i64>(14)? != 0,
                        backup_targets: crate::api::types::BackupTargetOverrides {
                            bind_paths,
                            volume_names,
                        },
                    },
                    archived: Some(row.get::<_, i64>(15)? != 0),
                });
            }

            Ok(Some(stack))
        })
        .await
        .context("get stack")
    }

    pub async fn insert_stack(
        &self,
        stack: &StackRecord,
        services: &[crate::api::types::ServiceSeed],
        now: &str,
    ) -> anyhow::Result<()> {
        let stack = stack.clone();
        let services = services.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            tx.execute(
                r#"
INSERT INTO stacks (
  id,
  name,
  compose_type,
  compose_files_json,
  env_file,
  backup_targets_json,
  backup_retention_keep_last,
  backup_retention_delete_after_stable_seconds,
  created_at,
  updated_at,
  last_check_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
"#,
                params![
                    stack.id,
                    stack.name,
                    stack.compose.kind,
                    serde_json::to_string(&stack.compose.compose_files)?,
                    stack.compose.env_file,
                    serde_json::to_string(&stack.backup.targets)?,
                    stack.backup.retention.keep_last as i64,
                    stack.backup.retention.delete_after_stable_seconds as i64,
                    now,
                    now,
                    now
                ],
            )?;

            for svc in services {
                tx.execute(
                    r#"
INSERT INTO services (
  id,
  stack_id,
  name,
  image_ref,
  image_tag,
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
"#,
                    params![
                        svc.id,
                        stack.id,
                        svc.name,
                        svc.image_ref,
                        svc.image_tag,
                        svc.auto_rollback as i64,
                        serde_json::to_string(&svc.backup_bind_paths)?,
                        serde_json::to_string(&svc.backup_volume_names)?,
                        now,
                        now
                    ],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("insert stack")
    }

    pub async fn update_stack_last_check_at(
        &self,
        stack_id: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE stacks SET last_check_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![stack_id, now],
            )?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn set_stack_archived(
        &self,
        stack_id: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = if archived {
                conn.execute(
                    r#"
UPDATE stacks
SET archived = 1, archived_at = ?2, archived_reason = ?3, updated_at = ?2
WHERE id = ?1
"#,
                    params![stack_id, now, reason],
                )?
            } else {
                conn.execute(
                    r#"
UPDATE stacks
SET archived = 0, archived_at = NULL, archived_reason = NULL, updated_at = ?2
WHERE id = ?1
"#,
                    params![stack_id, now],
                )?
            };
            Ok(changed > 0)
        })
        .await
        .context("set stack archived")
    }

    pub async fn set_service_archived(
        &self,
        service_id: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = if archived {
                conn.execute(
                    r#"
UPDATE services
SET archived = 1, archived_at = ?2, archived_reason = ?3, updated_at = ?2
WHERE id = ?1
"#,
                    params![service_id, now, reason],
                )?
            } else {
                conn.execute(
                    r#"
UPDATE services
SET archived = 0, archived_at = NULL, archived_reason = NULL, updated_at = ?2
WHERE id = ?1
"#,
                    params![service_id, now],
                )?
            };
            Ok(changed > 0)
        })
        .await
        .context("set service archived")
    }

    pub async fn sync_stack_from_compose(
        &self,
        stack_id: &str,
        compose_files: &[String],
        services: &[ComposeServiceSpec],
        now: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let compose_files = compose_files.to_vec();
        let services = services.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            tx.execute(
                r#"
UPDATE stacks
SET compose_files_json = ?2, updated_at = ?3
WHERE id = ?1
"#,
                params![stack_id, serde_json::to_string(&compose_files)?, now],
            )?;

            let existing_by_name = {
                let mut stmt = tx.prepare("SELECT id, name FROM services WHERE stack_id = ?1")?;
                let existing_rows = stmt.query_map(params![stack_id.clone()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                let mut m = BTreeMap::<String, String>::new();
                for r in existing_rows {
                    let (id, name) = r?;
                    m.insert(name, id);
                }
                m
            };

            let mut keep_ids = Vec::<String>::new();

            for svc in services {
                if let Some(id) = existing_by_name.get(&svc.name) {
                    tx.execute(
                        r#"
UPDATE services
SET
  image_ref = ?2,
  image_tag = ?3,
  current_digest = NULL,
  current_resolved_tag = NULL,
  current_resolved_tags_json = NULL,
  candidate_tag = NULL,
  candidate_resolved_tag = NULL,
  candidate_digest = NULL,
  candidate_arch_match = NULL,
  candidate_arch_json = NULL,
  ignore_rule_id = NULL,
  ignore_reason = NULL,
  checked_at = NULL,
  updated_at = ?4
WHERE id = ?1
"#,
                        params![id, svc.image_ref, svc.image_tag, now],
                    )?;
                    keep_ids.push(id.clone());
                } else {
                    let id = crate::ids::new_service_id();
                    tx.execute(
                        r#"
INSERT INTO services (
  id,
  stack_id,
  name,
  image_ref,
  image_tag,
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
"#,
                        params![
                            id,
                            stack_id,
                            svc.name,
                            svc.image_ref,
                            svc.image_tag,
                            1i64,
                            "{}",
                            "{}",
                            now,
                            now
                        ],
                    )?;
                    keep_ids.push(id);
                }
            }

            if keep_ids.is_empty() {
                tx.execute(
                    "DELETE FROM services WHERE stack_id = ?1",
                    params![stack_id],
                )?;
            } else {
                let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "DELETE FROM services WHERE stack_id = ? AND id NOT IN ({placeholders})"
                );
                let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + keep_ids.len());
                params.push(&stack_id);
                for id in &keep_ids {
                    params.push(id);
                }
                tx.execute(&sql, params.as_slice())?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("sync stack from compose")
    }

    pub async fn list_services_for_check(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<ServiceForCheck>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  name,
  image_ref,
  image_tag,
  current_digest,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_digest,
  candidate_resolved_tag
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| {
                Ok(ServiceForCheck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image_ref: row.get(2)?,
                    image_tag: row.get(3)?,
                    current_digest: row.get(4)?,
                    current_resolved_tag: row.get(5)?,
                    current_resolved_tags_json: row.get(6)?,
                    candidate_digest: row.get(7)?,
                    candidate_resolved_tag: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list services for check")
    }

    pub async fn list_services_for_runtime_scan(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<ServiceForRuntimeScan>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  name,
  image_ref,
  image_tag,
  current_digest,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_digest,
  candidate_resolved_tag
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| {
                Ok(ServiceForRuntimeScan {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image_ref: row.get(2)?,
                    image_tag: row.get(3)?,
                    current_digest: row.get(4)?,
                    current_resolved_tag: row.get(5)?,
                    current_resolved_tags_json: row.get(6)?,
                    candidate_digest: row.get(7)?,
                    candidate_resolved_tag: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list services for runtime scan")
    }

    pub async fn get_stack_compose_project(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT project
FROM discovered_compose_projects
WHERE
  stack_id = ?1
  AND status != 'missing'
  AND archived = 0
ORDER BY last_scan_at DESC
LIMIT 1
"#,
                    params![stack_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
        .context("get stack compose project")
    }

    pub async fn get_service_stack_id(&self, service_id: &str) -> anyhow::Result<Option<String>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT id, stack_id, image_ref, image_tag
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| row.get::<_, String>(1),
                )
                .optional()?)
        })
        .await
        .context("get service stack id")
    }

    pub async fn get_service_snapshot_target(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceSnapshotTarget>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT image_ref, current_digest, candidate_digest
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| {
                        Ok(ServiceSnapshotTarget {
                            image_ref: row.get(0)?,
                            current_digest: row.get(1)?,
                            candidate_digest: row.get(2)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get service snapshot target")
    }

    pub async fn list_snapshot_seed_targets(&self) -> anyhow::Result<Vec<(String, String)>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT image_ref, current_digest
FROM services
WHERE current_digest IS NOT NULL AND TRIM(current_digest) != ''
ORDER BY id ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list snapshot seed targets")
    }

    pub async fn list_snapshot_anchor_tags(
        &self,
        image_repo: &str,
        digest: &str,
    ) -> anyhow::Result<Vec<String>> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  image_ref,
  image_tag,
  current_digest,
  current_resolved_tag,
  candidate_tag,
  candidate_digest,
  candidate_resolved_tag
FROM services
WHERE
  (current_digest IS NOT NULL AND TRIM(current_digest) = ?1)
  OR (candidate_digest IS NOT NULL AND TRIM(candidate_digest) = ?1)
ORDER BY id ASC
"#,
            )?;
            let rows = stmt.query_map(params![digest], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?;

            let mut tags: BTreeSet<String> = BTreeSet::new();
            for row in rows {
                let (
                    image_ref,
                    image_tag,
                    current_digest,
                    current_resolved_tag,
                    candidate_tag,
                    candidate_digest,
                    candidate_resolved_tag,
                ) = row?;

                let Some(parsed) = crate::registry::ImageRef::parse(&image_ref).ok() else {
                    continue;
                };
                let row_repo = format!("{}/{}", parsed.registry, parsed.name);
                if row_repo != image_repo {
                    continue;
                }

                let current_matches = current_digest
                    .as_deref()
                    .is_some_and(|d| d.trim() == digest.as_str());
                let candidate_matches = candidate_digest
                    .as_deref()
                    .is_some_and(|d| d.trim() == digest.as_str());

                if current_matches {
                    let tag = image_tag.trim();
                    if !tag.is_empty() {
                        tags.insert(tag.to_string());
                    }
                    if let Some(tag) = current_resolved_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                }

                if candidate_matches {
                    if let Some(tag) = candidate_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                    if let Some(tag) = candidate_resolved_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                    let current_tag = image_tag.trim();
                    if !current_tag.is_empty() {
                        tags.insert(current_tag.to_string());
                    }
                }
            }

            Ok(tags.into_iter().collect())
        })
        .await
        .context("list snapshot anchor tags")
    }

    pub async fn is_stack_archived(&self, stack_id: &str) -> anyhow::Result<Option<bool>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT archived FROM stacks WHERE id = ?1",
                    params![stack_id],
                    |row| Ok(row.get::<_, i64>(0)? != 0),
                )
                .optional()?)
        })
        .await
        .context("is stack archived")
    }

    pub async fn is_service_archived(&self, service_id: &str) -> anyhow::Result<Option<bool>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT archived FROM services WHERE id = ?1",
                    params![service_id],
                    |row| Ok(row.get::<_, i64>(0)? != 0),
                )
                .optional()?)
        })
        .await
        .context("is service archived")
    }

    pub async fn has_unarchived_services_in_stack(&self, stack_id: &str) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM services WHERE stack_id = ?1 AND archived = 0 LIMIT 1",
                    params![stack_id],
                    |_row| Ok(()),
                )
                .optional()?
                .is_some())
        })
        .await
        .context("has unarchived services in stack")
    }

    pub async fn has_unarchived_services(&self, service_ids: &[String]) -> anyhow::Result<bool> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            if service_ids.is_empty() {
                return Ok(false);
            }
            let placeholders = service_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT 1 FROM services WHERE archived = 0 AND id IN ({placeholders}) LIMIT 1"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(service_ids.len());
            for id in &service_ids {
                params.push(id);
            }
            Ok(conn
                .query_row(&sql, params.as_slice(), |_row| Ok(()))
                .optional()?
                .is_some())
        })
        .await
        .context("has unarchived services")
    }

    pub async fn list_stack_ids(&self) -> anyhow::Result<Vec<String>> {
        self.call(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM stacks ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list stack ids")
    }

    pub async fn get_discovered_compose_project(
        &self,
        project: &str,
    ) -> anyhow::Result<Option<DiscoveredComposeProjectRecord>> {
        let project = project.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT stack_id
FROM discovered_compose_projects
WHERE project = ?1
"#,
                    params![project],
                    |row| {
                        Ok(DiscoveredComposeProjectRecord {
                            stack_id: row.get(0)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get discovered compose project")
    }

    pub async fn upsert_discovered_compose_project(
        &self,
        input: DiscoveredComposeProjectUpsert,
    ) -> anyhow::Result<()> {
        self.call(move |conn| {
            let last_config_files_json = input
                .last_config_files
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                r#"
INSERT INTO discovered_compose_projects (
  project,
  stack_id,
  status,
  last_seen_at,
  last_scan_at,
  last_error,
  last_config_files_json,
  archived,
  archived_at,
  archived_reason
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(project) DO UPDATE SET
  stack_id = COALESCE(excluded.stack_id, discovered_compose_projects.stack_id),
  status = excluded.status,
  last_seen_at = COALESCE(excluded.last_seen_at, discovered_compose_projects.last_seen_at),
  last_scan_at = excluded.last_scan_at,
  last_error = excluded.last_error,
  last_config_files_json = excluded.last_config_files_json
"#,
                params![
                    input.project,
                    input.stack_id,
                    input.status,
                    input.last_seen_at,
                    input.last_scan_at,
                    input.last_error,
                    last_config_files_json,
                    0i64,
                    Option::<String>::None,
                    Option::<String>::None
                ],
            )?;

            if input.unarchive_if_active && input.status == "active" {
                tx.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE project = ?1
"#,
                    params![input.project],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert discovered compose project")
    }

    pub async fn mark_discovered_compose_projects_missing_except(
        &self,
        seen_projects: &[String],
        now: &str,
    ) -> anyhow::Result<Vec<String>> {
        let seen_projects = seen_projects.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let newly_missing = if seen_projects.is_empty() {
                let mut stmt = tx.prepare(
                    r#"
	SELECT project
	FROM discovered_compose_projects
	WHERE status != 'missing'
	"#,
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                let newly_missing = rows.collect::<Result<Vec<_>, _>>()?;
                tx.execute(
                    r#"
	UPDATE discovered_compose_projects
	SET status = 'missing', last_scan_at = ?1
	WHERE status != 'missing'
	"#,
                    params![now],
                )?;
                newly_missing
            } else {
                let placeholders = seen_projects.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql_select = format!(
                    "SELECT project FROM discovered_compose_projects WHERE status != 'missing' AND project NOT IN ({placeholders})"
                );
                let mut params: Vec<&dyn rusqlite::ToSql> =
                    Vec::with_capacity(seen_projects.len());
                for p in &seen_projects {
                    params.push(p);
                }
                let mut stmt = tx.prepare(&sql_select)?;
                let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
                let newly_missing = rows.collect::<Result<Vec<_>, _>>()?;

                let sql_update = format!(
                    "UPDATE discovered_compose_projects SET status = 'missing', last_scan_at = ? WHERE status != 'missing' AND project NOT IN ({placeholders})"
                );
                let mut params2: Vec<&dyn rusqlite::ToSql> =
                    Vec::with_capacity(1 + seen_projects.len());
                params2.push(&now);
                for p in &seen_projects {
                    params2.push(p);
                }
                tx.execute(&sql_update, params2.as_slice())?;
                newly_missing
            };

            tx.commit()?;
            Ok(newly_missing)
        })
        .await
        .context("mark discovered compose projects missing")
    }

    pub async fn list_discovered_compose_projects(
        &self,
        archived: ArchivedFilter,
    ) -> anyhow::Result<Vec<crate::api::types::DiscoveredProject>> {
        self.call(move |conn| {
            let filter_clause = archived.where_clause("d.archived");
            let sql = format!(
                r#"
SELECT
  d.project,
  d.status,
  d.stack_id,
  d.last_config_files_json,
  d.last_seen_at,
  d.last_scan_at,
  d.last_error,
  d.archived
FROM discovered_compose_projects d
WHERE 1=1
{filter_clause}
ORDER BY d.project ASC
"#
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |row| {
                let config_files_json: Option<String> = row.get(3)?;
                let config_files = config_files_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());

                Ok(crate::api::types::DiscoveredProject {
                    project: row.get(0)?,
                    status: crate::api::types::DiscoveredProjectStatus::from_str(
                        row.get::<_, String>(1)?.as_str(),
                    ),
                    stack_id: row.get(2)?,
                    config_files,
                    last_seen_at: row.get(4)?,
                    last_scan_at: row.get(5)?,
                    last_error: row.get(6)?,
                    archived: row.get::<_, i64>(7)? != 0,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list discovered compose projects")
    }

    pub async fn set_discovered_compose_project_archived(
        &self,
        project: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let project = project.to_string();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = if archived {
                conn.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 1, archived_at = ?2, archived_reason = ?3
WHERE project = ?1
"#,
                    params![project, now, reason],
                )?
            } else {
                conn.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE project = ?1
"#,
                    params![project],
                )?
            };
            Ok(changed > 0)
        })
        .await
        .context("set discovered compose project archived")
    }

    pub async fn list_ignore_rules_for_service(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Vec<IgnoreRule>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, enabled, scope_type, scope_service_id, match_kind, match_value, note
FROM ignore_rules
WHERE enabled = 1 AND scope_type = 'service' AND scope_service_id = ?1
ORDER BY created_at DESC
"#,
            )?;
            let rows = stmt.query_map(params![service_id], |row| {
                Ok(IgnoreRule {
                    id: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? != 0,
                    scope: IgnoreRuleScope {
                        kind: row.get(2)?,
                        service_id: row.get(3)?,
                    },
                    matcher: IgnoreRuleMatch {
                        kind: row.get(4)?,
                        value: row.get(5)?,
                    },
                    note: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list ignore rules for service")
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_service_check_result(
        &self,
        service_id: &str,
        current_digest: Option<String>,
        current_resolved_tag: Option<String>,
        current_resolved_tags_json: Option<String>,
        candidate_tag: Option<String>,
        candidate_resolved_tag: Option<String>,
        candidate_digest: Option<String>,
        candidate_arch_match: Option<String>,
        candidate_arch_json: Option<String>,
        ignore_rule_id: Option<String>,
        ignore_reason: Option<String>,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE services
SET
  current_digest = ?2,
  current_resolved_tag = ?3,
  current_resolved_tags_json = ?4,
  candidate_tag = ?5,
  candidate_resolved_tag = ?6,
  candidate_digest = ?7,
  candidate_arch_match = ?8,
  candidate_arch_json = ?9,
  ignore_rule_id = ?10,
  ignore_reason = ?11,
  checked_at = ?12,
  updated_at = ?13
WHERE id = ?1
"#,
                params![
                    service_id,
                    current_digest,
                    current_resolved_tag,
                    current_resolved_tags_json,
                    candidate_tag,
                    candidate_resolved_tag,
                    candidate_digest,
                    candidate_arch_match,
                    candidate_arch_json,
                    ignore_rule_id,
                    ignore_reason,
                    checked_at,
                    now,
                ],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("update service check result")
    }

    pub async fn upsert_service_digest_tags_snapshot(
        &self,
        service_id: &str,
        digest: &str,
        snapshot_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        let digest = digest.to_string();
        let snapshot_json = snapshot_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO service_digest_tags_snapshots (
  service_id,
  digest,
  snapshot_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(service_id, digest) DO UPDATE SET
  snapshot_json = excluded.snapshot_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![service_id, digest, snapshot_json, checked_at, now],
            )?;
            Ok(())
        })
        .await
        .context("upsert service digest tags snapshot")
    }

    #[allow(dead_code)]
    pub async fn get_service_digest_tags_snapshot(
        &self,
        service_id: &str,
        digest: &str,
    ) -> anyhow::Result<Option<(String, String, String)>> {
        let service_id = service_id.to_string();
        let digest = digest.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_json, checked_at, updated_at
FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest = ?2
"#,
                    params![service_id, digest],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?)
        })
        .await
        .context("get service digest tags snapshot")
    }

    #[allow(dead_code)]
    pub async fn delete_service_digest_tags_snapshots_except(
        &self,
        service_id: &str,
        allowed_digests: &[String],
    ) -> anyhow::Result<usize> {
        let service_id = service_id.to_string();
        let mut allowed = allowed_digests.to_vec();
        allowed.retain(|d| !d.trim().is_empty());
        allowed.sort();
        allowed.dedup();
        if allowed.len() > 2 {
            // Defensive: the caller is expected to pass at most {current, candidate}.
            allowed.truncate(2);
        }

        self.call(move |conn| {
            let deleted = match allowed.len() {
                0 => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1
"#,
                    params![service_id],
                )?,
                1 => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest != ?2
"#,
                    params![service_id, allowed[0]],
                )?,
                _ => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest NOT IN (?2, ?3)
"#,
                    params![service_id, allowed[0], allowed[1]],
                )?,
            };
            Ok(deleted)
        })
        .await
        .context("delete service digest tags snapshots except")
    }

    pub async fn upsert_image_digest_tags_snapshot(
        &self,
        image_repo: &str,
        digest: &str,
        host_platform: &str,
        snapshot_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        let host_platform = host_platform.to_string();
        let snapshot_json = snapshot_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO image_digest_tags_snapshots (
  image_repo,
  digest,
  host_platform,
  snapshot_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(image_repo, digest, host_platform) DO UPDATE SET
  snapshot_json = excluded.snapshot_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![
                    image_repo,
                    digest,
                    host_platform,
                    snapshot_json,
                    checked_at,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("upsert image digest tags snapshot")
    }

    pub async fn get_image_digest_tags_snapshot(
        &self,
        image_repo: &str,
        digest: &str,
        host_platform: &str,
    ) -> anyhow::Result<Option<(String, String, String)>> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        let host_platform = host_platform.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_json, checked_at, updated_at
FROM image_digest_tags_snapshots
WHERE image_repo = ?1 AND digest = ?2 AND host_platform = ?3
"#,
                    params![image_repo, digest, host_platform],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?)
        })
        .await
        .context("get image digest tags snapshot")
    }

    pub async fn list_image_digest_tags_snapshots(
        &self,
    ) -> anyhow::Result<Vec<ImageDigestTagsSnapshotRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT image_repo, digest, host_platform, snapshot_json, checked_at, updated_at
FROM image_digest_tags_snapshots
ORDER BY updated_at DESC, image_repo ASC, digest ASC, host_platform ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ImageDigestTagsSnapshotRow {
                    image_repo: row.get(0)?,
                    host_platform: row.get(2)?,
                    snapshot_json: row.get(3)?,
                    checked_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list image digest tags snapshots")
    }

    pub async fn delete_expired_image_digest_tags_snapshots(
        &self,
        cutoff_checked_at: &str,
    ) -> anyhow::Result<u64> {
        let cutoff_checked_at = cutoff_checked_at.to_string();
        self.call(move |conn| {
            let deleted = conn.execute(
                r#"
DELETE FROM image_digest_tags_snapshots
WHERE checked_at < ?1
"#,
                params![cutoff_checked_at],
            )?;
            Ok(deleted as u64)
        })
        .await
        .context("delete expired image digest tags snapshots")
    }

    pub async fn list_version_inference_service_targets(
        &self,
    ) -> anyhow::Result<Vec<VersionInferenceServiceTargetRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT image_ref, image_tag, candidate_tag
FROM services
WHERE archived = 0
ORDER BY image_ref ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(VersionInferenceServiceTargetRow {
                    image_ref: row.get(0)?,
                    image_tag: row.get(1)?,
                    candidate_tag: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list version inference service targets")
    }

    pub async fn get_service_settings(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceSettings>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| {
                        let bind_paths_json: String = row.get(1)?;
                        let volume_names_json: String = row.get(2)?;
                        let bind_paths = serde_json::from_str(&bind_paths_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                        let volume_names =
                            serde_json::from_str(&volume_names_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;
                        Ok(ServiceSettings {
                            auto_rollback: row.get::<_, i64>(0)? != 0,
                            backup_targets: crate::api::types::BackupTargetOverrides {
                                bind_paths,
                                volume_names,
                            },
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get service settings")
    }

    pub async fn put_service_settings(
        &self,
        service_id: &str,
        settings: &ServiceSettings,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let settings = settings.clone();
        let now = now.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE services
SET
  auto_rollback = ?2,
  backup_targets_bind_paths_json = ?3,
  backup_targets_volume_names_json = ?4,
  updated_at = ?5
WHERE id = ?1
"#,
                params![
                    service_id,
                    settings.auto_rollback as i64,
                    serde_json::to_string(&settings.backup_targets.bind_paths)?,
                    serde_json::to_string(&settings.backup_targets.volume_names)?,
                    now
                ],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("put service settings")
    }

    pub async fn list_ignore_rules(&self) -> anyhow::Result<Vec<IgnoreRule>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, enabled, scope_type, scope_service_id, match_kind, match_value, note
FROM ignore_rules
ORDER BY created_at DESC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(IgnoreRule {
                    id: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? != 0,
                    scope: IgnoreRuleScope {
                        kind: row.get(2)?,
                        service_id: row.get(3)?,
                    },
                    matcher: IgnoreRuleMatch {
                        kind: row.get(4)?,
                        value: row.get(5)?,
                    },
                    note: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list ignore rules")
    }

    pub async fn insert_ignore_rule(&self, rule: &IgnoreRule, now: &str) -> anyhow::Result<()> {
        let rule = rule.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO ignore_rules (
  id,
  enabled,
  scope_type,
  scope_service_id,
  match_kind,
  match_value,
  note,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
"#,
                params![
                    rule.id,
                    rule.enabled as i64,
                    rule.scope.kind,
                    rule.scope.service_id,
                    rule.matcher.kind,
                    rule.matcher.value,
                    rule.note,
                    now,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("insert ignore rule")
    }

    pub async fn delete_ignore_rule(&self, rule_id: &str) -> anyhow::Result<bool> {
        let rule_id = rule_id.to_string();
        self.call(move |conn| {
            Ok(conn.execute("DELETE FROM ignore_rules WHERE id = ?1", params![rule_id])? > 0)
        })
        .await
        .context("delete ignore rule")
    }

    pub async fn get_notification_settings(&self) -> anyhow::Result<NotificationSettings> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT
  email_enabled,
  email_smtp_url,
  webhook_enabled,
  webhook_url,
  telegram_enabled,
  telegram_bot_token,
  telegram_chat_id,
  webpush_enabled,
  webpush_vapid_public_key,
  webpush_vapid_private_key,
  webpush_vapid_subject,
  event_update_enabled,
  event_new_version_enabled,
  event_ghcr_webhook_anomaly_enabled
FROM notification_settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(NotificationSettings {
                        email_enabled: row.get::<_, i64>(0)? != 0,
                        email_smtp_url: row.get(1)?,
                        webhook_enabled: row.get::<_, i64>(2)? != 0,
                        webhook_url: row.get(3)?,
                        telegram_enabled: row.get::<_, i64>(4)? != 0,
                        telegram_bot_token: row.get(5)?,
                        telegram_chat_id: row.get(6)?,
                        webpush_enabled: row.get::<_, i64>(7)? != 0,
                        webpush_vapid_public_key: row.get(8)?,
                        webpush_vapid_private_key: row.get(9)?,
                        webpush_vapid_subject: row.get(10)?,
                        event_update_enabled: row.get::<_, i64>(11)? != 0,
                        event_new_version_enabled: row.get::<_, i64>(12)? != 0,
                        event_ghcr_webhook_anomaly_enabled: row.get::<_, i64>(13)? != 0,
                    })
                },
            )?)
        })
        .await
        .context("get notification settings")
    }

    pub async fn put_notification_settings(
        &self,
        settings: &NotificationSettings,
        now: &str,
    ) -> anyhow::Result<()> {
        let settings = settings.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE notification_settings
SET
  email_enabled = ?1,
  email_smtp_url = ?2,
  webhook_enabled = ?3,
  webhook_url = ?4,
  telegram_enabled = ?5,
  telegram_bot_token = ?6,
  telegram_chat_id = ?7,
  webpush_enabled = ?8,
  webpush_vapid_public_key = ?9,
  webpush_vapid_private_key = ?10,
  webpush_vapid_subject = ?11,
  event_update_enabled = ?12,
  event_new_version_enabled = ?13,
  event_ghcr_webhook_anomaly_enabled = ?14,
  updated_at = ?15
WHERE id = 'default'
"#,
                params![
                    settings.email_enabled as i64,
                    settings.email_smtp_url,
                    settings.webhook_enabled as i64,
                    settings.webhook_url,
                    settings.telegram_enabled as i64,
                    settings.telegram_bot_token,
                    settings.telegram_chat_id,
                    settings.webpush_enabled as i64,
                    settings.webpush_vapid_public_key,
                    settings.webpush_vapid_private_key,
                    settings.webpush_vapid_subject,
                    settings.event_update_enabled as i64,
                    settings.event_new_version_enabled as i64,
                    settings.event_ghcr_webhook_anomaly_enabled as i64,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put notification settings")
    }

    pub async fn get_github_packages_settings(&self) -> anyhow::Result<GitHubPackagesSettingsDb> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT
  enabled,
  callback_url,
  pat,
  webhook_secret,
  updated_at
FROM github_packages_settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(GitHubPackagesSettingsDb {
                        enabled: row.get::<_, i64>(0)? != 0,
                        callback_url: row.get(1)?,
                        pat: row.get(2)?,
                        webhook_secret: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )?)
        })
        .await
        .context("get github packages settings")
    }

    pub async fn put_github_packages_settings(
        &self,
        settings: &GitHubPackagesSettingsDb,
        now: &str,
    ) -> anyhow::Result<()> {
        let settings = settings.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_settings
SET
  enabled = ?1,
  callback_url = ?2,
  pat = ?3,
  webhook_secret = ?4,
  updated_at = ?5
WHERE id = 'default'
"#,
                params![
                    settings.enabled as i64,
                    settings.callback_url,
                    settings.pat,
                    settings.webhook_secret,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put github packages settings")
    }

    pub async fn list_github_packages_targets(
        &self,
    ) -> anyhow::Result<Vec<GitHubPackagesTargetDb>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
FROM github_packages_targets
ORDER BY owner ASC, input ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                let warnings_json: String = row.get(4)?;
                let warnings: Vec<String> =
                    serde_json::from_str(&warnings_json).unwrap_or_else(|_| Vec::new());
                Ok(GitHubPackagesTargetDb {
                    id: row.get(0)?,
                    input: row.get(1)?,
                    kind: row.get(2)?,
                    owner: row.get(3)?,
                    warnings,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages targets")
    }

    pub async fn put_github_packages_targets(
        &self,
        targets: &[GitHubPackagesTargetDb],
        now: &str,
    ) -> anyhow::Result<()> {
        let targets = targets.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute("DELETE FROM github_packages_targets", [])?;
            for t in targets {
                tx.execute(
                    r#"
INSERT INTO github_packages_targets (
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
                    params![
                        t.id,
                        t.input,
                        t.kind,
                        t.owner,
                        serde_json::to_string(&t.warnings).unwrap_or_else(|_| "[]".to_string()),
                        now
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("put github packages targets")
    }

    pub async fn upsert_github_packages_target_by_input(
        &self,
        input: &str,
        kind: &str,
        owner: &str,
        warnings: &[String],
        now: &str,
    ) -> anyhow::Result<()> {
        let id = ulid::Ulid::new().to_string();
        let input = input.to_string();
        let kind = kind.to_string();
        let owner = owner.to_string();
        let warnings_json = serde_json::to_string(warnings).unwrap_or_else(|_| "[]".to_string());
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "DELETE FROM github_packages_targets WHERE input = ?1",
                params![input],
            )?;
            tx.execute(
                r#"
INSERT INTO github_packages_targets (
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
                params![id, input, kind, owner, warnings_json, now],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert github packages target by input")
    }

    pub async fn delete_github_packages_target_by_input(&self, input: &str) -> anyhow::Result<u32> {
        let input = input.to_string();
        self.call(move |conn| {
            let n = conn.execute(
                "DELETE FROM github_packages_targets WHERE input = ?1",
                params![input],
            )?;
            Ok(n as u32)
        })
        .await
        .context("delete github packages target by input")
    }

    pub async fn list_github_packages_repos(&self) -> anyhow::Result<Vec<GitHubPackagesRepoDb>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
ORDER BY owner ASC, repo ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(GitHubPackagesRepoDb {
                    owner: row.get(0)?,
                    repo: row.get(1)?,
                    selected: row.get::<_, i64>(2)? != 0,
                    webhook_state: row.get(3)?,
                    webhook_job_id: row.get(4)?,
                    hook_id: row.get(5)?,
                    last_sync_at: row.get(6)?,
                    last_audit_at: row.get(7)?,
                    last_op: row.get(8)?,
                    last_error: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos")
    }

    pub async fn upsert_github_packages_repos_default_selected(
        &self,
        repos: &[(String, String)],
        now: &str,
    ) -> anyhow::Result<u32> {
        let repos: Vec<(String, String)> = repos.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut inserted: u32 = 0;
            for (owner, repo) in repos {
                // The DB treats repo keys case-insensitively in several read paths (via `lower()`),
                // but the primary key is case-sensitive. Avoid creating case-variant duplicates by
                // skipping inserts when a case-insensitive match already exists.
                let exists: Option<i64> = tx
                    .query_row(
                        r#"
SELECT 1
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
                        params![&owner, &repo],
                        |row| row.get(0),
                    )
                    .optional()?;
                if exists.is_some() {
                    continue;
                }

                let n = tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, 1, ?3)
ON CONFLICT(owner, repo) DO NOTHING
"#,
                    params![owner, repo, now],
                )?;
                inserted += n as u32;
            }
            tx.commit()?;
            Ok(inserted)
        })
        .await
        .context("upsert github packages repos default selected")
    }

    pub async fn count_github_packages_repos_total(&self) -> anyhow::Result<u32> {
        self.call(|conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM github_packages_repos", [], |row| {
                    row.get::<_, i64>(0).map(|v| v as u32)
                })?,
            )
        })
        .await
        .context("count github packages repos total")
    }

    pub async fn count_github_packages_repos_selected_total(&self) -> anyhow::Result<u32> {
        self.call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM github_packages_repos WHERE selected = 1",
                [],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages repos selected total")
    }

    pub async fn count_github_packages_repos_filtered(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
    ) -> anyhow::Result<u32> {
        let q = q.map(|s| s.to_string());
        self.call(move |conn| {
            let mut sql = "SELECT COUNT(*) FROM github_packages_repos".to_string();
            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            Ok(conn.query_row(&sql, params.as_slice(), |row| {
                row.get::<_, i64>(0).map(|v| v as u32)
            })?)
        })
        .await
        .context("count github packages repos filtered")
    }

    pub async fn list_github_packages_repos_page(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<GitHubPackagesRepoDb>> {
        let q = q.map(|s| s.to_string());
        self.call(move |conn| {
            let mut sql = r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
"#
            .to_string();

            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            sql.push_str(" ORDER BY owner ASC, repo ASC LIMIT ? OFFSET ?");
            values.push(rusqlite::types::Value::from(limit as i64));
            values.push(rusqlite::types::Value::from(offset as i64));

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok(GitHubPackagesRepoDb {
                    owner: row.get(0)?,
                    repo: row.get(1)?,
                    selected: row.get::<_, i64>(2)? != 0,
                    webhook_state: row.get(3)?,
                    webhook_job_id: row.get(4)?,
                    hook_id: row.get(5)?,
                    last_sync_at: row.get(6)?,
                    last_audit_at: row.get(7)?,
                    last_op: row.get(8)?,
                    last_error: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos page")
    }

    pub async fn upsert_github_packages_repo_selected(
        &self,
        owner: &str,
        repo: &str,
        selected: bool,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.trim().to_string();
        let repo = repo.trim().to_string();
        let now = now.to_string();
        self.call(move |conn| {
            // Reads treat owner/repo case-insensitively (via `lower()`), but the primary key is
            // case-sensitive. Prefer updating an existing row that matches case-insensitively to
            // avoid creating case-variant duplicates. If duplicates already exist, keep the "best"
            // row (favoring ones with sync state) and delete the rest.
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let canonical: Option<(String, String)> = tx
                .query_row(
                    r#"
SELECT owner, repo
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
ORDER BY
  (hook_id IS NOT NULL) DESC,
  (last_sync_at IS NOT NULL) DESC,
  updated_at DESC
LIMIT 1
"#,
                    params![&owner, &repo],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if let Some((canon_owner, canon_repo)) = canonical {
                tx.execute(
                    r#"
UPDATE github_packages_repos
SET selected = ?3, updated_at = ?4
WHERE owner = ?1 AND repo = ?2
"#,
                    params![&canon_owner, &canon_repo, selected as i64, &now],
                )?;

                // Remove case-variant duplicates (keep the canonical row above).
                tx.execute(
                    r#"
DELETE FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
  AND NOT (owner = ?3 AND repo = ?4)
"#,
                    params![&owner, &repo, &canon_owner, &canon_repo],
                )?;
            } else {
                tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, ?3, ?4)
"#,
                    params![&owner, &repo, selected as i64, &now],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert github packages repo selected")
    }

    pub async fn get_github_packages_repo_selected(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Option<bool>> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT selected
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
            )?;
            let mut rows = stmt.query(params![owner, repo])?;
            if let Some(row) = rows.next()? {
                let selected = row.get::<_, i64>(0)? != 0;
                Ok(Some(selected))
            } else {
                Ok(None)
            }
        })
        .await
        .context("get github packages repo selected")
    }

    pub async fn get_github_packages_repo(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Option<GitHubPackagesRepoDb>> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
            )?;
            let row = stmt
                .query_row(params![owner, repo], |row| {
                    Ok(GitHubPackagesRepoDb {
                        owner: row.get(0)?,
                        repo: row.get(1)?,
                        selected: row.get::<_, i64>(2)? != 0,
                        webhook_state: row.get(3)?,
                        webhook_job_id: row.get(4)?,
                        hook_id: row.get(5)?,
                        last_sync_at: row.get(6)?,
                        last_audit_at: row.get(7)?,
                        last_op: row.get(8)?,
                        last_error: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
        .context("get github packages repo")
    }

    pub async fn list_github_packages_repos_selected_by_owner(
        &self,
        owner: &str,
    ) -> anyhow::Result<Vec<(String, bool)>> {
        let owner = owner.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT repo, selected
FROM github_packages_repos
WHERE lower(owner) = lower(?1)
ORDER BY repo ASC
"#,
            )?;
            let rows = stmt.query_map(params![owner], |row| {
                let repo: String = row.get(0)?;
                let selected = row.get::<_, i64>(1)? != 0;
                Ok((repo, selected))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos selected by owner")
    }

    pub async fn delete_github_packages_repo(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<bool> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        self.call(move |conn| {
            let n = conn.execute(
                r#"
DELETE FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![owner, repo],
            )?;
            Ok(n > 0)
        })
        .await
        .context("delete github packages repo")
    }

    pub async fn bulk_set_github_packages_repos_selected(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
        selected: bool,
        now: &str,
    ) -> anyhow::Result<u32> {
        let q = q.map(|s| s.to_string());
        let now = now.to_string();
        self.call(move |conn| {
            let mut sql =
                "UPDATE github_packages_repos SET selected = ?, updated_at = ?".to_string();
            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            values.push(rusqlite::types::Value::from(selected as i64));
            values.push(rusqlite::types::Value::from(now));

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            let n = conn.execute(&sql, params.as_slice())?;
            Ok(n as u32)
        })
        .await
        .context("bulk set github packages repos selected")
    }

    pub async fn put_github_packages_repos(
        &self,
        repos: &[(String, String, bool)],
        now: &str,
    ) -> anyhow::Result<()> {
        let repos: Vec<(String, String, bool)> = repos.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            // See `upsert_github_packages_repo_selected`: avoid creating case-variant duplicates by
            // reusing the canonical casing of any existing row that matches case-insensitively.
            let mut canonical: Vec<(String, String, bool)> = Vec::with_capacity(repos.len());
            for (owner, repo, selected) in &repos {
                let owner = owner.trim();
                let repo = repo.trim();
                if owner.is_empty() || repo.is_empty() {
                    continue;
                }

                let existing: Option<(String, String)> = tx
                    .query_row(
                        r#"
SELECT owner, repo
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
ORDER BY
  (hook_id IS NOT NULL) DESC,
  (last_sync_at IS NOT NULL) DESC,
  updated_at DESC
LIMIT 1
"#,
                        params![owner, repo],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                let (owner, repo) = existing.unwrap_or((owner.to_string(), repo.to_string()));
                canonical.push((owner, repo, *selected));
            }

            for (owner, repo, selected) in &canonical {
                tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(owner, repo) DO UPDATE SET
  selected = excluded.selected,
  updated_at = excluded.updated_at
"#,
                    params![owner, repo, *selected as i64, now],
                )?;
            }

            if canonical.is_empty() {
                tx.execute("DELETE FROM github_packages_repos", [])?;
            } else {
                // Avoid hitting SQLite's SQL-variable limit (commonly 999) by using a temp table
                // instead of `NOT IN (?, ?, ...)` with one placeholder per repo.
                tx.execute(
                    "CREATE TEMP TABLE IF NOT EXISTS tmp_github_packages_keep (full_name TEXT PRIMARY KEY)",
                    [],
                )?;
                tx.execute("DELETE FROM tmp_github_packages_keep", [])?;
                for (owner, repo, _) in &canonical {
                    let full_name = format!("{owner}/{repo}");
                    tx.execute(
                        "INSERT OR IGNORE INTO tmp_github_packages_keep (full_name) VALUES (?1)",
                        params![full_name],
                    )?;
                }
                tx.execute(
                    "DELETE FROM github_packages_repos WHERE (owner || '/' || repo) NOT IN (SELECT full_name FROM tmp_github_packages_keep)",
                    [],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("put github packages repos")
    }

    #[cfg(test)]
    pub async fn set_github_packages_repo_sync_result(
        &self,
        owner: &str,
        repo: &str,
        hook_id: Option<i64>,
        last_sync_at: Option<&str>,
        last_error: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let last_sync_at = last_sync_at.map(|s| s.to_string());
        let last_error = last_error.map(|s| s.to_string());
        let webhook_state = if last_error.is_some() {
            "error".to_string()
        } else if hook_id.is_some() {
            "ok".to_string()
        } else {
            "unknown".to_string()
        };
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  last_op = 'register',
  webhook_job_id = NULL,
  hook_id = ?4,
  last_sync_at = ?5,
  last_error = ?6,
  updated_at = ?7
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![
                    owner,
                    repo,
                    webhook_state,
                    hook_id,
                    last_sync_at,
                    last_error,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo sync result")
    }

    pub async fn set_github_packages_repo_webhook_job_state(
        &self,
        owner: &str,
        repo: &str,
        webhook_state: &str,
        webhook_job_id: Option<&str>,
        last_op: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let webhook_state = webhook_state.to_string();
        let webhook_job_id = webhook_job_id.map(|s| s.to_string());
        let last_op = last_op.map(|s| s.to_string());
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  webhook_job_id = ?4,
  last_op = ?5,
  updated_at = ?6
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![owner, repo, webhook_state, webhook_job_id, last_op, now],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo webhook job state")
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn set_github_packages_repo_webhook_result(
        &self,
        owner: &str,
        repo: &str,
        webhook_state: &str,
        hook_id: Option<i64>,
        last_sync_at: Option<&str>,
        last_audit_at: Option<&str>,
        last_error: Option<&str>,
        webhook_job_id: Option<&str>,
        last_op: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let webhook_state = webhook_state.to_string();
        let last_sync_at = last_sync_at.map(|s| s.to_string());
        let last_audit_at = last_audit_at.map(|s| s.to_string());
        let last_error = last_error.map(|s| s.to_string());
        let webhook_job_id = webhook_job_id.map(|s| s.to_string());
        let last_op = last_op.map(|s| s.to_string());
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  hook_id = ?4,
  last_sync_at = ?5,
  last_audit_at = ?6,
  last_error = ?7,
  webhook_job_id = ?8,
  last_op = ?9,
  updated_at = ?10
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![
                    owner,
                    repo,
                    webhook_state,
                    hook_id,
                    last_sync_at,
                    last_audit_at,
                    last_error,
                    webhook_job_id,
                    last_op,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo webhook result")
    }

    pub async fn list_github_packages_repos_for_job_state_summary(
        &self,
    ) -> anyhow::Result<Vec<(String, Option<String>)>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT webhook_state, last_audit_at
FROM github_packages_repos
WHERE selected = 1
"#,
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos state summary")
    }

    pub async fn count_github_packages_deliveries_total(&self) -> anyhow::Result<u32> {
        self.call(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM github_packages_deliveries",
                [],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages deliveries total")
    }

    pub async fn summarize_github_packages_deliveries(
        &self,
    ) -> anyhow::Result<GitHubPackagesWebhookDeliverySummary> {
        self.call(move |conn| {
            Ok(conn.query_row(
                r#"
SELECT
  SUM(CASE WHEN decision = 'processed' THEN 1 ELSE 0 END),
  SUM(CASE WHEN decision = 'ignored' THEN 1 ELSE 0 END),
  SUM(CASE WHEN decision = 'rejected' THEN 1 ELSE 0 END)
FROM github_packages_deliveries
"#,
                [],
                |row| {
                    Ok(GitHubPackagesWebhookDeliverySummary {
                        processed: row.get::<_, Option<i64>>(0)?.unwrap_or(0) as u32,
                        ignored: row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u32,
                        rejected: row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u32,
                    })
                },
            )?)
        })
        .await
        .context("summarize github packages deliveries")
    }

    pub async fn count_github_packages_deliveries_filtered(
        &self,
        decision: Option<&str>,
        q: Option<&str>,
    ) -> anyhow::Result<u32> {
        let decision = decision.map(|s| s.to_string());
        let q_like = q.map(|s| format!("%{}%", s.trim().to_ascii_lowercase()));
        self.call(move |conn| {
            Ok(conn.query_row(
                r#"
SELECT COUNT(*)
FROM github_packages_deliveries
WHERE (?1 IS NULL OR decision = ?1)
  AND (
    ?2 IS NULL
    OR lower(delivery_id) LIKE ?2
    OR lower(COALESCE(owner, '')) LIKE ?2
    OR lower(COALESCE(repo, '')) LIKE ?2
    OR lower(COALESCE(event, '')) LIKE ?2
    OR lower(COALESCE(action, '')) LIKE ?2
    OR lower(COALESCE(reason, '')) LIKE ?2
    OR lower(COALESCE(job_id, '')) LIKE ?2
    OR lower(COALESCE(job_ids_json, '')) LIKE ?2
  )
"#,
                params![decision, q_like],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages deliveries filtered")
    }

    pub async fn list_github_packages_deliveries_page(
        &self,
        decision: Option<&str>,
        q: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<GitHubPackagesWebhookDeliveryDb>> {
        let decision = decision.map(|s| s.to_string());
        let q_like = q.map(|s| format!("%{}%", s.trim().to_ascii_lowercase()));
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  reason,
  response_status,
  job_id,
  job_ids_json,
  attempt_count
FROM github_packages_deliveries
WHERE (?1 IS NULL OR decision = ?1)
  AND (
    ?2 IS NULL
    OR lower(delivery_id) LIKE ?2
    OR lower(COALESCE(owner, '')) LIKE ?2
    OR lower(COALESCE(repo, '')) LIKE ?2
    OR lower(COALESCE(event, '')) LIKE ?2
    OR lower(COALESCE(action, '')) LIKE ?2
    OR lower(COALESCE(reason, '')) LIKE ?2
    OR lower(COALESCE(job_id, '')) LIKE ?2
    OR lower(COALESCE(job_ids_json, '')) LIKE ?2
  )
ORDER BY received_at DESC, delivery_id DESC
LIMIT ?3 OFFSET ?4
"#,
            )?;
            let rows = stmt.query_map(params![decision, q_like, limit, offset], |row| {
                let job_id: Option<String> = row.get(10)?;
                let job_ids_json: Option<String> = row.get(11)?;
                Ok(GitHubPackagesWebhookDeliveryDb {
                    delivery_id: row.get(0)?,
                    received_at: row.get(1)?,
                    first_received_at: row.get(2)?,
                    owner: row.get(3)?,
                    repo: row.get(4)?,
                    event: row.get(5)?,
                    action: row.get(6)?,
                    decision: row.get(7)?,
                    reason: row.get(8)?,
                    response_status: row
                        .get::<_, Option<i64>>(9)?
                        .and_then(|value| u16::try_from(value).ok()),
                    job_ids: parse_github_packages_delivery_job_ids(
                        job_id.as_deref(),
                        job_ids_json.as_deref(),
                    ),
                    job_id,
                    attempt_count: row.get::<_, i64>(12)?.max(1) as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages deliveries page")
    }

    pub async fn record_github_packages_delivery(
        &self,
        input: GitHubPackagesWebhookDeliveryRecordInput,
    ) -> anyhow::Result<u32> {
        self.call(move |conn| {
            let delivery_id = input.delivery_id.clone();
            let job_ids_json = serde_json::to_string(&input.job_ids)?;
            conn.execute(
                r#"
INSERT INTO github_packages_deliveries (
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  reason,
  response_status,
  job_id,
  job_ids_json,
  attempt_count
)
VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1)
ON CONFLICT(delivery_id) DO UPDATE SET
  received_at = excluded.received_at,
  owner = COALESCE(excluded.owner, github_packages_deliveries.owner),
  repo = COALESCE(excluded.repo, github_packages_deliveries.repo),
  event = COALESCE(excluded.event, github_packages_deliveries.event),
  action = COALESCE(excluded.action, github_packages_deliveries.action),
  decision = excluded.decision,
  reason = excluded.reason,
  response_status = excluded.response_status,
  job_id = COALESCE(excluded.job_id, github_packages_deliveries.job_id),
  job_ids_json = COALESCE(excluded.job_ids_json, github_packages_deliveries.job_ids_json),
  attempt_count = github_packages_deliveries.attempt_count + 1
"#,
                params![
                    delivery_id,
                    input.received_at,
                    input.owner,
                    input.repo,
                    input.event,
                    input.action,
                    input.decision,
                    input.reason,
                    input.response_status.map(i64::from),
                    input.job_id,
                    job_ids_json,
                ],
            )?;
            conn.query_row(
                "SELECT attempt_count FROM github_packages_deliveries WHERE delivery_id = ?1",
                params![input.delivery_id],
                |row| row.get::<_, i64>(0).map(|value| value.max(1) as u32),
            )
            .context("load github packages delivery attempt count")
        })
        .await
        .context("record github packages delivery")
    }

    pub async fn increment_github_packages_delivery_attempt(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
        event: Option<&str>,
        action: Option<&str>,
    ) -> anyhow::Result<u32> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        let event = event.map(|s| s.to_string());
        let action = action.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE github_packages_deliveries
SET
  received_at = ?2,
  owner = COALESCE(?3, owner),
  repo = COALESCE(?4, repo),
  event = COALESCE(?5, event),
  action = COALESCE(?6, action),
  attempt_count = attempt_count + 1
WHERE delivery_id = ?1
"#,
                params![&delivery_id, &received_at, owner, repo, event, action],
            )?;
            if changed == 0 {
                return Err(anyhow::anyhow!(
                    "github packages delivery not found for duplicate attempt"
                ));
            }
            conn.query_row(
                "SELECT attempt_count FROM github_packages_deliveries WHERE delivery_id = ?1",
                params![delivery_id],
                |row| row.get::<_, i64>(0).map(|value| value.max(1) as u32),
            )
            .context("load github packages delivery attempt count")
        })
        .await
        .context("increment github packages delivery attempt")
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_github_packages_delivery_outcome(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
        event: Option<&str>,
        action: Option<&str>,
        decision: &str,
        reason: Option<&str>,
        response_status: Option<u16>,
        job_id: Option<&str>,
        job_ids: &[String],
    ) -> anyhow::Result<()> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        let event = event.map(|s| s.to_string());
        let action = action.map(|s| s.to_string());
        let decision = decision.to_string();
        let reason = reason.map(|s| s.to_string());
        let response_status = response_status.map(i64::from);
        let job_id = job_id.map(|s| s.to_string());
        let job_ids = if job_ids.is_empty() {
            None
        } else {
            Some(serde_json::to_string(job_ids)?)
        };
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_deliveries
SET
  received_at = ?2,
  owner = COALESCE(?3, owner),
  repo = COALESCE(?4, repo),
  event = COALESCE(?5, event),
  action = COALESCE(?6, action),
  decision = ?7,
  reason = ?8,
  response_status = ?9,
  job_id = COALESCE(?10, job_id),
  job_ids_json = COALESCE(?11, job_ids_json)
WHERE delivery_id = ?1
"#,
                params![
                    delivery_id,
                    received_at,
                    owner,
                    repo,
                    event,
                    action,
                    decision,
                    reason,
                    response_status,
                    job_id,
                    job_ids,
                ],
            )?;
            Ok(())
        })
        .await
        .context("update github packages delivery outcome")
    }

    pub async fn insert_github_packages_delivery_if_new(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
    ) -> anyhow::Result<bool> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
INSERT OR IGNORE INTO github_packages_deliveries (
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  response_status,
  attempt_count
)
VALUES (?1, ?2, ?2, ?3, ?4, 'package', 'published', 'processed', 200, 1)
"#,
                params![delivery_id, received_at, owner, repo],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("insert github packages delivery")
    }

    pub async fn github_packages_delivery_exists(&self, delivery_id: &str) -> anyhow::Result<bool> {
        let delivery_id = delivery_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT 1 FROM github_packages_deliveries WHERE delivery_id = ?1 LIMIT 1",
            )?;
            let mut rows = stmt.query(params![delivery_id])?;
            Ok(rows.next()?.is_some())
        })
        .await
        .context("check github packages delivery exists")
    }

    pub async fn upsert_web_push_subscription(
        &self,
        endpoint: &str,
        p256dh: &str,
        auth: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let endpoint = endpoint.to_string();
        let p256dh = p256dh.to_string();
        let auth = auth.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO web_push_subscriptions (endpoint, p256dh, auth, created_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(endpoint) DO UPDATE SET
  p256dh = excluded.p256dh,
  auth = excluded.auth
"#,
                params![endpoint, p256dh, auth, now],
            )?;
            Ok(())
        })
        .await
        .context("upsert web push subscription")
    }

    pub async fn delete_web_push_subscription(&self, endpoint: &str) -> anyhow::Result<bool> {
        let endpoint = endpoint.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                "DELETE FROM web_push_subscriptions WHERE endpoint = ?1",
                params![endpoint],
            )? > 0)
        })
        .await
        .context("delete web push subscription")
    }

    pub async fn list_web_push_subscriptions(
        &self,
    ) -> anyhow::Result<Vec<(String, String, String)>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT endpoint, p256dh, auth
FROM web_push_subscriptions
ORDER BY created_at ASC
LIMIT 500
"#,
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list web push subscriptions")
    }

    pub async fn get_instance_public_base_url(&self) -> anyhow::Result<Option<String>> {
        self.call(|conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT public_base_url
FROM settings
WHERE id = 'default'
"#,
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten())
        })
        .await
        .context("get instance public base url")
    }

    #[allow(dead_code)]
    pub async fn put_instance_public_base_url(
        &self,
        public_base_url: Option<String>,
        now: &str,
    ) -> anyhow::Result<()> {
        let public_base_url = public_base_url.map(|v| v.trim().to_string());
        let public_base_url =
            public_base_url.and_then(|v| if v.is_empty() { None } else { Some(v) });
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE settings
SET
  public_base_url = ?1,
  updated_at = ?2
WHERE id = 'default'
"#,
                params![public_base_url, now],
            )?;
            Ok(())
        })
        .await
        .context("put instance public base url")
    }

    pub async fn get_backup_settings(&self) -> anyhow::Result<BackupSettings> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT backup_enabled, backup_require_success, backup_base_dir, backup_skip_targets_over_bytes
FROM settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(BackupSettings {
                        enabled: row.get::<_, i64>(0)? != 0,
                        require_success: row.get::<_, i64>(1)? != 0,
                        base_dir: row.get(2)?,
                        skip_targets_over_bytes: row.get::<_, i64>(3)? as u64,
                    })
                },
            )?)
        })
        .await
        .context("get backup settings")
    }

    pub async fn get_resource_monitor_settings(&self) -> anyhow::Result<ResourceMonitorSettings> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT resource_monitor_enabled, resource_sample_interval_seconds
FROM settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    let raw_interval = row.get::<_, i64>(1)? as u64;
                    Ok(ResourceMonitorSettings {
                        enabled: row.get::<_, i64>(0)? != 0,
                        sample_interval_seconds:
                            crate::resource_usage::normalize_sample_interval_seconds(raw_interval),
                        retention_days: 30,
                    })
                },
            )?)
        })
        .await
        .context("get resource monitor settings")
    }

    pub async fn get_schedule_settings(&self) -> anyhow::Result<SchedulesSettings> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT
  schedule_update_check_enabled,
  schedule_update_check_cron,
  schedule_ghcr_webhook_audit_enabled,
  schedule_ghcr_webhook_audit_cron
FROM settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(SchedulesSettings {
                        update_check: ScheduleItemSettings {
                            enabled: row.get::<_, i64>(0)? != 0,
                            cron: row.get(1)?,
                        },
                        ghcr_webhook_audit: ScheduleItemSettings {
                            enabled: row.get::<_, i64>(2)? != 0,
                            cron: row.get(3)?,
                        },
                    })
                },
            )?)
        })
        .await
        .context("get schedule settings")
    }

    pub async fn get_deploy_welcome_settings(&self) -> anyhow::Result<DeployWelcomeSettings> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT deploy_welcome_never_auto_open, deploy_welcome_updated_at
FROM settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(DeployWelcomeSettings {
                        never_auto_open: row.get::<_, i64>(0)? != 0,
                        updated_at: row.get(1)?,
                    })
                },
            )?)
        })
        .await
        .context("get deploy welcome settings")
    }

    pub async fn put_deploy_welcome_settings(
        &self,
        never_auto_open: bool,
        now: &str,
    ) -> anyhow::Result<()> {
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE settings
SET
  deploy_welcome_never_auto_open = ?1,
  deploy_welcome_updated_at = ?2,
  updated_at = ?2
WHERE id = 'default'
"#,
                params![never_auto_open as i64, now],
            )?;
            Ok(())
        })
        .await
        .context("put deploy welcome settings")
    }

    pub async fn put_settings(
        &self,
        backup: &BackupSettings,
        resource_monitor: &ResourceMonitorSettings,
        schedules: &SchedulesSettings,
        public_base_url: Option<String>,
        now: &str,
    ) -> anyhow::Result<()> {
        let backup = backup.clone();
        let resource_monitor = resource_monitor.clone();
        let schedules = schedules.clone();
        let public_base_url = public_base_url.map(|v| v.trim().to_string());
        let public_base_url =
            public_base_url.and_then(|v| if v.is_empty() { None } else { Some(v) });
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE settings
SET
  backup_enabled = ?1,
  backup_require_success = ?2,
  backup_base_dir = ?3,
  backup_skip_targets_over_bytes = ?4,
  resource_monitor_enabled = ?5,
  resource_sample_interval_seconds = ?6,
  schedule_update_check_enabled = ?7,
  schedule_update_check_cron = ?8,
  schedule_ghcr_webhook_audit_enabled = ?9,
  schedule_ghcr_webhook_audit_cron = ?10,
  public_base_url = ?11,
  updated_at = ?12
WHERE id = 'default'
"#,
                params![
                    backup.enabled as i64,
                    backup.require_success as i64,
                    backup.base_dir,
                    backup.skip_targets_over_bytes as i64,
                    resource_monitor.enabled as i64,
                    resource_monitor.sample_interval_seconds as i64,
                    schedules.update_check.enabled as i64,
                    schedules.update_check.cron,
                    schedules.ghcr_webhook_audit.enabled as i64,
                    schedules.ghcr_webhook_audit.cron,
                    public_base_url,
                    now,
                ],
            )?;
            Ok(())
        })
        .await
        .context("put settings")
    }

    pub async fn list_service_resource_targets(
        &self,
    ) -> anyhow::Result<Vec<ServiceResourceTarget>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sv.id,
  sv.name,
  (
    SELECT d.project
    FROM discovered_compose_projects d
    WHERE
      d.stack_id = sv.stack_id
      AND d.archived = 0
      AND d.status != 'missing'
    ORDER BY d.last_scan_at DESC
    LIMIT 1
  ) AS compose_project
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY sv.stack_id ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                let compose_project: Option<String> = row.get(2)?;
                let service_id: String = row.get(0)?;
                let service_name: String = row.get(1)?;
                Ok(compose_project.map(|project| ServiceResourceTarget {
                    service_id,
                    service_name,
                    compose_project: project,
                }))
            })?;
            let mut out = Vec::new();
            for row in rows {
                if let Some(item) = row? {
                    out.push(item);
                }
            }
            Ok(out)
        })
        .await
        .context("list service resource targets")
    }

    pub async fn list_active_github_webhook_service_targets(
        &self,
    ) -> anyhow::Result<Vec<GithubWebhookServiceTarget>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  st.id,
  sv.id,
  sv.image_ref
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(GithubWebhookServiceTarget {
                    stack_id: row.get(0)?,
                    service_id: row.get(1)?,
                    image_ref: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list active github webhook service targets")
    }

    pub async fn get_service_resource_target(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceResourceTarget>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  sv.id,
  sv.name,
  (
    SELECT d.project
    FROM discovered_compose_projects d
    WHERE
      d.stack_id = sv.stack_id
      AND d.archived = 0
      AND d.status != 'missing'
    ORDER BY d.last_scan_at DESC
    LIMIT 1
  ) AS compose_project
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE sv.id = ?1 AND st.archived = 0 AND sv.archived = 0
"#,
                    params![service_id],
                    |row| {
                        let compose_project: Option<String> = row.get(2)?;
                        let service_id: String = row.get(0)?;
                        let service_name: String = row.get(1)?;
                        Ok(compose_project.map(|project| ServiceResourceTarget {
                            service_id,
                            service_name,
                            compose_project: project,
                        }))
                    },
                )
                .optional()?
                .flatten())
        })
        .await
        .context("get service resource target")
    }

    pub async fn insert_service_resource_samples(
        &self,
        rows: &[ServiceResourceSampleInput],
    ) -> anyhow::Result<usize> {
        let rows = rows.to_vec();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut inserted = 0usize;
            for row in rows {
                tx.execute(
                    r#"
INSERT INTO service_resource_samples (
  service_id,
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
"#,
                    params![
                        row.service_id,
                        row.sampled_at,
                        row.cpu_percent,
                        row.mem_used_bytes.map(|v| v as i64),
                        row.mem_limit_bytes.map(|v| v as i64),
                        row.net_rx_bytes.map(|v| v as i64),
                        row.net_tx_bytes.map(|v| v as i64),
                        row.block_read_bytes.map(|v| v as i64),
                        row.block_write_bytes.map(|v| v as i64),
                        row.pids.map(|v| v as i64),
                        row.container_count as i64,
                    ],
                )?;
                inserted = inserted.saturating_add(1);
            }
            tx.commit()?;
            Ok(inserted)
        })
        .await
        .context("insert service resource samples")
    }

    pub async fn list_service_resource_samples_since(
        &self,
        service_id: &str,
        since: &str,
    ) -> anyhow::Result<Vec<ServiceResourceSample>> {
        let service_id = service_id.to_string();
        let since = since.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count
FROM service_resource_samples
WHERE service_id = ?1 AND sampled_at >= ?2
ORDER BY sampled_at ASC
"#,
            )?;
            let rows = stmt.query_map(params![service_id, since], |row| {
                Ok(ServiceResourceSample {
                    sampled_at: row.get(0)?,
                    cpu_percent: row.get(1)?,
                    mem_used_bytes: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                    mem_limit_bytes: row.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                    net_rx_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    net_tx_bytes: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                    block_read_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                    block_write_bytes: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                    pids: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                    container_count: row.get::<_, i64>(9)? as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service resource samples since")
    }

    pub async fn delete_expired_service_resource_samples(
        &self,
        older_than: &str,
    ) -> anyhow::Result<u64> {
        let older_than = older_than.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
DELETE FROM service_resource_samples
WHERE sampled_at < ?1
"#,
                params![older_than],
            )? as u64)
        })
        .await
        .context("delete expired service resource samples")
    }

    pub async fn insert_job(&self, job: JobListItem) -> anyhow::Result<()> {
        self.call(move |conn| {
            conn.execute(
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
                    job.id,
                    job.r#type.as_str(),
                    job.scope.as_str(),
                    job.stack_id,
                    job.service_id,
                    job.status,
                    job.allow_arch_mismatch as i64,
                    job.backup_mode,
                    job.created_by,
                    job.reason,
                    job.created_at,
                    job.started_at,
                    job.finished_at,
                    serde_json::to_string(&job.summary_json)?
                ],
            )?;
            Ok(())
        })
        .await
        .context("insert job")
    }

    pub async fn claim_next_queued_job_by_type(
        &self,
        job_type: JobType,
        started_at: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let started_at = started_at.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let item: Option<JobListItem> = tx
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1 AND status = 'queued'
ORDER BY created_at ASC, id ASC
LIMIT 1
"#,
                    params![job_type],
                    |row| {
                        let summary_json: String = row.get(13)?;
                        let summary: serde_json::Value = serde_json::from_str(&summary_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
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
                    },
                )
                .optional()?;

            let Some(mut item) = item else {
                tx.commit()?;
                return Ok(None);
            };

            let changed = tx.execute(
                r#"
UPDATE jobs
SET status = 'running', started_at = ?2
WHERE id = ?1 AND status = 'queued'
"#,
                params![item.id, started_at],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }

            item.status = "running".to_string();
            item.started_at = Some(started_at);
            tx.commit()?;
            Ok(Some(item))
        })
        .await
        .context("claim next queued job by type")
    }

    pub async fn finish_job(
        &self,
        job_id: &str,
        status: &str,
        finished_at: &str,
        summary_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let status = status.to_string();
        let finished_at = finished_at.to_string();
        let mut summary_json = summary_json.clone();
        self.call(move |conn| {
            let previous_summary_raw = conn
                .query_row(
                    r#"
SELECT summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            if !summary_json.is_object() {
                summary_json = serde_json::json!({ "result": summary_json });
            }

            if let Some(previous_summary_raw) = previous_summary_raw {
                let previous_summary: serde_json::Value =
                    serde_json::from_str(&previous_summary_raw)
                        .unwrap_or_else(|_| serde_json::json!({}));
                if let Some(previous) = previous_summary.as_object()
                    && let Some(obj) = summary_json.as_object_mut()
                {
                    for (key, value) in previous {
                        obj.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }

            let summary_json = serde_json::to_string(&summary_json)?;
            conn.execute(
                r#"
UPDATE jobs
SET status = ?2, finished_at = ?3, summary_json = ?4
WHERE id = ?1
"#,
                params![job_id, status, finished_at, summary_json],
            )?;
            Ok(())
        })
        .await
        .context("finish job")
    }

    pub async fn merge_job_summary_fields(
        &self,
        job_id: &str,
        fields: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let fields = fields.clone();
        self.call(move |conn| {
            let summary_raw = conn
                .query_row(
                    r#"
SELECT summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let Some(summary_raw) = summary_raw else {
                return Ok(());
            };

            let mut summary: serde_json::Value = serde_json::from_str(&summary_raw)
                .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
            if !summary.is_object() {
                summary = serde_json::Value::Object(Default::default());
            }

            if let Some(summary_obj) = summary.as_object_mut()
                && let Some(fields_obj) = fields.as_object()
            {
                for (key, value) in fields_obj {
                    summary_obj.insert(key.clone(), value.clone());
                }
            }

            conn.execute(
                r#"
UPDATE jobs
SET summary_json = ?2
WHERE id = ?1
"#,
                params![&job_id, serde_json::to_string(&summary)?],
            )?;
            Ok(())
        })
        .await
        .context("merge job summary fields")
    }

    pub async fn set_job_progress(
        &self,
        job_id: &str,
        progress: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let progress = progress.clone();
        self.call(move |conn| {
            let summary_raw = conn
                .query_row(
                    r#"
SELECT summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let Some(summary_raw) = summary_raw else {
                return Ok(());
            };

            let mut summary: serde_json::Value = serde_json::from_str(&summary_raw)
                .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
            if !summary.is_object() {
                summary = serde_json::Value::Object(Default::default());
            }

            if let Some(obj) = summary.as_object_mut() {
                obj.insert("progress".to_string(), progress);
            }

            conn.execute(
                r#"
UPDATE jobs
SET summary_json = ?2
WHERE id = ?1
"#,
                params![&job_id, serde_json::to_string(&summary)?],
            )?;
            Ok(())
        })
        .await
        .context("set job progress")
    }

    pub async fn list_jobs(&self) -> anyhow::Result<Vec<JobListItem>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
ORDER BY created_at DESC
LIMIT 200
"#,
            )?;

            let rows = stmt.query_map([], |row| {
                let summary_json: String = row.get(13)?;
                let summary: serde_json::Value =
                    serde_json::from_str(&summary_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
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
            })?;

            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list jobs")
    }

    pub async fn list_jobs_by_type_and_statuses(
        &self,
        job_type: JobType,
        statuses: &[&str],
        limit: u32,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let statuses: Vec<String> = statuses.iter().map(|s| (*s).to_string()).collect();
        let limit = limit.max(1);
        self.call(move |conn| {
            if statuses.is_empty() {
                return Ok(Vec::new());
            }

            let placeholders = std::iter::repeat_n("?", statuses.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ? AND status IN ({placeholders})
ORDER BY created_at DESC
LIMIT ?
"#
            );

            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            values.push(rusqlite::types::Value::from(job_type));
            for status in &statuses {
                values.push(rusqlite::types::Value::from(status.clone()));
            }
            values.push(rusqlite::types::Value::from(limit as i64));
            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                let summary_json: String = row.get(13)?;
                let summary: serde_json::Value =
                    serde_json::from_str(&summary_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
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
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list jobs by type and statuses")
    }

    pub async fn count_jobs_by_type_and_status(
        &self,
        job_type: JobType,
        status: &str,
    ) -> anyhow::Result<u32> {
        let job_type = job_type.as_str().to_string();
        let status = status.to_string();
        self.call(move |conn| {
            let count = conn.query_row(
                r#"
SELECT COUNT(*)
FROM jobs
WHERE type = ?1 AND status = ?2
"#,
                params![job_type, status],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(count as u32)
        })
        .await
        .context("count jobs by type and status")
    }

    pub async fn has_pending_job_by_type_created_by_reason(
        &self,
        job_type: JobType,
        created_by: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let job_type = job_type.as_str().to_string();
        let created_by = created_by.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let row: Option<i64> = conn
                .query_row(
                    r#"
SELECT 1
FROM jobs
WHERE type = ?1
  AND status IN ('queued', 'running')
  AND created_by = ?2
  AND reason = ?3
LIMIT 1
"#,
                    params![job_type, created_by, reason],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(row.is_some())
        })
        .await
        .context("check pending job by type/created_by/reason")
    }

    pub async fn find_latest_pending_job_by_type(
        &self,
        job_type: JobType,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                params![job_type],
                |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
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
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending job by type")
    }

    pub async fn find_latest_pending_job_by_type_and_service_id(
        &self,
        job_type: JobType,
        service_id: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1
  AND service_id = ?2
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                params![job_type, service_id],
                |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
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
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending job by type and service id")
    }

    pub async fn find_latest_running_check_job(
        &self,
        scope: &JobScope,
        stack_id: Option<&str>,
        service_id: Option<&str>,
    ) -> anyhow::Result<Option<JobListItem>> {
        let scope = scope.as_str().to_string();
        let stack_id = stack_id.map(|s| s.to_string());
        let service_id = service_id.map(|s| s.to_string());
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'check'
  AND status = 'running'
  AND scope = ?1
  AND (?2 IS NULL OR stack_id = ?2)
  AND (?3 IS NULL OR service_id = ?3)
ORDER BY created_at DESC
LIMIT 1
"#,
            )?;

            let row = stmt
                .query_row(params![scope, stack_id, service_id], |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
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
                })
                .optional()?;

            Ok(row)
        })
        .await
        .context("find latest running check job")
    }

    pub async fn find_latest_running_runtime_scan_job(
        &self,
        scope: &JobScope,
        stack_id: Option<&str>,
        service_id: Option<&str>,
    ) -> anyhow::Result<Option<JobListItem>> {
        let scope = scope.as_str().to_string();
        let stack_id = stack_id.map(|s| s.to_string());
        let service_id = service_id.map(|s| s.to_string());
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'runtime_scan'
  AND status = 'running'
  AND scope = ?1
  AND (?2 IS NULL OR stack_id = ?2)
  AND (?3 IS NULL OR service_id = ?3)
ORDER BY created_at DESC
LIMIT 1
"#,
            )?;

            let row = stmt
                .query_row(params![scope, stack_id, service_id], |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
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
                })
                .optional()?;

            Ok(row)
        })
        .await
        .context("find latest running runtime scan job")
    }

    pub async fn recover_incomplete_jobs(
        &self,
        now: &str,
        reason: &str,
    ) -> anyhow::Result<Vec<String>> {
        let now = now.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let items: Vec<(String, String, String)> = {
                let mut stmt = conn.prepare(
                    r#"
SELECT id, status, summary_json
FROM jobs
WHERE finished_at IS NULL
ORDER BY created_at DESC
LIMIT 2000
"#,
                )?;

                let mut rows = stmt.query([])?;
                let mut items: Vec<(String, String, String)> = Vec::new();
                while let Some(row) = rows.next()? {
                    items.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                items
            };

            if items.is_empty() {
                return Ok(Vec::new());
            }

            let tx = conn.transaction()?;
            let mut recovered: Vec<String> = Vec::new();

            for (job_id, status, summary_raw) in items {
                if status == "queued" {
                    // queued jobs are not interrupted work; keep them pending for workers.
                    continue;
                }

                // Always leave an audit trail so operators can tell why the job ended.
                tx.execute(
                    r#"
INSERT INTO job_logs (job_id, ts, level, msg)
VALUES (?1, ?2, 'warn', ?3)
"#,
                    params![
                        job_id,
                        now,
                        format!("job recovered as terminated: reason={reason}")
                    ],
                )?;

                let is_terminal = matches!(status.as_str(), "success" | "failed" | "rolled_back");
                if is_terminal {
                    tx.execute(
                        r#"
UPDATE jobs
SET finished_at = ?2
WHERE id = ?1
"#,
                        params![job_id, now],
                    )?;
                    recovered.push(job_id);
                    continue;
                }

                let mut summary: serde_json::Value =
                    serde_json::from_str(&summary_raw).unwrap_or(serde_json::json!({}));
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

                tx.execute(
                    r#"
UPDATE jobs
SET status = 'failed', finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
                recovered.push(job_id);
            }

            tx.commit()?;
            Ok(recovered)
        })
        .await
        .context("recover incomplete jobs")
    }

    pub async fn terminate_job_as_failed(
        &self,
        job_id: &str,
        now: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let row: Option<(String, Option<String>, Option<String>)> = conn
                .query_row(
                    r#"
SELECT status, finished_at, summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((status, finished_at, summary_raw)) = row else {
                return Ok(false);
            };
            if finished_at.is_some() {
                return Ok(false);
            }

            // Always leave an audit trail so operators can tell why the job ended.
            conn.execute(
                r#"
INSERT INTO job_logs (job_id, ts, level, msg)
VALUES (?1, ?2, 'warn', ?3)
"#,
                params![
                    job_id,
                    now,
                    format!("job terminated: reason={reason} (previous_status={status})")
                ],
            )?;

            let mut summary: serde_json::Value =
                serde_json::from_str(&summary_raw.unwrap_or_else(|| "{}".to_string()))
                    .unwrap_or(serde_json::json!({}));
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

            let is_terminal = matches!(status.as_str(), "success" | "failed" | "rolled_back");
            if is_terminal {
                conn.execute(
                    r#"
UPDATE jobs
SET finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
            } else {
                conn.execute(
                    r#"
UPDATE jobs
SET status = 'failed', finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
            }

            Ok(true)
        })
        .await
        .context("terminate job as failed")
    }

    pub async fn get_job(&self, job_id: &str) -> anyhow::Result<Option<JobListItem>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![job_id],
                    |row| {
                        let summary_json: String = row.get(13)?;
                        let summary: serde_json::Value = serde_json::from_str(&summary_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
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
                    },
                )
                .optional()?)
        })
        .await
        .context("get job")
    }

    pub async fn list_job_logs(&self, job_id: &str) -> anyhow::Result<Vec<JobLogLine>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT ts, level, msg
FROM job_logs
WHERE job_id = ?1
ORDER BY id DESC
LIMIT 500
"#,
            )?;

            let rows = stmt.query_map(params![job_id], |row| {
                Ok(JobLogLine {
                    ts: row.get(0)?,
                    level: row.get(1)?,
                    msg: row.get(2)?,
                })
            })?;
            let mut out = rows.collect::<Result<Vec<_>, _>>()?;
            // Return ascending order for UI consumption while keeping the query "tail"-friendly.
            out.reverse();
            Ok(out)
        })
        .await
        .context("list job logs")
    }

    pub async fn list_job_logs_since(
        &self,
        job_id: &str,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobLogRow>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, ts, level, msg
FROM job_logs
WHERE job_id = ?1 AND id > ?2
ORDER BY id ASC
LIMIT ?3
"#,
            )?;

            let rows = stmt.query_map(params![job_id, after_id, limit as i64], |row| {
                Ok(JobLogRow {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    level: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job logs since")
    }

    pub async fn list_job_event_logs_since(
        &self,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobEventLogRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, job_id, ts, msg
FROM job_logs
WHERE level = 'event' AND id > ?1
ORDER BY id ASC
LIMIT ?2
"#,
            )?;

            let rows = stmt.query_map(params![after_id, limit as i64], |row| {
                Ok(JobEventLogRow {
                    id: row.get(0)?,
                    job_id: row.get(1)?,
                    ts: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job event logs since")
    }

    pub async fn get_job_logs_last_id(&self, job_id: &str) -> anyhow::Result<i64> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let v: i64 = conn.query_row(
                r#"
SELECT COALESCE(MAX(id), 0)
FROM job_logs
WHERE job_id = ?1
"#,
                params![job_id],
                |row| row.get(0),
            )?;
            Ok(v)
        })
        .await
        .context("get job logs last id")
    }

    pub async fn get_job_logs_global_last_id(&self) -> anyhow::Result<i64> {
        self.call(move |conn| {
            let v: i64 = conn.query_row(
                r#"
SELECT COALESCE(MAX(id), 0)
FROM job_logs
WHERE level = 'event'
"#,
                [],
                |row| row.get(0),
            )?;
            Ok(v)
        })
        .await
        .context("get global job logs last id")
    }

    pub async fn insert_job_log(&self, job_id: &str, line: &JobLogLine) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let line = line.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO job_logs (job_id, ts, level, msg) VALUES (?1, ?2, ?3, ?4)",
                params![job_id, line.ts, line.level, line.msg],
            )?;
            Ok(())
        })
        .await
        .context("insert job log")
    }

    pub async fn insert_backup(
        &self,
        backup_id: &str,
        stack_id: &str,
        job_id: &str,
        created_at: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let stack_id = stack_id.to_string();
        let job_id = job_id.to_string();
        let created_at = created_at.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO backups (id, stack_id, job_id, status, created_at)
VALUES (?1, ?2, ?3, 'running', ?4)
"#,
                params![backup_id, stack_id, job_id, created_at],
            )?;
            Ok(())
        })
        .await
        .context("insert backup")
    }

    pub async fn finish_backup(
        &self,
        backup_id: &str,
        status: &str,
        finished_at: &str,
        artifact_path: Option<&str>,
        size_bytes: Option<u64>,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let status = status.to_string();
        let finished_at = finished_at.to_string();
        let artifact_path = artifact_path.map(|s| s.to_string());
        let size_bytes = size_bytes.map(|v| v as i64);
        let error = error.map(|s| s.to_string());
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE backups
SET
  status = ?2,
  finished_at = ?3,
  artifact_path = ?4,
  size_bytes = ?5,
  error = ?6
WHERE id = ?1
"#,
                params![
                    backup_id,
                    status,
                    finished_at,
                    artifact_path,
                    size_bytes,
                    error
                ],
            )?;
            Ok(())
        })
        .await
        .context("finish backup")
    }

    pub async fn schedule_backup_cleanup(
        &self,
        backup_id: &str,
        cleanup_after: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let cleanup_after = cleanup_after.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET cleanup_after = ?2 WHERE id = ?1",
                params![backup_id, cleanup_after],
            )?;
            Ok(())
        })
        .await
        .context("schedule backup cleanup")
    }

    pub async fn mark_backup_deleted(
        &self,
        backup_id: &str,
        deleted_at: &str,
    ) -> anyhow::Result<()> {
        let backup_id = backup_id.to_string();
        let deleted_at = deleted_at.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE backups SET deleted_at = ?2 WHERE id = ?1",
                params![backup_id, deleted_at],
            )?;
            Ok(())
        })
        .await
        .context("mark backup deleted")
    }

    pub async fn list_due_backup_cleanups(
        &self,
        now: &str,
    ) -> anyhow::Result<Vec<BackupCleanupItem>> {
        let now = now.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, stack_id, job_id, artifact_path
FROM backups
WHERE
  status = 'success'
  AND deleted_at IS NULL
  AND artifact_path IS NOT NULL
  AND cleanup_after IS NOT NULL
  AND cleanup_after <= ?1
ORDER BY cleanup_after ASC
LIMIT 50
"#,
            )?;
            let rows = stmt.query_map(params![now], |row| {
                Ok(BackupCleanupItem {
                    id: row.get(0)?,
                    stack_id: row.get(1)?,
                    job_id: row.get(2)?,
                    artifact_path: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list due backup cleanups")
    }

    pub async fn list_success_backup_ids_for_stack(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id
FROM backups
WHERE stack_id = ?1 AND status = 'success' AND deleted_at IS NULL
ORDER BY created_at DESC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list success backups for stack")
    }
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<PathBuf> {
    let path = path.to_path_buf();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("create dir {:?}", parent))?;
    }
    Ok(path)
}

fn ensure_service_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "current_digest",
            ddl: "ALTER TABLE services ADD COLUMN current_digest TEXT",
        },
        Col {
            name: "current_resolved_tag",
            ddl: "ALTER TABLE services ADD COLUMN current_resolved_tag TEXT",
        },
        Col {
            name: "current_resolved_tags_json",
            ddl: "ALTER TABLE services ADD COLUMN current_resolved_tags_json TEXT",
        },
        Col {
            name: "candidate_tag",
            ddl: "ALTER TABLE services ADD COLUMN candidate_tag TEXT",
        },
        Col {
            name: "candidate_resolved_tag",
            ddl: "ALTER TABLE services ADD COLUMN candidate_resolved_tag TEXT",
        },
        Col {
            name: "candidate_digest",
            ddl: "ALTER TABLE services ADD COLUMN candidate_digest TEXT",
        },
        Col {
            name: "candidate_arch_match",
            ddl: "ALTER TABLE services ADD COLUMN candidate_arch_match TEXT",
        },
        Col {
            name: "candidate_arch_json",
            ddl: "ALTER TABLE services ADD COLUMN candidate_arch_json TEXT",
        },
        Col {
            name: "ignore_rule_id",
            ddl: "ALTER TABLE services ADD COLUMN ignore_rule_id TEXT",
        },
        Col {
            name: "ignore_reason",
            ddl: "ALTER TABLE services ADD COLUMN ignore_reason TEXT",
        },
        Col {
            name: "checked_at",
            ddl: "ALTER TABLE services ADD COLUMN checked_at TEXT",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(services)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_notification_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "webpush_vapid_private_key",
            ddl: "ALTER TABLE notification_settings ADD COLUMN webpush_vapid_private_key TEXT",
        },
        Col {
            name: "webpush_vapid_subject",
            ddl: "ALTER TABLE notification_settings ADD COLUMN webpush_vapid_subject TEXT",
        },
        Col {
            name: "event_update_enabled",
            ddl: "ALTER TABLE notification_settings ADD COLUMN event_update_enabled INTEGER NOT NULL DEFAULT 1",
        },
        Col {
            name: "event_new_version_enabled",
            ddl: "ALTER TABLE notification_settings ADD COLUMN event_new_version_enabled INTEGER NOT NULL DEFAULT 1",
        },
        Col {
            name: "event_ghcr_webhook_anomaly_enabled",
            ddl: "ALTER TABLE notification_settings ADD COLUMN event_ghcr_webhook_anomaly_enabled INTEGER NOT NULL DEFAULT 1",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(notification_settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_settings_deploy_welcome_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "deploy_welcome_never_auto_open",
            ddl: "ALTER TABLE settings ADD COLUMN deploy_welcome_never_auto_open INTEGER NOT NULL DEFAULT 0",
        },
        Col {
            name: "deploy_welcome_updated_at",
            ddl: "ALTER TABLE settings ADD COLUMN deploy_welcome_updated_at TEXT",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_settings_resource_monitor_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "resource_monitor_enabled",
            ddl: "ALTER TABLE settings ADD COLUMN resource_monitor_enabled INTEGER NOT NULL DEFAULT 1",
        },
        Col {
            name: "resource_sample_interval_seconds",
            ddl: "ALTER TABLE settings ADD COLUMN resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 30",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    conn.execute(
        r#"
UPDATE settings
SET resource_sample_interval_seconds = 30
WHERE resource_sample_interval_seconds NOT IN (10, 30, 60, 300)
"#,
        [],
    )?;

    Ok(())
}

fn ensure_settings_schedule_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "schedule_update_check_enabled",
            ddl: "ALTER TABLE settings ADD COLUMN schedule_update_check_enabled INTEGER NOT NULL DEFAULT 0",
        },
        Col {
            name: "schedule_update_check_cron",
            ddl: "ALTER TABLE settings ADD COLUMN schedule_update_check_cron TEXT NOT NULL DEFAULT '*/30 * * * *'",
        },
        Col {
            name: "schedule_ghcr_webhook_audit_enabled",
            ddl: "ALTER TABLE settings ADD COLUMN schedule_ghcr_webhook_audit_enabled INTEGER NOT NULL DEFAULT 1",
        },
        Col {
            name: "schedule_ghcr_webhook_audit_cron",
            ddl: "ALTER TABLE settings ADD COLUMN schedule_ghcr_webhook_audit_cron TEXT NOT NULL DEFAULT '0 3 * * *'",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_settings_public_base_url_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [Col {
        name: "public_base_url",
        ddl: "ALTER TABLE settings ADD COLUMN public_base_url TEXT",
    }];

    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_stack_archive_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "archived",
            ddl: "ALTER TABLE stacks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
        },
        Col {
            name: "archived_at",
            ddl: "ALTER TABLE stacks ADD COLUMN archived_at TEXT",
        },
        Col {
            name: "archived_reason",
            ddl: "ALTER TABLE stacks ADD COLUMN archived_reason TEXT",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(stacks)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_service_archive_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "archived",
            ddl: "ALTER TABLE services ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
        },
        Col {
            name: "archived_at",
            ddl: "ALTER TABLE services ADD COLUMN archived_at TEXT",
        },
        Col {
            name: "archived_reason",
            ddl: "ALTER TABLE services ADD COLUMN archived_reason TEXT",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(services)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    Ok(())
}

fn ensure_github_packages_repos_webhook_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "webhook_state",
            ddl: "ALTER TABLE github_packages_repos ADD COLUMN webhook_state TEXT NOT NULL DEFAULT 'unknown'",
        },
        Col {
            name: "webhook_job_id",
            ddl: "ALTER TABLE github_packages_repos ADD COLUMN webhook_job_id TEXT",
        },
        Col {
            name: "last_audit_at",
            ddl: "ALTER TABLE github_packages_repos ADD COLUMN last_audit_at TEXT",
        },
        Col {
            name: "last_op",
            ddl: "ALTER TABLE github_packages_repos ADD COLUMN last_op TEXT",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(github_packages_repos)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    conn.execute(
        r#"
UPDATE github_packages_repos
SET webhook_state = 'unknown'
WHERE webhook_state IS NULL OR trim(webhook_state) = ''
"#,
        [],
    )?;

    Ok(())
}

fn ensure_github_packages_deliveries_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Col<'a> {
        name: &'a str,
        ddl: &'a str,
    }

    let desired = [
        Col {
            name: "first_received_at",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN first_received_at TEXT",
        },
        Col {
            name: "event",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN event TEXT",
        },
        Col {
            name: "action",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN action TEXT",
        },
        Col {
            name: "decision",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN decision TEXT NOT NULL DEFAULT 'processed'",
        },
        Col {
            name: "reason",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN reason TEXT",
        },
        Col {
            name: "response_status",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN response_status INTEGER",
        },
        Col {
            name: "job_id",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN job_id TEXT",
        },
        Col {
            name: "job_ids_json",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN job_ids_json TEXT NOT NULL DEFAULT '[]'",
        },
        Col {
            name: "attempt_count",
            ddl: "ALTER TABLE github_packages_deliveries ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 1",
        },
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(github_packages_deliveries)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for col in desired {
        if existing.iter().any(|c| c == col.name) {
            continue;
        }
        conn.execute_batch(col.ddl)?;
    }

    conn.execute_batch(
        r#"
CREATE INDEX IF NOT EXISTS idx_github_packages_deliveries_received_delivery
  ON github_packages_deliveries(received_at DESC, delivery_id DESC);
"#,
    )?;

    conn.execute(
        r#"
UPDATE github_packages_deliveries
SET first_received_at = received_at
WHERE first_received_at IS NULL OR trim(first_received_at) = ''
"#,
        [],
    )?;

    conn.execute(
        r#"
UPDATE github_packages_deliveries
SET decision = 'processed'
WHERE decision IS NULL OR trim(decision) = ''
"#,
        [],
    )?;

    conn.execute(
        r#"
UPDATE github_packages_deliveries
SET response_status = 200
WHERE response_status IS NULL AND decision = 'processed'
"#,
        [],
    )?;

    conn.execute(
        r#"
UPDATE github_packages_deliveries
SET attempt_count = 1
WHERE attempt_count IS NULL OR attempt_count < 1
"#,
        [],
    )?;

    conn.execute(
        r#"
UPDATE github_packages_deliveries
SET job_ids_json = '[]'
WHERE job_ids_json IS NULL OR trim(job_ids_json) = ''
"#,
        [],
    )?;

    Ok(())
}

fn ensure_discovery_schema(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS discovered_compose_projects (
  project TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT,
  status TEXT NOT NULL,
  last_seen_at TEXT,
  last_scan_at TEXT,
  last_error TEXT,
  last_config_files_json TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  archived_at TEXT,
  archived_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_discovered_compose_projects_stack_id ON discovered_compose_projects(stack_id);
"#,
    )?;
    Ok(())
}

fn ensure_schema_migrations_table(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL
);
"#,
    )?;
    Ok(())
}

fn now_rfc3339() -> anyhow::Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

fn migration_applied(conn: &rusqlite::Connection, id: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE id = ?1",
            params![id],
            |_row| Ok(()),
        )
        .optional()?
        .is_some())
}

fn record_migration_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> anyhow::Result<()> {
    let applied_at = now_rfc3339()?;
    tx.execute(
        "INSERT INTO schema_migrations (id, applied_at) VALUES (?1, ?2)",
        params![id, applied_at],
    )?;
    Ok(())
}

fn apply_migration_0007_remove_manual_stacks(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0007_remove_manual_stacks";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM ignore_rules", [])?;
    tx.execute(
        "DELETE FROM jobs WHERE stack_id IS NOT NULL OR service_id IS NOT NULL",
        [],
    )?;
    tx.execute("DELETE FROM stacks", [])?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0008_drop_version_inference_snapshots(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0008_drop_version_inference_snapshots";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("DROP TABLE IF EXISTS image_version_inference_snapshots", [])?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn auto_archive_missing_discovery_projects_on_startup(
    conn: &rusqlite::Connection,
) -> anyhow::Result<()> {
    let now = now_rfc3339()?;
    conn.execute(
        r#"
UPDATE discovered_compose_projects
SET archived = 1, archived_at = ?1, archived_reason = 'auto_archive_on_restart'
WHERE status = 'missing' AND archived = 0
"#,
        params![now],
    )?;
    Ok(())
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS stacks (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  compose_type TEXT NOT NULL,
  compose_files_json TEXT NOT NULL,
  env_file TEXT,
  backup_targets_json TEXT NOT NULL,
  backup_retention_keep_last INTEGER NOT NULL,
  backup_retention_delete_after_stable_seconds INTEGER NOT NULL,
  archived INTEGER NOT NULL DEFAULT 0,
  archived_at TEXT,
  archived_reason TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_check_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS services (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  image_tag TEXT NOT NULL,
  current_digest TEXT,
  current_resolved_tag TEXT,
  current_resolved_tags_json TEXT,
  candidate_tag TEXT,
  candidate_resolved_tag TEXT,
  candidate_digest TEXT,
  candidate_arch_match TEXT,
  candidate_arch_json TEXT,
  ignore_rule_id TEXT,
  ignore_reason TEXT,
  checked_at TEXT,
  auto_rollback INTEGER NOT NULL,
  archived INTEGER NOT NULL DEFAULT 0,
  archived_at TEXT,
  archived_reason TEXT,
  backup_targets_bind_paths_json TEXT NOT NULL,
  backup_targets_volume_names_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_services_stack_id ON services(stack_id);

CREATE TABLE IF NOT EXISTS discovered_compose_projects (
  project TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT,
  status TEXT NOT NULL,
  last_seen_at TEXT,
  last_scan_at TEXT,
  last_error TEXT,
  last_config_files_json TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  archived_at TEXT,
  archived_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_discovered_compose_projects_stack_id ON discovered_compose_projects(stack_id);

CREATE TABLE IF NOT EXISTS schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ignore_rules (
  id TEXT PRIMARY KEY NOT NULL,
  enabled INTEGER NOT NULL,
  scope_type TEXT NOT NULL,
  scope_service_id TEXT NOT NULL,
  match_kind TEXT NOT NULL,
  match_value TEXT NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  id TEXT PRIMARY KEY NOT NULL,
  backup_enabled INTEGER NOT NULL,
  backup_require_success INTEGER NOT NULL,
  backup_base_dir TEXT NOT NULL,
  backup_skip_targets_over_bytes INTEGER NOT NULL,
  resource_monitor_enabled INTEGER NOT NULL DEFAULT 1,
  resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 30,
  schedule_update_check_enabled INTEGER NOT NULL DEFAULT 0,
  schedule_update_check_cron TEXT NOT NULL DEFAULT '*/30 * * * *',
  schedule_ghcr_webhook_audit_enabled INTEGER NOT NULL DEFAULT 1,
  schedule_ghcr_webhook_audit_cron TEXT NOT NULL DEFAULT '0 3 * * *',
  public_base_url TEXT,
  deploy_welcome_never_auto_open INTEGER NOT NULL DEFAULT 0,
  deploy_welcome_updated_at TEXT,
  updated_at TEXT
);

CREATE TABLE IF NOT EXISTS notification_settings (
  id TEXT PRIMARY KEY NOT NULL,
  email_enabled INTEGER NOT NULL,
  email_smtp_url TEXT,
  webhook_enabled INTEGER NOT NULL,
  webhook_url TEXT,
  telegram_enabled INTEGER NOT NULL,
  telegram_bot_token TEXT,
  telegram_chat_id TEXT,
  webpush_enabled INTEGER NOT NULL,
  webpush_vapid_public_key TEXT,
  webpush_vapid_private_key TEXT,
  webpush_vapid_subject TEXT,
  event_update_enabled INTEGER NOT NULL DEFAULT 1,
  event_new_version_enabled INTEGER NOT NULL DEFAULT 1,
  event_ghcr_webhook_anomaly_enabled INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT
);

CREATE TABLE IF NOT EXISTS web_push_subscriptions (
  endpoint TEXT PRIMARY KEY NOT NULL,
  p256dh TEXT NOT NULL,
  auth TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS github_packages_settings (
  id TEXT PRIMARY KEY NOT NULL,
  enabled INTEGER NOT NULL,
  callback_url TEXT NOT NULL,
  pat TEXT,
  webhook_secret TEXT,
  updated_at TEXT
);

CREATE TABLE IF NOT EXISTS github_packages_targets (
  id TEXT PRIMARY KEY NOT NULL,
  input TEXT NOT NULL,
  kind TEXT NOT NULL,
  owner TEXT NOT NULL,
  warnings_json TEXT NOT NULL,
  updated_at TEXT
);

CREATE TABLE IF NOT EXISTS github_packages_repos (
  owner TEXT NOT NULL,
  repo TEXT NOT NULL,
  selected INTEGER NOT NULL,
  webhook_state TEXT NOT NULL DEFAULT 'unknown',
  webhook_job_id TEXT,
  hook_id INTEGER,
  last_sync_at TEXT,
  last_audit_at TEXT,
  last_op TEXT,
  last_error TEXT,
  updated_at TEXT,
  PRIMARY KEY (owner, repo)
);
CREATE INDEX IF NOT EXISTS idx_github_packages_repos_selected ON github_packages_repos(selected);

CREATE TABLE IF NOT EXISTS github_packages_deliveries (
  delivery_id TEXT PRIMARY KEY NOT NULL,
  received_at TEXT NOT NULL,
  first_received_at TEXT NOT NULL,
  owner TEXT,
  repo TEXT,
  event TEXT,
  action TEXT,
  decision TEXT NOT NULL DEFAULT 'processed',
  reason TEXT,
  response_status INTEGER,
  job_id TEXT,
  job_ids_json TEXT NOT NULL DEFAULT '[]',
  attempt_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_github_packages_deliveries_received_delivery
  ON github_packages_deliveries(received_at DESC, delivery_id DESC);

CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY NOT NULL,
  type TEXT NOT NULL,
  scope TEXT NOT NULL,
  stack_id TEXT,
  service_id TEXT,
  status TEXT NOT NULL,
  allow_arch_mismatch INTEGER NOT NULL,
  backup_mode TEXT NOT NULL,
  created_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  summary_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_stack_id ON jobs(stack_id);

CREATE TABLE IF NOT EXISTS job_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  ts TEXT NOT NULL,
  level TEXT NOT NULL,
  msg TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_logs_job_id ON job_logs(job_id);

CREATE TABLE IF NOT EXISTS backups (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  artifact_path TEXT,
  size_bytes INTEGER,
  error TEXT,
  cleanup_after TEXT,
  deleted_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_backups_stack_id ON backups(stack_id);
CREATE INDEX IF NOT EXISTS idx_backups_cleanup_after ON backups(cleanup_after);

CREATE TABLE IF NOT EXISTS service_digest_tags_snapshots (
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  digest TEXT NOT NULL,
  snapshot_json TEXT NOT NULL,
  checked_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (service_id, digest)
);

CREATE TABLE IF NOT EXISTS image_digest_tags_snapshots (
  image_repo TEXT NOT NULL,
  digest TEXT NOT NULL,
  host_platform TEXT NOT NULL,
  snapshot_json TEXT NOT NULL,
  checked_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (image_repo, digest, host_platform)
);

CREATE TABLE IF NOT EXISTS service_resource_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  sampled_at TEXT NOT NULL,
  cpu_percent REAL NOT NULL,
  mem_used_bytes INTEGER,
  mem_limit_bytes INTEGER,
  net_rx_bytes INTEGER,
  net_tx_bytes INTEGER,
  block_read_bytes INTEGER,
  block_write_bytes INTEGER,
  pids INTEGER,
  container_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_service_resource_samples_service_time
  ON service_resource_samples(service_id, sampled_at);
CREATE INDEX IF NOT EXISTS idx_service_resource_samples_sampled_at
  ON service_resource_samples(sampled_at);
"#;
