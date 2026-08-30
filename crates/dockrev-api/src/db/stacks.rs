use super::stacks_backup_targets::{
    project_backup_target_overrides, prune_service_backup_target_policies_tx,
    put_service_backup_targets_tx,
};
use super::*;

fn serialize_service_homepage(
    homepage: &Option<crate::api::types::ServiceHomepage>,
) -> anyhow::Result<Option<String>> {
    homepage
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("serialize service homepage")
}

fn serialize_service_update_guard(
    update_guard: &Option<crate::api::types::ServiceUpdateGuard>,
) -> anyhow::Result<Option<String>> {
    update_guard
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("serialize service update guard")
}

fn deserialize_service_homepage(
    homepage_json: Option<String>,
) -> rusqlite::Result<Option<crate::api::types::ServiceHomepage>> {
    homepage_json
        .map(|value| {
            serde_json::from_str::<crate::api::types::ServiceHomepage>(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

fn deserialize_service_update_guard(
    update_guard_json: Option<String>,
) -> rusqlite::Result<Option<crate::api::types::ServiceUpdateGuard>> {
    update_guard_json
        .map(|value| {
            serde_json::from_str::<crate::api::types::ServiceUpdateGuard>(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

impl Db {
    pub async fn list_stacks(
        &self,
        archived: ArchivedFilter,
    ) -> anyhow::Result<Vec<StackListItem>> {
        self.call(move |conn| {
            let filter_clause = archived.where_clause("s.archived");
            let sql = format!(
                r#"
SELECT
  s.id,
  s.name,
  s.last_check_at,
  s.archived,
  (SELECT COUNT(1) FROM services sv WHERE sv.stack_id = s.id) AS services,
  (SELECT COUNT(1) FROM services sv WHERE sv.stack_id = s.id AND sv.archived = 1) AS archived_services,
  (
    SELECT COUNT(1)
    FROM services sv
    WHERE
      sv.stack_id = s.id
      AND sv.candidate_tag IS NOT NULL
      AND sv.ignore_rule_id IS NULL
      AND sv.candidate_arch_match = 'match'
  ) AS updates
FROM stacks s
WHERE 1=1
{filter_clause}
ORDER BY s.created_at DESC
"#,
            );
            let mut stmt = conn.prepare(&sql)?;

            let rows = stmt.query_map([], |row| {
                Ok(StackListItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: StackStatus::Unknown,
                    last_check_at: row.get(2)?,
                    archived: Some(row.get::<_, i64>(3)? != 0),
                    services: row.get::<_, i64>(4)? as u32,
                    archived_services: Some(row.get::<_, i64>(5)? as u32),
                    updates: row.get::<_, i64>(6)? as u32,
                })
            })?;

            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list stacks")
    }
    pub async fn get_stack(&self, stack_id: &str) -> anyhow::Result<Option<StackRecord>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let stack = conn
                .query_row(
                    r#"
SELECT
  id,
  name,
  compose_type,
  compose_files_json,
  env_file,
  backup_targets_json,
  backup_retention_keep_last,
  backup_retention_delete_after_stable_seconds,
  archived
FROM stacks
WHERE id = ?1
"#,
                    params![stack_id],
                    |row| {
                        let compose_files_json: String = row.get(3)?;
                        let backup_targets_json: String = row.get(5)?;

                        let compose_files: Vec<String> = serde_json::from_str(&compose_files_json)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;

                        let backup_targets: Vec<crate::api::types::BackupTarget> =
                            serde_json::from_str(&backup_targets_json).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?;

                        Ok(StackRecord {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            archived: row.get::<_, i64>(8)? != 0,
                            compose: ComposeConfig {
                                kind: row.get(2)?,
                                compose_files,
                                env_file: row.get(4)?,
                            },
                            backup: crate::api::types::StackBackupConfig {
                                targets: backup_targets,
                                retention: crate::api::types::BackupRetention {
                                    keep_last: row.get::<_, i64>(6)? as u32,
                                    delete_after_stable_seconds: row.get::<_, i64>(7)? as u32,
                                },
                            },
                            services: Vec::new(),
                        })
                    },
                )
                .optional()?;

            let Some(mut stack) = stack else {
                return Ok(None);
            };

            let mut stmt = conn.prepare(
                r#"
	SELECT
	  id,
	  name,
	  image_ref,
	  image_tag,
	  current_digest,
	  current_resolved_tag,
	  current_resolved_tags_json,
	  candidate_tag,
	  candidate_resolved_tag,
	  candidate_digest,
	  candidate_arch_match,
	  candidate_arch_json,
	  ignore_rule_id,
	  ignore_reason,
	  auto_rollback,
	  repo_url,
	  homepage_json,
	  update_guard_json,
	  services.archived
	FROM services
	WHERE stack_id = ?1
	ORDER BY name ASC
"#,
            )?;
            let mut rows = stmt.query(params![stack.id.clone()])?;

            let mut services = Vec::<(
                crate::api::types::Service,
                Option<String>,
                Option<String>,
                bool,
            )>::new();
            let mut service_ids = Vec::<String>::new();

            while let Some(row) = rows.next()? {
                let service_id: String = row.get(0)?;
                let service_name: String = row.get(1)?;
                let image_reference: String = row.get(2)?;
                let image_tag: String = row.get(3)?;
                let image_digest: Option<String> = row.get(4)?;
                let homepage = deserialize_service_homepage(row.get(16)?)?;
                let update_guard = deserialize_service_update_guard(row.get(17)?)?;

                let current_resolved_tag: Option<String> = row.get(5)?;
                let current_resolved_tags_json: Option<String> = row.get(6)?;

                let candidate_tag: Option<String> = row.get(7)?;
                let candidate_resolved_tag: Option<String> = row.get(8)?;
                let candidate_digest: Option<String> = row.get(9)?;
                let candidate_arch_match: Option<String> = row.get(10)?;
                let candidate_arch_json: Option<String> = row.get(11)?;
                let ignore_rule_id: Option<String> = row.get(12)?;
                let ignore_reason: Option<String> = row.get(13)?;

                let current_resolved_tags: Option<Vec<String>> = current_resolved_tags_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .and_then(|v| if v.is_empty() { None } else { Some(v) });

                let candidate_arch: Vec<String> = candidate_arch_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .unwrap_or_default();

                let candidate = match (candidate_tag, candidate_digest) {
                    (Some(tag), Some(digest)) => Some(crate::api::types::Candidate {
                        tag,
                        resolved_tag: candidate_resolved_tag,
                        digest,
                        arch_match: crate::api::types::ArchMatch::from_str(
                            candidate_arch_match.as_deref().unwrap_or("unknown"),
                        ),
                        arch: candidate_arch,
                    }),
                    _ => None,
                };

                let ignore = match (ignore_rule_id, ignore_reason) {
                    (Some(rule_id), Some(reason)) => Some(crate::api::types::IgnoreMatch {
                        matched: true,
                        rule_id,
                        reason,
                    }),
                    _ => None,
                };
                let has_candidate = candidate.is_some();

                services.push((
                    crate::api::types::Service {
                        id: service_id.clone(),
                        name: service_name,
                        image: ComposeRef {
                            reference: image_reference,
                            tag: image_tag,
                            digest: image_digest.clone(),
                            resolved_tag: current_resolved_tag.clone(),
                            resolved_tags: current_resolved_tags,
                        },
                        homepage,
                        update_guard,
                        candidate,
                        ignore,
                        version_inference: None,
                        new_version_discovery_count: None,
                        settings: ServiceSettings {
                            auto_rollback: row.get::<_, i64>(14)? != 0,
                            backup_targets: crate::api::types::BackupTargetOverrides {
                                bind_paths: BTreeMap::new(),
                                volume_names: BTreeMap::new(),
                            },
                            repo_url: row.get(15)?,
                        },
                        archived: Some(row.get::<_, i64>(18)? != 0),
                    },
                    image_digest,
                    current_resolved_tag.clone(),
                    has_candidate,
                ));
                service_ids.push(service_id);
            }

            drop(rows);
            drop(stmt);

            let mut policies_by_service =
                BTreeMap::<String, Vec<crate::db::ServiceBackupTargetPolicyRow>>::new();
            if !service_ids.is_empty() {
                let placeholders = service_ids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    r#"
SELECT service_id, target_kind, target_key, policy
FROM service_backup_target_policies
WHERE service_id IN ({placeholders})
ORDER BY service_id ASC, target_kind ASC, target_key ASC
"#
                );
                let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(service_ids.len());
                for service_id in &service_ids {
                    params.push(service_id);
                }
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params.as_slice(), |row| {
                    let policy = match row.get::<_, String>(3)?.as_str() {
                        "stop_related_services" => {
                            crate::api::types::BackupTargetPolicy::StopRelatedServices
                        }
                        "live_backup" => crate::api::types::BackupTargetPolicy::LiveBackup,
                        _ => crate::api::types::BackupTargetPolicy::Disabled,
                    };
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        crate::db::ServiceBackupTargetPolicyRow {
                            key: row.get(2)?,
                            policy,
                        },
                    ))
                })?;
                for row in rows {
                    let (service_id, target_kind, policy_row) = row?;
                    let entry = policies_by_service.entry(service_id).or_default();
                    let wants_volume = target_kind == "volume";
                    if wants_volume != policy_row.key.starts_with('/') {
                        entry.push(policy_row);
                    }
                }
            }

            let discovery_counts =
                super::new_version_discoveries::count_new_version_discoveries_for_services_conn(
                    conn,
                    &services
                        .iter()
                        .filter(|(_, _, _, has_candidate)| *has_candidate)
                        .map(|(service, current_digest, current_resolved_tag, _)| {
                            super::new_version_discoveries::NewVersionDiscoveryBaseline {
                                service_id: service.id.clone(),
                                current_image_ref: service.image.reference.clone(),
                                current_digest: current_digest.clone(),
                                current_display_tag: current_resolved_tag
                                    .clone()
                                    .or_else(|| Some(service.image.tag.clone())),
                                current_tag: service.image.tag.clone(),
                            }
                        })
                        .collect::<Vec<_>>(),
                )?;

            for (mut service, _, _, _) in services {
                let policies = policies_by_service.remove(&service.id).unwrap_or_default();
                service.settings.backup_targets = crate::api::types::BackupTargetOverrides {
                    bind_paths: project_backup_target_overrides(&policies, false),
                    volume_names: project_backup_target_overrides(&policies, true),
                };
                service.new_version_discovery_count = discovery_counts.get(&service.id).copied();
                stack.services.push(service);
            }

            Ok(Some(stack))
        })
        .await
        .context("get stack")
    }

    #[allow(dead_code)] // Replaced on the API hot path by OperationalReadModel's query-only pool.
    pub async fn list_homepage_nav_services(&self) -> anyhow::Result<Vec<HomepageNavServiceRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  st.id,
  st.name,
  st.last_check_at,
  sv.id,
  sv.name,
  sv.image_ref,
  sv.image_tag,
  sv.current_digest,
  sv.current_resolved_tag,
  sv.current_resolved_tags_json,
  sv.candidate_tag,
  sv.candidate_resolved_tag,
  sv.candidate_digest,
  sv.candidate_arch_match,
  sv.candidate_arch_json,
  sv.ignore_rule_id,
  sv.ignore_reason,
  sv.auto_rollback,
  sv.backup_targets_bind_paths_json,
  sv.backup_targets_volume_names_json,
  sv.repo_url,
  sv.homepage_json,
  sv.update_guard_json,
  sv.archived
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                let bind_paths_json: String = row.get(18)?;
                let volume_names_json: String = row.get(19)?;
                let homepage = deserialize_service_homepage(row.get(21)?)?;
                let update_guard = deserialize_service_update_guard(row.get(22)?)?;
                let bind_paths: BTreeMap<String, crate::api::types::TernaryChoice> =
                    serde_json::from_str(&bind_paths_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let volume_names: BTreeMap<String, crate::api::types::TernaryChoice> =
                    serde_json::from_str(&volume_names_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let current_resolved_tags_json: Option<String> = row.get(9)?;
                let current_resolved_tags: Option<Vec<String>> = current_resolved_tags_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
                    .and_then(|values| {
                        if values.is_empty() {
                            None
                        } else {
                            Some(values)
                        }
                    });
                let candidate_arch_json: Option<String> = row.get(14)?;
                let candidate_arch: Vec<String> = candidate_arch_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
                    .unwrap_or_default();
                let candidate = match (
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(12)?,
                ) {
                    (Some(tag), Some(digest)) => Some(Candidate {
                        tag,
                        resolved_tag: row.get(11)?,
                        digest,
                        arch_match: crate::api::types::ArchMatch::from_str(
                            row.get::<_, Option<String>>(13)?
                                .as_deref()
                                .unwrap_or("unknown"),
                        ),
                        arch: candidate_arch,
                    }),
                    _ => None,
                };
                let ignore = match (
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                ) {
                    (Some(rule_id), Some(reason)) => Some(IgnoreMatch {
                        matched: true,
                        rule_id,
                        reason,
                    }),
                    _ => None,
                };
                Ok(HomepageNavServiceRow {
                    stack_id: row.get(0)?,
                    stack_name: row.get(1)?,
                    stack_last_check_at: row.get(2)?,
                    service: Service {
                        id: row.get(3)?,
                        name: row.get(4)?,
                        image: ComposeRef {
                            reference: row.get(5)?,
                            tag: row.get(6)?,
                            digest: row.get(7)?,
                            resolved_tag: row.get(8)?,
                            resolved_tags: current_resolved_tags,
                        },
                        homepage,
                        update_guard,
                        candidate,
                        ignore,
                        version_inference: Some(VersionInferenceState {
                            status: "ready".to_string(),
                            reason: None,
                            checked_at: None,
                        }),
                        new_version_discovery_count: None,
                        settings: ServiceSettings {
                            auto_rollback: row.get::<_, i64>(17)? != 0,
                            backup_targets: crate::api::types::BackupTargetOverrides {
                                bind_paths,
                                volume_names,
                            },
                            repo_url: row.get(20)?,
                        },
                        archived: Some(row.get::<_, i64>(23)? != 0),
                    },
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list homepage nav services")
    }

    pub async fn insert_stack(
        &self,
        stack: &StackRecord,
        services: &[crate::api::types::ServiceSeed],
        now: &str,
    ) -> anyhow::Result<()> {
        let stack = stack.clone();
        let event_stack_id = stack.id.clone();
        let services = services.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            tx.execute(
                r#"
INSERT INTO stacks (
  id,
  name,
  compose_type,
  compose_files_json,
  env_file,
  backup_targets_json,
  backup_retention_keep_last,
  backup_retention_delete_after_stable_seconds,
  created_at,
  updated_at,
  last_check_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
"#,
                params![
                    stack.id,
                    stack.name,
                    stack.compose.kind,
                    serde_json::to_string(&stack.compose.compose_files)?,
                    stack.compose.env_file,
                    serde_json::to_string(&stack.backup.targets)?,
                    stack.backup.retention.keep_last as i64,
                    stack.backup.retention.delete_after_stable_seconds as i64,
                    now,
                    now,
                    now
                ],
            )?;

            for svc in services {
                let homepage_json = serialize_service_homepage(&svc.homepage)?;
                let update_guard_json = serialize_service_update_guard(&svc.update_guard)?;
                tx.execute(
                    r#"
INSERT INTO services (
  id,
  stack_id,
  name,
  image_ref,
  image_tag,
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json,
  homepage_json,
  update_guard_json,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
"#,
                    params![
                        svc.id,
                        stack.id,
                        svc.name,
                        svc.image_ref,
                        svc.image_tag,
                        svc.auto_rollback as i64,
                        serde_json::to_string(&svc.backup_bind_paths)?,
                        serde_json::to_string(&svc.backup_volume_names)?,
                        homepage_json,
                        update_guard_json,
                        now,
                        now
                    ],
                )?;
                for (target_kind, row) in super::service_policy_rows_from_overrides(
                    &crate::api::types::BackupTargetOverrides {
                        bind_paths: svc.backup_bind_paths.clone(),
                        volume_names: svc.backup_volume_names.clone(),
                    },
                ) {
                    tx.execute(
                        r#"
INSERT INTO service_backup_target_policies (
  service_id,
  target_kind,
  target_key,
  policy,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
"#,
                        params![svc.id, target_kind, row.key, row.policy.as_str(), now],
                    )?;
                }
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("insert stack")?;
        self.management_events
            .publish_change(
                "stacks",
                "stack",
                event_stack_id,
                serde_json::json!({ "operation": "created" }),
            )
            .await;
        Ok(())
    }

    pub async fn update_stack_last_check_at(
        &self,
        stack_id: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let stack_id = stack_id.to_string();
        let event_stack_id = stack_id.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE stacks SET last_check_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![stack_id, now],
            )?;
            Ok(())
        })
        .await?;
        self.management_events
            .publish_change(
                "stacks",
                "stack",
                event_stack_id,
                serde_json::json!({ "operation": "checked" }),
            )
            .await;
        Ok(())
    }

    pub async fn set_stack_archived(
        &self,
        stack_id: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        let event_stack_id = stack_id.clone();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        let changed = self
            .call(move |conn| {
                let changed = if archived {
                    conn.execute(
                        r#"
UPDATE stacks
SET archived = 1, archived_at = ?2, archived_reason = ?3, updated_at = ?2
WHERE id = ?1
"#,
                        params![stack_id, now, reason],
                    )?
                } else {
                    conn.execute(
                        r#"
UPDATE stacks
SET archived = 0, archived_at = NULL, archived_reason = NULL, updated_at = ?2
WHERE id = ?1
"#,
                        params![stack_id, now],
                    )?
                };
                Ok(changed > 0)
            })
            .await
            .context("set stack archived")?;
        if changed {
            self.management_events
                .publish_change(
                    "stacks",
                    "stack",
                    event_stack_id,
                    serde_json::json!({ "operation": if archived { "archived" } else { "restored" } }),
                )
                .await;
        }
        Ok(changed)
    }

    pub async fn set_service_archived(
        &self,
        service_id: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let event_service_id = service_id.clone();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        let changed = self
            .call(move |conn| {
                let changed = if archived {
                    conn.execute(
                        r#"
UPDATE services
SET archived = 1, archived_at = ?2, archived_reason = ?3, updated_at = ?2
WHERE id = ?1
"#,
                        params![service_id, now, reason],
                    )?
                } else {
                    conn.execute(
                        r#"
UPDATE services
SET archived = 0, archived_at = NULL, archived_reason = NULL, updated_at = ?2
WHERE id = ?1
"#,
                        params![service_id, now],
                    )?
                };
                Ok(changed > 0)
            })
            .await
            .context("set service archived")?;
        if changed {
            self.management_events
                .publish_change(
                    "services",
                    "service",
                    event_service_id,
                    serde_json::json!({ "operation": if archived { "archived" } else { "restored" } }),
                )
                .await;
        }
        Ok(changed)
    }
    pub async fn sync_stack_from_compose_guarded(
        &self,
        stack_id: &str,
        compose_files: &[String],
        services: &[ComposeServiceSpec],
        now: &str,
        expected_generations: Option<Vec<(String, i64)>>,
    ) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        let event_stack_id = stack_id.clone();
        let compose_files = compose_files.to_vec();
        let services = services.to_vec();
        let now = now.to_string();
        let expected_generations = expected_generations.clone();
        let changed = self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current_generations = {
                let mut stmt = tx.prepare(
                    "SELECT id, accepted_state_generation FROM services WHERE stack_id = ?1 ORDER BY id",
                )?;
                stmt.query_map(params![stack_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            let blocked = current_generations
                .iter()
                .any(|(_, generation)| generation % 2 != 0)
                || expected_generations
                    .as_ref()
                    .is_some_and(|expected| expected != &current_generations);
            if blocked {
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                r#"
UPDATE stacks
SET compose_files_json = ?2, updated_at = ?3
WHERE id = ?1
"#,
                params![stack_id, serde_json::to_string(&compose_files)?, now],
            )?;
            let existing_by_name = {
                let mut stmt = tx.prepare(
                    "SELECT id, name, image_ref, image_tag, homepage_json, update_guard_json FROM services WHERE stack_id = ?1",
                )?;
                let existing_rows = stmt.query_map(params![stack_id.clone()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        deserialize_service_homepage(row.get(4)?)?,
                        deserialize_service_update_guard(row.get(5)?)?,
                    ))
                })?;
                let mut m = BTreeMap::<
                    String,
                    (
                        String,
                        String,
                        String,
                        Option<crate::api::types::ServiceHomepage>,
                        Option<crate::api::types::ServiceUpdateGuard>,
                    ),
                >::new();
                for r in existing_rows {
                    let (id, name, image_ref, image_tag, homepage, update_guard) = r?;
                    m.insert(name, (id, image_ref, image_tag, homepage, update_guard));
                }
                m
            };
            let mut keep_ids = Vec::<String>::new();
            for svc in services {
                let declared_bind_paths = svc
                    .backup_bind_paths
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let declared_volume_names = svc
                    .backup_volume_names
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                if let Some((
                    id,
                    existing_image_ref,
                    existing_image_tag,
                    existing_homepage,
                    existing_update_guard,
                )) =
                    existing_by_name.get(&svc.name)
                {
                    let homepage_json = serialize_service_homepage(&svc.homepage)?;
                    let update_guard_json = serialize_service_update_guard(&svc.update_guard)?;
                    if existing_image_ref == &svc.image_ref && existing_image_tag == &svc.image_tag {
                        if existing_homepage == &svc.homepage
                            && existing_update_guard == &svc.update_guard
                        {
                            tx.execute(
                                r#"
UPDATE services
SET updated_at = ?2
WHERE id = ?1
"#,
                                params![id, now],
                            )?;
                        } else {
                            tx.execute(
                                r#"
UPDATE services
SET homepage_json = ?2, update_guard_json = ?3, updated_at = ?4
WHERE id = ?1
"#,
                                params![id, homepage_json, update_guard_json, now],
                            )?;
                        }
                        prune_service_backup_target_policies_tx(
                            &tx,
                            id,
                            &declared_bind_paths,
                            &declared_volume_names,
                        )?;
                        keep_ids.push(id.clone());
                        continue;
                    }
                    let preserve_repo_url =
                        crate::snapshot_worker::image_repo_from_image_ref(existing_image_ref)
                            .zip(crate::snapshot_worker::image_repo_from_image_ref(
                                &svc.image_ref,
                            ))
                            .map(|(existing, incoming)| existing == incoming)
                            .unwrap_or(false);
                    let image_ref = svc.image_ref.clone();
                    let image_tag = svc.image_tag.clone();
                    tx.execute(
                        r#"
UPDATE services
SET
  image_ref = ?2,
  image_tag = ?3,
  repo_url = CASE WHEN ?4 THEN repo_url ELSE NULL END,
  homepage_json = ?5,
  update_guard_json = ?6,
  current_digest = NULL,
  current_resolved_tag = NULL,
  current_runtime_started_at = NULL,
  current_resolved_tags_json = NULL,
  candidate_tag = NULL,
  candidate_resolved_tag = NULL,
  candidate_digest = NULL,
  candidate_arch_match = NULL,
  candidate_arch_json = NULL,
  ignore_rule_id = NULL,
  ignore_reason = NULL,
  checked_at = NULL,
  updated_at = ?7
WHERE id = ?1
"#,
                        params![
                            id,
                            image_ref,
                            image_tag,
                            preserve_repo_url,
                            homepage_json,
                            update_guard_json,
                            now
                        ],
                    )?;
                    super::new_version_notifications::reconcile_service_new_version_notifications_tx(
                        &tx,
                        id,
                        &svc.image_ref,
                        &svc.image_tag,
                        None,
                        &now,
                    )?;
                    prune_service_backup_target_policies_tx(
                        &tx,
                        id,
                        &declared_bind_paths,
                        &declared_volume_names,
                    )?;
                    keep_ids.push(id.clone());
                } else {
                    let id = crate::ids::new_service_id();
                    let homepage_json = serialize_service_homepage(&svc.homepage)?;
                    let update_guard_json = serialize_service_update_guard(&svc.update_guard)?;
                    tx.execute(
                        r#"
INSERT INTO services (
  id,
  stack_id,
  name,
  image_ref,
  image_tag,
  auto_rollback,
  backup_targets_bind_paths_json,
  backup_targets_volume_names_json,
  homepage_json,
  update_guard_json,
  created_at,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
"#,
                        params![
                            id,
                            stack_id,
                            svc.name,
                            svc.image_ref,
                            svc.image_tag,
                            1i64,
                            "{}",
                            "{}",
                            homepage_json,
                            update_guard_json,
                            now,
                            now
                        ],
                    )?;
                    prune_service_backup_target_policies_tx(
                        &tx,
                        &id,
                        &declared_bind_paths,
                        &declared_volume_names,
                    )?;
                    keep_ids.push(id);
                }
            }
            if keep_ids.is_empty() {
                tx.execute(
                    "DELETE FROM services WHERE stack_id = ?1",
                    params![stack_id],
                )?;
            } else {
                let placeholders = keep_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "DELETE FROM services WHERE stack_id = ? AND id NOT IN ({placeholders})"
                );
                let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + keep_ids.len());
                params.push(&stack_id);
                for id in &keep_ids {
                    params.push(id);
                }
                tx.execute(&sql, params.as_slice())?;
            }
            tx.commit()?;
            Ok(true)
        })
            .await
            .context("sync stack from compose")?;
        if changed {
            self.management_events
                .publish_change(
                    "stacks",
                    "stack",
                    event_stack_id,
                    serde_json::json!({ "operation": "compose_synced" }),
                )
                .await;
        }
        Ok(changed)
    }
    pub async fn put_service_backup_targets(
        &self,
        service_id: &str,
        update: &crate::db::ServiceBackupTargetsUpdate,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let update = update.clone();
        let now = now.to_string();
        self.call(move |conn| put_service_backup_targets_tx(conn, &service_id, &update, &now))
            .await
            .context("put service backup targets")
    }
    pub async fn list_services_for_check(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<ServiceForCheck>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  name,
  image_ref,
  image_tag,
  current_digest,
  current_runtime_started_at,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_tag,
  candidate_digest,
  candidate_resolved_tag,
  candidate_arch_match,
  candidate_arch_json,
  ignore_rule_id,
  ignore_reason,
  checked_at,
  accepted_state_generation
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| {
                Ok(ServiceForCheck {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image_ref: row.get(2)?,
                    image_tag: row.get(3)?,
                    current_digest: row.get(4)?,
                    current_runtime_started_at: row.get(5)?,
                    current_resolved_tag: row.get(6)?,
                    current_resolved_tags_json: row.get(7)?,
                    candidate_tag: row.get(8)?,
                    candidate_digest: row.get(9)?,
                    candidate_resolved_tag: row.get(10)?,
                    candidate_arch_match: row.get(11)?,
                    candidate_arch_json: row.get(12)?,
                    ignore_rule_id: row.get(13)?,
                    ignore_reason: row.get(14)?,
                    checked_at: row.get(15)?,
                    accepted_state_generation: row.get(16)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list services for check")
    }

    pub async fn list_services_for_runtime_scan(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<ServiceForRuntimeScan>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  name,
  image_ref,
  image_tag,
  current_digest,
  current_runtime_started_at,
  current_resolved_tag,
  current_resolved_tags_json,
  candidate_tag,
  candidate_digest,
  candidate_resolved_tag,
  candidate_arch_match,
  candidate_arch_json,
  ignore_rule_id,
  ignore_reason,
  checked_at,
  accepted_state_generation
FROM services
WHERE stack_id = ?1
ORDER BY name ASC
"#,
            )?;
            let rows = stmt.query_map(params![stack_id], |row| {
                Ok(ServiceForRuntimeScan {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    image_ref: row.get(2)?,
                    image_tag: row.get(3)?,
                    current_digest: row.get(4)?,
                    current_runtime_started_at: row.get(5)?,
                    current_resolved_tag: row.get(6)?,
                    current_resolved_tags_json: row.get(7)?,
                    candidate_tag: row.get(8)?,
                    candidate_digest: row.get(9)?,
                    candidate_resolved_tag: row.get(10)?,
                    candidate_arch_match: row.get(11)?,
                    candidate_arch_json: row.get(12)?,
                    ignore_rule_id: row.get(13)?,
                    ignore_reason: row.get(14)?,
                    checked_at: row.get(15)?,
                    accepted_state_generation: row.get(16)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list services for runtime scan")
    }

    pub async fn get_stack_compose_project(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT project
FROM discovered_compose_projects
WHERE
  stack_id = ?1
  AND status != 'missing'
  AND archived = 0
ORDER BY last_scan_at DESC
LIMIT 1
"#,
                    params![stack_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
        .context("get stack compose project")
    }

    pub async fn get_service_stack_id(&self, service_id: &str) -> anyhow::Result<Option<String>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT id, stack_id, image_ref, image_tag
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| row.get::<_, String>(1),
                )
                .optional()?)
        })
        .await
        .context("get service stack id")
    }

    pub async fn get_service_snapshot_target(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceSnapshotTarget>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT image_ref, image_tag, current_digest, candidate_digest
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| {
                        Ok(ServiceSnapshotTarget {
                            image_ref: row.get(0)?,
                            current_tag: row.get(1)?,
                            current_digest: row.get(2)?,
                            candidate_digest: row.get(3)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get service snapshot target")
    }

    pub async fn get_service_new_version_timeline_context(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceNewVersionTimelineContext>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  image_ref,
  image_tag,
  current_digest,
  current_runtime_started_at,
  current_resolved_tag,
  candidate_tag,
  candidate_resolved_tag,
  candidate_digest
FROM services
WHERE id = ?1
"#,
                    params![service_id],
                    |row| {
                        Ok(ServiceNewVersionTimelineContext {
                            image_ref: row.get(0)?,
                            current_tag: row.get(1)?,
                            current_digest: row.get(2)?,
                            current_runtime_started_at: row.get(3)?,
                            current_resolved_tag: row.get(4)?,
                            candidate_tag: row.get(5)?,
                            candidate_resolved_tag: row.get(6)?,
                            candidate_digest: row.get(7)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get service new version timeline context")
    }

    pub async fn list_snapshot_seed_targets(&self) -> anyhow::Result<Vec<(String, String)>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT services.image_ref, services.current_digest
FROM services
JOIN stacks ON stacks.id = services.stack_id
WHERE services.archived = 0 AND stacks.archived = 0
  AND services.current_digest IS NOT NULL AND TRIM(services.current_digest) != ''
ORDER BY services.id ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list snapshot seed targets")
    }

    pub async fn list_snapshot_anchor_tags(
        &self,
        image_repo: &str,
        digest: &str,
    ) -> anyhow::Result<Vec<String>> {
        let image_repo = image_repo.to_string();
        let digest = digest.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  image_ref,
  image_tag,
  current_digest,
  current_resolved_tag,
  candidate_tag,
  candidate_digest,
  candidate_resolved_tag
FROM services
WHERE
  (current_digest IS NOT NULL AND TRIM(current_digest) = ?1)
  OR (candidate_digest IS NOT NULL AND TRIM(candidate_digest) = ?1)
ORDER BY id ASC
"#,
            )?;
            let rows = stmt.query_map(params![digest], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?;

            let mut tags: BTreeSet<String> = BTreeSet::new();
            for row in rows {
                let (
                    image_ref,
                    image_tag,
                    current_digest,
                    current_resolved_tag,
                    candidate_tag,
                    candidate_digest,
                    candidate_resolved_tag,
                ) = row?;

                let Some(parsed) = crate::registry::ImageRef::parse(&image_ref).ok() else {
                    continue;
                };
                let row_repo = format!("{}/{}", parsed.registry, parsed.name);
                if row_repo != image_repo {
                    continue;
                }

                let current_matches = current_digest
                    .as_deref()
                    .is_some_and(|d| d.trim() == digest.as_str());
                let candidate_matches = candidate_digest
                    .as_deref()
                    .is_some_and(|d| d.trim() == digest.as_str());

                if current_matches {
                    let tag = image_tag.trim();
                    if !tag.is_empty() {
                        tags.insert(tag.to_string());
                    }
                    if let Some(tag) = current_resolved_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                }

                if candidate_matches {
                    if let Some(tag) = candidate_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                    if let Some(tag) = candidate_resolved_tag
                        .as_deref()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                    {
                        tags.insert(tag.to_string());
                    }
                    let current_tag = image_tag.trim();
                    if !current_tag.is_empty() {
                        tags.insert(current_tag.to_string());
                    }
                }
            }

            Ok(tags.into_iter().collect())
        })
        .await
        .context("list snapshot anchor tags")
    }

    pub async fn is_stack_archived(&self, stack_id: &str) -> anyhow::Result<Option<bool>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT archived FROM stacks WHERE id = ?1",
                    params![stack_id],
                    |row| Ok(row.get::<_, i64>(0)? != 0),
                )
                .optional()?)
        })
        .await
        .context("is stack archived")
    }

    pub async fn is_service_archived(&self, service_id: &str) -> anyhow::Result<Option<bool>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT archived FROM services WHERE id = ?1",
                    params![service_id],
                    |row| Ok(row.get::<_, i64>(0)? != 0),
                )
                .optional()?)
        })
        .await
        .context("is service archived")
    }

    pub async fn has_unarchived_services_in_stack(&self, stack_id: &str) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM services WHERE stack_id = ?1 AND archived = 0 LIMIT 1",
                    params![stack_id],
                    |_row| Ok(()),
                )
                .optional()?
                .is_some())
        })
        .await
        .context("has unarchived services in stack")
    }

    pub async fn has_unarchived_services(&self, service_ids: &[String]) -> anyhow::Result<bool> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            if service_ids.is_empty() {
                return Ok(false);
            }
            let placeholders = service_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT 1 FROM services WHERE archived = 0 AND id IN ({placeholders}) LIMIT 1"
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(service_ids.len());
            for id in &service_ids {
                params.push(id);
            }
            Ok(conn
                .query_row(&sql, params.as_slice(), |_row| Ok(()))
                .optional()?
                .is_some())
        })
        .await
        .context("has unarchived services")
    }

    pub async fn list_stack_ids(&self) -> anyhow::Result<Vec<String>> {
        self.call(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM stacks ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list stack ids")
    }
}
