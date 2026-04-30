use super::*;

fn map_serde_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

pub(super) fn auto_update_policy_from_row(
    mode: Option<String>,
    enabled: Option<i64>,
    rules_json: Option<String>,
    updated_at: Option<String>,
    default_mode: crate::api::types::AutoUpdatePolicyMode,
) -> rusqlite::Result<crate::api::types::AutoUpdatePolicy> {
    let rules = rules_json
        .as_deref()
        .filter(|raw| !raw.trim().is_empty())
        .map(serde_json::from_str::<Vec<crate::api::types::AutoUpdateRule>>)
        .transpose()
        .map_err(map_serde_error)?
        .unwrap_or_default();
    Ok(crate::api::types::AutoUpdatePolicy {
        mode: mode
            .as_deref()
            .map(crate::api::types::AutoUpdatePolicyMode::from_str)
            .unwrap_or(default_mode),
        enabled: enabled.unwrap_or_default() != 0,
        rules,
        updated_at,
    })
}

fn map_auto_update_pending_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutoUpdatePendingRow> {
    let summary_json: String = row.get(17)?;
    let summary_json = serde_json::from_str(&summary_json).map_err(map_serde_error)?;
    Ok(AutoUpdatePendingRow {
        id: row.get(0)?,
        policy_scope_type: row.get(1)?,
        policy_scope_id: row.get(2)?,
        rule_id: row.get(3)?,
        stack_id: row.get(4)?,
        service_id: row.get(5)?,
        source_check_job_id: row.get(6)?,
        candidate_tag: row.get(7)?,
        candidate_display_tag: row.get(8)?,
        candidate_digest: row.get(9)?,
        current_display_tag: row.get(10)?,
        first_seen_at: row.get(11)?,
        due_at: row.get(12)?,
        min_age_seconds: row.get::<_, i64>(13)?.max(0) as u32,
        min_version_lag: row.get::<_, i64>(14)?.max(0) as u32,
        status: row.get(15)?,
        update_job_id: row.get(16)?,
        summary_json,
    })
}

impl Db {
    pub async fn get_auto_update_policy(
        &self,
        scope_type: &str,
        scope_id: &str,
        default_mode: crate::api::types::AutoUpdatePolicyMode,
    ) -> anyhow::Result<crate::api::types::AutoUpdatePolicy> {
        let scope_type = scope_type.to_string();
        let scope_id = scope_id.to_string();
        self.call(move |conn| {
            let row = conn
                .query_row(
                    r#"
SELECT mode, enabled, rules_json, updated_at
FROM auto_update_policies
WHERE scope_type = ?1 AND scope_id = ?2
"#,
                    params![scope_type, scope_id],
                    |row| {
                        auto_update_policy_from_row(
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            default_mode.clone(),
                        )
                    },
                )
                .optional()?;
            Ok(row.unwrap_or(crate::api::types::AutoUpdatePolicy {
                mode: default_mode,
                enabled: false,
                rules: Vec::new(),
                updated_at: None,
            }))
        })
        .await
        .context("get auto update policy")
    }

