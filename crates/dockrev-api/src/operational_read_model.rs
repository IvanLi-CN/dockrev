use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio_rusqlite::Connection;

use crate::{
    api::types::{
        ArchMatch, BackupTargetOverrides, Candidate, ComposeRef, IgnoreMatch, JobCompactListItem,
        JobProgress, JobResultReason, ResourceMonitorSettings, Service, ServiceSettings,
        VersionInferenceState,
    },
    db::{
        HomepageNavServiceRow, ImageDigestTagsSnapshotRow, JobListFilters, NewVersionDiscoveryRow,
        NotificationTargetKey, StableCandidateDisplayTagsByNotificationTarget,
    },
};

const SNAPSHOT_TARGET_BATCH_SIZE: usize = 400;

/// Read-only connections reserved for API hot paths. Command-side `Db` remains the only
/// business write owner; callers here must not fall back to a generic repository call.
#[derive(Clone)]
pub struct OperationalReadModel {
    readers: Vec<Connection>,
    next_reader: Arc<AtomicUsize>,
}

impl OperationalReadModel {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let mut readers = Vec::with_capacity(2);
        for _ in 0..2 {
            let reader = Connection::open(path).await?;
            reader
                .call(|conn| {
                    conn.execute_batch("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000;")?;
                    Ok::<(), anyhow::Error>(())
                })
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            readers.push(reader);
        }
        Ok(Self {
            readers,
            next_reader: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub async fn list_active_service_ids(&self) -> anyhow::Result<Vec<String>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT sv.id FROM services sv JOIN stacks st ON st.id = sv.stack_id WHERE sv.archived = 0 AND st.archived = 0 ORDER BY sv.id",
            )?;
            Ok::<Vec<String>, anyhow::Error>(stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<String>, _>>()?)
        }).await
    }

    pub async fn list_homepage_nav_services(&self) -> anyhow::Result<Vec<HomepageNavServiceRow>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  st.id, st.name, st.last_check_at,
  sv.id, sv.name, sv.image_ref, sv.image_tag, sv.current_digest,
  sv.current_resolved_tag, sv.current_resolved_tags_json,
  sv.candidate_tag, sv.candidate_resolved_tag, sv.candidate_digest,
  sv.candidate_arch_match, sv.candidate_arch_json,
  sv.ignore_rule_id, sv.ignore_reason, sv.auto_rollback,
  sv.backup_targets_bind_paths_json, sv.backup_targets_volume_names_json,
  sv.repo_url, sv.homepage_json, sv.update_guard_json, sv.archived
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], homepage_nav_service_from_row)?;
            Ok::<_, anyhow::Error>(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn get_resource_monitor_settings(&self) -> anyhow::Result<ResourceMonitorSettings> {
        self.call(|conn| {
            conn.query_row(
                "SELECT resource_monitor_enabled, resource_sample_interval_seconds FROM settings WHERE id = 'default'",
                [],
                |row| {
                    let raw_interval = row.get::<_, i64>(1)? as u64;
                    Ok(ResourceMonitorSettings {
                        enabled: row.get::<_, i64>(0)? != 0,
                        sample_interval_seconds:
                            crate::resource_usage::normalize_sample_interval_seconds(raw_interval),
                        retention_days: crate::resource_usage::RESOURCE_MONITOR_RETENTION_DAYS,
                    })
                },
            )
            .map_err(Into::into)
        })
        .await
    }

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
            for chunk in targets.chunks(SNAPSHOT_TARGET_BATCH_SIZE) {
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
                    "SELECT image_repo, digest, host_platform, snapshot_json, checked_at, updated_at FROM image_digest_tags_snapshots WHERE host_platform = ?1 AND ({clauses}) ORDER BY updated_at DESC, image_repo ASC, digest ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut params: Vec<&dyn rusqlite::ToSql> =
                    Vec::with_capacity(chunk.len() * 2 + 1);
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
    }

    pub async fn list_new_version_discoveries_for_services(
        &self,
        service_ids: &[String],
    ) -> anyhow::Result<Vec<NewVersionDiscoveryRow>> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            Ok(crate::db::list_new_version_discoveries_for_services_conn(
                conn,
                &service_ids,
            )?)
        })
        .await
    }

    pub async fn list_stable_candidate_display_tags_for_notification_targets(
        &self,
        targets: &[NotificationTargetKey],
    ) -> anyhow::Result<StableCandidateDisplayTagsByNotificationTarget> {
        let targets = targets.to_vec();
        self.call(move |conn| {
            Ok(
                crate::db::list_stable_candidate_display_tags_for_notification_targets_conn(
                    conn, &targets,
                )?,
            )
        })
        .await
    }

    pub async fn list_compact_jobs(
        &self,
        filters: JobListFilters,
    ) -> anyhow::Result<(Vec<JobCompactListItem>, Option<(String, String)>)> {
        let limit = filters.limit.clamp(1, 200);
        self.call(move |conn| {
            let mut where_clauses = vec!["1 = 1".to_string()];
            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            if !filters.types.is_empty() {
                let placeholders = std::iter::repeat_n("?", filters.types.len()).collect::<Vec<_>>().join(",");
                where_clauses.push(format!("j.type IN ({placeholders})"));
                values.extend(filters.types.into_iter().map(rusqlite::types::Value::from));
            }
            if let Some(status) = filters.status { where_clauses.push("j.status = ?".to_string()); values.push(status.into()); }
            if let Some(stack_id) = filters.stack_id {
                where_clauses.push("(j.stack_id = ? OR EXISTS (SELECT 1 FROM job_service_targets jst JOIN services target_service ON target_service.id = jst.service_id WHERE jst.job_id = j.id AND target_service.stack_id = ?))".to_string());
                values.push(stack_id.clone().into()); values.push(stack_id.into());
            }
            if let Some(service_id) = filters.service_id {
                where_clauses.push("EXISTS (SELECT 1 FROM job_service_targets jst WHERE jst.job_id = j.id AND jst.service_id = ?)".to_string());
                values.push(service_id.into());
            }
            if let Some((created_at, id)) = filters.cursor {
                where_clauses.push("(j.created_at < ? OR (j.created_at = ? AND j.id < ?))".to_string());
                values.push(created_at.clone().into()); values.push(created_at.into()); values.push(id.into());
            }
            values.push(((limit + 1) as i64).into());
            let sql = format!(r#"SELECT j.id, j.type, j.scope, j.stack_id, j.service_id, j.status, j.created_by, j.reason,
                j.created_at, j.started_at, j.finished_at, j.summary_json,
                COALESCE(service.name, stack.name, j.type)
              FROM jobs j
              LEFT JOIN services service ON service.id = j.service_id
              LEFT JOIN stacks stack ON stack.id = j.stack_id
              WHERE {} ORDER BY j.created_at DESC, j.id DESC LIMIT ?"#, where_clauses.join(" AND "));
            let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value as &dyn rusqlite::ToSql).collect();
            let mut stmt = conn.prepare(&sql)?;
            let mut jobs = stmt
                .query_map(params.as_slice(), compact_job_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = if jobs.len() > limit as usize {
                jobs.truncate(limit as usize);
                jobs.last().map(|job| (job.created_at.clone(), job.id.clone()))
            } else { None };
            Ok::<_, anyhow::Error>((jobs, next_cursor))
        }).await
    }

    async fn call<R, F>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        self.readers[index]
            .call(f)
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))
    }
}

