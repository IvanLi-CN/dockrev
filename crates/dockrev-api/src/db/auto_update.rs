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
    let summary_json: String = row.get(18)?;
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
        candidate_id: row.get(17)?,
        summary_json,
    })
}

fn map_auto_update_candidate_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutoUpdateCandidateRow> {
    Ok(AutoUpdateCandidateRow {
        id: row.get(0)?,
        stack_id: row.get(1)?,
        service_id: row.get(2)?,
        image_ref: row.get(3)?,
        raw_tag: row.get(4)?,
        candidate_digest: row.get(5)?,
        resolved_version: row.get(6)?,
        status: row.get(7)?,
        reason: row.get(8)?,
        attempts: row.get::<_, i64>(9)?.max(0) as u32,
        retry_at: row.get(10)?,
        discovered_at: row.get(11)?,
        source_job_id: row.get(12)?,
        current_tag: row.get(13)?,
        current_display_tag: row.get(14)?,
        current_digest: row.get(15)?,
        settled_at: row.get(16)?,
        updated_at: row.get(18)?,
        policy_status: row.get(19)?,
        policy_reason: row.get(20)?,
        policy_rule_id: row.get(21)?,
        policy_evaluated_at: row.get(22)?,
    })
}

const AUTO_UPDATE_CANDIDATE_COLUMNS: &str = "id, stack_id, service_id, image_ref, raw_tag, candidate_digest, resolved_version, status, reason, attempts, retry_at, discovered_at, source_job_id, current_tag, current_display_tag, current_digest, settled_at, created_at, updated_at, policy_status, policy_reason, policy_rule_id, policy_evaluated_at";

