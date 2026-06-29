use std::collections::BTreeMap;

use super::*;

impl Db {
    async fn list_service_backup_target_policy_rows(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Vec<(String, crate::db::ServiceBackupTargetPolicyRow)>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT target_kind, target_key, policy
FROM service_backup_target_policies
WHERE service_id = ?1
ORDER BY target_kind ASC, target_key ASC
"#,
            )?;
            let rows = stmt.query_map(params![service_id], |row| {
                let policy = match row.get::<_, String>(2)?.as_str() {
                    "stop_related_services" => {
                        crate::api::types::BackupTargetPolicy::StopRelatedServices
                    }
                    "live_backup" => crate::api::types::BackupTargetPolicy::LiveBackup,
                    _ => crate::api::types::BackupTargetPolicy::Disabled,
                };
                Ok((
                    row.get::<_, String>(0)?,
                    crate::db::ServiceBackupTargetPolicyRow {
                        key: row.get(1)?,
                        policy,
                    },
                ))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service backup target policy rows")
    }

    pub async fn get_stored_service_settings(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<StoredServiceSettings>> {
        let rows = self
            .list_service_backup_target_policy_rows(service_id)
            .await?;
        let service_id = service_id.to_string();
        let stored = self
            .call(move |conn| {
                Ok(conn
                    .query_row(
                        r#"
SELECT
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json,
  repo_url,
  repo_url_auto_disabled,
  auto_policy.mode,
  auto_policy.enabled,
  auto_policy.rules_json,
  auto_policy.updated_at
FROM services
LEFT JOIN auto_update_policies auto_policy
  ON auto_policy.scope_type = 'service' AND auto_policy.scope_id = services.id
WHERE id = ?1
"#,
                        params![service_id],
                        |row| {
                            Ok(StoredServiceSettings {
                                auto_update_policy:
                                    super::auto_update::auto_update_policy_from_row(
                                        row.get(5)?,
                                        row.get(6)?,
                                        row.get(7)?,
                                        row.get(8)?,
                                        crate::api::types::AutoUpdatePolicyMode::Inherit,
                                    )?,
                                settings: ServiceSettings {
                                    auto_rollback: row.get::<_, i64>(0)? != 0,
                                    backup_targets: crate::api::types::BackupTargetOverrides {
                                        bind_paths: BTreeMap::new(),
                                        volume_names: BTreeMap::new(),
                                    },
                                    repo_url: row.get(3)?,
                                },
                                repo_url_auto_disabled: row.get::<_, i64>(4)? != 0,
                            })
                        },
                    )
                    .optional()?)
            })
            .await
            .context("get stored service settings")?;
        let Some(mut stored) = stored else {
            return Ok(None);
        };
        for (target_kind, row) in rows {
            let choice = match row.policy {
                crate::api::types::BackupTargetPolicy::Disabled => {
                    crate::api::types::TernaryChoice::Skip
                }
                crate::api::types::BackupTargetPolicy::StopRelatedServices => {
                    crate::api::types::TernaryChoice::Force
                }
                crate::api::types::BackupTargetPolicy::LiveBackup => {
                    crate::api::types::TernaryChoice::Inherit
                }
            };
            if target_kind == "volume" {
                stored
                    .settings
                    .backup_targets
                    .volume_names
                    .insert(row.key, choice);
            } else {
                stored
                    .settings
                    .backup_targets
                    .bind_paths
                    .insert(row.key, choice);
            }
        }
        Ok(Some(stored))
    }

    pub async fn get_service_settings(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceSettings>> {
        Ok(self
            .get_stored_service_settings(service_id)
            .await?
            .map(|stored| stored.settings))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn put_service_settings(
        &self,
        service_id: &str,
        settings: &ServiceSettings,
        now: &str,
    ) -> anyhow::Result<bool> {
        self.put_service_settings_with_repo_auto_disabled(service_id, settings, false, now)
            .await
    }

    pub async fn put_service_settings_with_repo_auto_disabled(
        &self,
        service_id: &str,
        settings: &ServiceSettings,
        repo_url_auto_disabled: bool,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let settings = settings.clone();
        let repo_url_auto_disabled = repo_url_auto_disabled as i64;
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                r#"
UPDATE services
SET
  auto_rollback = ?2,
  backup_targets_bind_paths_json = ?3,
  backup_targets_volume_names_json = ?4,
  repo_url = ?5,
  repo_url_auto_disabled = ?6,
  updated_at = ?7
WHERE id = ?1
"#,
                params![
                    service_id,
                    settings.auto_rollback as i64,
                    serde_json::to_string(&settings.backup_targets.bind_paths)?,
                    serde_json::to_string(&settings.backup_targets.volume_names)?,
                    settings.repo_url,
                    repo_url_auto_disabled,
                    now
                ],
            )?;
            if changed > 0 {
                tx.execute(
                    "DELETE FROM service_backup_target_policies WHERE service_id = ?1",
                    params![service_id.clone()],
                )?;
                for (target_kind, row) in
                    super::service_policy_rows_from_overrides(&settings.backup_targets)
                {
                    tx.execute(
                        r#"
INSERT INTO service_backup_target_policies (
  service_id,
  target_kind,
  target_key,
  policy,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
"#,
                        params![
                            service_id.clone(),
                            target_kind,
                            row.key,
                            row.policy.as_str(),
                            now.clone(),
                        ],
                    )?;
                }
            }
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("put service settings with repo auto disabled")
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
}
