use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ServiceOperationTarget {
    pub(crate) service_id: String,
    pub(crate) stack_id: String,
}

fn blocks_targets(
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
    if job.r#type.as_str() == "stack_lifecycle" && !persisted_service_ids.is_empty() {
        return targets.iter().any(|target| {
            persisted_service_ids
                .iter()
                .any(|service_id| service_id == &target.service_id)
        });
    }
    targets.iter().any(|target| match job.r#type.as_str() {
        "rollback" | "service_lifecycle" => {
            job.service_id.as_deref() == Some(target.service_id.as_str())
        }
        "update" | "stack_lifecycle" => match job.scope {
            JobScope::All => true,
            JobScope::Stack => job.stack_id.as_deref() == Some(target.stack_id.as_str()),
            JobScope::Service => job.service_id.as_deref() == Some(target.service_id.as_str()),
        },
        _ => false,
    })
}

fn better_conflict(candidate: &JobListItem, current: Option<&JobListItem>) -> bool {
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

impl Db {
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
SELECT id, type, scope, stack_id, service_id, status, created_by, reason, created_at,
  started_at, finished_at, allow_arch_mismatch, backup_mode, summary_json
FROM jobs
WHERE type IN ('update', 'rollback', 'service_lifecycle', 'stack_lifecycle') AND status IN ('queued', 'running')
ORDER BY CASE status WHEN 'running' THEN 0 ELSE 1 END, created_at DESC, id DESC
"#,
                )?;
                statement.query_map([], map_job_list_item_row)?.collect::<Result<Vec<_>, _>>()?
            };
            let mut conflict: Option<JobListItem> = None;
            for candidate in candidates {
                let persisted_service_ids = {
                    let mut statement = tx.prepare("SELECT service_id FROM job_service_targets WHERE job_id = ?1")?;
                    statement.query_map([&candidate.id], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?
                };
                if blocks_targets(&candidate, &persisted_service_ids, &targets) && better_conflict(&candidate, conflict.as_ref()) {
                    conflict = Some(candidate);
                }
            }
            if conflict.is_none() {
                insert_job_tx(&tx, &job)?;
                for target in &targets {
                    tx.execute(
                        "INSERT OR IGNORE INTO job_service_targets (job_id, service_id) SELECT ?1, id FROM services WHERE id = ?2",
                        params![&job.id, &target.service_id],
                    )?;
                }
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
SELECT j.id, j.type, j.scope, j.stack_id, j.service_id, j.status,
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
ORDER BY CASE j.status WHEN 'running' THEN 0 ELSE 1 END, j.created_at DESC, j.id DESC
LIMIT 1
"#,
                params![stack_id, service_id],
                map_job_list_item_row,
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending update blocking service")
    }

    pub async fn find_latest_pending_stack_lifecycle_blocking_service(
        &self,
        stack_id: &str,
        service_id: &str,
    ) -> anyhow::Result<Option<JobListItem>> {
        let stack_id = stack_id.to_string();
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT j.id, j.type, j.scope, j.stack_id, j.service_id, j.status,
  j.created_by, j.reason, j.created_at, j.started_at, j.finished_at,
  j.allow_arch_mismatch, j.backup_mode, j.summary_json
FROM jobs j
WHERE j.type = 'stack_lifecycle'
  AND j.status IN ('queued', 'running')
  AND (
    EXISTS (
      SELECT 1 FROM job_service_targets jst
      WHERE jst.job_id = j.id AND jst.service_id = ?2
    )
    OR (
      NOT EXISTS (SELECT 1 FROM job_service_targets jst WHERE jst.job_id = j.id)
      AND j.scope = 'stack' AND j.stack_id = ?1
    )
  )
ORDER BY CASE j.status WHEN 'running' THEN 0 ELSE 1 END, j.created_at DESC, j.id DESC
LIMIT 1
"#,
                params![stack_id, service_id],
                map_job_list_item_row,
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("find latest pending stack lifecycle blocking service")
    }
}
