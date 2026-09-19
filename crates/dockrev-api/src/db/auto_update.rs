use super::*;

fn map_serde_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

include!("auto_update_row_mappers.rs");

const AUTO_UPDATE_CANDIDATE_COLUMNS: &str = "id, stack_id, service_id, image_ref, raw_tag, candidate_digest, resolved_version, resolved_tags, status, reason, attempts, retry_at, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, settled_at, created_at, updated_at, policy_status, policy_reason, policy_rule_id, policy_evaluated_at, policy_scope_type, policy_scope_id, update_job_id, last_error, superseded_at, superseded_by_candidate_id, hydration_origin, settlement_generation";
const AUTO_UPDATE_CANDIDATE_COLUMNS_QUALIFIED: &str = "c.id, c.stack_id, c.service_id, c.image_ref, c.raw_tag, c.candidate_digest, c.resolved_version, c.resolved_tags, c.status, c.reason, c.attempts, c.retry_at, c.discovered_at, c.source_job_id, c.source, c.current_tag, c.current_display_tag, c.current_digest, c.settled_at, c.created_at, c.updated_at, c.policy_status, c.policy_reason, c.policy_rule_id, c.policy_evaluated_at, c.policy_scope_type, c.policy_scope_id, c.update_job_id, c.last_error, c.superseded_at, c.superseded_by_candidate_id, c.hydration_origin, c.settlement_generation";

