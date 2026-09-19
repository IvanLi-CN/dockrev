impl Db {
    pub(super) async fn sync_auto_update_candidate_policy_for_claimed_job(
        &self,
        job: &JobListItem,
    ) -> anyhow::Result<()> {
        if job.r#type.as_str() == "update" {
            self.sync_auto_update_candidate_policy_for_job(
                &job.id,
                job.started_at.as_deref().unwrap_or_default(),
            )
            .await?;
        }
        Ok(())
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

    pub async fn list_auto_update_candidates_for_policy_reconciliation(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<AutoUpdateCandidateRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE status IN ('awaiting_inference', 'ready', 'unresolved') AND policy_status IS NOT 'completed' AND EXISTS (SELECT 1 FROM services s WHERE s.id = auto_update_candidates.service_id AND s.candidate_digest = auto_update_candidates.candidate_digest) AND (policy_evaluated_at IS NULL OR policy_evaluated_at < updated_at OR EXISTS (SELECT 1 FROM auto_update_policies p WHERE p.scope_type = auto_update_candidates.policy_scope_type AND p.scope_id = auto_update_candidates.policy_scope_id AND (auto_update_candidates.policy_evaluated_at IS NULL OR auto_update_candidates.policy_evaluated_at < p.updated_at)) OR (policy_status = 'delayed' AND NOT EXISTS (SELECT 1 FROM auto_update_pending p WHERE p.service_id = auto_update_candidates.service_id AND p.candidate_digest = auto_update_candidates.candidate_digest AND p.status IN ('pending', 'enqueuing', 'enqueued')))) ORDER BY updated_at ASC LIMIT ?1"
            ))?;
            let rows = stmt.query_map(params![limit as i64], map_auto_update_candidate_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list auto update candidates for policy reconciliation")
    }

    #[allow(dead_code)]
    pub async fn get_auto_update_pending_by_id(
        &self,
        pending_id: &str,
    ) -> anyhow::Result<Option<AutoUpdatePendingRow>> {
        let pending_id = pending_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
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
WHERE id = ?1
"#,
                    params![pending_id],
                    map_auto_update_pending_row,
                )
                .optional()?)
        })
        .await
        .context("get auto update pending")
    }

    pub async fn reconcile_auto_update_pending_claims(
        &self,
        stale_before: &str,
        now: &str,
    ) -> anyhow::Result<usize> {
        let stale_before = stale_before.to_string();
        let now = now.to_string();
        let db_now = now.clone();
        let recovered_jobs = self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let rows = {
                let mut stmt = tx.prepare(
                    r#"
SELECT id, service_id, candidate_digest, updated_at,
       policy_scope_type, policy_scope_id, rule_id, COALESCE(candidate_id, '')
FROM auto_update_pending
WHERE status = 'enqueuing' AND updated_at <= ?1
"#,
                )?;
                stmt.query_map(params![stale_before], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            let mut repaired = 0;
            let mut recovered_jobs = Vec::new();
            let target_digest = super::canonical_digest_sql(
                "json_extract(target.value, '$.targetDigest')",
            );
            let pending_digest = super::canonical_digest_sql("?2");
            for (
                pending_id,
                service_id,
                candidate_digest,
                claimed_at,
                policy_scope_type,
                policy_scope_id,
                rule_id,
                candidate_id,
            ) in rows
            {
                let existing_job = tx
                    .query_row(
                        &format!(
                            r#"
SELECT j.id
FROM jobs j
WHERE j.created_by = 'auto-policy'
  AND j.status IN ('queued', 'running', 'success')
  AND j.created_at >= ?3
  AND json_valid(j.summary_json)
  AND EXISTS (
    SELECT 1
    FROM json_each(j.summary_json, '$.targets') target
    WHERE json_extract(target.value, '$.serviceId') = ?1
      AND {target_digest} = {pending_digest}
      AND json_extract(target.value, '$.autoPolicyContext.pendingId') = ?4
      AND json_extract(target.value, '$.autoPolicyContext.ruleId') = ?5
      AND json_extract(target.value, '$.autoPolicyContext.policyScopeType') = ?6
      AND json_extract(target.value, '$.autoPolicyContext.policyScopeId') = ?7
      AND json_extract(target.value, '$.autoPolicyContext.candidateId') = ?8
  )
ORDER BY j.created_at DESC, j.id DESC
LIMIT 1
"#
                        ),
                        params![
                            service_id,
                            candidate_digest,
                            claimed_at,
                            pending_id,
                            rule_id,
                            policy_scope_type,
                            policy_scope_id,
                            candidate_id,
                        ],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if let Some(job_id) = existing_job {
                    tx.execute(
                        r#"
UPDATE auto_update_pending
SET status = 'enqueued', update_job_id = ?2, updated_at = ?3
WHERE id = ?1 AND status = 'enqueuing'
"#,
                        params![pending_id, job_id, db_now],
                    )?;
                    recovered_jobs.push(job_id);
                } else {
                    tx.execute(
                        r#"
UPDATE auto_update_pending
SET status = 'pending', updated_at = ?2
WHERE id = ?1 AND status = 'enqueuing'
"#,
                        params![pending_id, db_now],
                    )?;
                }
                repaired += 1;
            }
            tx.commit()?;
            Ok((repaired, recovered_jobs))
        })
        .await
        .context("reconcile auto update pending claims")?;
        let (repaired, recovered_jobs) = recovered_jobs;
        for job_id in &recovered_jobs {
            self.sync_auto_update_candidate_policy_for_job(job_id, &now)
                .await?;
        }
        Ok(repaired)
    }
}
