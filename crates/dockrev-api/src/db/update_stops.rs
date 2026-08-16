use super::*;

#[derive(Clone, Debug)]
pub(crate) struct UpdateStopControl {
    pub apply_committed_at: Option<String>,
    pub stop_requested_at: Option<String>,
    pub stop_requested_by: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingUpdateStopRecovery {
    pub job_id: String,
    pub snapshot_json: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UpdateStopRequestOutcome {
    Requested,
    AlreadyRequested,
    ApplyCommitted,
    Ineligible,
}

impl Db {
    pub async fn create_update_stop_control(&self, job_id: &str, now: &str) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO update_job_stop_controls (job_id, updated_at) VALUES (?1, ?2)",
                params![job_id, now],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_update_stop_control(
        &self,
        job_id: &str,
    ) -> anyhow::Result<Option<UpdateStopControl>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT apply_committed_at, stop_requested_at, stop_requested_by FROM update_job_stop_controls WHERE job_id = ?1",
                params![job_id],
                |row| {
                    Ok(UpdateStopControl {
                        apply_committed_at: row.get(0)?,
                        stop_requested_at: row.get(1)?,
                        stop_requested_by: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
    }

    pub async fn request_update_job_stop(
        &self,
        job_id: &str,
        principal: &str,
        now: &str,
    ) -> anyhow::Result<UpdateStopRequestOutcome> {
        let job_id = job_id.to_string();
        let principal = principal.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let job = tx
                .query_row(
                    "SELECT type, status, summary_json FROM jobs WHERE id = ?1",
                    params![job_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
                )
                .optional()?;
            let eligible = job.is_some_and(|(job_type, status, summary)| {
                job_type == "update"
                    && status == "running"
                    && serde_json::from_str::<serde_json::Value>(&summary)
                        .ok()
                        .and_then(|value| value.get("mode").and_then(serde_json::Value::as_str).map(str::to_string))
                        .as_deref()
                        == Some("apply")
            });
            if !eligible {
                tx.commit()?;
                return Ok(UpdateStopRequestOutcome::Ineligible);
            }
            let changed = tx.execute(
                "UPDATE update_job_stop_controls SET stop_requested_at = ?2, stop_requested_by = ?3, updated_at = ?2 WHERE job_id = ?1 AND stop_requested_at IS NULL AND apply_committed_at IS NULL",
                params![job_id, now, principal],
            )?;
            if changed == 1 {
                tx.commit()?;
                return Ok(UpdateStopRequestOutcome::Requested);
            }
            let control = tx.query_row(
                "SELECT apply_committed_at, stop_requested_at FROM update_job_stop_controls WHERE job_id = ?1",
                params![job_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            ).optional()?;
            tx.commit()?;
            Ok(match control {
                Some((Some(_), _)) => UpdateStopRequestOutcome::ApplyCommitted,
                Some((None, Some(_))) => UpdateStopRequestOutcome::AlreadyRequested,
                _ => UpdateStopRequestOutcome::Ineligible,
            })
        }).await
    }

    pub async fn commit_update_job_apply(&self, job_id: &str, now: &str) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                "UPDATE update_job_stop_controls SET apply_committed_at = ?2, updated_at = ?2 WHERE job_id = ?1 AND apply_committed_at IS NULL AND stop_requested_at IS NULL",
                params![job_id, now],
            )? == 1)
        }).await
    }

    pub async fn save_update_stop_recovery_snapshot(
        &self,
        job_id: &str,
        snapshot: &crate::backup::BackupRecoverySnapshot,
        now: &str,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let snapshot = serde_json::to_string(snapshot)?;
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE update_job_stop_controls SET recovery_snapshot_json = ?2, recovery_attempted_at = NULL, recovery_error = NULL, updated_at = ?3 WHERE job_id = ?1",
                params![job_id, snapshot, now],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn clear_update_stop_recovery_snapshot(
        &self,
        job_id: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE update_job_stop_controls SET recovery_snapshot_json = NULL, recovery_error = NULL, updated_at = ?2 WHERE job_id = ?1",
                params![job_id, now],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn claim_pending_update_stop_recoveries(
        &self,
        now: &str,
    ) -> anyhow::Result<Vec<PendingUpdateStopRecovery>> {
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let entries = {
                let mut stmt = tx.prepare(
                    "SELECT job_id, recovery_snapshot_json FROM update_job_stop_controls WHERE recovery_snapshot_json IS NOT NULL AND recovery_attempted_at IS NULL",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(PendingUpdateStopRecovery {
                        job_id: row.get(0)?,
                        snapshot_json: row.get(1)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for entry in &entries {
                tx.execute(
                    "UPDATE update_job_stop_controls SET recovery_attempted_at = ?2, updated_at = ?2 WHERE job_id = ?1 AND recovery_attempted_at IS NULL",
                    params![entry.job_id, now],
                )?;
            }
            tx.commit()?;
            Ok(entries)
        })
        .await
    }

    pub async fn record_update_stop_recovery_error(
        &self,
        job_id: &str,
        error: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let error = error.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE update_job_stop_controls SET recovery_error = ?2, updated_at = ?3 WHERE job_id = ?1",
                params![job_id, error, now],
            )?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::api::types::{JobListItem, JobScope, JobType};

    fn update_job(id: &str) -> JobListItem {
        JobListItem {
            id: id.to_string(),
            r#type: JobType::Update,
            scope: JobScope::Service,
            stack_id: Some("stack-1".to_string()),
            service_id: Some("service-1".to_string()),
            status: "running".to_string(),
            created_at: "2026-08-16T00:00:00Z".to_string(),
            created_by: "ivan".to_string(),
            reason: "ui".to_string(),
            started_at: Some("2026-08-16T00:00:00Z".to_string()),
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({ "mode": "apply" }),
        }
    }

    #[tokio::test]
    async fn stop_request_and_apply_commit_are_mutually_exclusive() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.insert_job(update_job("job-stop")).await.unwrap();
        db.create_update_stop_control("job-stop", "2026-08-16T00:00:00Z")
            .await
            .unwrap();

        assert_eq!(
            db.request_update_job_stop("job-stop", "alice", "2026-08-16T00:01:00Z")
                .await
                .unwrap(),
            UpdateStopRequestOutcome::Requested
        );
        assert!(
            !db.commit_update_job_apply("job-stop", "2026-08-16T00:02:00Z")
                .await
                .unwrap()
        );
        assert_eq!(
            db.request_update_job_stop("job-stop", "alice", "2026-08-16T00:03:00Z")
                .await
                .unwrap(),
            UpdateStopRequestOutcome::AlreadyRequested
        );
    }

    #[tokio::test]
    async fn apply_commit_locks_stop_and_recovery_is_claimed_once() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        db.insert_job(update_job("job-apply")).await.unwrap();
        db.create_update_stop_control("job-apply", "2026-08-16T00:00:00Z")
            .await
            .unwrap();

        assert!(
            db.commit_update_job_apply("job-apply", "2026-08-16T00:01:00Z")
                .await
                .unwrap()
        );
        assert_eq!(
            db.request_update_job_stop("job-apply", "alice", "2026-08-16T00:02:00Z")
                .await
                .unwrap(),
            UpdateStopRequestOutcome::ApplyCommitted
        );

        let snapshot = crate::backup::BackupRecoverySnapshot {
            stack_id: "stack-1".to_string(),
            services: vec!["web".to_string()],
        };
        db.save_update_stop_recovery_snapshot("job-apply", &snapshot, "2026-08-16T00:03:00Z")
            .await
            .unwrap();
        assert_eq!(
            db.claim_pending_update_stop_recoveries("2026-08-16T00:04:00Z")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.claim_pending_update_stop_recoveries("2026-08-16T00:05:00Z")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