    pub async fn put_auto_update_policy(
        &self,
        scope_type: &str,
        scope_id: &str,
        policy: &crate::api::types::AutoUpdatePolicy,
        now: &str,
    ) -> anyhow::Result<()> {
        let scope_type = scope_type.to_string();
        let scope_id = scope_id.to_string();
        let policy = policy.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO auto_update_policies (
  scope_type,
  scope_id,
  mode,
  enabled,
  rules_json,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(scope_type, scope_id) DO UPDATE SET
  mode = excluded.mode,
  enabled = excluded.enabled,
  rules_json = excluded.rules_json,
  updated_at = excluded.updated_at
"#,
                params![
                    scope_type,
                    scope_id,
                    policy.mode.as_str(),
                    policy.enabled as i64,
                    serde_json::to_string(&policy.rules)?,
                    now,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put auto update policy")
    }

    pub async fn reserve_auto_update_pending(
        &self,
        input: &AutoUpdatePendingInput,
        now: &str,
    ) -> anyhow::Result<AutoUpdatePendingRow> {
        let input = input.clone();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                r#"
INSERT OR IGNORE INTO auto_update_pending (
  id,
  policy_scope_type,
  policy_scope_id,
  rule_id,
  stack_id,
  service_id,
  source_check_job_id,
  candidate_tag,
  candidate_display_tag,
  candidate_digest,
  current_display_tag,
  first_seen_at,
  due_at,
  min_age_seconds,
  min_version_lag,
  status,
  created_at,
  updated_at,
  summary_json
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 'pending', ?16, ?17, ?18)
"#,
                params![
                    input.id,
                    input.policy_scope_type,
                    input.policy_scope_id,
                    input.rule_id,
                    input.stack_id,
                    input.service_id,
                    input.source_check_job_id,
                    input.candidate_tag,
                    input.candidate_display_tag,
                    input.candidate_digest,
                    input.current_display_tag,
                    input.first_seen_at,
                    input.due_at,
                    input.min_age_seconds as i64,
                    input.min_version_lag as i64,
                    now,
                    now,
                    serde_json::to_string(&input.summary_json)?
                ],
            )?;
            let row = tx.query_row(
                r#"
SELECT
  id,
  policy_scope_type,
  policy_scope_id,
  rule_id,
  stack_id,
  service_id,
  source_check_job_id,
  candidate_tag,
  candidate_display_tag,
  candidate_digest,
  current_display_tag,
  first_seen_at,
  due_at,
  min_age_seconds,
  min_version_lag,
  status,
  update_job_id,
  summary_json
FROM auto_update_pending
WHERE service_id = ?1 AND rule_id = ?2 AND candidate_digest = ?3
  AND status IN ('pending', 'enqueuing', 'enqueued')
ORDER BY created_at ASC, id ASC
LIMIT 1
"#,
                params![
                    input.service_id,
                    input.rule_id,
                    input.candidate_digest
                ],
                map_auto_update_pending_row,
            )?;
            tx.commit()?;
            Ok(row)
        })
        .await
        .context("reserve auto update pending")
    }

    pub async fn list_due_auto_update_pending(
        &self,
        now: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<AutoUpdatePendingRow>> {
        let now = now.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  policy_scope_type,
  policy_scope_id,
  rule_id,
  stack_id,
  service_id,
  source_check_job_id,
  candidate_tag,
  candidate_display_tag,
  candidate_digest,
  current_display_tag,
  first_seen_at,
  due_at,
  min_age_seconds,
  min_version_lag,
  status,
  update_job_id,
  summary_json
FROM auto_update_pending
WHERE status = 'pending' AND due_at <= ?1
ORDER BY due_at ASC, created_at ASC
LIMIT ?2
"#,
            )?;
            let rows = stmt.query_map(params![now, limit as i64], map_auto_update_pending_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list due auto update pending")
    }

    pub async fn try_claim_auto_update_pending(
        &self,
        pending_id: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let pending_id = pending_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
UPDATE auto_update_pending
SET status = 'enqueuing', updated_at = ?2
WHERE id = ?1 AND status = 'pending'
"#,
                params![pending_id, now],
            )? > 0)
        })
        .await
        .context("claim auto update pending")
    }

    pub async fn mark_auto_update_pending_enqueued(
        &self,
        pending_id: &str,
        update_job_id: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let pending_id = pending_id.to_string();
        let update_job_id = update_job_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE auto_update_pending
SET status = 'enqueued', update_job_id = ?2, updated_at = ?3
WHERE id = ?1
"#,
                params![pending_id, update_job_id, now],
            )?;
            Ok(())
        })
        .await
        .context("mark auto update pending enqueued")
    }

    pub async fn mark_auto_update_pending_skipped(
        &self,
        pending_id: &str,
        reason: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let pending_id = pending_id.to_string();
        let reason = reason.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let mut summary = conn
                .query_row(
                    "SELECT summary_json FROM auto_update_pending WHERE id = ?1",
                    params![&pending_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            if !summary.is_object() {
                summary = serde_json::json!({});
            }
            if let Some(obj) = summary.as_object_mut() {
                obj.insert("skipReason".to_string(), serde_json::json!(reason));
                obj.insert("skippedAt".to_string(), serde_json::json!(now));
            }
            conn.execute(
                r#"
UPDATE auto_update_pending
SET status = 'skipped', summary_json = ?2, updated_at = ?3
WHERE id = ?1
"#,
                params![pending_id, serde_json::to_string(&summary)?, now],
            )?;
            Ok(())
        })
        .await
        .context("mark auto update pending skipped")
    }

    pub async fn release_auto_update_pending_claim(
        &self,
        pending_id: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let pending_id = pending_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE auto_update_pending
SET status = 'pending', updated_at = ?2
WHERE id = ?1 AND status = 'enqueuing'
"#,
                params![pending_id, now],
            )?;
            Ok(())
        })
        .await
        .context("release auto update pending claim")
    }
}