impl Db {
    pub async fn upsert_auto_update_candidate(
        &self,
        input: &AutoUpdateCandidateInput,
        now: &str,
    ) -> anyhow::Result<AutoUpdateCandidateRow> {
        let input = input.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO auto_update_candidates (
  id, stack_id, service_id, image_ref, raw_tag, candidate_digest,
  resolved_version, status, reason, attempts, retry_at, discovered_at,
  source_job_id, current_tag, current_display_tag, current_digest,
  created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
ON CONFLICT(service_id, candidate_digest) DO UPDATE SET
  stack_id = excluded.stack_id,
  image_ref = excluded.image_ref,
  raw_tag = excluded.raw_tag,
  resolved_version = COALESCE(excluded.resolved_version, auto_update_candidates.resolved_version),
  status = CASE
    WHEN auto_update_candidates.status = 'superseded'
      THEN auto_update_candidates.status
    WHEN auto_update_candidates.status = 'ready'
      AND excluded.status <> 'ready'
      THEN auto_update_candidates.status
    WHEN auto_update_candidates.status = 'unresolved'
      AND excluded.status = 'awaiting_inference'
      THEN auto_update_candidates.status
    ELSE excluded.status
  END,
  reason = COALESCE(excluded.reason, auto_update_candidates.reason),
  retry_at = COALESCE(excluded.retry_at, auto_update_candidates.retry_at),
  current_tag = excluded.current_tag,
  current_display_tag = excluded.current_display_tag,
  current_digest = excluded.current_digest,
  updated_at = excluded.updated_at
"#,
                params![
                    input.id,
                    input.stack_id,
                    input.service_id,
                    input.image_ref,
                    input.raw_tag,
                    input.candidate_digest,
                    input.resolved_version,
                    input.status,
                    input.reason,
                    input.attempts as i64,
                    input.retry_at,
                    input.discovered_at,
                    input.source_job_id,
                    input.current_tag,
                    input.current_display_tag,
                    input.current_digest,
                    now,
                    now,
                ],
            )?;
            Ok(conn.query_row(
                &format!("SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE service_id = ?1 AND candidate_digest = ?2"),
                params![input.service_id, input.candidate_digest],
                map_auto_update_candidate_row,
            )?)
        })
        .await
        .context("upsert auto update candidate")
    }

    pub async fn get_auto_update_candidate(
        &self,
        service_id: &str,
        candidate_digest: &str,
    ) -> anyhow::Result<Option<AutoUpdateCandidateRow>> {
        let service_id = service_id.to_string();
        let candidate_digest = candidate_digest.to_string();
        self.call(move |conn| {
            Ok(conn.query_row(
                &format!("SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE service_id = ?1 AND candidate_digest = ?2"),
                params![service_id, candidate_digest],
                map_auto_update_candidate_row,
            )
            .optional()?)
        })
        .await
        .context("get auto update candidate")
    }

    pub async fn settle_auto_update_candidate(
        &self,
        service_id: &str,
        candidate_digest: &str,
        status: &str,
        resolved_version: Option<&str>,
        reason: Option<&str>,
        attempts: u32,
        retry_at: Option<&str>,
        settled_at: Option<&str>,
        now: &str,
    ) -> anyhow::Result<Option<AutoUpdateCandidateRow>> {
        let service_id = service_id.to_string();
        let candidate_digest = candidate_digest.to_string();
        let status = status.to_string();
        let resolved_version = resolved_version.map(str::to_string);
        let reason = reason.map(str::to_string);
        let retry_at = retry_at.map(str::to_string);
        let settled_at = settled_at.map(str::to_string);
        let now = now.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE auto_update_candidates
SET status = ?3,
    resolved_version = COALESCE(?4, resolved_version),
    reason = ?5,
    attempts = ?6,
    retry_at = ?7,
    settled_at = ?8,
    updated_at = ?9
WHERE service_id = ?1 AND candidate_digest = ?2
  AND status <> 'superseded'
  AND (
    status IS NOT ?3
    OR resolved_version IS NOT COALESCE(?4, resolved_version)
    OR reason IS NOT ?5
    OR attempts IS NOT ?6
    OR retry_at IS NOT ?7
    OR settled_at IS NOT ?8
  )
"#,
                params![service_id, candidate_digest, status, resolved_version, reason, attempts as i64, retry_at, settled_at, now],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            Ok(conn.query_row(
                &format!("SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE service_id = ?1 AND candidate_digest = ?2"),
                params![service_id, candidate_digest],
                map_auto_update_candidate_row,
            )
            .optional()?)
        })
        .await
        .context("settle auto update candidate")
    }

    pub async fn supersede_auto_update_candidates(
        &self,
        service_id: &str,
        candidate_digest: &str,
        discovered_at: &str,
        candidate_id: &str,
        now: &str,
    ) -> anyhow::Result<usize> {
        let service_id = service_id.to_string();
        let candidate_digest = candidate_digest.to_string();
        let discovered_at = discovered_at.to_string();
        let candidate_id = candidate_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let superseded = tx.execute(
                r#"
UPDATE auto_update_candidates
SET status = 'superseded',
    reason = 'newer_candidate',
    settled_at = ?3,
    policy_status = 'skipped',
    policy_reason = 'candidate_superseded',
    policy_evaluated_at = ?3,
    updated_at = ?3
WHERE service_id = ?1 AND candidate_digest <> ?2
  AND status IN ('awaiting_inference', 'ready', 'unresolved')
  AND (discovered_at < ?4 OR (discovered_at = ?4 AND id < ?5))
"#,
                params![service_id, candidate_digest, now, discovered_at, candidate_id],
            )?;

            let pending_ids = {
                let mut stmt = tx.prepare(r#"
SELECT p.id
FROM auto_update_pending p
JOIN auto_update_candidates c
  ON c.service_id = p.service_id AND c.candidate_digest = p.candidate_digest
WHERE p.service_id = ?1
  AND p.candidate_digest <> ?2
  AND p.status IN ('pending', 'enqueuing')
  AND c.status = 'superseded'
  AND (c.discovered_at < ?3 OR (c.discovered_at = ?3 AND c.id < ?4))
"#)?;
                stmt.query_map(
                    params![service_id, candidate_digest, discovered_at, candidate_id],
                    |row| row.get::<_, String>(0),
                )?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for pending_id in pending_ids {
                let mut summary = tx
                    .query_row(
                        "SELECT summary_json FROM auto_update_pending WHERE id = ?1",
                        params![pending_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                if !summary.is_object() {
                    summary = serde_json::json!({});
                }
                if let Some(object) = summary.as_object_mut() {
                    object.insert("skipReason".to_string(), serde_json::json!("candidate_superseded"));
                    object.insert("skippedAt".to_string(), serde_json::json!(&now));
                }
                tx.execute(
                    "UPDATE auto_update_pending SET status = 'skipped', summary_json = ?2, updated_at = ?3 WHERE id = ?1",
                    params![pending_id, serde_json::to_string(&summary)?, now],
                )?;
            }
            tx.commit()?;
            Ok(superseded)
        })
        .await
        .context("supersede auto update candidates")
    }

    pub async fn list_auto_update_candidates_for_retry(
        &self,
        now: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        let now = now.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE status = 'awaiting_inference' AND (retry_at IS NULL OR retry_at <= ?1) ORDER BY discovered_at ASC LIMIT ?2"
            ))?;
            let rows = stmt.query_map(params![now, limit as i64], map_auto_update_candidate_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list auto update candidates for retry")
    }

    pub async fn list_auto_update_candidates_for_events(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE status = 'awaiting_inference' ORDER BY discovered_at ASC LIMIT ?1"
            ))?;
            let rows = stmt.query_map(params![limit as i64], map_auto_update_candidate_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list auto update candidates for events")
    }

    pub async fn list_latest_auto_update_candidates(
        &self,
        service_ids: &[String],
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            let mut out = Vec::new();
            let mut stmt = conn.prepare(&format!(
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE service_id = ?1 ORDER BY discovered_at DESC, id DESC LIMIT 1"
            ))?;
            for service_id in service_ids {
                if let Ok(row) = stmt.query_row(params![service_id], map_auto_update_candidate_row) {
                    out.push(row);
                }
            }
            Ok(out)
        })
        .await
        .context("list latest auto update candidates")
    }

    pub async fn list_auto_update_candidates_for_digest(
        &self,
        image_ref: &str,
        candidate_digest: &str,
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        let image_ref = image_ref.to_string();
        let candidate_digest = candidate_digest.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE image_ref = ?1 AND candidate_digest = ?2 AND status = 'awaiting_inference' ORDER BY discovered_at ASC"
            ))?;
            let rows = stmt.query_map(params![image_ref, candidate_digest], map_auto_update_candidate_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list auto update candidates for digest")
    }

    pub async fn reopen_auto_update_candidate_inference(
        &self,
        service_id: &str,
        candidate_digest: &str,
        reason: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let candidate_digest = candidate_digest.to_string();
        let reason = reason.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
UPDATE auto_update_candidates
SET status = 'awaiting_inference',
    reason = ?3,
    attempts = 0,
    retry_at = NULL,
    settled_at = NULL,
    policy_status = 'waiting_inference',
    policy_reason = ?3,
    policy_rule_id = NULL,
    policy_evaluated_at = ?4,
    updated_at = ?4
WHERE service_id = ?1
  AND candidate_digest = ?2
  AND status = 'unresolved'
"#,
                params![service_id, candidate_digest, reason, now],
            )? > 0)
        })
        .await
        .context("reopen auto update candidate inference")
    }

    pub async fn set_auto_update_candidate_policy(
        &self,
        service_id: &str,
        candidate_digest: &str,
        policy_status: &str,
        policy_reason: Option<&str>,
        rule_id: Option<&str>,
        evaluated_at: &str,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        let candidate_digest = candidate_digest.to_string();
        let policy_status = policy_status.to_string();
        let policy_reason = policy_reason.map(str::to_string);
        let rule_id = rule_id.map(str::to_string);
        let evaluated_at = evaluated_at.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE auto_update_candidates
SET policy_status = ?3,
    policy_reason = ?4,
    policy_rule_id = ?5,
    policy_evaluated_at = ?6,
    updated_at = ?6
WHERE service_id = ?1 AND candidate_digest = ?2
  AND status <> 'superseded'
"#,
                params![
                    service_id,
                    candidate_digest,
                    policy_status,
                    policy_reason,
                    rule_id,
                    evaluated_at
                ],
            )?;
            Ok(())
        })
        .await
        .context("set auto update candidate policy")
    }

    /// Reconciles the policy projection with the update job linked by the pending action.
    /// The update job is the execution authority; the candidate projection is only a read model.
    pub async fn sync_auto_update_candidate_policy_for_job(
        &self,
        update_job_id: &str,
        now: &str,
    ) -> anyhow::Result<usize> {
        let update_job_id = update_job_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut stmt = tx.prepare(
                r#"
SELECT p.service_id, p.candidate_digest, p.status, j.status, j.started_at, j.finished_at
FROM auto_update_pending p
JOIN jobs j ON j.id = p.update_job_id
WHERE p.update_job_id = ?1
"#,
            )?;
            let rows = stmt
                .query_map(params![update_job_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            let mut changed = 0;
            for (
                service_id,
                candidate_digest,
                pending_status,
                job_status,
                started_at,
                finished_at,
            ) in rows
            {
                let projection = match (pending_status.as_str(), job_status.as_str()) {
                    ("skipped", _) | (_, "cancelled") => ("skipped", "update_job_cancelled"),
                    (_, "success") => ("completed", "update_job_completed"),
                    (_, "failed") => ("failed", "update_job_failed"),
                    (_, "rolled_back") => ("failed", "update_job_rolled_back"),
                    (_, "running") => ("running", "update_job_running"),
                    (_, "queued") => ("queued", "update_job_queued"),
                    _ => continue,
                };
                let evaluated_at = finished_at.or(started_at).unwrap_or_else(|| now.clone());
                changed += tx.execute(
                    r#"
UPDATE auto_update_candidates
SET policy_status = ?3,
    policy_reason = ?4,
    policy_evaluated_at = ?5,
    updated_at = ?5
WHERE service_id = ?1
  AND candidate_digest = ?2
  AND status <> 'superseded'
"#,
                    params![
                        service_id,
                        candidate_digest,
                        projection.0,
                        projection.1,
                        evaluated_at
                    ],
                )?;
            }
            tx.commit()?;
            Ok(changed)
        })
        .await
        .context("sync auto update candidate policy for job")
    }

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
            let stale_rows = {
                let mut stmt = tx.prepare(
                    r#"
SELECT id, summary_json
FROM auto_update_pending
WHERE service_id = ?1 AND rule_id = ?2 AND candidate_digest = ?3
  AND status IN ('pending', 'enqueuing', 'enqueued')
  AND (policy_scope_type != ?4 OR policy_scope_id != ?5)
"#,
                )?;
                let rows = stmt.query_map(
                    params![
                        input.service_id,
                        input.rule_id,
                        input.candidate_digest,
                        input.policy_scope_type,
                        input.policy_scope_id,
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for (pending_id, summary_raw) in stale_rows {
                let mut summary = serde_json::from_str::<serde_json::Value>(&summary_raw)
                    .unwrap_or_else(|_| serde_json::json!({}));
                if !summary.is_object() {
                    summary = serde_json::json!({});
                }
                if let Some(obj) = summary.as_object_mut() {
                    obj.insert("skipReason".to_string(), serde_json::json!("policy_changed"));
                    obj.insert("skippedAt".to_string(), serde_json::json!(now));
                }
                tx.execute(
                    r#"
UPDATE auto_update_pending
SET status = 'skipped', summary_json = ?2, updated_at = ?3
WHERE id = ?1
"#,
                    params![pending_id, serde_json::to_string(&summary)?, now],
                )?;
            }
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
  candidate_id,
  summary_json
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 'pending', ?16, ?17, ?18, ?19)
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
                    input.candidate_id,
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
  candidate_id,
  summary_json
FROM auto_update_pending
WHERE service_id = ?1 AND rule_id = ?2 AND candidate_digest = ?3
  AND policy_scope_type = ?4 AND policy_scope_id = ?5
  AND status IN ('pending', 'enqueuing', 'enqueued')
ORDER BY created_at ASC, id ASC
LIMIT 1
"#,
                params![
                    input.service_id,
                    input.rule_id,
                    input.candidate_digest,
                    input.policy_scope_type,
                    input.policy_scope_id,
                ],
                map_auto_update_pending_row,
            )?;
            tx.commit()?;
            Ok(row)
        })
        .await
        .context("reserve auto update pending")
    }

    pub async fn list_auto_update_pending_candidates(
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
  candidate_id,
  summary_json
FROM auto_update_pending
WHERE status = 'pending'
ORDER BY
  CASE WHEN due_at <= ?1 THEN 0 ELSE 1 END ASC,
  due_at ASC,
  created_at ASC
LIMIT ?2
"#,
            )?;
            let rows = stmt.query_map(params![now, limit as i64], map_auto_update_pending_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list auto update pending candidates")
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
        let db_update_job_id = update_job_id.clone();
        let db_now = now.clone();
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
        .context("mark auto update pending enqueued")?;
        self.sync_auto_update_candidate_policy_for_job(&db_update_job_id, &db_now)
            .await?;
        Ok(())
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
            conn.execute(
                r#"
UPDATE auto_update_candidates
SET policy_status = 'skipped',
    policy_reason = ?2,
    policy_evaluated_at = ?3,
    updated_at = ?3
WHERE status <> 'superseded'
  AND EXISTS (
    SELECT 1
    FROM auto_update_pending p
    WHERE p.id = ?1
      AND p.service_id = auto_update_candidates.service_id
      AND p.candidate_digest = auto_update_candidates.candidate_digest
  )
"#,
                params![pending_id, reason, now],
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn candidate_input(
        id: &str,
        digest: &str,
        status: &str,
        discovered_at: &str,
    ) -> AutoUpdateCandidateInput {
        AutoUpdateCandidateInput {
            id: id.to_string(),
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            image_ref: "ghcr.io/acme/app".to_string(),
            raw_tag: "latest".to_string(),
            candidate_digest: digest.to_string(),
            resolved_version: (status == "ready").then(|| "1.4.0".to_string()),
            status: status.to_string(),
            reason: Some("test".to_string()),
            attempts: 0,
            retry_at: None,
            discovered_at: discovered_at.to_string(),
            source_job_id: "check".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
        }
    }

    #[tokio::test]
    async fn candidate_settlement_is_unique_and_idempotent() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let input = candidate_input(
            "candidate-1",
            "sha256:new",
            "awaiting_inference",
            "2026-04-30T00:00:00Z",
        );
        let first = db
            .upsert_auto_update_candidate(&input, "2026-04-30T00:00:00Z")
            .await
            .unwrap();
        let second = db
            .upsert_auto_update_candidate(&input, "2026-04-30T00:01:00Z")
            .await
            .unwrap();
        assert_eq!(first.id, second.id);

        let settled = db
            .settle_auto_update_candidate(
                "service",
                "sha256:new",
                "ready",
                Some("1.4.0"),
                Some("digest_bound_version"),
                0,
                None,
                Some("2026-04-30T00:02:00Z"),
                "2026-04-30T00:02:00Z",
            )
            .await
            .unwrap()
            .expect("first settlement changes the row");
        assert_eq!(settled.status, "ready");
        assert_eq!(settled.resolved_version.as_deref(), Some("1.4.0"));

        let repeated = db
            .settle_auto_update_candidate(
                "service",
                "sha256:new",
                "ready",
                Some("1.4.0"),
                Some("digest_bound_version"),
                0,
                None,
                Some("2026-04-30T00:02:00Z"),
                "2026-04-30T00:03:00Z",
            )
            .await
            .unwrap();
        assert!(repeated.is_none(), "repeated settlement must be a no-op");
    }

    #[tokio::test]
    async fn newer_candidate_supersedes_old_candidate_and_pending_action() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let old = db
            .upsert_auto_update_candidate(
                &candidate_input(
                    "candidate-old",
                    "sha256:old-candidate",
                    "ready",
                    "2026-04-30T00:00:00Z",
                ),
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        let new = db
            .upsert_auto_update_candidate(
                &candidate_input(
                    "candidate-new",
                    "sha256:new-candidate",
                    "ready",
                    "2026-04-30T01:00:00Z",
                ),
                "2026-04-30T01:00:00Z",
            )
            .await
            .unwrap();

        let pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: "pending-old".to_string(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule".to_string(),
                    stack_id: "stack".to_string(),
                    service_id: "service".to_string(),
                    source_check_job_id: "check".to_string(),
                    candidate_tag: "latest".to_string(),
                    candidate_display_tag: "1.3.0".to_string(),
                    candidate_digest: "sha256:old-candidate".to_string(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({}),
                    candidate_id: Some(old.id.clone()),
                },
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        assert_eq!(pending.status, "pending");

        db.supersede_auto_update_candidates(
            "service",
            "sha256:new-candidate",
            &new.discovered_at,
            &new.id,
            "2026-04-30T01:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(
            db.get_auto_update_candidate("service", "sha256:old-candidate")
                .await
                .unwrap()
                .unwrap()
                .status,
            "superseded"
        );
        assert!(
            db.list_auto_update_pending_candidates("2026-04-30T02:00:00Z", 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unresolved_candidate_can_be_reopened_for_force_inference() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.upsert_auto_update_candidate(
            &candidate_input(
                "candidate-unresolved",
                "sha256:unresolved",
                "unresolved",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            db.reopen_auto_update_candidate_inference(
                "service",
                "sha256:unresolved",
                "force",
                "2026-04-30T01:00:00Z",
            )
            .await
            .unwrap()
        );
        let candidate = db
            .get_auto_update_candidate("service", "sha256:unresolved")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(candidate.status, "awaiting_inference");
        assert_eq!(candidate.attempts, 0);
        assert_eq!(candidate.retry_at, None);
        assert_eq!(
            candidate.policy_status.as_deref(),
            Some("waiting_inference")
        );
    }
}
