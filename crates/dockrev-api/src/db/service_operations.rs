use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ServiceOperationTarget {
    pub(crate) service_id: String,
    pub(crate) stack_id: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceAcceptedState {
    pub(crate) image_ref: String,
    pub(crate) image_tag: String,
    pub(crate) current_digest: Option<String>,
    pub(crate) current_runtime_started_at: Option<String>,
    pub(crate) current_resolved_tag: Option<String>,
    pub(crate) current_resolved_tags_json: Option<String>,
    pub(crate) candidate_tag: Option<String>,
    pub(crate) candidate_resolved_tag: Option<String>,
    pub(crate) candidate_digest: Option<String>,
    pub(crate) candidate_arch_match: Option<String>,
    pub(crate) candidate_arch_json: Option<String>,
    pub(crate) ignore_rule_id: Option<String>,
    pub(crate) ignore_reason: Option<String>,
    pub(crate) checked_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VersionedServiceAcceptedState {
    pub(crate) generation: i64,
    pub(crate) state: ServiceAcceptedState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServiceOperationAcceptedStateLease {
    pub(crate) service_id: String,
    pub(crate) opened_generation: i64,
    pub(crate) baseline: ServiceAcceptedState,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum ServiceOperationAcquireOutcome {
    Acquired(Vec<ServiceOperationAcceptedStateLease>),
    Conflict(Box<JobListItem>),
}

#[derive(Clone, Debug)]
pub(crate) struct ServiceAcceptedStateSettlement {
    pub(crate) service_id: String,
    pub(crate) opened_generation: i64,
    pub(crate) state: ServiceAcceptedState,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceAcceptedStateSnapshot {
    schema_version: u32,
    #[serde(flatten)]
    state: ServiceAcceptedState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AcceptedStateCasOutcome {
    Applied { generation: i64 },
    Rejected { current_generation: Option<i64> },
}

fn map_versioned_service_accepted_state(
    row: &rusqlite::Row<'_>,
    generation_column: usize,
    state_column: usize,
) -> rusqlite::Result<VersionedServiceAcceptedState> {
    Ok(VersionedServiceAcceptedState {
        generation: row.get(generation_column)?,
        state: ServiceAcceptedState {
            image_ref: row.get(state_column)?,
            image_tag: row.get(state_column + 1)?,
            current_digest: row.get(state_column + 2)?,
            current_runtime_started_at: row.get(state_column + 3)?,
            current_resolved_tag: row.get(state_column + 4)?,
            current_resolved_tags_json: row.get(state_column + 5)?,
            candidate_tag: row.get(state_column + 6)?,
            candidate_resolved_tag: row.get(state_column + 7)?,
            candidate_digest: row.get(state_column + 8)?,
            candidate_arch_match: row.get(state_column + 9)?,
            candidate_arch_json: row.get(state_column + 10)?,
            ignore_rule_id: row.get(state_column + 11)?,
            ignore_reason: row.get(state_column + 12)?,
            checked_at: row.get(state_column + 13)?,
        },
    })
}

fn find_blocking_job_tx(
    tx: &rusqlite::Transaction<'_>,
    targets: &[ServiceOperationTarget],
) -> anyhow::Result<Option<JobListItem>> {
    let candidates = {
        let mut statement = tx.prepare(
            r#"
SELECT id, type, scope, stack_id, service_id, status, created_by, reason, created_at,
  started_at, finished_at, allow_arch_mismatch, backup_mode, summary_json
FROM jobs
WHERE type IN ('update', 'rollback', 'service_lifecycle', 'stack_lifecycle', 'managed_override_reconcile')
  AND status IN ('queued', 'running')
ORDER BY CASE status WHEN 'running' THEN 0 ELSE 1 END, created_at DESC, id DESC
"#,
        )?;
        statement
            .query_map([], map_job_list_item_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut conflict = None;
    for candidate in candidates {
        let persisted_service_ids = {
            let mut statement =
                tx.prepare("SELECT service_id FROM job_service_targets WHERE job_id = ?1")?;
            statement
                .query_map([&candidate.id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if blocks_targets(&candidate, &persisted_service_ids, targets)
            && better_conflict(&candidate, conflict.as_ref())
        {
            conflict = Some(candidate);
        }
    }
    Ok(conflict)
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
        "update" | "stack_lifecycle" | "managed_override_reconcile" => match job.scope {
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
    pub(crate) async fn get_versioned_service_accepted_state(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<VersionedServiceAcceptedState>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                r#"
SELECT
  accepted_state_generation,
  image_ref,
  image_tag,
  current_digest,
  current_runtime_started_at,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_tag,
  candidate_resolved_tag,
  candidate_digest,
  candidate_arch_match,
  candidate_arch_json,
  ignore_rule_id,
  ignore_reason,
  checked_at
FROM services
WHERE id = ?1
"#,
                [service_id],
                |row| map_versioned_service_accepted_state(row, 0, 1),
            )
            .optional()
            .map_err(Into::into)
        })
        .await
        .context("get versioned service accepted state")
    }

    pub(crate) async fn insert_service_operation_job_with_accepted_state_if_unblocked(
        &self,
        job: JobListItem,
        targets: Vec<ServiceOperationTarget>,
        initial_log: Option<JobLogLine>,
    ) -> anyhow::Result<ServiceOperationAcquireOutcome> {
        let event_job_id = job.id.clone();
        let event_scope = job.scope.as_str().to_string();
        let event_stack_id = job.stack_id.clone();
        let event_service_id = job.service_id.clone();
        let event_type = job.r#type.as_str().to_string();
        let job_id = job.id.clone();
        let outcome = self
            .call(move |conn| {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                if let Some(conflict) = find_blocking_job_tx(&tx, &targets)? {
                    tx.commit()?;
                    return Ok(ServiceOperationAcquireOutcome::Conflict(Box::new(conflict)));
                }

                insert_job_tx(&tx, &job)?;
                let mut leases = Vec::new();
                let mut claimed_service_ids = BTreeSet::new();
                for target in &targets {
                    if !claimed_service_ids.insert(target.service_id.clone()) {
                        continue;
                    }
                    let (actual_stack_id, versioned) = tx
                        .query_row(
                            r#"
SELECT
  stack_id,
  accepted_state_generation,
  image_ref,
  image_tag,
  current_digest,
  current_runtime_started_at,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_tag,
  candidate_resolved_tag,
  candidate_digest,
  candidate_arch_match,
  candidate_arch_json,
  ignore_rule_id,
  ignore_reason,
  checked_at
FROM services
WHERE id = ?1
"#,
                            [&target.service_id],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    map_versioned_service_accepted_state(row, 1, 2)?,
                                ))
                            },
                        )
                        .optional()?
                        .ok_or_else(|| {
                            anyhow::anyhow!("service operation target not found: {}", target.service_id)
                        })?;
                    if actual_stack_id != target.stack_id {
                        anyhow::bail!(
                            "service operation target stack mismatch: service={} expected={} actual={}",
                            target.service_id,
                            target.stack_id,
                            actual_stack_id
                        );
                    }
                    if versioned.generation % 2 != 0 {
                        anyhow::bail!(
                            "service accepted state already has an open mutation: service={} generation={}",
                            target.service_id,
                            versioned.generation
                        );
                    }
                    let opened_generation = versioned.generation + 1;
                    let changed = tx.execute(
                        r#"
UPDATE services
SET accepted_state_generation = ?3
WHERE id = ?1
  AND accepted_state_generation = ?2
  AND accepted_state_generation % 2 = 0
"#,
                        params![target.service_id, versioned.generation, opened_generation],
                    )?;
                    if changed != 1 {
                        anyhow::bail!(
                            "service accepted state changed while acquiring mutation: {}",
                            target.service_id
                        );
                    }
                    let baseline_snapshot_json = serde_json::to_string(&ServiceAcceptedStateSnapshot {
                        schema_version: 1,
                        state: versioned.state.clone(),
                    })?;
                    tx.execute(
                        r#"
INSERT INTO job_service_targets (
  job_id, service_id, opened_generation, baseline_snapshot_json
)
SELECT ?1, id, ?3, ?4
FROM services
WHERE id = ?2
ON CONFLICT(job_id, service_id) DO UPDATE SET
  opened_generation = excluded.opened_generation,
  baseline_snapshot_json = excluded.baseline_snapshot_json
"#,
                        params![job.id, target.service_id, opened_generation, baseline_snapshot_json],
                    )?;
                    leases.push(ServiceOperationAcceptedStateLease {
                        service_id: target.service_id.clone(),
                        opened_generation,
                        baseline: versioned.state,
                    });
                }
                if let Some(line) = initial_log {
                    tx.execute(
                        "INSERT INTO job_logs (job_id, ts, level, msg) VALUES (?1, ?2, ?3, ?4)",
                        params![job.id, line.ts, line.level, line.msg],
                    )?;
                }
                tx.commit()?;
                Ok(ServiceOperationAcquireOutcome::Acquired(leases))
            })
            .await
            .context("atomically acquire service accepted state operation")?;
        if matches!(outcome, ServiceOperationAcquireOutcome::Acquired(_)) {
            self.management_events
                .publish_change(
                    "jobs",
                    "job",
                    event_job_id,
                    serde_json::json!({
                        "jobId": job_id,
                        "status": "queued",
                        "jobType": event_type,
                        "scope": event_scope,
                        "stackId": event_stack_id,
                        "serviceId": event_service_id,
                    }),
                )
                .await;
        }
        Ok(outcome)
    }

    pub(super) fn settle_service_operation_accepted_states_tx(
        tx: &rusqlite::Transaction<'_>,
        job_id: &str,
        settlements: &[ServiceAcceptedStateSettlement],
        now: &str,
    ) -> anyhow::Result<()> {
        let active = tx
            .query_row(
                "SELECT 1 FROM jobs WHERE id = ?1 AND status IN ('queued', 'running')",
                [job_id],
                |_row| Ok(()),
            )
            .optional()?
            .is_some();
        if !active {
            anyhow::bail!("service operation job is not active: {job_id}");
        }
        let persisted_targets = {
            let mut statement = tx.prepare(
                "SELECT service_id, opened_generation, baseline_snapshot_json FROM job_service_targets WHERE job_id = ?1 AND opened_generation IS NOT NULL ORDER BY service_id",
            )?;
            statement
                .query_map([job_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let settlement_by_service = settlements
            .iter()
            .map(|s| (s.service_id.as_str(), s))
            .collect::<BTreeMap<_, _>>();
        if persisted_targets.len() != settlement_by_service.len()
            || persisted_targets
                .iter()
                .any(|(id, _, _)| !settlement_by_service.contains_key(id.as_str()))
        {
            anyhow::bail!("service operation settlement targets do not match acquired targets");
        }
        for (service_id, opened_generation, baseline_json) in persisted_targets {
            let settlement = settlement_by_service[service_id.as_str()];
            if settlement.opened_generation != opened_generation {
                anyhow::bail!(
                    "service operation settlement generation mismatch: service={service_id}"
                );
            }
            let baseline = baseline_json.ok_or_else(|| {
                anyhow::anyhow!("service operation baseline is missing: {service_id}")
            })?;
            if serde_json::from_str::<ServiceAcceptedStateSnapshot>(&baseline)?.schema_version != 1
            {
                anyhow::bail!("unsupported service operation baseline schema: {service_id}");
            }
            let state = &settlement.state;
            let changed = tx.execute(
                r#"UPDATE services SET image_ref = ?3, image_tag = ?4, current_digest = ?5,
current_runtime_started_at = ?6, current_resolved_tag = ?7, current_resolved_tags_json = ?8,
candidate_tag = ?9, candidate_resolved_tag = ?10, candidate_digest = ?11,
candidate_arch_match = ?12, candidate_arch_json = ?13, ignore_rule_id = ?14,
ignore_reason = ?15, checked_at = ?16, updated_at = ?17, accepted_state_generation = ?18
WHERE id = ?1 AND accepted_state_generation = ?2 AND accepted_state_generation % 2 = 1"#,
                params![
                    service_id,
                    opened_generation,
                    state.image_ref,
                    state.image_tag,
                    state.current_digest,
                    state.current_runtime_started_at,
                    state.current_resolved_tag,
                    state.current_resolved_tags_json,
                    state.candidate_tag,
                    state.candidate_resolved_tag,
                    state.candidate_digest,
                    state.candidate_arch_match,
                    state.candidate_arch_json,
                    state.ignore_rule_id,
                    state.ignore_reason,
                    state.checked_at,
                    now,
                    opened_generation + 1
                ],
            )?;
            if changed != 1 {
                anyhow::bail!(
                    "service accepted state changed before settlement: service={service_id}"
                );
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn settle_service_operation_accepted_states(
        &self,
        job_id: &str,
        settlements: &[ServiceAcceptedStateSettlement],
        now: &str,
    ) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let settlements = settlements.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            Self::settle_service_operation_accepted_states_tx(&tx, &job_id, &settlements, &now)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("settle service operation accepted states")
    }

    pub async fn insert_service_operation_job_if_unblocked(
        &self,
        job: JobListItem,
        targets: Vec<ServiceOperationTarget>,
        initial_log: Option<JobLogLine>,
    ) -> anyhow::Result<Option<JobListItem>> {
        match self
            .insert_service_operation_job_with_accepted_state_if_unblocked(
                job,
                targets,
                initial_log,
            )
            .await?
        {
            ServiceOperationAcquireOutcome::Acquired(_) => Ok(None),
            ServiceOperationAcquireOutcome::Conflict(job) => Ok(Some(*job)),
        }
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

#[cfg(test)]
mod accepted_state_tests {
    use std::path::Path;

    use super::*;

    async fn seed_service(db: &Db) -> (String, String) {
        let stack_id = "stack_accepted_state".to_string();
        let service_id = "service_accepted_state".to_string();
        let insert_stack_id = stack_id.clone();
        let insert_service_id = service_id.clone();
        db.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES (?1, 'accepted-state', 'path', '[]', '[]', 0, 0, ?3, ?3, ?3)
"#,
                params![insert_stack_id, insert_service_id, "2026-08-30T00:00:00Z"],
            )?;
            conn.execute(
                r#"
INSERT INTO services (
  id, stack_id, name, image_ref, image_tag,
  current_digest, current_resolved_tag, current_resolved_tags_json,
  candidate_tag, candidate_resolved_tag, candidate_digest,
  candidate_arch_match, candidate_arch_json, checked_at,
  auto_rollback, backup_targets_bind_paths_json, backup_targets_volume_names_json,
  created_at, updated_at
) VALUES (
  ?1, ?2, 'web', 'ghcr.io/acme/web:latest', 'latest',
  'sha256:old', '1.0.0', '["1.0.0"]',
  'latest', '1.1.0', 'sha256:new', 'match', '["linux/amd64"]', ?3,
  1, '{}', '{}', ?3, ?3
)
"#,
                params![insert_service_id, insert_stack_id, "2026-08-30T00:00:00Z"],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        (stack_id, service_id)
    }

    fn state(current_digest: &str, candidate_digest: Option<&str>) -> ServiceAcceptedState {
        ServiceAcceptedState {
            image_ref: "ghcr.io/acme/web:latest".to_string(),
            image_tag: "latest".to_string(),
            current_digest: Some(current_digest.to_string()),
            current_runtime_started_at: Some("2026-08-30T00:00:00Z".to_string()),
            current_resolved_tag: Some("1.0.0".to_string()),
            current_resolved_tags_json: Some("[\"1.0.0\"]".to_string()),
            candidate_tag: candidate_digest.map(|_| "latest".to_string()),
            candidate_resolved_tag: candidate_digest.map(|_| "1.1.0".to_string()),
            candidate_digest: candidate_digest.map(str::to_string),
            candidate_arch_match: candidate_digest.map(|_| "match".to_string()),
            candidate_arch_json: candidate_digest.map(|_| "[\"linux/amd64\"]".to_string()),
            ignore_rule_id: None,
            ignore_reason: None,
            checked_at: Some("2026-08-30T00:00:00Z".to_string()),
        }
    }

    #[tokio::test]
    async fn mutation_generation_rejects_observations_across_terminal_settlement() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let (stack_id, service_id) = seed_service(&db).await;
        let observed = db
            .get_versioned_service_accepted_state(&service_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(observed.generation, 0);

        let now = "2026-08-30T00:01:00Z";
        let mut job = crate::api::types::JobRecord::new_running(
            "job_accepted_state".to_string(),
            JobType::Update,
            JobScope::Service,
            Some(stack_id.clone()),
            Some(service_id.clone()),
            now,
        );
        job.summary_json = serde_json::json!({"mode": "apply"});
        let acquired = db
            .insert_service_operation_job_with_accepted_state_if_unblocked(
                job.to_db(),
                vec![ServiceOperationTarget {
                    service_id: service_id.clone(),
                    stack_id,
                }],
                None,
            )
            .await
            .unwrap();
        let ServiceOperationAcquireOutcome::Acquired(leases) = acquired else {
            panic!("mutation lease should be acquired");
        };
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].opened_generation, 1);
        assert_eq!(
            leases[0].baseline.current_digest.as_deref(),
            Some("sha256:old")
        );

        let blocked = db
            .compare_and_swap_service_accepted_state_observation(
                &service_id,
                1,
                &state("sha256:transient", None),
                now,
            )
            .await
            .unwrap();
        assert!(matches!(
            blocked,
            AcceptedStateCasOutcome::Rejected {
                current_generation: Some(1)
            }
        ));

        db.settle_service_operation_accepted_states(
            "job_accepted_state",
            &[ServiceAcceptedStateSettlement {
                service_id: service_id.clone(),
                opened_generation: 1,
                state: state("sha256:old", Some("sha256:new")),
            }],
            "2026-08-30T00:02:00Z",
        )
        .await
        .unwrap();

        let still_blocked = db
            .compare_and_swap_service_accepted_state_observation(
                &service_id,
                2,
                &state("sha256:old", None),
                "2026-08-30T00:02:01Z",
            )
            .await
            .unwrap();
        assert!(matches!(
            still_blocked,
            AcceptedStateCasOutcome::Rejected {
                current_generation: Some(2)
            }
        ));

        db.finish_job(
            "job_accepted_state",
            "rolled_back",
            "2026-08-30T00:02:02Z",
            &serde_json::json!({"mode": "apply"}),
        )
        .await
        .unwrap();

        let stale = db
            .compare_and_swap_service_accepted_state_observation(
                &service_id,
                observed.generation,
                &state("sha256:transient", None),
                "2026-08-30T00:02:03Z",
            )
            .await
            .unwrap();
        assert!(matches!(
            stale,
            AcceptedStateCasOutcome::Rejected {
                current_generation: Some(2)
            }
        ));

        let applied = db
            .compare_and_swap_service_accepted_state_observation(
                &service_id,
                2,
                &state("sha256:old", None),
                "2026-08-30T00:02:04Z",
            )
            .await
            .unwrap();
        assert_eq!(applied, AcceptedStateCasOutcome::Applied { generation: 4 });
    }
}
