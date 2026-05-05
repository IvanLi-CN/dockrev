use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::*;

pub(super) fn ensure_parent_dir(path: &Path) -> anyhow::Result<PathBuf> {
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
            name: "current_runtime_started_at",
            ddl: "ALTER TABLE services ADD COLUMN current_runtime_started_at TEXT",
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
        Col {
            name: "repo_url",
            ddl: "ALTER TABLE services ADD COLUMN repo_url TEXT",
        },
        Col {
            name: "homepage_json",
            ddl: "ALTER TABLE services ADD COLUMN homepage_json TEXT",
        },
        Col {
            name: "update_guard_json",
            ddl: "ALTER TABLE services ADD COLUMN update_guard_json TEXT",
        },
        Col {
            name: "repo_url_auto_disabled",
            ddl: "ALTER TABLE services ADD COLUMN repo_url_auto_disabled INTEGER NOT NULL DEFAULT 0",
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
            ddl: "ALTER TABLE settings ADD COLUMN resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 10",
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
SET resource_sample_interval_seconds = 10
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

fn ensure_github_packages_delivery_events_schema(
    conn: &rusqlite::Connection,
) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS github_packages_delivery_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  delivery_id TEXT NOT NULL,
  received_at TEXT NOT NULL,
  payload_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_github_packages_delivery_events_delivery_id
  ON github_packages_delivery_events(delivery_id, id DESC);
"#,
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

fn ensure_auto_update_schema(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS auto_update_policies (
  scope_type TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  mode TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  rules_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (scope_type, scope_id)
);
CREATE INDEX IF NOT EXISTS idx_auto_update_policies_scope
  ON auto_update_policies(scope_type, scope_id);

CREATE TABLE IF NOT EXISTS auto_update_pending (
  id TEXT PRIMARY KEY NOT NULL,
  policy_scope_type TEXT NOT NULL,
  policy_scope_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  source_check_job_id TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  due_at TEXT NOT NULL,
  min_age_seconds INTEGER NOT NULL,
  min_version_lag INTEGER NOT NULL,
  status TEXT NOT NULL,
  update_job_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  summary_json TEXT NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_auto_update_pending_active_candidate
  ON auto_update_pending(service_id, rule_id, candidate_digest)
  WHERE status IN ('pending', 'enqueuing', 'enqueued');
CREATE INDEX IF NOT EXISTS idx_auto_update_pending_due
  ON auto_update_pending(status, due_at);
"#,
    )?;
    Ok(())
}

fn ensure_service_tag_history_schema(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS service_tag_history (
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  image_repo TEXT NOT NULL,
  tag TEXT NOT NULL,
  last_used_at TEXT NOT NULL,
  use_count INTEGER NOT NULL DEFAULT 1,
  source TEXT NOT NULL,
  PRIMARY KEY (service_id, image_repo, tag)
);
CREATE INDEX IF NOT EXISTS idx_service_tag_history_service_last_used
  ON service_tag_history(service_id, last_used_at DESC);
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

pub(super) fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    ensure_service_columns(conn)?;
    ensure_notification_columns(conn)?;
    ensure_settings_deploy_welcome_columns(conn)?;
    ensure_settings_resource_monitor_columns(conn)?;
    ensure_settings_schedule_columns(conn)?;
    ensure_settings_public_base_url_columns(conn)?;
    ensure_stack_archive_columns(conn)?;
    ensure_service_archive_columns(conn)?;
    ensure_discovery_schema(conn)?;
    ensure_auto_update_schema(conn)?;
    ensure_service_tag_history_schema(conn)?;
    ensure_github_packages_repos_webhook_columns(conn)?;
    ensure_github_packages_deliveries_columns(conn)?;
    ensure_github_packages_delivery_events_schema(conn)?;
    ensure_schema_migrations_table(conn)?;
    apply_migration_0007_remove_manual_stacks(conn)?;
    apply_migration_0008_drop_version_inference_snapshots(conn)?;
    apply_migration_0009_add_new_version_notifications(conn)?;
    apply_migration_0010_add_new_version_discoveries(conn)?;
    apply_migration_0011_track_candidate_display_tags_in_new_version_discoveries(conn)?;
    apply_migration_0012_track_image_ref_in_new_version_discoveries(conn)?;
    auto_archive_missing_discovery_projects_on_startup(conn)?;
    Ok(())
}

pub(super) fn ensure_defaults(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
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
            10i64,
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

fn apply_migration_0009_add_new_version_notifications(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0009_add_new_version_notifications";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS new_version_notifications (
  id TEXT PRIMARY KEY NOT NULL,
  service_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  image_tag TEXT NOT NULL,
  current_tag TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  status TEXT NOT NULL,
  sent_channels_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  sent_at TEXT,
  superseded_at TEXT,
  last_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_new_version_notifications_service_status
  ON new_version_notifications(service_id, status, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_new_version_notifications_active_service_digest
  ON new_version_notifications(service_id, candidate_digest)
  WHERE status IN ('pending', 'sent');
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0010_add_new_version_discoveries(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0010_add_new_version_discoveries";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS service_new_version_discoveries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL,
  image_ref TEXT NOT NULL DEFAULT '',
  source_job_id TEXT NOT NULL,
  discovered_at TEXT NOT NULL,
  current_digest TEXT NOT NULL DEFAULT '',
  current_display_tag TEXT NOT NULL DEFAULT '',
  current_tag TEXT NOT NULL DEFAULT '',
  candidate_tag TEXT NOT NULL DEFAULT '',
  candidate_digest TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_service_new_version_discoveries_unique_candidate
  ON service_new_version_discoveries(
    service_id,
    image_ref,
    current_digest,
    current_display_tag,
    current_tag,
    candidate_tag,
    candidate_digest,
    candidate_display_tag
  );
CREATE INDEX IF NOT EXISTS idx_service_new_version_discoveries_service_discovered_at
  ON service_new_version_discoveries(service_id, discovered_at DESC, id DESC);
"#,
    )?;
    new_version_discoveries::backfill_new_version_discoveries_from_successful_checks_conn(&tx)?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0011_track_candidate_display_tags_in_new_version_discoveries(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0011_track_candidate_display_tags_in_new_version_discoveries";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut stmt = tx.prepare("PRAGMA table_info(service_new_version_discoveries)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if !existing.iter().any(|column| column == "candidate_tag") {
        tx.execute_batch(
            "ALTER TABLE service_new_version_discoveries ADD COLUMN candidate_tag TEXT NOT NULL DEFAULT ''",
        )?;
    }

    if !existing
        .iter()
        .any(|column| column == "candidate_display_tag")
    {
        tx.execute_batch(
            "ALTER TABLE service_new_version_discoveries ADD COLUMN candidate_display_tag TEXT NOT NULL DEFAULT ''",
        )?;
    }

    tx.execute_batch(
        r#"
DROP INDEX IF EXISTS idx_service_new_version_discoveries_unique_candidate;
CREATE UNIQUE INDEX IF NOT EXISTS idx_service_new_version_discoveries_unique_candidate
  ON service_new_version_discoveries(
    service_id,
    current_digest,
    current_display_tag,
    current_tag,
    candidate_tag,
    candidate_digest,
    candidate_display_tag
  );
DELETE FROM service_new_version_discoveries;
"#,
    )?;
    new_version_discoveries::backfill_new_version_discoveries_from_successful_checks_conn(&tx)?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0012_track_image_ref_in_new_version_discoveries(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0012_track_image_ref_in_new_version_discoveries";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut stmt = tx.prepare("PRAGMA table_info(service_new_version_discoveries)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if !existing.iter().any(|column| column == "image_ref") {
        tx.execute_batch(
            "ALTER TABLE service_new_version_discoveries ADD COLUMN image_ref TEXT NOT NULL DEFAULT ''",
        )?;
    }

    tx.execute_batch(
        r#"
DROP INDEX IF EXISTS idx_service_new_version_discoveries_unique_candidate;
CREATE UNIQUE INDEX IF NOT EXISTS idx_service_new_version_discoveries_unique_candidate
  ON service_new_version_discoveries(
    service_id,
    image_ref,
    current_digest,
    current_display_tag,
    current_tag,
    candidate_tag,
    candidate_digest,
    candidate_display_tag
  );
DELETE FROM service_new_version_discoveries;
"#,
    )?;
    new_version_discoveries::backfill_new_version_discoveries_from_successful_checks_conn(&tx)?;
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

pub(super) const SCHEMA: &str = r#"
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
  current_runtime_started_at TEXT,
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
  repo_url TEXT,
  homepage_json TEXT,
  update_guard_json TEXT,
  repo_url_auto_disabled INTEGER NOT NULL DEFAULT 0,
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
  resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 10,
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

CREATE TABLE IF NOT EXISTS github_packages_delivery_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  delivery_id TEXT NOT NULL REFERENCES github_packages_deliveries(delivery_id) ON DELETE CASCADE,
  received_at TEXT NOT NULL,
  payload_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_github_packages_delivery_events_delivery_id
  ON github_packages_delivery_events(delivery_id, id DESC);

CREATE TABLE IF NOT EXISTS new_version_notifications (
  id TEXT PRIMARY KEY NOT NULL,
  service_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  image_tag TEXT NOT NULL,
  current_tag TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  status TEXT NOT NULL,
  sent_channels_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  sent_at TEXT,
  superseded_at TEXT,
  last_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_new_version_notifications_service_status
  ON new_version_notifications(service_id, status, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_new_version_notifications_active_service_digest
  ON new_version_notifications(service_id, candidate_digest)
  WHERE status IN ('pending', 'sent');

CREATE TABLE IF NOT EXISTS service_new_version_discoveries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL,
  image_ref TEXT NOT NULL DEFAULT '',
  source_job_id TEXT NOT NULL,
  discovered_at TEXT NOT NULL,
  current_digest TEXT NOT NULL DEFAULT '',
  current_display_tag TEXT NOT NULL DEFAULT '',
  current_tag TEXT NOT NULL DEFAULT '',
  candidate_tag TEXT NOT NULL DEFAULT '',
  candidate_digest TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_service_new_version_discoveries_unique_candidate
  ON service_new_version_discoveries(
    service_id,
    image_ref,
    current_digest,
    current_display_tag,
    current_tag,
    candidate_tag,
    candidate_digest,
    candidate_display_tag
  );
CREATE INDEX IF NOT EXISTS idx_service_new_version_discoveries_service_discovered_at
  ON service_new_version_discoveries(service_id, discovered_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS auto_update_policies (
  scope_type TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  mode TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  rules_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (scope_type, scope_id)
);
CREATE INDEX IF NOT EXISTS idx_auto_update_policies_scope
  ON auto_update_policies(scope_type, scope_id);

CREATE TABLE IF NOT EXISTS auto_update_pending (
  id TEXT PRIMARY KEY NOT NULL,
  policy_scope_type TEXT NOT NULL,
  policy_scope_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  source_check_job_id TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  due_at TEXT NOT NULL,
  min_age_seconds INTEGER NOT NULL,
  min_version_lag INTEGER NOT NULL,
  status TEXT NOT NULL,
  update_job_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  summary_json TEXT NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_auto_update_pending_active_candidate
  ON auto_update_pending(service_id, rule_id, candidate_digest)
  WHERE status IN ('pending', 'enqueuing', 'enqueued');
CREATE INDEX IF NOT EXISTS idx_auto_update_pending_due
  ON auto_update_pending(status, due_at);

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