fn homepage_nav_service_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<HomepageNavServiceRow> {
    let bind_paths_json: String = row.get(18)?;
    let volume_names_json: String = row.get(19)?;
    let homepage = serde_json::from_str(
        row.get::<_, Option<String>>(21)?
            .as_deref()
            .unwrap_or("null"),
    )
    .map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(21, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let update_guard = serde_json::from_str(
        row.get::<_, Option<String>>(22)?
            .as_deref()
            .unwrap_or("null"),
    )
    .map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(22, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let bind_paths: BTreeMap<String, crate::api::types::TernaryChoice> =
        serde_json::from_str(&bind_paths_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                18,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let volume_names: BTreeMap<String, crate::api::types::TernaryChoice> =
        serde_json::from_str(&volume_names_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                19,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let current_resolved_tags = row
        .get::<_, Option<String>>(9)?
        .as_deref()
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .filter(|values| !values.is_empty());
    let candidate_arch = row
        .get::<_, Option<String>>(14)?
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
            arch_match: ArchMatch::from_str(
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
                backup_targets: BackupTargetOverrides {
                    bind_paths,
                    volume_names,
                },
                repo_url: row.get(20)?,
            },
            archived: Some(row.get::<_, i64>(23)? != 0),
        },
    })
}

fn compact_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobCompactListItem> {
    let job_type: String = row.get(1)?;
    let status: String = row.get(5)?;
    let summary_json: String = row.get(11)?;
    let summary: serde_json::Value = serde_json::from_str(&summary_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let progress = summary
        .get("progress")
        .cloned()
        .and_then(|value| serde_json::from_value::<JobProgress>(value).ok());
    let result_reason: Option<JobResultReason> = crate::api::types::result_reason_from_summary(
        &job_type,
        &status,
        &summary,
        progress.as_ref(),
    );
    let target_version = ["targetDisplayTag", "targetTag", "to"]
        .iter()
        .find_map(|key| summary.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let fallback_display_label: String = row.get(12)?;
    let display_label =
        lifecycle_action_display_label(&job_type, &summary).unwrap_or(fallback_display_label);
    Ok(JobCompactListItem {
        id: row.get(0)?,
        r#type: job_type,
        scope: row.get(2)?,
        stack_id: row.get(3)?,
        service_id: row.get(4)?,
        status,
        created_by: row.get(6)?,
        reason: row.get(7)?,
        created_at: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        progress,
        result_reason,
        display_label,
        target_version,
    })
}

fn lifecycle_action_display_label(job_type: &str, summary: &serde_json::Value) -> Option<String> {
    if !matches!(job_type, "service_lifecycle" | "stack_lifecycle") {
        return None;
    }
    match summary.get("action").and_then(serde_json::Value::as_str) {
        Some("start") => Some("启动任务".to_string()),
        Some("stop") => Some("停止任务".to_string()),
        Some("restart") => Some("重启任务".to_string()),
        _ => None,
    }
}
