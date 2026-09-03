use super::*;

#[allow(dead_code)] // API hot paths use the query-only OperationalReadModel.
const TARGET_BATCH_SIZE: usize = 400;

impl Db {
    pub(crate) async fn compare_and_swap_service_accepted_state_observation(
        &self,
        service_id: &str,
        expected_generation: i64,
        state: &ServiceAcceptedState,
        now: &str,
    ) -> anyhow::Result<AcceptedStateCasOutcome> {
        self.compare_and_swap_service_accepted_state_observation_with_notification_reconcile(
            service_id,
            expected_generation,
            state,
            now,
            false,
        )
        .await
    }

    pub(crate) async fn compare_and_swap_service_accepted_state_observation_with_notification_reconcile(
        &self,
        service_id: &str,
        expected_generation: i64,
        state: &ServiceAcceptedState,
        now: &str,
        reconcile_notifications: bool,
    ) -> anyhow::Result<AcceptedStateCasOutcome> {
        let service_id = service_id.to_string();
        let state = state.clone();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                r#"
UPDATE services
SET
  current_digest = ?4,
  current_runtime_started_at = ?5,
  current_resolved_tag = ?6,
  current_resolved_tags_json = ?7,
  candidate_tag = ?8,
  candidate_resolved_tag = ?9,
  candidate_digest = ?10,
  candidate_arch_match = ?11,
  candidate_arch_json = ?12,
  ignore_rule_id = ?13,
  ignore_reason = ?14,
  checked_at = ?15,
  updated_at = ?16,
  accepted_state_generation = ?17
WHERE id = ?1
  AND accepted_state_generation = ?2
  AND accepted_state_generation % 2 = 0
  AND image_ref = ?3
  AND image_tag = ?18
  AND NOT EXISTS (
    SELECT 1
    FROM job_service_targets target
    JOIN jobs job ON job.id = target.job_id
    WHERE target.service_id = services.id
      AND target.opened_generation IS NOT NULL
      AND job.status IN ('queued', 'running')
  )
"#,
                params![
                    service_id,
                    expected_generation,
                    state.image_ref,
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
                    expected_generation + 2,
                    state.image_tag,
                ],
            )?;
            let outcome = if changed == 1 {
                if reconcile_notifications {
                    super::new_version_notifications::reconcile_service_new_version_notifications_tx(
                        &tx,
                        &service_id,
                        &state.image_ref,
                        &state.image_tag,
                        state.candidate_digest.as_deref(),
                        &now,
                    )?;
                }
                AcceptedStateCasOutcome::Applied {
                    generation: expected_generation + 2,
                }
            } else {
                let current_generation = tx
                    .query_row(
                        "SELECT accepted_state_generation FROM services WHERE id = ?1",
                        [&service_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?;
                AcceptedStateCasOutcome::Rejected { current_generation }
            };
            tx.commit()?;
            Ok(outcome)
        })
        .await
        .context("compare and swap service accepted state observation")
    }

    pub async fn upsert_cleanup_inventory_snapshot(
        &self,
        snapshot_key: &str,
        snapshot_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let snapshot_key = snapshot_key.to_string();
        let snapshot_json = snapshot_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO cleanup_inventory_snapshots (
  snapshot_key,
  snapshot_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(snapshot_key) DO UPDATE SET
  snapshot_json = excluded.snapshot_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![snapshot_key, snapshot_json, checked_at, now],
            )?;
            Ok(())
        })
        .await
        .context("upsert cleanup inventory snapshot")
    }

    pub async fn get_cleanup_inventory_snapshot(
        &self,
        snapshot_key: &str,
    ) -> anyhow::Result<Option<CleanupInventorySnapshotRow>> {
        let snapshot_key = snapshot_key.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_key, snapshot_json, checked_at, updated_at
FROM cleanup_inventory_snapshots
WHERE snapshot_key = ?1
"#,
                    params![snapshot_key],
                    |row| {
                        Ok(CleanupInventorySnapshotRow {
                            snapshot_key: row.get(0)?,
                            snapshot_json: row.get(1)?,
                            checked_at: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get cleanup inventory snapshot")
    }

    pub async fn upsert_deploy_check_report_snapshot(
        &self,
        snapshot_key: &str,
        report_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let snapshot_key = snapshot_key.to_string();
        let report_json = report_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO deploy_check_report_snapshots (
  snapshot_key,
  report_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(snapshot_key) DO UPDATE SET
  report_json = excluded.report_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![snapshot_key, report_json, checked_at, now],
            )?;
            Ok(())
        })
        .await
        .context("upsert deploy check report snapshot")
    }

    pub async fn get_deploy_check_report_snapshot(
        &self,
        snapshot_key: &str,
    ) -> anyhow::Result<Option<DeployCheckReportSnapshotRow>> {
        let snapshot_key = snapshot_key.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_key, report_json, checked_at, updated_at
FROM deploy_check_report_snapshots
WHERE snapshot_key = ?1
"#,
                    params![snapshot_key],
                    |row| {
                        Ok(DeployCheckReportSnapshotRow {
                            snapshot_key: row.get(0)?,
                            report_json: row.get(1)?,
                            checked_at: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get deploy check report snapshot")
    }

    pub async fn list_ignore_rules_for_service(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Vec<IgnoreRule>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, enabled, scope_type, scope_service_id, match_kind, match_value, note
FROM ignore_rules
WHERE enabled = 1 AND scope_type = 'service' AND scope_service_id = ?1
ORDER BY created_at DESC
"#,
            )?;
            let rows = stmt.query_map(params![service_id], |row| {
                Ok(IgnoreRule {
                    id: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? != 0,
                    scope: IgnoreRuleScope {
                        kind: row.get(2)?,
                        service_id: row.get(3)?,
                    },
                    matcher: IgnoreRuleMatch {
                        kind: row.get(4)?,
                        value: row.get(5)?,
                    },
                    note: row.get(6)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list ignore rules for service")
    }

    #[allow(dead_code, clippy::too_many_arguments)]
    pub async fn update_service_check_result(
        &self,
        service_id: &str,
        current_digest: Option<String>,
        current_resolved_tag: Option<String>,
        current_resolved_tags_json: Option<String>,
        candidate_tag: Option<String>,
        candidate_resolved_tag: Option<String>,
        candidate_digest: Option<String>,
        candidate_arch_match: Option<String>,
        candidate_arch_json: Option<String>,
        ignore_rule_id: Option<String>,
        ignore_reason: Option<String>,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        self.update_service_check_result_with_runtime_started_at(
            service_id,
            current_digest,
            None,
            current_resolved_tag,
            current_resolved_tags_json,
            candidate_tag,
            candidate_resolved_tag,
            candidate_digest,
            candidate_arch_match,
            candidate_arch_json,
            ignore_rule_id,
            ignore_reason,
            checked_at,
            now,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_service_check_result_with_runtime_started_at(
        &self,
        service_id: &str,
        current_digest: Option<String>,
        current_runtime_started_at: Option<String>,
        current_resolved_tag: Option<String>,
        current_resolved_tags_json: Option<String>,
        candidate_tag: Option<String>,
        candidate_resolved_tag: Option<String>,
        candidate_digest: Option<String>,
        candidate_arch_match: Option<String>,
        candidate_arch_json: Option<String>,
        ignore_rule_id: Option<String>,
        ignore_reason: Option<String>,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        let Some(existing) = self
            .get_versioned_service_accepted_state(&service_id)
            .await?
        else {
            return Ok(false);
        };
        let outcome = self
            .compare_and_swap_service_accepted_state_observation(
                &service_id,
                existing.generation,
                &ServiceAcceptedState {
                    image_ref: existing.state.image_ref,
                    image_tag: existing.state.image_tag,
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
                    checked_at: Some(checked_at),
                },
                &now,
            )
            .await?;
        Ok(matches!(outcome, AcceptedStateCasOutcome::Applied { .. }))
    }

    pub async fn upsert_service_digest_tags_snapshot(
        &self,
        service_id: &str,
        digest: &str,
        snapshot_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        let digest = digest.to_string();
        let snapshot_json = snapshot_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO service_digest_tags_snapshots (
  service_id,
  digest,
  snapshot_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(service_id, digest) DO UPDATE SET
  snapshot_json = excluded.snapshot_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![service_id, digest, snapshot_json, checked_at, now],
            )?;
            Ok(())
        })
        .await
        .context("upsert service digest tags snapshot")
    }

    #[allow(dead_code)]
    pub async fn get_service_digest_tags_snapshot(
        &self,
        service_id: &str,
        digest: &str,
    ) -> anyhow::Result<Option<(String, String, String)>> {
        let service_id = service_id.to_string();
        let digest = digest.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_json, checked_at, updated_at
FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest = ?2
"#,
                    params![service_id, digest],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?)
        })
        .await
        .context("get service digest tags snapshot")
    }

    #[allow(dead_code)]
    pub async fn delete_service_digest_tags_snapshots_except(
        &self,
        service_id: &str,
        allowed_digests: &[String],
    ) -> anyhow::Result<usize> {
        let service_id = service_id.to_string();
        let mut allowed = allowed_digests.to_vec();
        allowed.retain(|d| !d.trim().is_empty());
        allowed.sort();
        allowed.dedup();
        if allowed.len() > 2 {
            // Defensive: the caller is expected to pass at most {current, candidate}.
            allowed.truncate(2);
        }

        self.call(move |conn| {
            let deleted = match allowed.len() {
                0 => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1
"#,
                    params![service_id],
                )?,
                1 => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest != ?2
"#,
                    params![service_id, allowed[0]],
                )?,
                _ => conn.execute(
                    r#"
DELETE FROM service_digest_tags_snapshots
WHERE service_id = ?1 AND digest NOT IN (?2, ?3)
"#,
                    params![service_id, allowed[0], allowed[1]],
                )?,
            };
            Ok(deleted)
        })
        .await
        .context("delete service digest tags snapshots except")
    }

    pub async fn upsert_image_digest_tags_snapshot(
        &self,
        image_repo: &str,
        digest: &str,
        host_platform: &str,
        snapshot_json: &str,
        checked_at: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        let host_platform = host_platform.to_string();
        let snapshot_json = snapshot_json.to_string();
        let checked_at = checked_at.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO image_digest_tags_snapshots (
  image_repo,
  digest,
  host_platform,
  snapshot_json,
  checked_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(image_repo, digest, host_platform) DO UPDATE SET
  snapshot_json = excluded.snapshot_json,
  checked_at = excluded.checked_at,
  updated_at = excluded.updated_at
"#,
                params![
                    image_repo,
                    digest,
                    host_platform,
                    snapshot_json,
                    checked_at,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("upsert image digest tags snapshot")
    }

    pub async fn get_image_digest_tags_snapshot(
        &self,
        image_repo: &str,
        digest: &str,
        host_platform: &str,
    ) -> anyhow::Result<Option<(String, String, String)>> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        let host_platform = host_platform.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT snapshot_json, checked_at, updated_at
FROM image_digest_tags_snapshots
WHERE image_repo = ?1 AND digest = ?2 AND host_platform = ?3
"#,
                    params![image_repo, digest, host_platform],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?)
        })
        .await
        .context("get image digest tags snapshot")
    }

    #[allow(dead_code)] // API hot paths use the query-only OperationalReadModel.
    pub async fn list_image_digest_tags_snapshots_for_targets(
        &self,
        host_platform: &str,
        targets: &[(String, String)],
    ) -> anyhow::Result<Vec<ImageDigestTagsSnapshotRow>> {
        let host_platform = host_platform.to_string();
        let targets = targets
            .iter()
            .map(|(image_repo, digest)| (image_repo.trim(), digest.trim()))
            .filter(|(image_repo, digest)| !image_repo.is_empty() && !digest.is_empty())
            .map(|(image_repo, digest)| (image_repo.to_string(), digest.to_string()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Ok(Vec::new());
        }

        self.call(move |conn| {
            let mut out = Vec::new();
            for chunk in targets.chunks(TARGET_BATCH_SIZE) {
                let clauses = chunk
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        let image_ref_pos = index * 2 + 2;
                        let digest_pos = index * 2 + 3;
                        format!("(image_repo = ?{image_ref_pos} AND digest = ?{digest_pos})")
                    })
                    .collect::<Vec<_>>()
                    .join(" OR ");
                let sql = format!(
                    r#"
SELECT image_repo, digest, host_platform, snapshot_json, checked_at, updated_at
FROM image_digest_tags_snapshots
WHERE host_platform = ?1
  AND ({clauses})
ORDER BY updated_at DESC, image_repo ASC, digest ASC
"#,
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len() * 2 + 1);
                params.push(&host_platform);
                for (image_repo, digest) in chunk {
                    params.push(image_repo);
                    params.push(digest);
                }
                let rows = stmt.query_map(params.as_slice(), |row| {
                    Ok(ImageDigestTagsSnapshotRow {
                        image_repo: row.get(0)?,
                        digest: row.get(1)?,
                        host_platform: row.get(2)?,
                        snapshot_json: row.get(3)?,
                        checked_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                })?;
                out.extend(rows.collect::<Result<Vec<_>, _>>()?);
            }
            Ok(out)
        })
        .await
        .context("list image digest tags snapshots for targets")
    }

    pub async fn list_image_digest_tags_snapshots(
        &self,
    ) -> anyhow::Result<Vec<ImageDigestTagsSnapshotRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT image_repo, digest, host_platform, snapshot_json, checked_at, updated_at
FROM image_digest_tags_snapshots
ORDER BY updated_at DESC, image_repo ASC, digest ASC, host_platform ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ImageDigestTagsSnapshotRow {
                    image_repo: row.get(0)?,
                    digest: row.get(1)?,
                    host_platform: row.get(2)?,
                    snapshot_json: row.get(3)?,
                    checked_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list image digest tags snapshots")
    }

    pub async fn delete_expired_image_digest_tags_snapshots(
        &self,
        cutoff_checked_at: &str,
    ) -> anyhow::Result<u64> {
        let cutoff_checked_at = cutoff_checked_at.to_string();
        self.call(move |conn| {
            let deleted = conn.execute(
                r#"
DELETE FROM image_digest_tags_snapshots
WHERE checked_at < ?1
"#,
                params![cutoff_checked_at],
            )?;
            Ok(deleted as u64)
        })
        .await
        .context("delete expired image digest tags snapshots")
    }

    pub async fn list_version_inference_service_targets(
        &self,
    ) -> anyhow::Result<Vec<VersionInferenceServiceTargetRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT image_ref, image_tag, candidate_tag
FROM services
WHERE archived = 0
ORDER BY image_ref ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(VersionInferenceServiceTargetRow {
                    image_ref: row.get(0)?,
                    image_tag: row.get(1)?,
                    candidate_tag: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list version inference service targets")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn list_image_digest_tags_snapshots_for_targets_batches_large_target_sets() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        let checked_at = "2026-03-20T00:00:00Z";
        let snapshot = crate::api::types::ServiceDigestTagsSnapshotResponse {
            digest: "sha256:placeholder".to_string(),
            tags: vec!["latest".to_string(), "1.0.0".to_string()],
            checked_at: checked_at.to_string(),
            scan: crate::api::types::ServiceDigestTagsScanSummary {
                repo_tags_total: 2,
                repo_tags_considered: 2,
                manifests_ok: 2,
                manifests_timeout: 0,
                manifests_error: 0,
            },
        };

        let mut targets = Vec::new();
        for idx in 0..450 {
            let digest = format!("sha256:{idx:064x}");
            let mut snapshot = snapshot.clone();
            snapshot.digest = digest.clone();
            db.upsert_image_digest_tags_snapshot(
                "ghcr.io/acme/web",
                &digest,
                "linux/amd64",
                &serde_json::to_string(&snapshot).unwrap(),
                checked_at,
                checked_at,
            )
            .await
            .unwrap();
            targets.push(("ghcr.io/acme/web".to_string(), digest));
        }

        let rows = db
            .list_image_digest_tags_snapshots_for_targets("linux/amd64", &targets)
            .await
            .unwrap();

        assert_eq!(rows.len(), 450);
        assert!(
            rows.iter()
                .any(|row| row.digest == format!("sha256:{:064x}", 0))
        );
        assert!(
            rows.iter()
                .any(|row| row.digest == format!("sha256:{:064x}", 449))
        );
    }
}
