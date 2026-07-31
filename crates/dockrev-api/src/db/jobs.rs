use super::*;
use std::time::{Duration, Instant};

const SLOW_JOB_CLAIM_WARN_THRESHOLD: Duration = Duration::from_millis(25);
const SLOW_JOB_CLAIM_WARN_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default)]
pub struct JobListFilters {
    pub types: Vec<String>,
    pub status: Option<String>,
    pub stack_id: Option<String>,
    pub service_id: Option<String>,
    pub cursor: Option<(String, String)>,
    pub limit: u32,
}

#[derive(Clone, Debug)]
pub struct JobListPage {
    pub jobs: Vec<JobListItem>,
    pub next_cursor: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct ServiceOperationTarget {
    pub(crate) service_id: String,
    pub(crate) stack_id: String,
}

const CLAIM_NEXT_QUEUED_JOB_SQL: &str = r#"
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
"#;

fn service_operation_job_blocks_targets(
    job: &JobListItem,
    persisted_service_ids: &[String],
    targets: &[ServiceOperationTarget],
) -> bool {
    if job.r#type.as_str() == "update"
        && job
            .summary_json
            .get("mode")
            .and_then(|value| value.as_str())
            == Some("dry-run")
    {
        return false;
    }

    if job.r#type.as_str() == "update" {
        if !persisted_service_ids.is_empty() {
            return targets.iter().any(|target| {
                persisted_service_ids
                    .iter()
                    .any(|service_id| service_id == &target.service_id)
            });
        }
        if job
            .summary_json
            .get("targets")
            .is_some_and(|value| value.is_array())
        {
            return false;
        }
    }

    targets.iter().any(|target| match job.r#type.as_str() {
        "rollback" | "service_lifecycle" => {
            job.service_id.as_deref() == Some(target.service_id.as_str())
        }
        "update" => match job.scope {
            JobScope::All => true,
            JobScope::Stack => job.stack_id.as_deref() == Some(target.stack_id.as_str()),
            JobScope::Service => job.service_id.as_deref() == Some(target.service_id.as_str()),
        },
        _ => false,
    })
}

fn is_better_service_operation_conflict(
    candidate: &JobListItem,
    current: Option<&JobListItem>,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    let candidate_rank = usize::from(candidate.status == "running");
    let current_rank = usize::from(current.status == "running");
    candidate_rank > current_rank
        || (candidate_rank == current_rank
            && (candidate.created_at > current.created_at
                || (candidate.created_at == current.created_at && candidate.id > current.id)))
}

fn should_emit_slow_job_claim_warning(
    warned_at_by_type: &std::sync::Mutex<BTreeMap<String, Instant>>,
    job_type: &str,
    elapsed: Duration,
    now: Instant,
) -> bool {
    if elapsed < SLOW_JOB_CLAIM_WARN_THRESHOLD {
        return false;
    }

    let mut warned_at_by_type = warned_at_by_type
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if warned_at_by_type
        .get(job_type)
        .is_some_and(|last_warned_at| {
            now.saturating_duration_since(*last_warned_at) < SLOW_JOB_CLAIM_WARN_INTERVAL
        })
    {
        return false;
    }

    warned_at_by_type.insert(job_type.to_string(), now);
    true
}

impl Db {
    #[cfg(test)]
    pub async fn list_jobs(&self) -> anyhow::Result<Vec<JobListItem>> {
        Ok(self
            .list_jobs_page(JobListFilters {
                limit: 2_000,
                ..Default::default()
            })
            .await?
            .jobs)
    }

    pub async fn insert_job(&self, job: JobListItem) -> anyhow::Result<()> {
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            insert_job_tx(&tx, &job)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("insert job")
    }

