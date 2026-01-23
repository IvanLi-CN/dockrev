use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use tokio_rusqlite::Connection;

use crate::api::types::{
    BackupSettings, ComposeConfig, ComposeRef, IgnoreRule, IgnoreRuleMatch, IgnoreRuleScope,
    JobListItem, JobLogLine, JobScope, JobType, NotificationSettings, ServiceSettings,
    StackListItem, StackRecord, StackStatus,
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
    pub image_ref: String,
    pub image_tag: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchivedFilter {
    Exclude,
    Include,
    Only,
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
            ensure_stack_archive_columns(conn)?;
            ensure_service_archive_columns(conn)?;
            ensure_discovery_schema(conn)?;
            ensure_schema_migrations_table(conn)?;
            apply_migration_0007_remove_manual_stacks(conn)?;
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
  backup_skip_targets_over_bytes
) VALUES (?1, ?2, ?3, ?4, ?5)
"#,
                params!["default", 1i64, 1i64, "/data/backups", 104857600i64],
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
  webpush_vapid_subject
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
	  candidate_tag,
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
                let bind_paths_json: String = row.get(13)?;
                let volume_names_json: String = row.get(14)?;
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

                let candidate_tag: Option<String> = row.get(5)?;
                let candidate_digest: Option<String> = row.get(6)?;
                let candidate_arch_match: Option<String> = row.get(7)?;
                let candidate_arch_json: Option<String> = row.get(8)?;
                let ignore_rule_id: Option<String> = row.get(9)?;
                let ignore_reason: Option<String> = row.get(10)?;

                let candidate_arch: Vec<String> = candidate_arch_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .unwrap_or_default();

                let candidate = match (candidate_tag, candidate_digest) {
                    (Some(tag), Some(digest)) => Some(crate::api::types::Candidate {
                        tag,
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
                    },
                    candidate,
                    ignore,
                    settings: ServiceSettings {
                        auto_rollback: row.get::<_, i64>(11)? != 0,
                        backup_targets: crate::api::types::BackupTargetOverrides {
                            bind_paths,
                            volume_names,
                        },
                    },
                    archived: Some(row.get::<_, i64>(12)? != 0),
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
  candidate_tag = NULL,
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
SELECT id, image_ref, image_tag
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| {
                Ok(ServiceForCheck {
                    id: row.get(0)?,
                    image_ref: row.get(1)?,
                    image_tag: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list services for check")
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
        candidate_tag: Option<String>,
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
  candidate_tag = ?3,
  candidate_digest = ?4,
  candidate_arch_match = ?5,
  candidate_arch_json = ?6,
  ignore_rule_id = ?7,
  ignore_reason = ?8,
  checked_at = ?9,
  updated_at = ?10
WHERE id = ?1
"#,
                params![
                    service_id,
                    current_digest,
                    candidate_tag,
                    candidate_digest,
                    candidate_arch_match,
                    candidate_arch_json,
                    ignore_rule_id,
                    ignore_reason,
                    checked_at,
                    now
                ],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("update service check result")
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
  webpush_vapid_subject
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
  updated_at = ?12
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
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put notification settings")
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

    pub async fn put_backup_settings(
        &self,
        backup: &BackupSettings,
        now: &str,
    ) -> anyhow::Result<()> {
        let backup = backup.clone();
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
  updated_at = ?5
WHERE id = 'default'
"#,
                params![
                    backup.enabled as i64,
                    backup.require_success as i64,
                    backup.base_dir,
                    backup.skip_targets_over_bytes as i64,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put backup settings")
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
        let summary_json = serde_json::to_string(summary_json)?;
        self.call(move |conn| {
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
ORDER BY id ASC
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
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job logs")
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
            name: "candidate_tag",
            ddl: "ALTER TABLE services ADD COLUMN candidate_tag TEXT",
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
  candidate_tag TEXT,
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
  updated_at TEXT
);

CREATE TABLE IF NOT EXISTS web_push_subscriptions (
  endpoint TEXT PRIMARY KEY NOT NULL,
  p256dh TEXT NOT NULL,
  auth TEXT NOT NULL,
  created_at TEXT NOT NULL
);

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
"#;
