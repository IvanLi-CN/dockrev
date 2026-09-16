impl Db {
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
WHERE id = ?1 AND status = 'queued'
"#,
                    params![query_job_id, query_started_at],
                )? == 1)
            })
            .await
            .context("claim queued job by id")?;
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
