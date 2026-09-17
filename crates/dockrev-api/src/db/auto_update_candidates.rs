impl Db {
    pub async fn upsert_auto_update_candidate(
        &self,
        input: &AutoUpdateCandidateInput,
        now: &str,
    ) -> anyhow::Result<AutoUpdateCandidateRow> {
        let input = input.clone();
        let now = now.to_string();
        let settled_at =
            matches!(input.status.as_str(), "ready" | "unresolved").then(|| now.clone());
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO auto_update_candidates (
  id, stack_id, service_id, image_ref, raw_tag, candidate_digest,
  resolved_version, status, reason, attempts, retry_at, discovered_at,
  source_job_id, source, current_tag, current_display_tag, current_digest,
  settled_at, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
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
  reason = CASE
    WHEN auto_update_candidates.status = 'superseded'
      THEN auto_update_candidates.reason
    WHEN auto_update_candidates.status = 'ready' AND excluded.status <> 'ready'
      THEN auto_update_candidates.reason
    WHEN auto_update_candidates.status = 'unresolved' AND excluded.status = 'awaiting_inference'
      THEN auto_update_candidates.reason
    ELSE COALESCE(excluded.reason, auto_update_candidates.reason)
  END,
  retry_at = COALESCE(excluded.retry_at, auto_update_candidates.retry_at),
  source_job_id = CASE
    WHEN excluded.source IN ('schedule', 'github_webhook')
      THEN excluded.source_job_id
    ELSE auto_update_candidates.source_job_id
  END,
  source = CASE
    WHEN excluded.source IN ('schedule', 'github_webhook')
      THEN excluded.source
    WHEN auto_update_candidates.source IS NULL OR auto_update_candidates.source = 'unknown'
      THEN excluded.source
    ELSE auto_update_candidates.source
  END,
  current_tag = excluded.current_tag,
  current_display_tag = excluded.current_display_tag,
  current_digest = excluded.current_digest,
  settled_at = COALESCE(auto_update_candidates.settled_at, excluded.settled_at),
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
                    input.source,
                    input.current_tag,
                    input.current_display_tag,
                    input.current_digest,
                    settled_at,
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
        input: &AutoUpdateCandidateSettlementInput,
    ) -> anyhow::Result<Option<AutoUpdateCandidateRow>> {
        let input = input.clone();
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
    (
      status = ?3
      OR (?3 = 'ready' AND status IN ('awaiting_inference', 'unresolved'))
      OR (?3 = 'unresolved' AND status = 'awaiting_inference')
    )
  )
  AND (
    status IS NOT ?3
    OR resolved_version IS NOT COALESCE(?4, resolved_version)
    OR reason IS NOT ?5
    OR attempts IS NOT ?6
    OR retry_at IS NOT ?7
    OR settled_at IS NOT ?8
  )
"#,
                params![
                    input.service_id,
                    input.candidate_digest,
                    input.status,
                    input.resolved_version,
                    input.reason,
                    input.attempts as i64,
                    input.retry_at,
                    input.settled_at,
                    input.now,
                ],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            Ok(conn.query_row(
                &format!("SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE service_id = ?1 AND candidate_digest = ?2"),
                params![input.service_id, input.candidate_digest],
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

            let pending_rows = {
                let mut stmt = tx.prepare(r#"
SELECT p.id, p.update_job_id, p.summary_json, j.status
FROM auto_update_pending p
JOIN auto_update_candidates c
  ON c.service_id = p.service_id AND c.candidate_digest = p.candidate_digest
LEFT JOIN jobs j ON j.id = p.update_job_id
WHERE p.service_id = ?1
  AND p.candidate_digest <> ?2
  AND p.status IN ('pending', 'enqueuing', 'enqueued')
  AND c.status = 'superseded'
  AND (c.discovered_at < ?3 OR (c.discovered_at = ?3 AND c.id < ?4))
"#)?;
                stmt.query_map(
                    params![service_id, candidate_digest, discovered_at, candidate_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for (pending_id, update_job_id, summary_raw, update_job_status) in pending_rows {
                let mut summary = serde_json::from_str::<serde_json::Value>(&summary_raw)
                    .unwrap_or_else(|_| serde_json::json!({}));
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
                if let Some(update_job_id) = update_job_id {
                    tx.execute(
                        r#"
UPDATE jobs
SET status = 'cancelled', finished_at = ?2
WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'
"#,
                        params![update_job_id, now],
                    )?;
                    if update_job_status.as_deref() == Some("running") {
                        tx.execute(
                            r#"
UPDATE update_job_stop_controls
SET stop_requested_at = ?2,
    stop_requested_by = 'auto-policy-supersession',
    updated_at = ?2
WHERE job_id = ?1
  AND stop_requested_at IS NULL
  AND apply_committed_at IS NULL
"#,
                            params![update_job_id, now],
                        )?;
                    }
                }
            }
            tx.commit()?;
            Ok(superseded)
        })
        .await
        .context("supersede auto update candidates")
    }

    pub async fn list_latest_auto_update_candidates(
        &self,
        service_ids: &[String],
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            let mut out = Vec::new();
            let mut stmt = conn.prepare(
                "SELECT c.id, c.stack_id, c.service_id, c.image_ref, c.raw_tag, c.candidate_digest, c.resolved_version, c.status, c.reason, c.attempts, c.retry_at, c.discovered_at, c.source_job_id, c.source, c.current_tag, c.current_display_tag, c.current_digest, c.settled_at, c.created_at, c.updated_at, c.policy_status, c.policy_reason, c.policy_rule_id, c.policy_evaluated_at FROM auto_update_candidates c JOIN services s ON s.id = c.service_id WHERE c.service_id = ?1 AND (s.candidate_digest = c.candidate_digest OR (s.candidate_digest IS NULL AND c.policy_status = 'completed' AND s.current_digest = c.candidate_digest)) ORDER BY c.discovered_at DESC, c.id DESC LIMIT 1",
            )?;
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

}
