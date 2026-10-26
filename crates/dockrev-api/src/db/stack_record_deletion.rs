use super::*;

fn summary_artifact_path(summary_json: &str, stack_id: &str) -> Option<String> {
    let summary = serde_json::from_str::<serde_json::Value>(summary_json).ok()?;
    summary
        .get("stacks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|stack| stack.get("stackId").and_then(serde_json::Value::as_str) == Some(stack_id))
        .and_then(|stack| stack.get("backup"))
        .and_then(|backup| backup.get("artifactPath"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Clone, Debug)]
pub(crate) struct StackRecordDeletionBackupArtifact {
    pub artifact_path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct StackRecordDeletionSnapshot {
    pub stack_id: String,
    pub stack_name: String,
    pub compose_files: Vec<String>,
    pub missing: bool,
    pub has_pending_job: bool,
    pub backup_artifacts: Vec<StackRecordDeletionBackupArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StackRecordDeletionCommit {
    Deleted,
    AlreadyAbsent,
    Conflict(String),
}

fn insert_stack_record_deletion_audit_tx(
    tx: &rusqlite::Transaction<'_>,
    stack_id: &str,
    actor_principal: &str,
    outcome: &str,
    occurred_at: &str,
    backup_artifacts_json: &str,
    pending_artifact_cleanup: bool,
) -> rusqlite::Result<()> {
    tx.execute(
        r#"
INSERT INTO stack_record_deletion_audits (
  id, stack_id, actor_principal, outcome, deleted_at,
  backup_artifacts_json, pending_artifact_cleanup, cleanup_error
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
"#,
        params![
            ulid::Ulid::new().to_string(),
            stack_id,
            actor_principal,
            outcome,
            occurred_at,
            backup_artifacts_json,
            pending_artifact_cleanup as i64,
        ],
    )?;
    Ok(())
}

impl Db {
    pub async fn record_stack_record_deletion_attempt(
        &self,
        stack_id: &str,
        actor_principal: &str,
        outcome: &str,
        occurred_at: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let actor_principal = actor_principal.to_string();
        let outcome = outcome.to_string();
        let occurred_at = occurred_at.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            insert_stack_record_deletion_audit_tx(
                &tx,
                &stack_id,
                &actor_principal,
                &outcome,
                &occurred_at,
                "[]",
                false,
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("record stack record deletion attempt")
    }

    pub async fn record_stack_record_deletion_already_absent(
        &self,
        stack_id: &str,
        actor_principal: &str,
        occurred_at: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let actor_principal = actor_principal.to_string();
        let occurred_at = occurred_at.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let already_audited = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM stack_record_deletion_audits WHERE stack_id = ?1 AND outcome IN ('deleted', 'already_absent'))",
                    params![stack_id.clone()],
                    |row| Ok(row.get::<_, i64>(0)? != 0),
                )?;
            if !already_audited {
                insert_stack_record_deletion_audit_tx(
                    &tx,
                    &stack_id,
                    &actor_principal,
                    "already_absent",
                    &occurred_at,
                    "[]",
                    false,
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("record already absent stack record deletion")
    }

    pub async fn begin_stack_record_deletion(
        &self,
        stack_id: &str,
        actor_principal: &str,
        occurred_at: &str,
        backup_artifacts_json: &str,
        pending_artifact_cleanup: bool,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let actor_principal = actor_principal.to_string();
        let occurred_at = occurred_at.to_string();
        let backup_artifacts_json = backup_artifacts_json.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            insert_stack_record_deletion_audit_tx(
                &tx,
                &stack_id,
                &actor_principal,
                "staging",
                &occurred_at,
                &backup_artifacts_json,
                pending_artifact_cleanup,
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("begin stack record deletion")
    }

    pub async fn pending_stack_record_deletion_intent(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT backup_artifacts_json FROM stack_record_deletion_audits WHERE stack_id = ?1 AND outcome = 'staging' ORDER BY deleted_at DESC, id DESC LIMIT 1",
                    params![stack_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
        .context("load pending stack record deletion intent")
    }

    pub async fn finish_stack_record_deletion_intent(
        &self,
        stack_id: &str,
        outcome: &str,
        pending_artifact_cleanup: bool,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let outcome = outcome.to_string();
        let error = error.map(ToOwned::to_owned);
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE stack_record_deletion_audits
SET outcome = ?2, pending_artifact_cleanup = ?3, cleanup_error = ?4
WHERE id = (
  SELECT id
  FROM stack_record_deletion_audits
  WHERE stack_id = ?1 AND outcome = 'staging'
  ORDER BY deleted_at DESC, id DESC
  LIMIT 1
)
"#,
                params![stack_id, outcome, pending_artifact_cleanup as i64, error],
            )?;
            Ok(())
        })
        .await
        .context("finish stack record deletion intent")
    }

    pub async fn pending_stack_record_deletion_cleanup(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT backup_artifacts_json FROM stack_record_deletion_audits WHERE stack_id = ?1 AND outcome IN ('deleted', 'already_absent') AND pending_artifact_cleanup = 1 ORDER BY deleted_at DESC LIMIT 1",
                    params![stack_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
        .context("load pending stack record deletion cleanup")
    }

    pub async fn complete_stack_record_deletion_cleanup(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE stack_record_deletion_audits SET pending_artifact_cleanup = 0, cleanup_error = NULL WHERE stack_id = ?1 AND outcome IN ('deleted', 'already_absent')",
                params![stack_id],
            )?;
            Ok(())
        })
        .await
        .context("complete stack record deletion cleanup")
    }

    pub async fn stack_record_deletion_snapshot(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Option<StackRecordDeletionSnapshot>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let Some((stack_name, compose_files_json, missing, has_pending_job)) = conn
                .query_row(
                    r#"
SELECT
  s.name,
  s.compose_files_json,
  EXISTS (
    SELECT 1 FROM discovered_compose_projects d
    WHERE d.stack_id = s.id AND d.status = 'missing'
  ) AND NOT EXISTS (
    SELECT 1 FROM discovered_compose_projects d
    WHERE d.stack_id = s.id AND d.status <> 'missing'
  ),
  EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.status IN ('queued', 'running')
      AND (
        j.stack_id = s.id
        OR j.service_id IN (SELECT id FROM services WHERE stack_id = s.id)
        OR EXISTS (
          SELECT 1
          FROM job_service_targets target
          WHERE target.job_id = j.id
            AND target.service_id IN (SELECT id FROM services WHERE stack_id = s.id)
        )
      )
  )
FROM stacks s
WHERE s.id = ?1
"#,
                    params![stack_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)? != 0,
                            row.get::<_, i64>(3)? != 0,
                        ))
                    },
                )
                .optional()?
            else {
                return Ok(None);
            };

            let compose_files = serde_json::from_str::<Vec<String>>(&compose_files_json)
                .map_err(|error| anyhow::anyhow!("decode stack compose files: {error}"))?;
            let mut stmt = conn.prepare(
                r#"
SELECT b.artifact_path, COALESCE(j.summary_json, '{}')
FROM backups b
LEFT JOIN jobs j ON j.id = b.job_id
WHERE b.stack_id = ?1
ORDER BY b.created_at DESC, b.id DESC
"#,
            )?;
            let artifacts = stmt
                .query_map(params![stack_id.clone()], |row| {
                    let path = row
                        .get::<_, Option<String>>(0)?
                        .filter(|value| !value.trim().is_empty())
                        .or_else(|| {
                            row.get::<_, String>(1)
                                .ok()
                                .and_then(|summary| summary_artifact_path(&summary, &stack_id))
                        });
                    Ok(path
                        .map(|artifact_path| StackRecordDeletionBackupArtifact { artifact_path }))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect();

            Ok(Some(StackRecordDeletionSnapshot {
                stack_id,
                stack_name,
                compose_files,
                missing,
                has_pending_job,
                backup_artifacts: artifacts,
            }))
        })
        .await
        .context("load stack record deletion snapshot")
    }

    pub async fn commit_stack_record_deletion(
        &self,
        stack_id: &str,
        actor_principal: &str,
        deleted_at: &str,
        backup_artifacts_json: &str,
        pending_artifact_cleanup: bool,
    ) -> anyhow::Result<StackRecordDeletionCommit> {
        let stack_id = stack_id.to_string();
        let actor_principal = actor_principal.to_string();
        let deleted_at = deleted_at.to_string();
        let backup_artifacts_json = backup_artifacts_json.to_string();
        self.call(move |conn| {
            if conn
                .query_row(
                    "SELECT 1 FROM stack_record_deletion_audits WHERE stack_id = ?1 AND outcome = 'deleted' LIMIT 1",
                    params![stack_id.clone()],
                    |_| Ok(()),
                )
                .optional()?
                .is_some()
            {
                return Ok(StackRecordDeletionCommit::AlreadyAbsent);
            }

            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let Some((_stack_name, missing, has_pending_job)) = tx
                .query_row(
                    r#"
SELECT
  s.name,
  EXISTS (
    SELECT 1 FROM discovered_compose_projects d
    WHERE d.stack_id = s.id AND d.status = 'missing'
  ) AND NOT EXISTS (
    SELECT 1 FROM discovered_compose_projects d
    WHERE d.stack_id = s.id AND d.status <> 'missing'
  ),
  EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.status IN ('queued', 'running')
      AND (
        j.stack_id = s.id
        OR j.service_id IN (SELECT id FROM services WHERE stack_id = s.id)
        OR EXISTS (
          SELECT 1
          FROM job_service_targets target
          WHERE target.job_id = j.id
            AND target.service_id IN (SELECT id FROM services WHERE stack_id = s.id)
        )
      )
  )
FROM stacks s
WHERE s.id = ?1
"#,
                    params![stack_id.clone()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)? != 0,
                            row.get::<_, i64>(2)? != 0,
                        ))
                    },
                )
                .optional()?
            else {
                return Ok(StackRecordDeletionCommit::AlreadyAbsent);
            };

            if !missing {
                insert_stack_record_deletion_audit_tx(
                    &tx,
                    &stack_id,
                    &actor_principal,
                    "conflict",
                    &deleted_at,
                    "[]",
                    false,
                )?;
                tx.commit()?;
                return Ok(StackRecordDeletionCommit::Conflict(
                    "stack is no longer missing".to_string(),
                ));
            }
            if has_pending_job {
                insert_stack_record_deletion_audit_tx(
                    &tx,
                    &stack_id,
                    &actor_principal,
                    "conflict",
                    &deleted_at,
                    "[]",
                    false,
                )?;
                tx.commit()?;
                return Ok(StackRecordDeletionCommit::Conflict(
                    "stack has a pending operation".to_string(),
                ));
            }

            // A stack-scoped job can retain its service targets even when the
            // legacy stack_id/service_id columns are empty. Treat those targets
            // as ownership for the record deletion transaction as well.
            let related_jobs = r#"
