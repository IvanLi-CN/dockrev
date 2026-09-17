impl Db {
    pub async fn reopen_auto_update_pending_for_recovered_jobs(
        &self,
        job_ids: &[String],
        now: &str,
    ) -> anyhow::Result<usize> {
        if job_ids.is_empty() {
            return Ok(0);
        }
        let job_ids = job_ids.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut reopened = 0;
            for job_id in &job_ids {
                tx.execute(
                    r#"
UPDATE auto_update_candidates
SET policy_status = 'delayed',
    policy_reason = 'update_job_recovered',
    policy_evaluated_at = ?2,
    updated_at = ?2
WHERE status <> 'superseded'
  AND EXISTS (
    SELECT 1
    FROM auto_update_pending p
    WHERE p.update_job_id = ?1
      AND p.status = 'enqueued'
      AND p.service_id = auto_update_candidates.service_id
      AND p.candidate_digest = auto_update_candidates.candidate_digest
  )
"#,
                    params![job_id, now],
                )?;
                reopened += tx.execute(
                    r#"
UPDATE auto_update_pending
SET status = 'pending', update_job_id = NULL, updated_at = ?2
WHERE update_job_id = ?1
  AND status = 'enqueued'
  AND EXISTS (SELECT 1 FROM jobs j WHERE j.id = ?1 AND j.status = 'failed')
"#,
                    params![job_id, now],
                )?;
            }
            tx.commit()?;
            Ok(reopened)
        })
        .await
        .context("reopen auto update pending after job recovery")
    }

    pub async fn fail_corrupt_auto_update_job(
        &self,
        job_id: &str,
        now: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                r#"
UPDATE jobs
SET status = 'failed',
    finished_at = ?2,
    summary_json = CASE
      WHEN json_valid(summary_json) THEN json_set(summary_json, '$.recoveryError', ?3, '$.recoveredAt', ?2)
      ELSE json_object('recoveryError', ?3, 'recoveredAt', ?2)
    END
WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'
"#,
                params![job_id, now, reason],
            )?;
            if changed > 0 {
                tx.execute(
                    r#"
UPDATE auto_update_pending
SET status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json) THEN json_set(summary_json, '$.skipReason', ?2, '$.skippedAt', ?3)
      ELSE json_object('skipReason', ?2, 'skippedAt', ?3)
    END,
    updated_at = ?3
WHERE update_job_id = ?1 AND status = 'enqueued'
"#,
                    params![job_id, reason, now],
                )?;
                tx.execute(
                    r#"
UPDATE auto_update_candidates
SET policy_status = 'failed',
    policy_reason = ?2,
    policy_evaluated_at = ?3,
    updated_at = ?3
WHERE status <> 'superseded'
  AND EXISTS (
    SELECT 1 FROM auto_update_pending p
    WHERE p.update_job_id = ?1
      AND p.service_id = auto_update_candidates.service_id
      AND p.candidate_digest = auto_update_candidates.candidate_digest
  )
"#,
                    params![job_id, reason, now],
                )?;
            }
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("fail corrupt auto update job")
    }

    pub async fn list_enqueued_auto_update_jobs(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<JobListItem>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  j.id,
  j.type,
  j.scope,
  j.stack_id,
  j.service_id,
  j.status,
  j.created_by,
  j.reason,
  j.created_at,
  j.started_at,
  j.finished_at,
  j.allow_arch_mismatch,
  j.backup_mode,
  j.summary_json
FROM jobs j
WHERE j.type = 'update'
  AND j.status = 'queued'
  AND j.created_by = 'auto-policy'
  AND EXISTS (
    SELECT 1
    FROM auto_update_pending p
    WHERE p.update_job_id = j.id
      AND p.status = 'enqueued'
  )
ORDER BY j.created_at ASC, j.id ASC
LIMIT ?1
"#,
            )?;
            let rows = stmt.query_map(params![limit as i64], map_job_list_item_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list enqueued auto update jobs")
    }

    pub async fn claim_queued_job_by_id(
        &self,
        job_id: &str,
        started_at: &str,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let query_job_id = job_id.clone();
        let started_at = started_at.to_string();
        let query_started_at = started_at.clone();
        let claimed = self
            .call(move |conn| {
                Ok(conn.execute(
                    r#"
UPDATE jobs
SET status = 'running', started_at = ?2
WHERE id = ?1
  AND status = 'queued'
  AND created_by = 'auto-policy'
  AND EXISTS (
    SELECT 1
    FROM auto_update_pending p
    JOIN services s ON s.id = p.service_id
    JOIN auto_update_candidates c
      ON c.service_id = p.service_id
     AND c.candidate_digest = p.candidate_digest
    WHERE p.update_job_id = jobs.id
      AND p.status = 'enqueued'
      AND s.candidate_digest = p.candidate_digest
      AND (p.candidate_id IS NULL OR p.candidate_id = c.id)
      AND c.status <> 'superseded'
      AND c.policy_status = 'queued'
      AND (
        (
          p.policy_scope_type = 'service'
          AND p.policy_scope_id = s.id
          AND EXISTS (
            SELECT 1
            FROM auto_update_policies policy
            WHERE policy.scope_type = 'service'
              AND policy.scope_id = s.id
              AND policy.mode = 'override'
              AND policy.enabled <> 0
              AND policy.updated_at = json_extract(p.summary_json, '$.policyUpdatedAt')
              AND EXISTS (
                SELECT 1
                FROM json_each(CASE WHEN json_valid(policy.rules_json) THEN policy.rules_json ELSE '[]' END) AS rule
                WHERE json_extract(rule.value, '$.id') = p.rule_id
                  AND json_extract(rule.value, '$.enabled') <> 0
              )
          )
        )
        OR (
          p.policy_scope_type = 'stack'
          AND p.policy_scope_id = s.stack_id
          AND NOT EXISTS (
            SELECT 1
            FROM auto_update_policies service_policy
            WHERE service_policy.scope_type = 'service'
              AND service_policy.scope_id = s.id
              AND service_policy.mode <> 'inherit'
          )
          AND EXISTS (
            SELECT 1
            FROM auto_update_policies policy
            WHERE policy.scope_type = 'stack'
              AND policy.scope_id = s.stack_id
              AND policy.mode = 'override'
              AND policy.enabled <> 0
              AND policy.updated_at = json_extract(p.summary_json, '$.policyUpdatedAt')
              AND EXISTS (
                SELECT 1
                FROM json_each(CASE WHEN json_valid(policy.rules_json) THEN policy.rules_json ELSE '[]' END) AS rule
                WHERE json_extract(rule.value, '$.id') = p.rule_id
                  AND json_extract(rule.value, '$.enabled') <> 0
              )
          )
        )
      )
  )
"#,
                    params![query_job_id, query_started_at],
                )? == 1)
            })
            .await
            .context("claim queued job by id")?;
        if !claimed {
            let cancel_job_id = job_id.clone();
            let cancel_started_at = started_at.clone();
            let cancelled = self.call(move |conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let changed = tx.execute(
                    r#"
UPDATE jobs
SET status = 'cancelled', finished_at = ?2
WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'
"#,
                    params![cancel_job_id, cancel_started_at],
                )?;
                if changed > 0 {
                    tx.execute(
                        r#"
UPDATE auto_update_pending
SET status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json) THEN json_set(summary_json, '$.skipReason', 'policy_changed_before_start', '$.skippedAt', ?2)
      ELSE json_object('skipReason', 'policy_changed_before_start', 'skippedAt', ?2)
    END,
    updated_at = ?2
WHERE update_job_id = ?1 AND status = 'enqueued'
"#,
                        params![cancel_job_id, cancel_started_at],
                    )?;
                }
                tx.commit()?;
                Ok(changed > 0)
            })
            .await
            .context("cancel stale queued auto policy job")?;
            if cancelled {
                self.sync_auto_update_candidate_policy_for_job(job_id.as_str(), started_at.as_str())
                    .await?;
            }
        }
        if claimed {
            self.sync_auto_update_candidate_policy_for_job(job_id.as_str(), started_at.as_str())
                .await?;
            self.management_events
                .publish_change(
                    "jobs",
                    "job",
                    job_id.clone(),
                    serde_json::json!({
                        "jobId": job_id,
                        "status": "running",
                        "jobType": "update",
                    }),
                )
                .await;
        }
        Ok(claimed)
    }
}
