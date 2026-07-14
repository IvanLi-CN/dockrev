use super::*;

impl Db {
    pub async fn insert_job(&self, job: JobListItem) -> anyhow::Result<()> {
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO jobs (
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  allow_arch_mismatch,
  backup_mode,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  summary_json
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
"#,
                params![
                    job.id,
                    job.r#type.as_str(),
                    job.scope.as_str(),
                    job.stack_id,
                    job.service_id,
                    job.status,
                    job.allow_arch_mismatch as i64,
                    job.backup_mode,
                    job.created_by,
                    job.reason,
                    job.created_at,
                    job.started_at,
                    job.finished_at,
                    serde_json::to_string(&job.summary_json)?
                ],
            )?;
            Ok(())
        })
        .await
        .context("insert job")
    }

    pub async fn insert_or_reuse_webhook_check_job_for_service(
        &self,
        job: JobListItem,
        now: &str,
        stale_threshold: time::Duration,
    ) -> anyhow::Result<PendingJobUpsert> {
        let service_id = job
            .service_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("service_id is required for webhook service check"))?;
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing = tx
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'check'
  AND scope = 'service'
  AND service_id = ?1
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                    params![&service_id],
                    map_job_list_item_row,
                )
                .optional()?;

            if let Some(mut existing) = existing {
                if existing.status == "running" && job_is_stale(&existing, &now, stale_threshold) {
                    terminate_job_as_failed_tx(&tx, &existing, &now, "stale_check")?;
                } else {
                    merge_job_summary_value(&mut existing.summary_json, &job.summary_json);
                    tx.execute(
                        r#"
UPDATE jobs
SET summary_json = ?2
WHERE id = ?1
"#,
                        params![&existing.id, serde_json::to_string(&existing.summary_json)?],
                    )?;
                    tx.commit()?;
                    return Ok(PendingJobUpsert::Reused(Box::new(existing)));
                }
            }

            insert_job_tx(&tx, &job)?;
            tx.commit()?;
            Ok(PendingJobUpsert::Inserted)
        })
        .await
        .context("insert or reuse webhook check job for service")
    }

    pub async fn insert_or_reuse_webhook_discovery_job(
        &self,
        job: JobListItem,
        now: &str,
        stale_threshold: time::Duration,
    ) -> anyhow::Result<PendingJobUpsert> {
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing = tx
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'discovery'
  AND scope = 'all'
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                    [],
                    map_job_list_item_row,
                )
                .optional()?;

            if let Some(existing) = existing {
                if existing.status == "running" && job_is_stale(&existing, &now, stale_threshold) {
                    terminate_job_as_failed_tx(&tx, &existing, &now, "stale_check")?;
                } else {
                    tx.commit()?;
                    return Ok(PendingJobUpsert::Reused(Box::new(existing)));
                }
            }

            insert_job_tx(&tx, &job)?;
            tx.commit()?;
            Ok(PendingJobUpsert::Inserted)
        })
        .await
        .context("insert or reuse webhook discovery job")
    }

    pub async fn claim_next_queued_job_by_type(
        &self,
        job_type: JobType,
        started_at: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let started_at = started_at.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let item: Option<JobListItem> = tx
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1 AND status = 'queued'
ORDER BY created_at ASC, id ASC
LIMIT 1
"#,
                    params![job_type],
                    |row| {
                        let summary_json: String = row.get(13)?;
                        let summary: serde_json::Value = serde_json::from_str(&summary_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;
                        Ok(JobListItem {
                            id: row.get(0)?,
                            r#type: JobType::from_str(&row.get::<_, String>(1)?),
                            scope: JobScope::from_str(&row.get::<_, String>(2)?),
                            stack_id: row.get(3)?,
                            service_id: row.get(4)?,
                            status: row.get(5)?,
                            created_by: row.get(6)?,
                            reason: row.get(7)?,
                            created_at: row.get(8)?,
                            started_at: row.get(9)?,
                            finished_at: row.get(10)?,
                            allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                            backup_mode: row.get(12)?,
                            summary_json: summary,
                        })
                    },
                )
                .optional()?;

            let Some(mut item) = item else {
                tx.commit()?;
                return Ok(None);
            };

            let changed = tx.execute(
                r#"
UPDATE jobs
SET status = 'running', started_at = ?2
WHERE id = ?1 AND status = 'queued'
"#,
                params![item.id, started_at],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }

            item.status = "running".to_string();
            item.started_at = Some(started_at);
            tx.commit()?;
            Ok(Some(item))
        })
        .await
        .context("claim next queued job by type")
    }

    pub async fn finish_job(
        &self,
        job_id: &str,
        status: &str,
        finished_at: &str,
        summary_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let status = status.to_string();
        let finished_at = finished_at.to_string();
        let mut summary_json = summary_json.clone();
        self.call(move |conn| {
            let previous = conn
                .query_row(
                    r#"
SELECT type, summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;

            if !summary_json.is_object() {
                summary_json = serde_json::json!({ "result": summary_json });
            }

            if let Some((_, previous_summary_raw)) = previous.as_ref() {
                let previous_summary: serde_json::Value =
                    serde_json::from_str(previous_summary_raw)
                        .unwrap_or_else(|_| serde_json::json!({}));
                if let Some(previous) = previous_summary.as_object()
                    && let Some(obj) = summary_json.as_object_mut()
                {
                    for (key, value) in previous {
                        obj.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }

            let summary_json_str = serde_json::to_string(&summary_json)?;
            conn.execute(
                r#"
UPDATE jobs
SET status = ?2, finished_at = ?3, summary_json = ?4
WHERE id = ?1
"#,
                params![job_id, status, finished_at, summary_json_str],
            )?;
            if status == "success"
                && previous
                    .as_ref()
                    .is_some_and(|(job_type, _)| job_type == "check")
            {
                new_version_discoveries::record_new_version_discoveries_from_summary_conn(
                    conn,
                    &job_id,
                    &finished_at,
                    &summary_json,
                )?;
            }
            Ok(())
        })
        .await
        .context("finish job")
    }

    pub async fn merge_job_summary_fields(
        &self,
        job_id: &str,
        fields: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let fields = fields.clone();
        self.call(move |conn| {
            let summary_raw = conn
                .query_row(
                    r#"
SELECT summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let Some(summary_raw) = summary_raw else {
                return Ok(());
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
                params![&job_id, serde_json::to_string(&summary)?],
            )?;
            Ok(())
        })
        .await
        .context("merge job summary fields")
    }

    pub async fn set_job_progress(
        &self,
        job_id: &str,
        progress: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let progress = progress.clone();
        self.call(move |conn| {
            let summary_raw = conn
                .query_row(
                    r#"
SELECT summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let Some(summary_raw) = summary_raw else {
                return Ok(());
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
                params![&job_id, serde_json::to_string(&summary)?],
            )?;
            Ok(())
        })
        .await
        .context("set job progress")
    }

    pub async fn list_jobs(&self) -> anyhow::Result<Vec<JobListItem>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
ORDER BY created_at DESC
LIMIT 2000
"#,
            )?;

            let rows = stmt.query_map([], |row| {
                let summary_json: String = row.get(13)?;
                let summary: serde_json::Value =
                    serde_json::from_str(&summary_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(JobListItem {
                    id: row.get(0)?,
                    r#type: JobType::from_str(&row.get::<_, String>(1)?),
                    scope: JobScope::from_str(&row.get::<_, String>(2)?),
                    stack_id: row.get(3)?,
                    service_id: row.get(4)?,
                    status: row.get(5)?,
                    created_by: row.get(6)?,
                    reason: row.get(7)?,
                    created_at: row.get(8)?,
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                    backup_mode: row.get(12)?,
                    summary_json: summary,
                })
            })?;

            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list jobs")
    }

    pub async fn list_jobs_by_type_and_statuses(
        &self,
        job_type: JobType,
        statuses: &[&str],
        limit: u32,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let statuses: Vec<String> = statuses.iter().map(|s| (*s).to_string()).collect();
        let limit = limit.max(1);
        self.call(move |conn| {
            if statuses.is_empty() {
                return Ok(Vec::new());
            }

            let placeholders = std::iter::repeat_n("?", statuses.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ? AND status IN ({placeholders})
ORDER BY created_at DESC
LIMIT ?
"#
            );

            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            values.push(rusqlite::types::Value::from(job_type));
            for status in &statuses {
                values.push(rusqlite::types::Value::from(status.clone()));
            }
            values.push(rusqlite::types::Value::from(limit as i64));
            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                let summary_json: String = row.get(13)?;
                let summary: serde_json::Value =
                    serde_json::from_str(&summary_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(JobListItem {
                    id: row.get(0)?,
                    r#type: JobType::from_str(&row.get::<_, String>(1)?),
                    scope: JobScope::from_str(&row.get::<_, String>(2)?),
                    stack_id: row.get(3)?,
                    service_id: row.get(4)?,
                    status: row.get(5)?,
                    created_by: row.get(6)?,
                    reason: row.get(7)?,
                    created_at: row.get(8)?,
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                    backup_mode: row.get(12)?,
                    summary_json: summary,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list jobs by type and statuses")
    }

    pub async fn count_jobs_by_type_and_status(
        &self,
        job_type: JobType,
        status: &str,
    ) -> anyhow::Result<u32> {
        let job_type = job_type.as_str().to_string();
        let status = status.to_string();
        self.call(move |conn| {
            let count = conn.query_row(
                r#"
SELECT COUNT(*)
FROM jobs
WHERE type = ?1 AND status = ?2
"#,
                params![job_type, status],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(count as u32)
        })
        .await
        .context("count jobs by type and status")
    }

    pub async fn has_pending_job_by_type_created_by_reason(
        &self,
        job_type: JobType,
        created_by: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let job_type = job_type.as_str().to_string();
        let created_by = created_by.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let row: Option<i64> = conn
                .query_row(
                    r#"
SELECT 1
FROM jobs
WHERE type = ?1
  AND status IN ('queued', 'running')
  AND created_by = ?2
  AND reason = ?3
LIMIT 1
"#,
                    params![job_type, created_by, reason],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(row.is_some())
        })
        .await
        .context("check pending job by type/created_by/reason")
    }

    pub async fn find_latest_pending_job_by_type(
        &self,
        job_type: JobType,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                params![job_type],
                |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                    Ok(JobListItem {
                        id: row.get(0)?,
                        r#type: JobType::from_str(&row.get::<_, String>(1)?),
                        scope: JobScope::from_str(&row.get::<_, String>(2)?),
                        stack_id: row.get(3)?,
                        service_id: row.get(4)?,
                        status: row.get(5)?,
                        created_by: row.get(6)?,
                        reason: row.get(7)?,
                        created_at: row.get(8)?,
                        started_at: row.get(9)?,
                        finished_at: row.get(10)?,
                        allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                        backup_mode: row.get(12)?,
                        summary_json: summary,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending job by type")
    }

    pub async fn find_latest_pending_job_by_type_and_service_id(
        &self,
        job_type: JobType,
        service_id: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let job_type = job_type.as_str().to_string();
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = ?1
  AND service_id = ?2
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
LIMIT 1
"#,
                params![job_type, service_id],
                |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                    Ok(JobListItem {
                        id: row.get(0)?,
                        r#type: JobType::from_str(&row.get::<_, String>(1)?),
                        scope: JobScope::from_str(&row.get::<_, String>(2)?),
                        stack_id: row.get(3)?,
                        service_id: row.get(4)?,
                        status: row.get(5)?,
                        created_by: row.get(6)?,
                        reason: row.get(7)?,
                        created_at: row.get(8)?,
                        started_at: row.get(9)?,
                        finished_at: row.get(10)?,
                        allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                        backup_mode: row.get(12)?,
                        summary_json: summary,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending job by type and service id")
    }

    pub async fn find_latest_running_check_job(
        &self,
        scope: &JobScope,
        stack_id: Option<&str>,
        service_id: Option<&str>,
    ) -> anyhow::Result<Option<JobListItem>> {
        let scope = scope.as_str().to_string();
        let stack_id = stack_id.map(|s| s.to_string());
        let service_id = service_id.map(|s| s.to_string());
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'check'
  AND status = 'running'
  AND scope = ?1
  AND (?2 IS NULL OR stack_id = ?2)
  AND (?3 IS NULL OR service_id = ?3)
ORDER BY created_at DESC
LIMIT 1
"#,
            )?;

            let row = stmt
                .query_row(params![scope, stack_id, service_id], |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                    Ok(JobListItem {
                        id: row.get(0)?,
                        r#type: JobType::from_str(&row.get::<_, String>(1)?),
                        scope: JobScope::from_str(&row.get::<_, String>(2)?),
                        stack_id: row.get(3)?,
                        service_id: row.get(4)?,
                        status: row.get(5)?,
                        created_by: row.get(6)?,
                        reason: row.get(7)?,
                        created_at: row.get(8)?,
                        started_at: row.get(9)?,
                        finished_at: row.get(10)?,
                        allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                        backup_mode: row.get(12)?,
                        summary_json: summary,
                    })
                })
                .optional()?;

            Ok(row)
        })
        .await
        .context("find latest running check job")
    }

    pub async fn find_latest_running_runtime_scan_job(
        &self,
        scope: &JobScope,
        stack_id: Option<&str>,
        service_id: Option<&str>,
    ) -> anyhow::Result<Option<JobListItem>> {
        let scope = scope.as_str().to_string();
        let stack_id = stack_id.map(|s| s.to_string());
        let service_id = service_id.map(|s| s.to_string());
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE type = 'runtime_scan'
  AND status = 'running'
  AND scope = ?1
  AND (?2 IS NULL OR stack_id = ?2)
  AND (?3 IS NULL OR service_id = ?3)
ORDER BY created_at DESC
LIMIT 1
"#,
            )?;

            let row = stmt
                .query_row(params![scope, stack_id, service_id], |row| {
                    let summary_json: String = row.get(13)?;
                    let summary: serde_json::Value =
                        serde_json::from_str(&summary_json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                    Ok(JobListItem {
                        id: row.get(0)?,
                        r#type: JobType::from_str(&row.get::<_, String>(1)?),
                        scope: JobScope::from_str(&row.get::<_, String>(2)?),
                        stack_id: row.get(3)?,
                        service_id: row.get(4)?,
                        status: row.get(5)?,
                        created_by: row.get(6)?,
                        reason: row.get(7)?,
                        created_at: row.get(8)?,
                        started_at: row.get(9)?,
                        finished_at: row.get(10)?,
                        allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                        backup_mode: row.get(12)?,
                        summary_json: summary,
                    })
                })
                .optional()?;

            Ok(row)
        })
        .await
        .context("find latest running runtime scan job")
    }

    pub async fn recover_incomplete_jobs(
        &self,
        now: &str,
        reason: &str,
    ) -> anyhow::Result<Vec<String>> {
        let now = now.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let items: Vec<(String, String, String)> = {
                let mut stmt = conn.prepare(
                    r#"
SELECT id, status, summary_json
FROM jobs
WHERE finished_at IS NULL
ORDER BY created_at DESC
LIMIT 2000
"#,
                )?;

                let mut rows = stmt.query([])?;
                let mut items: Vec<(String, String, String)> = Vec::new();
                while let Some(row) = rows.next()? {
                    items.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                items
            };

            if items.is_empty() {
                return Ok(Vec::new());
            }

            let tx = conn.transaction()?;
            let mut recovered: Vec<String> = Vec::new();

            for (job_id, status, summary_raw) in items {
                if status == "queued" {
                    // queued jobs are not interrupted work; keep them pending for workers.
                    continue;
                }

                // Always leave an audit trail so operators can tell why the job ended.
                tx.execute(
                    r#"
INSERT INTO job_logs (job_id, ts, level, msg)
VALUES (?1, ?2, 'warn', ?3)
"#,
                    params![
                        job_id,
                        now,
                        format!("job recovered as terminated: reason={reason}")
                    ],
                )?;

                let is_terminal = matches!(status.as_str(), "success" | "failed" | "rolled_back");
                if is_terminal {
                    tx.execute(
                        r#"
UPDATE jobs
SET finished_at = ?2
WHERE id = ?1
"#,
                        params![job_id, now],
                    )?;
                    recovered.push(job_id);
                    continue;
                }

                let mut summary: serde_json::Value =
                    serde_json::from_str(&summary_raw).unwrap_or(serde_json::json!({}));
                if !summary.is_object() {
                    summary = serde_json::json!({});
                }
                if let Some(obj) = summary.as_object_mut() {
                    obj.insert(
                        "terminated".to_string(),
                        serde_json::json!({
                            "reason": reason,
                            "at": now,
                        }),
                    );
                }

                tx.execute(
                    r#"
UPDATE jobs
SET status = 'failed', finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
                recovered.push(job_id);
            }

            tx.commit()?;
            Ok(recovered)
        })
        .await
        .context("recover incomplete jobs")
    }

    pub async fn terminate_job_as_failed(
        &self,
        job_id: &str,
        now: &str,
        reason: &str,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        let reason = reason.to_string();
        self.call(move |conn| {
            let row: Option<(String, Option<String>, Option<String>)> = conn
                .query_row(
                    r#"
SELECT status, finished_at, summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((status, finished_at, summary_raw)) = row else {
                return Ok(false);
            };
            if finished_at.is_some() {
                return Ok(false);
            }

            // Always leave an audit trail so operators can tell why the job ended.
            conn.execute(
                r#"
INSERT INTO job_logs (job_id, ts, level, msg)
VALUES (?1, ?2, 'warn', ?3)
"#,
                params![
                    job_id,
                    now,
                    format!("job terminated: reason={reason} (previous_status={status})")
                ],
            )?;

            let mut summary: serde_json::Value =
                serde_json::from_str(&summary_raw.unwrap_or_else(|| "{}".to_string()))
                    .unwrap_or(serde_json::json!({}));
            if !summary.is_object() {
                summary = serde_json::json!({});
            }
            if let Some(obj) = summary.as_object_mut() {
                obj.insert(
                    "terminated".to_string(),
                    serde_json::json!({
                        "reason": reason,
                        "at": now,
                    }),
                );
            }

            let is_terminal = matches!(status.as_str(), "success" | "failed" | "rolled_back");
            if is_terminal {
                conn.execute(
                    r#"
UPDATE jobs
SET finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
            } else {
                conn.execute(
                    r#"
UPDATE jobs
SET status = 'failed', finished_at = ?2, summary_json = ?3
WHERE id = ?1
"#,
                    params![job_id, now, serde_json::to_string(&summary)?],
                )?;
            }

            Ok(true)
        })
        .await
        .context("terminate job as failed")
    }

    pub async fn get_job(&self, job_id: &str) -> anyhow::Result<Option<JobListItem>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  id,
  type,
  scope,
  stack_id,
  service_id,
  status,
  created_by,
  reason,
  created_at,
  started_at,
  finished_at,
  allow_arch_mismatch,
  backup_mode,
  summary_json
FROM jobs
WHERE id = ?1
"#,
                    params![job_id],
                    |row| {
                        let summary_json: String = row.get(13)?;
                        let summary: serde_json::Value = serde_json::from_str(&summary_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;
                        Ok(JobListItem {
                            id: row.get(0)?,
                            r#type: JobType::from_str(&row.get::<_, String>(1)?),
                            scope: JobScope::from_str(&row.get::<_, String>(2)?),
                            stack_id: row.get(3)?,
                            service_id: row.get(4)?,
                            status: row.get(5)?,
                            created_by: row.get(6)?,
                            reason: row.get(7)?,
                            created_at: row.get(8)?,
                            started_at: row.get(9)?,
                            finished_at: row.get(10)?,
                            allow_arch_mismatch: row.get::<_, i64>(11)? != 0,
                            backup_mode: row.get(12)?,
                            summary_json: summary,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get job")
    }

    pub async fn list_job_logs(&self, job_id: &str) -> anyhow::Result<Vec<JobLogLine>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT ts, level, msg
FROM job_logs
WHERE job_id = ?1
ORDER BY id DESC
LIMIT 500
"#,
            )?;

            let rows = stmt.query_map(params![job_id], |row| {
                Ok(JobLogLine {
                    ts: row.get(0)?,
                    level: row.get(1)?,
                    msg: row.get(2)?,
                })
            })?;
            let mut out = rows.collect::<Result<Vec<_>, _>>()?;
            // Return ascending order for UI consumption while keeping the query "tail"-friendly.
            out.reverse();
            Ok(out)
        })
        .await
        .context("list job logs")
    }

    pub async fn list_job_logs_since(
        &self,
        job_id: &str,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobLogRow>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, ts, level, msg
FROM job_logs
WHERE job_id = ?1 AND id > ?2
ORDER BY id ASC
LIMIT ?3
"#,
            )?;

            let rows = stmt.query_map(params![job_id, after_id, limit as i64], |row| {
                Ok(JobLogRow {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    level: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job logs since")
    }

    pub async fn list_job_event_logs_since(
        &self,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobEventLogRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, job_id, ts, msg
FROM job_logs
WHERE level = 'event' AND id > ?1
ORDER BY id ASC
LIMIT ?2
"#,
            )?;

            let rows = stmt.query_map(params![after_id, limit as i64], |row| {
                Ok(JobEventLogRow {
                    id: row.get(0)?,
                    job_id: row.get(1)?,
                    ts: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job event logs since")
    }

    pub async fn get_job_logs_last_id(&self, job_id: &str) -> anyhow::Result<i64> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let v: i64 = conn.query_row(
                r#"
SELECT COALESCE(MAX(id), 0)
FROM job_logs
WHERE job_id = ?1
"#,
                params![job_id],
                |row| row.get(0),
            )?;
            Ok(v)
        })
        .await
        .context("get job logs last id")
    }

    pub async fn get_job_logs_global_last_id(&self) -> anyhow::Result<i64> {
        self.call(move |conn| {
            let v: i64 = conn.query_row(
                r#"
SELECT COALESCE(MAX(id), 0)
FROM job_logs
WHERE level = 'event'
"#,
                [],
                |row| row.get(0),
            )?;
            Ok(v)
        })
        .await
        .context("get global job logs last id")
    }

    pub async fn insert_job_log(&self, job_id: &str, line: &JobLogLine) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let line = line.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO job_logs (job_id, ts, level, msg) VALUES (?1, ?2, ?3, ?4)",
                params![job_id, line.ts, line.level, line.msg],
            )?;
            Ok(())
        })
        .await
        .context("insert job log")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn list_jobs_returns_the_latest_two_thousand_jobs() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();

        for index in 0..=2_000 {
            db.insert_job(JobListItem {
                id: format!("job-{index:04}"),
                r#type: JobType::Update,
                scope: JobScope::Service,
                stack_id: Some("stack-test".to_string()),
                service_id: Some("service-test".to_string()),
                status: "success".to_string(),
                created_at: format!("2026-01-01T00:{:02}:{:02}Z", index / 60, index % 60),
                created_by: "test".to_string(),
                reason: "ui".to_string(),
                started_at: None,
                finished_at: None,
                allow_arch_mismatch: false,
                backup_mode: "inherit".to_string(),
                summary_json: serde_json::json!({}),
            })
            .await
            .unwrap();
        }

        let jobs = db.list_jobs().await.unwrap();

        assert_eq!(jobs.len(), 2_000);
        assert_eq!(jobs.first().map(|job| job.id.as_str()), Some("job-2000"));
        assert_eq!(jobs.last().map(|job| job.id.as_str()), Some("job-0001"));
        assert!(!jobs.iter().any(|job| job.id == "job-0000"));
    }
}