SELECT j.id
FROM jobs j
WHERE j.stack_id = ?1
   OR j.service_id IN (SELECT id FROM services WHERE stack_id = ?1)
   OR EXISTS (
     SELECT 1
     FROM job_service_targets target
     WHERE target.job_id = j.id
       AND target.service_id IN (SELECT id FROM services WHERE stack_id = ?1)
   )
"#;
            tx.execute(
                &format!(
                    "DELETE FROM notification_dispatch_outbox WHERE job_id IN ({related_jobs})"
                ),
                params![stack_id.clone()],
            )?;
            tx.execute(
                &format!(
                    "DELETE FROM github_packages_deliveries WHERE (job_id IN ({related_jobs}) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(github_packages_deliveries.job_ids_json) THEN github_packages_deliveries.job_ids_json ELSE '[]' END) ids WHERE ids.value IN ({related_jobs}))) AND (job_id IN ({related_jobs}) OR job_id IS NULL) AND NOT EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(github_packages_deliveries.job_ids_json) THEN github_packages_deliveries.job_ids_json ELSE '[]' END) ids WHERE ids.value NOT IN ({related_jobs}))"
                ),
                params![stack_id.clone()],
            )?;
            tx.execute(
                &format!(
                    "UPDATE github_packages_deliveries SET job_id = CASE WHEN job_id IN ({related_jobs}) THEN NULL ELSE job_id END, job_ids_json = (SELECT COALESCE(json_group_array(ids.value), '[]') FROM json_each(CASE WHEN json_valid(github_packages_deliveries.job_ids_json) THEN github_packages_deliveries.job_ids_json ELSE '[]' END) ids WHERE ids.value NOT IN ({related_jobs})) WHERE job_id IN ({related_jobs}) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(github_packages_deliveries.job_ids_json) THEN github_packages_deliveries.job_ids_json ELSE '[]' END) ids WHERE ids.value IN ({related_jobs}))"
                ),
                params![stack_id.clone()],
            )?;
            tx.execute(
                &format!(
                    "DELETE FROM notification_items WHERE source_job_id IN ({related_jobs}) OR id IN (SELECT notification_item_id FROM new_version_notifications WHERE service_id IN (SELECT id FROM services WHERE stack_id = ?1)) OR id IN (SELECT notification_item_id FROM notification_anomaly_occurrences WHERE source_job_id IN ({related_jobs}))"
                ),
                params![stack_id.clone()],
            )?;
            tx.execute(
                &format!(
                    "DELETE FROM notification_anomaly_occurrences WHERE source_job_id IN ({related_jobs})"
                ),
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM auto_update_pending WHERE stack_id = ?1 OR service_id IN (SELECT id FROM services WHERE stack_id = ?1)",
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM auto_update_candidates WHERE stack_id = ?1 OR service_id IN (SELECT id FROM services WHERE stack_id = ?1)",
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM auto_update_policies WHERE (scope_type = 'stack' AND scope_id = ?1) OR (scope_type = 'service' AND scope_id IN (SELECT id FROM services WHERE stack_id = ?1))",
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM ignore_rules WHERE scope_service_id IN (SELECT id FROM services WHERE stack_id = ?1)",
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM new_version_notifications WHERE service_id IN (SELECT id FROM services WHERE stack_id = ?1)",
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM service_new_version_discoveries WHERE service_id IN (SELECT id FROM services WHERE stack_id = ?1)",
                params![stack_id.clone()],
            )?;
            tx.execute(
                &format!("DELETE FROM jobs WHERE id IN ({related_jobs})"),
                params![stack_id.clone()],
            )?;
            tx.execute(
                "DELETE FROM discovered_compose_projects WHERE stack_id = ?1",
                params![stack_id],
            )?;
            tx.execute("DELETE FROM stacks WHERE id = ?1", params![stack_id.clone()])?;
            let updated_intent = tx.execute(
                r#"
UPDATE stack_record_deletion_audits
SET actor_principal = ?2,
    outcome = 'deleted',
    deleted_at = ?3,
    backup_artifacts_json = ?4,
    pending_artifact_cleanup = ?5,
    cleanup_error = NULL
WHERE id = (
  SELECT id
  FROM stack_record_deletion_audits
  WHERE stack_id = ?1 AND outcome = 'staging'
  ORDER BY deleted_at DESC, id DESC
  LIMIT 1
)
"#,
                params![
                    stack_id,
                    actor_principal,
                    deleted_at,
                    backup_artifacts_json,
                    pending_artifact_cleanup as i64,
                ],
            )?;
            if updated_intent == 0 {
                insert_stack_record_deletion_audit_tx(
                    &tx,
                    &stack_id,
                    &actor_principal,
                    "deleted",
                    &deleted_at,
                    &backup_artifacts_json,
                    pending_artifact_cleanup,
                )?;
            }
            tx.commit()?;
            Ok(StackRecordDeletionCommit::Deleted)
        })
        .await
        .context("commit stack record deletion")
    }

    pub async fn mark_stack_record_deletion_cleanup_error(
        &self,
        stack_id: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let error = error.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE stack_record_deletion_audits
SET pending_artifact_cleanup = 1, cleanup_error = ?2
WHERE stack_id = ?1 AND outcome IN ('deleted', 'already_absent')
"#,
                params![stack_id, error],
            )?;
            Ok(())
        })
        .await
        .context("record stack deletion artifact cleanup error")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::api::types::{
        ComposeConfig, JobListItem, JobScope, JobType, ServiceSeed, StackBackupConfig, StackRecord,
    };
    use serde_json::json;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dockrev-stack-record-deletion-db-{}.sqlite3",
            ulid::Ulid::new()
        ))
    }

    fn now() -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    }

    async fn seed_stack(db: &Db, stack_id: &str, service_id: &str, status: &str) -> String {
        let compose_file = std::env::temp_dir()
            .join(format!("dockrev-missing-compose-{}.yml", ulid::Ulid::new()))
            .display()
            .to_string();
        let timestamp = now();
        db.insert_stack(
            &StackRecord {
                id: stack_id.to_string(),
                name: "missing-media".to_string(),
                archived: false,
                compose: ComposeConfig {
                    kind: "path".to_string(),
                    compose_files: vec![compose_file.clone()],
                    env_file: None,
                },
                backup: StackBackupConfig::default(),
                services: Vec::new(),
            },
            &[ServiceSeed {
                id: service_id.to_string(),
                name: "web".to_string(),
                image_ref: "ghcr.io/example/web".to_string(),
                image_tag: "latest".to_string(),
                homepage: None,
                update_guard: None,
                auto_rollback: true,
                backup_bind_paths: BTreeMap::new(),
                backup_volume_names: BTreeMap::new(),
            }],
            &timestamp,
        )
        .await
        .unwrap();
        db.upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
            project: format!("project-{stack_id}"),
            stack_id: Some(stack_id.to_string()),
            status: status.to_string(),
            last_seen_at: None,
            last_scan_at: timestamp,
            last_error: (status == "missing").then(|| "compose_files_missing".to_string()),
            last_config_files: Some(vec![compose_file]),
            unarchive_if_active: false,
        })
        .await
        .unwrap();
        stack_id.to_string()
    }

    #[tokio::test]
    async fn deletion_removes_owned_metadata_and_keeps_audit_idempotently() {
        let path = temp_db_path();
        let db = Db::open(&path).await.unwrap();
        let stack_id = "stack-record-delete";
        let service_id = "service-record-delete";
        let stack_id = seed_stack(&db, stack_id, service_id, "missing").await;
        let project = format!("project-{stack_id}");
        let secondary_project = format!("project-{stack_id}-secondary");
        let timestamp = now();
        db.upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
            project: secondary_project.clone(),
            stack_id: Some(stack_id.clone()),
            status: "missing".to_string(),
            last_seen_at: None,
            last_scan_at: timestamp.clone(),
            last_error: Some("compose_files_missing".to_string()),
            last_config_files: None,
            unarchive_if_active: false,
        })
        .await
        .unwrap();
        db.set_stack_archived(&stack_id, true, Some("user_archive"), &timestamp)
            .await
            .unwrap();
        db.set_discovered_compose_project_archived(
            &project,
            true,
            Some("user_archive"),
            &timestamp,
        )
        .await
        .unwrap();
        let managed_rows = db
            .list_stacks_with_state(ArchivedFilter::Include, StackStateFilter::All)
            .await
            .unwrap();
        assert_eq!(managed_rows.len(), 1);
        assert!(managed_rows[0].missing);
        assert_eq!(db.get_stack_compose_project(&stack_id).await.unwrap(), None);
        assert_eq!(
            db.get_stack_compose_projects_for_cleanup(&stack_id)
                .await
                .unwrap(),
            vec![secondary_project.clone(), project.clone()]
        );

        db.insert_job(JobListItem {
            id: "job-record-delete".to_string(),
            r#type: JobType::StackLifecycle,
            scope: JobScope::Stack,
            stack_id: Some(stack_id.to_string()),
            service_id: None,
            status: "success".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            created_at: timestamp.clone(),
            started_at: Some(timestamp.clone()),
            finished_at: Some(timestamp.clone()),
            allow_arch_mismatch: false,
            backup_mode: "none".to_string(),
            summary_json: json!({}),
        })
        .await
        .unwrap();
        db.insert_job(JobListItem {
            id: "job-target-only-delete".to_string(),
            r#type: JobType::Update,
            scope: JobScope::All,
            stack_id: None,
            service_id: None,
            status: "success".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            created_at: timestamp.clone(),
            started_at: None,
            finished_at: Some(timestamp.clone()),
            allow_arch_mismatch: false,
            backup_mode: "none".to_string(),
            summary_json: json!({ "targets": [{ "serviceId": service_id }] }),
        })
        .await
        .unwrap();
        db.insert_backup(
            "backup-record-delete",
            &stack_id,
            "job-record-delete",
            &timestamp,
        )
        .await
        .unwrap();
        db.finish_backup(
            "backup-record-delete",
            "success",
            &timestamp,
            Some("/srv/dockrev/backups/stack-record-delete/backup.tar.zst"),
            Some(12),
            None,
        )
        .await
        .unwrap();

        let timestamp_for_rows = timestamp.clone();
        let stack_id_for_pending = stack_id.clone();
        let stack_id_for_candidate = stack_id.clone();
        db.call(move |conn| {
            conn.execute(
                "INSERT INTO ignore_rules (id, enabled, scope_type, scope_service_id, match_kind, match_value, created_at, updated_at) VALUES ('ignore-record-delete', 1, 'service', ?1, 'tag', 'latest', ?2, ?2)",
                rusqlite::params![service_id, timestamp_for_rows],
            )?;
            conn.execute(
                "INSERT INTO auto_update_policies (scope_type, scope_id, mode, enabled, rules_json, created_at, updated_at) VALUES ('service', ?1, 'override', 1, '[]', 'now', 'now')",
                rusqlite::params![service_id],
            )?;
            conn.execute(
                "INSERT INTO auto_update_pending (id, policy_scope_type, policy_scope_id, rule_id, stack_id, service_id, source_check_job_id, candidate_tag, candidate_display_tag, candidate_digest, current_display_tag, first_seen_at, due_at, min_age_seconds, min_version_lag, status, created_at, updated_at) VALUES ('pending-record-delete', 'service', ?1, 'rule', ?2, ?1, 'job-record-delete', 'v2', 'v2', 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'latest', 'now', 'later', 0, 0, 'settled', 'now', 'now')",
                rusqlite::params![service_id, stack_id_for_pending],
            )?;
            conn.execute(
                "INSERT INTO auto_update_candidates (id, stack_id, service_id, image_ref, raw_tag, candidate_digest, status, discovered_at, source_job_id, created_at, updated_at) VALUES ('candidate-record-delete', ?1, ?2, 'ghcr.io/example/web', 'v2', 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'settled', 'now', 'job-record-delete', 'now', 'now')",
                rusqlite::params![stack_id_for_candidate, service_id],
            )?;
            conn.execute(
                "INSERT INTO notification_items (id, kind, identity_key, title, body, target_url, source_job_id, created_at) VALUES ('item-record-delete', 'job_finished', 'record-delete', 'title', 'body', '/', 'job-record-delete', 'now')",
                [],
            )?;
            conn.execute(
                "INSERT INTO new_version_notifications (id, service_id, job_id, reason, image_ref, image_tag, current_tag, current_display_tag, candidate_tag, candidate_display_tag, candidate_digest, status, created_at, notification_item_id) VALUES ('notification-record-delete', ?1, 'job-record-delete', 'check', 'ghcr.io/example/web', 'latest', 'latest', 'latest', 'v2', 'v2', 'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'sent', 'now', 'item-record-delete')",
                rusqlite::params![service_id],
            )?;
            conn.execute(
                "INSERT INTO notification_anomaly_occurrences (id, owner, repo, state, occurrence_count, batch_id, notification_item_id, created_at, source_job_id, source_status) VALUES ('anomaly-record-delete', 'example', 'web', 'active', 1, 'batch', 'item-record-delete', 'now', 'job-record-delete', 'success')",
                [],
            )?;
            conn.execute(
                "INSERT INTO notification_dispatch_outbox (job_id, finished_at, summary_json) VALUES ('job-record-delete', 'now', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO github_packages_deliveries (delivery_id, received_at, first_received_at, decision, job_id, job_ids_json) VALUES ('delivery-record-delete', 'now', 'now', 'processed', 'job-record-delete', '[\"job-target-only-delete\",\"job-unrelated\"]')",
                [],
            )?;
            conn.execute(
                "INSERT INTO github_packages_delivery_events (delivery_id, received_at, payload_json) VALUES ('delivery-record-delete', 'now', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO service_new_version_discoveries (service_id, source_job_id, discovered_at, candidate_digest) VALUES (?1, 'job-record-delete', 'now', 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd')",
                rusqlite::params![service_id],
            )?;
            conn.execute(
                "INSERT INTO cleanup_inventory_snapshots (snapshot_key, snapshot_json, checked_at, updated_at) VALUES ('global-record-delete', '{}', 'now', 'now')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let result = db
            .commit_stack_record_deletion(
                &stack_id,
                "ivan",
                &timestamp,
                r#"[{"artifactPath":"/srv/dockrev/backups/stack-record-delete/backup.tar.zst","present":false}]"#,
                false,
            )
            .await
            .unwrap();
        assert_eq!(result, StackRecordDeletionCommit::Deleted);

        let counts = db
            .call(|conn| {
                let count = |table: &str| -> rusqlite::Result<i64> {
                    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                };
                Ok(vec![
                    count("stacks")?,
                    count("services")?,
                    count("jobs")?,
                    count("backups")?,
                    count("new_version_notifications")?,
                    count("service_new_version_discoveries")?,
                    count("notification_items")?,
                    count("notification_anomaly_occurrences")?,
                    count("notification_dispatch_outbox")?,
                    count("github_packages_deliveries")?,
                    count("github_packages_delivery_events")?,
                    count("cleanup_inventory_snapshots")?,
                    count("stack_record_deletion_audits")?,
                ])
            })
            .await
            .unwrap();
        assert_eq!(counts, vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(
            db.get_stack_compose_projects_for_cleanup(&stack_id)
                .await
                .unwrap(),
            Vec::<String>::new()
        );
        let audit = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT actor_principal, outcome, pending_artifact_cleanup FROM stack_record_deletion_audits WHERE outcome = 'deleted'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )?)
            })
            .await
            .unwrap();
        assert_eq!(audit, ("ivan".to_string(), "deleted".to_string(), 0));
        let delivery = db
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT job_id, job_ids_json FROM github_packages_deliveries WHERE delivery_id = 'delivery-record-delete'",
                    [],
                    |row| {
                        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
                    },
                )?)
            })
            .await
            .unwrap();
        assert_eq!(delivery, (None, "[\"job-unrelated\"]".to_string()));

        assert_eq!(
            db.commit_stack_record_deletion(&stack_id, "ivan", &timestamp, "[]", false)
                .await
                .unwrap(),
            StackRecordDeletionCommit::AlreadyAbsent
        );

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }

    #[tokio::test]
    async fn deletion_conflicts_for_non_missing_and_pending_records() {
        let path = temp_db_path();
        let db = Db::open(&path).await.unwrap();
        let non_missing = seed_stack(&db, "stack-stopped", "service-stopped", "stopped").await;
        let invalid = seed_stack(&db, "stack-invalid", "service-invalid", "invalid").await;
        let pending = seed_stack(&db, "stack-pending", "service-pending", "missing").await;
        let target_pending = seed_stack(
            &db,
            "stack-target-pending",
            "service-target-pending",
            "missing",
        )
        .await;
        db.insert_job(JobListItem {
            id: "job-pending-record-delete".to_string(),
            r#type: JobType::Update,
            scope: JobScope::Stack,
            stack_id: Some(pending.clone()),
            service_id: None,
            status: "running".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            created_at: now(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "none".to_string(),
            summary_json: json!({}),
        })
        .await
        .unwrap();
        db.insert_job(JobListItem {
            id: "job-target-pending-record-delete".to_string(),
            r#type: JobType::Update,
            scope: JobScope::Stack,
            stack_id: None,
            service_id: None,
            status: "running".to_string(),
            created_by: "test".to_string(),
            reason: "test".to_string(),
            created_at: now(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "none".to_string(),
            summary_json: json!({"serviceIds": ["service-target-pending"]}),
        })
        .await
        .unwrap();

        assert!(matches!(
            db.commit_stack_record_deletion(&non_missing, "ivan", &now(), "[]", false)
                .await
                .unwrap(),
            StackRecordDeletionCommit::Conflict(reason) if reason == "stack is no longer missing"
        ));
        assert!(matches!(
            db.commit_stack_record_deletion(&invalid, "ivan", &now(), "[]", false)
                .await
                .unwrap(),
            StackRecordDeletionCommit::Conflict(reason) if reason == "stack is no longer missing"
        ));
        assert!(matches!(
            db.commit_stack_record_deletion(&pending, "ivan", &now(), "[]", false)
                .await
                .unwrap(),
            StackRecordDeletionCommit::Conflict(reason) if reason == "stack has a pending operation"
        ));
        assert!(matches!(
            db.commit_stack_record_deletion(&target_pending, "ivan", &now(), "[]", false)
                .await
                .unwrap(),
            StackRecordDeletionCommit::Conflict(reason) if reason == "stack has a pending operation"
        ));

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }
}
