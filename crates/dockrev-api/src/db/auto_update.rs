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
        source: row.get(13)?,
        current_tag: row.get(14)?,
        current_display_tag: row.get(15)?,
        current_digest: row.get(16)?,
        settled_at: row.get(17)?,
        updated_at: row.get(19)?,
        policy_status: row.get(20)?,
        policy_reason: row.get(21)?,
        policy_rule_id: row.get(22)?,
        policy_evaluated_at: row.get(23)?,
    })
}

const AUTO_UPDATE_CANDIDATE_COLUMNS: &str = "id, stack_id, service_id, image_ref, raw_tag, candidate_digest, resolved_version, status, reason, attempts, retry_at, discovered_at, source_job_id, source, current_tag, current_display_tag, current_digest, settled_at, created_at, updated_at, policy_status, policy_reason, policy_rule_id, policy_evaluated_at";

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
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 'pending', ?16, ?17, CASE WHEN ?18 IS NULL OR EXISTS (SELECT 1 FROM auto_update_candidates c WHERE c.id = ?18 AND c.service_id = ?6 AND c.candidate_digest = ?10) THEN ?18 ELSE NULL END, ?19)
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
        let candidate_digest = candidate_digest.to_string();
        let policy_scope_type = policy_scope_type.to_string();
        let policy_scope_id = policy_scope_id.to_string();
        let rule_id = rule_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
UPDATE auto_update_pending
SET status = 'enqueuing', updated_at = ?7
WHERE id = ?1
  AND status = 'pending'
  AND service_id = ?2
  AND candidate_digest = ?3
  AND policy_scope_type = ?4
  AND policy_scope_id = ?5
  AND rule_id = ?6
  AND EXISTS (
    SELECT 1
    FROM auto_update_candidates c
    WHERE c.service_id = ?2
      AND c.candidate_digest = ?3
      AND c.status <> 'superseded'
      AND c.policy_status = 'delayed'
      AND c.policy_rule_id = ?6
      AND (auto_update_pending.candidate_id IS NULL OR c.id = auto_update_pending.candidate_id)
  )
  AND EXISTS (
    SELECT 1
    FROM services s
    WHERE s.id = ?2
      AND s.candidate_digest = ?3
  )
  AND EXISTS (
    SELECT 1
    FROM services s
    LEFT JOIN auto_update_policies service_policy
      ON service_policy.scope_type = 'service'
     AND service_policy.scope_id = s.id
    LEFT JOIN auto_update_policies stack_policy
      ON stack_policy.scope_type = 'stack'
     AND stack_policy.scope_id = s.stack_id
    WHERE s.id = ?2
      AND (
        (
          ?4 = 'service'
          AND ?5 = s.id
          AND service_policy.mode = 'override'
          AND service_policy.enabled <> 0
          AND EXISTS (
            SELECT 1
            FROM json_each(CASE WHEN json_valid(service_policy.rules_json)
              THEN service_policy.rules_json ELSE '[]' END) AS rule
            WHERE json_extract(rule.value, '$.id') = ?6
              AND json_extract(rule.value, '$.enabled') <> 0
          )
        )
        OR (
          ?4 = 'stack'
          AND ?5 = s.stack_id
          AND COALESCE(service_policy.mode, 'inherit') = 'inherit'
          AND stack_policy.mode = 'override'
          AND stack_policy.enabled <> 0
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
"#,
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

include!("auto_update_claims.rs");

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
        db.settle_auto_update_candidate(&AutoUpdateCandidateSettlementInput {
            service_id: "service".to_string(),
            candidate_digest: "sha256:monotonic".to_string(),
            status: "ready".to_string(),
            resolved_version: Some("1.4.0".to_string()),
            reason: Some("digest_bound_version".to_string()),
            attempts: 1,
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
                reason: Some("version_inference_pending".to_string()),
                attempts: 2,
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
                    "targetDigest": "sha256:linked"
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
    async fn pending_claim_rechecks_the_current_effective_policy() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.call(|conn| {
            conn.execute(
                "INSERT INTO stacks (id, name, compose_type, compose_files_json, backup_targets_json, backup_retention_keep_last, backup_retention_delete_after_stable_seconds, created_at, updated_at, last_check_at) VALUES ('stack', 'stack', 'path', '[]', '[]', 0, 0, '2026-04-30', '2026-04-30', '2026-04-30')",
                [],
            )?;
            conn.execute(
                "INSERT INTO services (id, stack_id, name, image_ref, image_tag, current_digest, candidate_digest, auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json, created_at, updated_at) VALUES ('service', 'stack', 'service', 'ghcr.io/acme/app', 'latest', 'sha256:old', 'sha256:new', 0, '{}', '{}', '2026-04-30', '2026-04-30')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        db.upsert_auto_update_candidate(
            &candidate_input(
                "candidate-policy-recheck",
                "sha256:new",
                "ready",
                "2026-04-30T00:00:00Z",
            ),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
        db.set_auto_update_candidate_policy(
            "service",
            "sha256:new",
            "delayed",
            Some("policy_matched"),
            Some("rule"),
            "2026-04-30T00:00:00Z",
        )
        .await
        .unwrap();
        let policy = crate::api::types::AutoUpdatePolicy {
            mode: crate::api::types::AutoUpdatePolicyMode::Override,
            enabled: false,
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
                    candidate_digest: "sha256:new".to_string(),
                    current_display_tag: "1.0.0".to_string(),
                    first_seen_at: "2026-04-30T00:00:00Z".to_string(),
                    due_at: "2026-04-30T00:00:00Z".to_string(),
                    min_age_seconds: 0,
                    min_version_lag: 0,
                    summary_json: serde_json::json!({}),
                    candidate_id: Some("service:sha256:new".to_string()),
                },
                "2026-04-30T00:01:00Z",
            )
            .await
            .unwrap();
        assert!(
            !db.try_claim_auto_update_pending_if_current(
                &pending.id,
                "service",
                "sha256:new",
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
        db.create_update_stop_control("running-auto-update-job", "2026-04-30T00:00:31Z")
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
