impl Db {
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