include!("auto_update_candidates.rs");

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
        let mut input = input.clone();
        input.candidate_digest = canonical_auto_update_digest(&input.candidate_digest);
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let stale_rows = {
                let mut stmt = tx.prepare(
                    r#"
SELECT id, update_job_id, summary_json
FROM auto_update_pending
WHERE service_id = ?1 AND rule_id = ?2 AND candidate_digest = ?3
  AND status IN ('pending', 'enqueuing', 'enqueued')
  AND (LOWER(TRIM(policy_scope_type)) != LOWER(TRIM(?4)) OR LOWER(TRIM(policy_scope_id)) != LOWER(TRIM(?5)))
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
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for (pending_id, update_job_id, summary_raw) in stale_rows {
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
                if let Some(update_job_id) = update_job_id {
                    cancel_stale_auto_policy_job(&tx, &update_job_id, &now)?;
                }
            }
            tx.execute(
                r#"
UPDATE auto_update_pending
SET source_check_job_id = ?6,
    candidate_tag = ?7,
    candidate_display_tag = ?8,
    current_display_tag = ?9,
    current_digest = COALESCE(NULLIF(TRIM(json_extract(
      CASE WHEN json_valid(?13) THEN ?13 ELSE '{}' END,
      '$.currentDigest'
    )), ''), current_digest),
    due_at = ?10,
    min_age_seconds = ?11,
    min_version_lag = ?12,
    summary_json = ?13,
    updated_at = ?14,
    candidate_id = CASE
      WHEN ?15 IS NULL OR EXISTS (
        SELECT 1 FROM auto_update_candidates c
        WHERE c.id = ?15 AND c.service_id = ?4 AND c.candidate_digest = ?5
      ) THEN ?15
      ELSE NULL
    END
WHERE service_id = ?4
  AND rule_id = ?2
  AND candidate_digest = ?5
  AND LOWER(TRIM(policy_scope_type)) = LOWER(TRIM(?1)) AND LOWER(TRIM(policy_scope_id)) = LOWER(TRIM(?3))
  AND status = 'pending'
"#,
                params![
                    input.policy_scope_type,
                    input.rule_id,
                    input.policy_scope_id,
                    input.service_id,
                    input.candidate_digest,
                    input.source_check_job_id,
                    input.candidate_tag,
                    input.candidate_display_tag,
                    input.current_display_tag,
                    input.due_at,
                    input.min_age_seconds as i64,
                    input.min_version_lag as i64,
                    serde_json::to_string(&input.summary_json)?,
                    now,
                    input.candidate_id,
                ],
            )?;
            insert_auto_update_pending_if_missing(&tx, &input, &now)?;
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
  AND LOWER(TRIM(policy_scope_type)) = LOWER(TRIM(?4)) AND LOWER(TRIM(policy_scope_id)) = LOWER(TRIM(?5))
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

    #[allow(dead_code)]
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

    #[allow(clippy::too_many_arguments)]
    pub async fn try_claim_auto_update_pending_if_current(
        &self,
        pending_id: &str,
        service_id: &str,
        candidate_digest: &str,
        policy_scope_type: &str,
        policy_scope_id: &str,
        rule_id: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let pending_id = pending_id.to_string();
        let service_id = service_id.to_string();
        let candidate_digest = canonical_auto_update_digest(candidate_digest);
        let policy_scope_type = policy_scope_type.to_string();
        let policy_scope_id = policy_scope_id.to_string();
        let rule_id = rule_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let pending_candidate_digest =
                super::strict_canonical_digest_sql("auto_update_pending.candidate_digest");
            let candidate_digest_expr = super::strict_canonical_digest_sql("c.candidate_digest");
            let service_candidate_digest = super::strict_canonical_digest_sql("s.candidate_digest");
            let expected_candidate_digest = super::strict_canonical_digest_sql("?3");
            let pending_current_digest = super::strict_canonical_digest_sql(
                "NULLIF(TRIM(COALESCE(NULLIF(TRIM(auto_update_pending.current_digest), ''), json_extract(CASE WHEN json_valid(auto_update_pending.summary_json) THEN auto_update_pending.summary_json ELSE '{}' END, '$.currentDigest'))), '')",
            );
            let service_current_digest = super::strict_canonical_digest_sql("s.current_digest");
            let sql = format!(
                r#"
UPDATE auto_update_pending
SET status = 'enqueuing', updated_at = ?7
WHERE id = ?1
  AND status = 'pending'
  AND service_id = ?2
  AND {pending_candidate_digest} = {expected_candidate_digest}
  AND LOWER(TRIM(policy_scope_type)) = LOWER(TRIM(?4))
  AND LOWER(TRIM(policy_scope_id)) = LOWER(TRIM(?5))
  AND rule_id = ?6
  AND EXISTS (
    SELECT 1
    FROM auto_update_candidates c
    WHERE c.service_id = ?2
      AND {candidate_digest_expr} = {expected_candidate_digest}
      AND c.status <> 'superseded'
      AND c.policy_status = 'delayed'
      AND c.policy_rule_id = ?6
      AND (auto_update_pending.candidate_id IS NULL OR c.id = auto_update_pending.candidate_id)
  )
  AND EXISTS (
    SELECT 1
    FROM services s
    WHERE s.id = ?2
      AND {service_candidate_digest} = {expected_candidate_digest}
      AND NULLIF(TRIM(s.current_digest), '') IS NOT NULL
      AND {pending_current_digest} = {service_current_digest}
  )
  AND EXISTS (
    SELECT 1
    FROM services s
    LEFT JOIN auto_update_policies service_policy
      ON LOWER(TRIM(service_policy.scope_type)) = 'service'
     AND LOWER(TRIM(service_policy.scope_id)) = LOWER(TRIM(s.id))
    LEFT JOIN auto_update_policies stack_policy
      ON LOWER(TRIM(stack_policy.scope_type)) = 'stack'
     AND LOWER(TRIM(stack_policy.scope_id)) = LOWER(TRIM(s.stack_id))
    WHERE s.id = ?2
      AND (
        (
          LOWER(TRIM(?4)) = 'service'
          AND LOWER(TRIM(?5)) = LOWER(TRIM(s.id))
          AND service_policy.mode = 'override'
          AND service_policy.enabled <> 0
          AND service_policy.updated_at = json_extract(auto_update_pending.summary_json, '$.policyUpdatedAt')
          AND EXISTS (
            SELECT 1
            FROM json_each(CASE WHEN json_valid(service_policy.rules_json)
              THEN service_policy.rules_json ELSE '[]' END) AS rule
            WHERE json_extract(rule.value, '$.id') = ?6
              AND json_extract(rule.value, '$.enabled') <> 0
          )
        )
        OR (
          LOWER(TRIM(?4)) = 'stack'
          AND LOWER(TRIM(?5)) = LOWER(TRIM(s.stack_id))
          AND COALESCE(service_policy.mode, 'inherit') = 'inherit'
          AND stack_policy.mode = 'override'
          AND stack_policy.enabled <> 0
          AND stack_policy.updated_at = json_extract(auto_update_pending.summary_json, '$.policyUpdatedAt')
          AND EXISTS (
            SELECT 1
            FROM json_each(CASE WHEN json_valid(stack_policy.rules_json)
              THEN stack_policy.rules_json ELSE '[]' END) AS rule
            WHERE json_extract(rule.value, '$.id') = ?6
              AND json_extract(rule.value, '$.enabled') <> 0
          )
        )
      )
  )
  AND EXISTS (
    SELECT 1
    FROM auto_update_candidates c
    JOIN jobs source_job ON source_job.id = auto_update_pending.source_check_job_id
    WHERE c.id = auto_update_pending.candidate_id
      AND LOWER(c.source) IN ('schedule', 'github_webhook')
      AND LOWER(source_job.type) = 'check'
      AND LOWER(source_job.status) = 'success'
      AND (
        (LOWER(c.source) = 'schedule'
          AND LOWER(source_job.reason) = 'schedule'
          AND LOWER(source_job.created_by) = 'schedule')
        OR (LOWER(c.source) = 'github_webhook'
          AND LOWER(source_job.created_by) IN ('webhook', 'github')
          AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
      )
      AND (
        (LOWER(source_job.scope) = 'service'
          AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(c.stack_id))
          AND LOWER(TRIM(source_job.service_id)) = LOWER(TRIM(c.service_id)))
        OR (LOWER(source_job.scope) = 'stack'
          AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(c.stack_id))
          AND source_job.service_id IS NULL)
        OR (LOWER(source_job.scope) = 'all'
          AND source_job.stack_id IS NULL
          AND source_job.service_id IS NULL)
      )
  )
  AND json_extract(auto_update_pending.summary_json, '$.policyUpdatedAt') IS NOT NULL
"#,
            );
            Ok(conn.execute(
                &sql,
                params![
                    pending_id,
                    service_id,
                    candidate_digest,
                    policy_scope_type,
                    policy_scope_id,
                    rule_id,
                    now
                ],
            )? > 0)
        })
        .await
        .context("claim current auto update pending")
    }

    pub async fn mark_auto_update_pending_enqueued(
        &self,
        pending_id: &str,
        update_job_id: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let pending_id = pending_id.to_string();
        let update_job_id = update_job_id.to_string();
        let now = now.to_string();
        let db_update_job_id = update_job_id.clone();
        let db_now = now.clone();
        let marked = self
            .call(move |conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let changed = tx.execute(
                    r#"
UPDATE auto_update_pending
SET status = 'enqueued', update_job_id = ?2, updated_at = ?3
WHERE id = ?1 AND status = 'enqueuing'
"#,
                    params![pending_id, update_job_id, now],
                )?;
                if changed == 0 {
                    tx.execute(
                        r#"
UPDATE jobs
SET status = 'cancelled', finished_at = ?2
WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'
"#,
                        params![update_job_id, now],
                    )?;
                    tx.execute(
                        r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT ?1, ?2, 'auto-policy-enqueue-race', ?2
WHERE EXISTS (
  SELECT 1 FROM jobs
  WHERE id = ?1 AND status = 'running' AND created_by = 'auto-policy'
)
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
                        params![update_job_id, now],
                    )?;
                }
                tx.commit()?;
                Ok(changed > 0)
            })
            .await
            .context("mark auto update pending enqueued")?;
        if marked {
            self.sync_auto_update_candidate_policy_for_job(&db_update_job_id, &db_now)
                .await?;
        }
        Ok(marked)
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
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let pending = tx
                .query_row(
                    "SELECT status, update_job_id, summary_json FROM auto_update_pending WHERE id = ?1",
                    params![&pending_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((status, update_job_id, summary_raw)) = pending else {
                tx.commit()?;
                return Ok(());
            };
            if !matches!(status.as_str(), "pending" | "enqueuing" | "enqueued") {
                tx.commit()?;
                return Ok(());
            }
            let mut summary = serde_json::from_str::<serde_json::Value>(&summary_raw)
                .unwrap_or_else(|_| serde_json::json!({}));
            if !summary.is_object() {
                summary = serde_json::json!({});
            }
            if let Some(obj) = summary.as_object_mut() {
                obj.insert("skipReason".to_string(), serde_json::json!(reason));
                obj.insert("skippedAt".to_string(), serde_json::json!(now));
            }
            let changed = tx.execute(
                r#"
UPDATE auto_update_pending
SET status = 'skipped', summary_json = ?2, updated_at = ?3
WHERE id = ?1 AND status IN ('pending', 'enqueuing', 'enqueued')
"#,
                params![pending_id, serde_json::to_string(&summary)?, now],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(());
            }
            if let Some(update_job_id) = update_job_id {
                tx.execute(
                    r#"
UPDATE jobs
SET status = 'cancelled', finished_at = ?2
WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'
"#,
                    params![update_job_id, now],
                )?;
                tx.execute(
                    r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT ?1, ?2, 'auto-policy-skip', ?2
WHERE EXISTS (
  SELECT 1 FROM jobs
  WHERE id = ?1 AND status = 'running' AND created_by = 'auto-policy'
)
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
                    params![update_job_id, now],
                )?;
            }
            let pending_digest = super::canonical_digest_sql("p.candidate_digest");
            let active_digest = super::canonical_digest_sql("active.candidate_digest");
            let candidate_digest =
                super::canonical_digest_sql("auto_update_candidates.candidate_digest");
            let sql = format!(
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
      AND {pending_digest} = {candidate_digest}
  )
  AND NOT EXISTS (
    SELECT 1
    FROM auto_update_pending active
    WHERE active.service_id = auto_update_candidates.service_id
      AND {active_digest} = {candidate_digest}
      AND active.id <> ?1
      AND active.status IN ('pending', 'enqueuing', 'enqueued')
  )
"#,
            );
            tx.execute(&sql, params![pending_id, reason, now])?;
            tx.commit()?;
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

include!("auto_update_claims.rs");

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

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
            source: "schedule".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.0.0".to_string(),
            current_digest: Some("sha256:old".to_string()),
        }
    }

    #[tokio::test]
    async fn stale_inference_settlement_cannot_regress_ready_candidate() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.upsert_auto_update_candidate(
            &candidate_input(
                "candidate-monotonic",
                "sha256:monotonic",
                "awaiting_inference",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
        let generation = db
            .begin_auto_update_candidate_inference(
                "service",
                "sha256:monotonic",
                "2026-04-30T00:01:00Z",
            )
            .await
            .unwrap()
            .unwrap()
            .evidence_generation;
        db.settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:monotonic".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            resolved_tags: None,
            reason: Some("digest_bound_version".to_string()),
            last_error: None,
            attempts: 1,
            evidence_generation: generation,
            retry_at: None,
            settled_at: Some("2026-04-30T00:02:00Z".to_string()),
            now: "2026-04-30T00:02:00Z".to_string(),
        })
        .await
        .unwrap()
        .unwrap();

        let stale = db
            .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
                service_id: "service".to_string(),
                candidate_digest: "sha256:monotonic".to_string(),
                status: "awaiting_inference".to_string(),
                resolved_version: None,
                resolved_tags: None,
                reason: Some("version_inference_pending".to_string()),
                last_error: None,
                attempts: 2,
                evidence_generation: 2,
                retry_at: Some("2026-04-30T00:07:00Z".to_string()),
                settled_at: None,
                now: "2026-04-30T00:03:00Z".to_string(),
            })
            .await
            .unwrap();
        assert!(stale.is_none());
        let current = db
            .get_auto_update_candidate("service", "sha256:monotonic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.status, "ready");
        assert_eq!(current.resolved_version.as_deref(), Some("1.4.0"));

        let stale_ready = db
            .settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
                service_id: "service".to_string(),
                candidate_digest: "sha256:monotonic".to_string(),
                status: "ready".to_string(),
                resolved_version: Some("1.3.0".to_string()),
                resolved_tags: None,
                reason: Some("stale_digest_evidence".to_string()),
                last_error: None,
                attempts: 0,
                evidence_generation: 0,
                retry_at: None,
                settled_at: Some("2026-04-30T00:05:00Z".to_string()),
                now: "2026-04-30T00:06:00Z".to_string(),
            })
            .await
            .unwrap();
        assert!(stale_ready.is_none());
        let current = db
            .get_auto_update_candidate("service", "sha256:monotonic")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.resolved_version.as_deref(), Some("1.4.0"));
    }

    #[tokio::test]
    async fn stale_pending_claim_reconciles_created_job_or_reopens() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let make_pending = |id: &str, digest: &str| AutoUpdatePendingInput {
            id: id.to_string(),
            policy_scope_type: "stack".to_string(),
            policy_scope_id: "stack".to_string(),
            rule_id: "rule".to_string(),
            stack_id: "stack".to_string(),
            service_id: "service".to_string(),
            source_check_job_id: "check".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: "1.4.0".to_string(),
            candidate_digest: digest.to_string(),
            current_display_tag: "1.0.0".to_string(),
            first_seen_at: "2026-04-30T00:00:00Z".to_string(),
            due_at: "2026-04-30T00:00:00Z".to_string(),
            min_age_seconds: 0,
            min_version_lag: 0,
            summary_json: serde_json::json!({}),
            candidate_id: None,
        };
        let linked = db
            .reserve_auto_update_pending(
                &make_pending("pending-linked", "sha256:linked"),
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        let reopened = db
            .reserve_auto_update_pending(
                &make_pending("pending-reopened", "sha256:reopened"),
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        assert!(
            db.try_claim_auto_update_pending(&linked.id, "2026-04-30T00:00:01Z")
                .await
                .unwrap()
        );
        assert!(
            db.try_claim_auto_update_pending(&reopened.id, "2026-04-30T00:00:01Z")
                .await
                .unwrap()
        );
        db.insert_job(crate::api::types::JobListItem {
            id: "recovered-auto-policy-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "queued".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:00:02Z".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({
                "targets": [{
                    "serviceId": "service",
                    "targetDigest": "sha256:linked",
                    "autoPolicyContext": {
                        "pendingId": "pending-linked",
                        "candidateId": "",
                        "ruleId": "rule",
                        "policyScopeType": "stack",
                        "policyScopeId": "stack"
                    }
                }]
            }),
        })
        .await
        .unwrap();

        assert_eq!(
            db.reconcile_auto_update_pending_claims(
                "2026-04-30T00:05:00Z",
                "2026-04-30T00:06:00Z",
            )
            .await
            .unwrap(),
            2
        );
        let linked = db
            .list_auto_update_pending_candidates("2026-04-30T00:06:00Z", 10)
            .await
            .unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, reopened.id);
        assert_eq!(linked[0].status, "pending");
        let linked = db
            .get_auto_update_pending_by_id("pending-linked")
            .await
            .unwrap();
        assert_eq!(
            linked.unwrap().update_job_id.as_deref(),
            Some("recovered-auto-policy-job")
        );
    }

    #[tokio::test]
    async fn stale_pending_claim_does_not_attach_a_terminal_job() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: "pending-terminal".to_string(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule".to_string(),
                    stack_id: "stack".to_string(),
                    service_id: "service".to_string(),
                    source_check_job_id: "check".to_string(),
                    candidate_tag: "latest".to_string(),
                    candidate_display_tag: "1.4.0".to_string(),
                    candidate_digest: "sha256:terminal".to_string(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({}),
                    candidate_id: None,
                },
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        assert!(
            db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:01Z")
                .await
                .unwrap()
        );
        db.insert_job(crate::api::types::JobListItem {
            id: "terminal-auto-policy-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "failed".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:00:02Z".to_string(),
            started_at: Some("2026-04-30T00:00:02Z".to_string()),
            finished_at: Some("2026-04-30T00:00:03Z".to_string()),
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({
                "targets": [{
                    "serviceId": "service",
                    "targetDigest": "sha256:terminal"
                }]
            }),
        })
        .await
        .unwrap();

        assert_eq!(
            db.reconcile_auto_update_pending_claims(
                "2026-04-30T00:05:00Z",
                "2026-04-30T00:06:00Z",
            )
            .await
            .unwrap(),
            1
        );
        let pending = db
            .get_auto_update_pending_by_id("pending-terminal")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.update_job_id, None);
    }

    #[tokio::test]
    async fn enqueuing_claim_recovers_a_successful_auto_policy_job() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: "pending-success".to_string(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule".to_string(),
                    stack_id: "stack".to_string(),
                    service_id: "service".to_string(),
                    source_check_job_id: "check".to_string(),
                    candidate_tag: "latest".to_string(),
                    candidate_display_tag: "1.4.0".to_string(),
                    candidate_digest: "sha256:success".to_string(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({}),
                    candidate_id: None,
                },
                "2026-04-30T00:00:00Z",
            )
            .await
            .unwrap();
        assert!(
            db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:01Z")
                .await
                .unwrap()
        );
        db.insert_job(crate::api::types::JobListItem {
            id: "successful-auto-policy-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "success".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:00:02Z".to_string(),
            started_at: Some("2026-04-30T00:00:02Z".to_string()),
            finished_at: Some("2026-04-30T00:00:03Z".to_string()),
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({
                "targets": [{
                    "serviceId": "service",
                    "targetDigest": "SUCCESS",
                    "autoPolicyContext": {
                        "pendingId": "pending-success",
                        "candidateId": "",
                        "ruleId": "rule",
                        "policyScopeType": "stack",
                        "policyScopeId": "stack"
                    }
                }]
            }),
        })
        .await
        .unwrap();

        assert_eq!(
            db.reconcile_auto_update_pending_claims(
                "2026-04-30T00:05:00Z",
                "2026-04-30T00:06:00Z",
            )
            .await
            .unwrap(),
            1
        );
        let pending = db
            .get_auto_update_pending_by_id("pending-success")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pending.status, "enqueued");
        assert_eq!(
            pending.update_job_id.as_deref(),
            Some("successful-auto-policy-job")
        );
    }

    #[tokio::test]
    async fn pending_claim_rechecks_the_current_effective_policy() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO jobs (id, type, scope, stack_id, service_id, status, allow_arch_mismatch, backup_mode, created_by, reason, created_at, summary_json) VALUES ('check', 'check', 'SERVICE', 'STACK', 'SERVICE', 'success', 0, 'inherit', 'schedule', 'schedule', '2026-04-30T00:00:00Z', '{}')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        db.upsert_auto_update_candidate(
            &candidate_input(
                "candidate-policy-recheck",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
        db.set_auto_update_candidate_policy(
            "service",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "delayed",
            Some("policy_matched"),
            Some("rule"),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
        let mut policy = crate::api::types::AutoUpdatePolicy {
            mode: crate::api::types::AutoUpdatePolicyMode::Override,
            enabled: true,
            rules: vec![crate::api::types::AutoUpdateRule {
                id: "rule".to_string(),
                name: "rule".to_string(),
                enabled: true,
                matcher: crate::api::types::AutoUpdateMatcher {
                    kind: crate::api::types::AutoUpdateMatcherType::Glob,
                    pattern: "latest".to_string(),
                },
                action: crate::api::types::AutoUpdateRuleAction::Immediate,
                delay: crate::api::types::AutoUpdateDelay {
                    min_age_seconds: 0,
                    min_version_lag: 0,
                },
            }],
            updated_at: None,
        };
        db.put_auto_update_policy("stack", "stack", &policy, "2026-04-30T00:01:00Z")
            .await
            .unwrap();
        let pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: "pending-policy-recheck".to_string(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule".to_string(),
                    stack_id: "stack".to_string(),
                    service_id: "service".to_string(),
                    source_check_job_id: "check".to_string(),
                    candidate_tag: "latest".to_string(),
                    candidate_display_tag: "1.4.0".to_string(),
                    candidate_digest: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({
                        "policyUpdatedAt": "2026-04-30T00:01:00Z",
                        "currentDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    }),
                    candidate_id: Some("candidate-policy-recheck".to_string()),
                },
                "2026-04-30T00:01:00Z",
            )
            .await
            .unwrap();
        assert!(
            db.try_claim_auto_update_pending_if_current(
                &pending.id,
                "service",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "stack",
                "stack",
                "rule",
                "2026-04-30T00:01:01Z",
            )
            .await
            .unwrap()
        );
        db.release_auto_update_pending_claim(&pending.id, "2026-04-30T00:01:02Z")
            .await
            .unwrap();
        policy.rules[0].matcher.pattern = "not-latest".to_string();
        db.put_auto_update_policy("stack", "stack", &policy, "2026-04-30T00:02:00Z")
            .await
            .unwrap();
        assert!(
            !db.try_claim_auto_update_pending_if_current(
                &pending.id,
                "service",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "stack",
                "stack",
                "rule",
                "2026-04-30T00:01:01Z",
            )
            .await
            .unwrap()
        );
        assert_eq!(
            db.get_auto_update_pending_by_id(&pending.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
    }

    include!("auto_update_recovery_tests.rs");

    #[tokio::test]
    async fn newer_candidate_supersedes_old_candidate_and_pending_action() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:current', 'sha256:new-candidate', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let old = db
            .upsert_auto_update_candidate(
                &candidate_input(
                    "candidate-old",
                    "sha256:old-candidate",
                    "ready",
                    "2026-04-30T00:10:00Z",
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
                    "2026-04-30T00:00:00Z",
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

        db.insert_job(crate::api::types::JobListItem {
            id: "old-auto-update-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "queued".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();
        assert!(
            db.try_claim_auto_update_pending(&pending.id, "2026-04-30T00:00:30Z")
                .await
                .unwrap()
        );
        assert!(
            db.mark_auto_update_pending_enqueued(
                &pending.id,
                "old-auto-update-job",
                "2026-04-30T00:00:31Z",
            )
            .await
            .unwrap()
        );

        db.supersede_auto_update_candidates(
            "service",
            "sha256:new-candidate",
            &new.discovered_at,
            &new.id,
            "2026-04-30T01:00:00Z",
        )
        .await
        .unwrap();

        let old_candidate = db
            .get_auto_update_candidate("service", "sha256:old-candidate")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old_candidate.status, "superseded");
        assert_eq!(
            old_candidate.superseded_by_candidate_id.as_deref(),
            Some(new.id.as_str())
        );
        assert_eq!(
            old_candidate.superseded_at.as_deref(),
            Some("2026-04-30T01:00:00Z")
        );
        assert!(
            db.list_auto_update_pending_candidates("2026-04-30T02:00:00Z", 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.get_job("old-auto-update-job")
                .await
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );

        db.insert_job(crate::api::types::JobListItem {
            id: "late-auto-update-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "queued".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T01:00:00Z".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();
        assert!(
            !db.mark_auto_update_pending_enqueued(
                &pending.id,
                "late-auto-update-job",
                "2026-04-30T01:00:01Z",
            )
            .await
            .unwrap()
        );
        assert_eq!(
            db.get_job("late-auto-update-job")
                .await
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );

        let running_pending = db
            .reserve_auto_update_pending(
                &AutoUpdatePendingInput {
                    id: "pending-running-old".to_string(),
                    policy_scope_type: "stack".to_string(),
                    policy_scope_id: "stack".to_string(),
                    rule_id: "rule-running".to_string(),
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
        db.insert_job(crate::api::types::JobListItem {
            id: "running-auto-update-job".to_string(),
            r#type: crate::api::types::JobType::Update,
            scope: crate::api::types::JobScope::Service,
            stack_id: Some("stack".to_string()),
            service_id: Some("service".to_string()),
            status: "running".to_string(),
            created_by: "auto-policy".to_string(),
            reason: "auto_policy".to_string(),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            started_at: Some("2026-04-30T00:00:31Z".to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();
        assert!(
            db.try_claim_auto_update_pending(&running_pending.id, "2026-04-30T00:00:32Z")
                .await
                .unwrap()
        );
        assert!(
            db.mark_auto_update_pending_enqueued(
                &running_pending.id,
                "running-auto-update-job",
                "2026-04-30T00:00:33Z",
            )
            .await
            .unwrap()
        );
        db.supersede_auto_update_candidates(
            "service",
            "sha256:new-candidate",
            &new.discovered_at,
            &new.id,
            "2026-04-30T01:00:01Z",
        )
        .await
        .unwrap();
        let stop = db
            .get_update_stop_control("running-auto-update-job")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stop.stop_requested_by.as_deref(),
            Some("auto-policy-supersession")
        );
    }
}