    /// Atomically reserves all requested services for a mutating service operation.
    ///
    /// Read-only update previews are intentionally excluded: they must remain usable
    /// while another service operation is in progress.
    pub async fn insert_service_operation_job_if_unblocked(
        &self,
        job: JobListItem,
        targets: Vec<ServiceOperationTarget>,
        initial_log: Option<JobLogLine>,
    ) -> anyhow::Result<Option<JobListItem>> {
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidates = {
                let mut statement = tx.prepare(
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
WHERE type IN ('update', 'rollback', 'service_lifecycle')
  AND status IN ('queued', 'running')
ORDER BY
  CASE status WHEN 'running' THEN 0 ELSE 1 END,
  created_at DESC,
  id DESC
"#,
                )?;
                statement
                    .query_map([], map_job_list_item_row)?
                    .collect::<Result<Vec<_>, _>>()?
            };

            let mut conflict: Option<JobListItem> = None;
            for candidate in candidates {
                let persisted_service_ids = {
                    let mut statement =
                        tx.prepare("SELECT service_id FROM job_service_targets WHERE job_id = ?1")?;
                    statement
                        .query_map([&candidate.id], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?
                };
                if service_operation_job_blocks_targets(
                    &candidate,
                    &persisted_service_ids,
                    &targets,
                ) && is_better_service_operation_conflict(&candidate, conflict.as_ref())
                {
                    conflict = Some(candidate);
                }
            }

            if conflict.is_none() {
                insert_job_tx(&tx, &job)?;
                if let Some(line) = initial_log {
                    tx.execute(
                        "INSERT INTO job_logs (job_id, ts, level, msg) VALUES (?1, ?2, ?3, ?4)",
                        params![job.id, line.ts, line.level, line.msg],
                    )?;
                }
            }
            tx.commit()?;
            Ok(conflict)
        })
        .await
        .context("atomically insert service operation job")
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
        let query_job_type = job_type.clone();
        let started_at = started_at.to_string();
        let claim_started_at = Instant::now();
        let result = self
            .call(move |conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let item: Option<JobListItem> = tx
                    .query_row(
                        CLAIM_NEXT_QUEUED_JOB_SQL,
                        params![query_job_type],
                        map_job_list_item_row,
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
            .context("claim next queued job by type");

        let elapsed = claim_started_at.elapsed();
        if let Ok(item) = result.as_ref()
            && should_emit_slow_job_claim_warning(
                &self.slow_job_claim_warnings,
                &job_type,
                elapsed,
                Instant::now(),
            )
        {
            tracing::warn!(
                job_type = %job_type,
                duration_ms = elapsed.as_millis() as u64,
                claimed = item.is_some(),
                threshold_ms = SLOW_JOB_CLAIM_WARN_THRESHOLD.as_millis() as u64,
                "slow queued job claim"
            );
        }

        result
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
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                r#"
UPDATE jobs
SET status = ?2, finished_at = ?3, summary_json = ?4
WHERE id = ?1
"#,
                params![job_id, status, finished_at, summary_json_str],
            )?;
            let direct_service_id = tx
                .query_row(
                    "SELECT service_id FROM jobs WHERE id = ?1",
                    params![&job_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            replace_job_service_targets_tx(
                &tx,
                &job_id,
                direct_service_id.as_deref(),
                &summary_json,
            )?;
            if status == "success"
                && previous
                    .as_ref()
                    .is_some_and(|(job_type, _)| job_type == "check")
            {
                new_version_discoveries::record_new_version_discoveries_from_summary_conn(
                    &tx,
                    &job_id,
                    &finished_at,
                    &summary_json,
                )?;
            }
            tx.commit()?;
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

    pub async fn list_jobs_page(&self, filters: JobListFilters) -> anyhow::Result<JobListPage> {
        let limit = filters.limit.clamp(1, 2_000);
        self.call(move |conn| {
            let mut where_clauses = vec!["1 = 1".to_string()];
            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            if !filters.types.is_empty() {
                let placeholders = std::iter::repeat_n("?", filters.types.len())
                    .collect::<Vec<_>>()
                    .join(",");
                where_clauses.push(format!("j.type IN ({placeholders})"));
                values.extend(filters.types.iter().cloned().map(rusqlite::types::Value::from));
            }
            if let Some(status) = filters.status {
                where_clauses.push("j.status = ?".to_string());
                values.push(status.into());
            }
            if let Some(stack_id) = filters.stack_id {
                where_clauses.push(
                    "(j.stack_id = ? OR EXISTS (SELECT 1 FROM job_service_targets jst JOIN services target_service ON target_service.id = jst.service_id WHERE jst.job_id = j.id AND target_service.stack_id = ?))".to_string(),
                );
                values.push(stack_id.clone().into());
                values.push(stack_id.into());
            }
            if let Some(service_id) = filters.service_id {
                where_clauses.push(
                    "EXISTS (SELECT 1 FROM job_service_targets jst WHERE jst.job_id = j.id AND jst.service_id = ?)".to_string(),
                );
                values.push(service_id.into());
            }
            if let Some((created_at, id)) = filters.cursor {
                where_clauses.push("(j.created_at < ? OR (j.created_at = ? AND j.id < ?))".to_string());
                values.push(created_at.clone().into());
                values.push(created_at.into());
                values.push(id.into());
            }
            values.push(((limit + 1) as i64).into());
            let sql = format!(
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
WHERE {}
ORDER BY j.created_at DESC, j.id DESC
LIMIT ?
"#,
                where_clauses.join(" AND ")
            );
            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|value| value as &dyn rusqlite::ToSql).collect();
            let mut stmt = conn.prepare(&sql)?;
            let mut jobs = stmt
                .query_map(params.as_slice(), map_job_list_item_row)?
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if jobs.len() > limit as usize {
                jobs.truncate(limit as usize);
                jobs.last()
                    .map(|job| (job.created_at.clone(), job.id.clone()))
            } else {
                None
            };
            Ok(JobListPage { jobs, next_cursor })
        })
        .await
        .context("list jobs page")
    }

    pub async fn purge_expired_terminal_jobs(
        &self,
        older_than: &str,
        batch_size: u32,
    ) -> anyhow::Result<u64> {
        let older_than = older_than.to_string();
        let batch_size = batch_size.clamp(1, 10_000) as i64;
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
DELETE FROM jobs
WHERE id IN (
  SELECT id
  FROM jobs
  WHERE status IN ('success', 'failed', 'rolled_back')
    AND COALESCE(finished_at, created_at) < ?1
  ORDER BY COALESCE(finished_at, created_at) ASC, id ASC
  LIMIT ?2
)
"#,
                params![older_than, batch_size],
            )? as u64)
        })
        .await
        .context("purge expired terminal jobs")
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

    pub async fn find_latest_pending_update_blocking_service(
        &self,
        stack_id: &str,
        service_id: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let stack_id = stack_id.to_string();
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  j.id, j.type, j.scope, j.stack_id, j.service_id, j.status,
  j.created_by, j.reason, j.created_at, j.started_at, j.finished_at,
  j.allow_arch_mismatch, j.backup_mode, j.summary_json
FROM jobs j
WHERE j.type = 'update'
  AND j.status IN ('queued', 'running')
  AND COALESCE(json_extract(j.summary_json, '$.mode'), '') != 'dry-run'
  AND (
    EXISTS (
      SELECT 1 FROM job_service_targets jst
      WHERE jst.job_id = j.id AND jst.service_id = ?2
    )
    OR (
      NOT EXISTS (SELECT 1 FROM job_service_targets jst WHERE jst.job_id = j.id)
      AND json_type(j.summary_json, '$.targets') IS NULL
      AND (
        j.scope = 'all'
        OR (j.scope = 'stack' AND j.stack_id = ?1)
        OR (j.scope = 'service' AND j.service_id = ?2)
      )
    )
  )
ORDER BY
  CASE j.status WHEN 'running' THEN 0 ELSE 1 END,
  j.created_at DESC,
  j.id DESC
LIMIT 1
"#,
                params![stack_id, service_id],
                |row| {
                    let summary_json: String = row.get(13)?;
                    let summary = serde_json::from_str(&summary_json).map_err(|e| {
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
        .context("find latest pending update blocking service")
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
#[path = "jobs_tests.rs"]
mod tests;
