use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, params};

use super::{Db, merge_job_summary_value};

impl Db {
    pub async fn merge_job_summary_fields(
        &self,
        job_id: &str,
        fields: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let query_job_id = job_id.clone();
        let fields = fields.clone();
        let event = self
            .call(move |conn| {
                let row = conn
                    .query_row(
                        r#"
SELECT type, scope, stack_id, service_id, status, summary_json
FROM jobs
WHERE id = ?1
"#,
                        params![&query_job_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, Option<String>>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )
                    .optional()?;

                let Some((job_type, scope, stack_id, service_id, status, summary_raw)) = row else {
                    return Ok(None);
                };

                let mut summary: serde_json::Value = serde_json::from_str(&summary_raw)
                    .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                merge_job_summary_value(&mut summary, &fields);

                conn.execute(
                    r#"
UPDATE jobs
SET summary_json = ?2
WHERE id = ?1
"#,
                    params![&query_job_id, serde_json::to_string(&summary)?],
                )?;
                Ok(Some((job_type, scope, stack_id, service_id, status)))
            })
            .await
            .context("merge job summary fields")?;

        if let Some((job_type, scope, stack_id, service_id, status)) = event {
            self.management_events
                .publish_change(
                    "jobs",
                    "job",
                    job_id.clone(),
                    serde_json::json!({
                        "jobId": job_id,
                        "jobType": job_type,
                        "scope": scope,
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "status": status,
                        "operation": "summary_updated",
                    }),
                )
                .await;
        }
        Ok(())
    }

    pub async fn set_job_progress(
        &self,
        job_id: &str,
        progress: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let query_job_id = job_id.clone();
        let progress = progress.clone();
        let event = self
            .call(move |conn| {
                let row = conn
                    .query_row(
                        r#"
SELECT type, scope, stack_id, service_id, status, summary_json
FROM jobs
WHERE id = ?1
"#,
                        params![&query_job_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, Option<String>>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )
                    .optional()?;

                let Some((job_type, scope, stack_id, service_id, status, summary_raw)) = row else {
                    return Ok(None);
                };

                let mut summary: serde_json::Value = serde_json::from_str(&summary_raw)
                    .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                if !summary.is_object() {
                    summary = serde_json::Value::Object(Default::default());
                }

                if let Some(obj) = summary.as_object_mut() {
                    obj.insert("progress".to_string(), progress);
                }

                conn.execute(
                    r#"
UPDATE jobs
SET summary_json = ?2
WHERE id = ?1
"#,
                    params![&query_job_id, serde_json::to_string(&summary)?],
                )?;
                Ok(Some((job_type, scope, stack_id, service_id, status)))
            })
            .await
            .context("set job progress")?;

        if let Some((job_type, scope, stack_id, service_id, status)) = event {
            self.management_events
                .publish_change(
                    "jobs",
                    "job",
                    job_id.clone(),
                    serde_json::json!({
                        "jobId": job_id,
                        "jobType": job_type,
                        "scope": scope,
                        "stackId": stack_id,
                        "serviceId": service_id,
                        "status": status,
                        "operation": "progress_updated",
                    }),
                )
                .await;
        }
        Ok(())
    }
}
