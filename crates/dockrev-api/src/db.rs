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

#[derive(Clone)]
pub struct Db {
    conn: Connection,
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
  webpush_vapid_public_key
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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
                    Option::<String>::None
                ],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn list_stacks(&self) -> anyhow::Result<Vec<StackListItem>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  s.id,
  s.name,
  s.last_check_at,
  (SELECT COUNT(1) FROM services sv WHERE sv.stack_id = s.id) AS services
FROM stacks s
ORDER BY s.created_at DESC
"#,
            )?;

            let rows = stmt.query_map([], |row| {
                Ok(StackListItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: StackStatus::Unknown,
                    services: row.get::<_, i64>(3)? as u32,
                    updates: 0,
                    last_check_at: row.get(2)?,
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
  backup_retention_delete_after_stable_seconds
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
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let mut rows = stmt.query(params![stack.id.clone()])?;

            while let Some(row) = rows.next()? {
                let bind_paths_json: String = row.get(5)?;
                let volume_names_json: String = row.get(6)?;
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

                stack.services.push(crate::api::types::Service {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image: ComposeRef {
                        reference: row.get(2)?,
                        tag: row.get(3)?,
                        digest: None,
                    },
                    candidate: None,
                    ignore: None,
                    settings: ServiceSettings {
                        auto_rollback: row.get::<_, i64>(4)? != 0,
                        backup_targets: crate::api::types::BackupTargetOverrides {
                            bind_paths,
                            volume_names,
                        },
                    },
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
  webpush_vapid_public_key
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
  updated_at = ?10
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

    pub async fn insert_job(
        &self,
        job: JobListItem,
        created_by: &str,
        reason: &str,
    ) -> anyhow::Result<()> {
        let created_by = created_by.to_string();
        let reason = reason.to_string();
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
                    created_by,
                    reason,
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
                let summary_json: String = row.get(11)?;
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
                    created_at: row.get(6)?,
                    started_at: row.get(7)?,
                    finished_at: row.get(8)?,
                    allow_arch_mismatch: row.get::<_, i64>(9)? != 0,
                    backup_mode: row.get(10)?,
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
                        let summary_json: String = row.get(11)?;
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
                            created_at: row.get(6)?,
                            started_at: row.get(7)?,
                            finished_at: row.get(8)?,
                            allow_arch_mismatch: row.get::<_, i64>(9)? != 0,
                            backup_mode: row.get(10)?,
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
  auto_rollback INTEGER NOT NULL,
  backup_targets_bind_paths_json TEXT NOT NULL,
  backup_targets_volume_names_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_services_stack_id ON services(stack_id);

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
"#;
