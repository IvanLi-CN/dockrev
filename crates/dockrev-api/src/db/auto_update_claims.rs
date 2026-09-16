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
                "SELECT {AUTO_UPDATE_CANDIDATE_COLUMNS} FROM auto_update_candidates WHERE status IN ('ready', 'unresolved') AND (policy_evaluated_at IS NULL OR policy_evaluated_at < updated_at) ORDER BY updated_at ASC LIMIT ?1"
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
SELECT id, service_id, candidate_digest, updated_at
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
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            let mut repaired = 0;
            let mut recovered_jobs = Vec::new();
            for (pending_id, service_id, candidate_digest, claimed_at) in rows {
                let existing_job = tx
                    .query_row(
                        r#"
SELECT j.id
FROM jobs j
WHERE j.created_by = 'auto-policy'
  AND j.status IN ('queued', 'running', 'success', 'failed', 'cancelled', 'rolled_back')
  AND j.created_at >= ?3
  AND json_valid(j.summary_json)
  AND EXISTS (
    SELECT 1
    FROM json_each(j.summary_json, '$.targets') target
    WHERE json_extract(target.value, '$.serviceId') = ?1
      AND json_extract(target.value, '$.targetDigest') = ?2
  )
ORDER BY j.created_at DESC, j.id DESC
LIMIT 1
"#,
                        params![service_id, candidate_digest, claimed_at],
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
