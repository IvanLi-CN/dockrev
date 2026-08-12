use super::*;

impl Db {
    pub async fn get_discovered_compose_project(
        &self,
        project: &str,
    ) -> anyhow::Result<Option<DiscoveredComposeProjectRecord>> {
        let project = project.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT stack_id
FROM discovered_compose_projects
WHERE project = ?1
"#,
                    params![project],
                    |row| {
                        Ok(DiscoveredComposeProjectRecord {
                            stack_id: row.get(0)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get discovered compose project")
    }

    pub async fn upsert_discovered_compose_project(
        &self,
        input: DiscoveredComposeProjectUpsert,
    ) -> anyhow::Result<()> {
        self.call(move |conn| {
            let last_config_files_json = input
                .last_config_files
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                r#"
INSERT INTO discovered_compose_projects (
  project,
  stack_id,
  status,
  last_seen_at,
  last_scan_at,
  last_error,
  last_config_files_json,
  archived,
  archived_at,
  archived_reason
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(project) DO UPDATE SET
  stack_id = COALESCE(excluded.stack_id, discovered_compose_projects.stack_id),
  status = excluded.status,
  last_seen_at = COALESCE(excluded.last_seen_at, discovered_compose_projects.last_seen_at),
  last_scan_at = excluded.last_scan_at,
  last_error = excluded.last_error,
  last_config_files_json = COALESCE(
    excluded.last_config_files_json,
    discovered_compose_projects.last_config_files_json
  )
"#,
                params![
                    input.project,
                    input.stack_id,
                    input.status,
                    input.last_seen_at,
                    input.last_scan_at,
                    input.last_error,
                    last_config_files_json,
                    0i64,
                    Option::<String>::None,
                    Option::<String>::None
                ],
            )?;

            if input.unarchive_if_active && input.status != "missing" {
                let project = input.project.clone();
                let scanned_at = input.last_scan_at.clone();
                tx.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE project = ?1
  AND archived_reason IN (
    'auto_archive_on_restart',
    'auto_archive_compose_files_missing'
  )
"#,
                    params![project],
                )?;
                tx.execute(
                    r#"
UPDATE stacks
SET archived = 0, archived_at = NULL, archived_reason = NULL, updated_at = ?2
WHERE id = (
  SELECT stack_id
  FROM discovered_compose_projects
  WHERE project = ?1
)
  AND EXISTS (
    SELECT 1
    FROM discovered_compose_projects
    WHERE project = ?1
      AND archived = 0
      AND archived_reason IS NULL
  )
  AND archived_reason IN (
    'auto_archive_on_restart',
    'auto_archive_compose_files_missing'
  )
"#,
                    params![project, scanned_at],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert discovered compose project")
    }

    pub async fn list_persisted_discovered_compose_projects_except(
        &self,
        seen_projects: &[String],
    ) -> anyhow::Result<Vec<PersistedDiscoveredComposeProject>> {
        let seen_projects = seen_projects.to_vec();
        self.call(move |conn| {
            let mut sql = String::from(
                r#"
SELECT
  d.project,
  d.stack_id,
  d.status,
  d.last_config_files_json,
  s.compose_files_json
FROM discovered_compose_projects d
LEFT JOIN stacks s ON s.id = d.stack_id
"#,
            );
            if !seen_projects.is_empty() {
                let placeholders = seen_projects
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                sql.push_str(&format!("WHERE d.project NOT IN ({placeholders})"));
            }
            sql.push_str(" ORDER BY d.project ASC");

            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(seen_projects.len());
            for project in &seen_projects {
                params.push(project);
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                let last_config_files_json: Option<String> = row.get(3)?;
                let stack_compose_files_json: Option<String> = row.get(4)?;
                let last_config_files = last_config_files_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                    .filter(|files| !files.is_empty());
                let stack_compose_files = stack_compose_files_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                    .filter(|files| !files.is_empty());

                Ok(PersistedDiscoveredComposeProject {
                    project: row.get(0)?,
                    stack_id: row.get(1)?,
                    status: row.get(2)?,
                    compose_files: last_config_files.or(stack_compose_files),
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list persisted discovered compose projects")
    }

    pub async fn archive_discovered_compose_project_for_missing_compose_files(
        &self,
        project: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let project = project.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                r#"
UPDATE discovered_compose_projects
SET archived = 1,
    archived_at = ?2,
    archived_reason = 'auto_archive_compose_files_missing'
WHERE project = ?1
  AND (
    archived = 0
    OR archived_reason IN (
      'auto_archive_on_restart',
      'auto_archive_compose_files_missing'
    )
  )
"#,
                params![project, now],
            )?;
            tx.execute(
                r#"
UPDATE stacks
SET archived = 1,
    archived_at = ?2,
    archived_reason = 'auto_archive_compose_files_missing',
    updated_at = ?2
WHERE id = (
  SELECT stack_id
  FROM discovered_compose_projects
  WHERE project = ?1
)
  AND (
    SELECT archived_reason IN (
      'auto_archive_on_restart',
      'auto_archive_compose_files_missing'
    )
    FROM discovered_compose_projects
    WHERE project = ?1
  )
  AND (
    archived = 0
    OR archived_reason IN (
      'auto_archive_on_restart',
      'auto_archive_compose_files_missing'
    )
  )
"#,
                params![project, now],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("archive discovered compose project with missing compose files")
    }

    pub async fn list_discovered_compose_projects(
        &self,
        archived: ArchivedFilter,
    ) -> anyhow::Result<Vec<crate::api::types::DiscoveredProject>> {
        self.call(move |conn| {
            let filter_clause = archived.where_clause("d.archived");
            let sql = format!(
                r#"
SELECT
  d.project,
  d.status,
  d.stack_id,
  d.last_config_files_json,
  d.last_seen_at,
  d.last_scan_at,
  d.last_error,
  d.archived
FROM discovered_compose_projects d
WHERE 1=1
{filter_clause}
ORDER BY d.project ASC
"#
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |row| {
                let config_files_json: Option<String> = row.get(3)?;
                let config_files = config_files_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());

                Ok(crate::api::types::DiscoveredProject {
                    project: row.get(0)?,
                    status: crate::api::types::DiscoveredProjectStatus::from_str(
                        row.get::<_, String>(1)?.as_str(),
                    ),
                    stack_id: row.get(2)?,
                    config_files,
                    last_seen_at: row.get(4)?,
                    last_scan_at: row.get(5)?,
                    last_error: row.get(6)?,
                    archived: row.get::<_, i64>(7)? != 0,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list discovered compose projects")
    }

    pub async fn set_discovered_compose_project_archived(
        &self,
        project: &str,
        archived: bool,
        reason: Option<&str>,
        now: &str,
    ) -> anyhow::Result<bool> {
        let project = project.to_string();
        let now = now.to_string();
        let reason = reason.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = if archived {
                conn.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 1, archived_at = ?2, archived_reason = ?3
WHERE project = ?1
"#,
                    params![project, now, reason],
                )?
            } else {
                conn.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE project = ?1
"#,
                    params![project],
                )?
            };
            Ok(changed > 0)
        })
        .await
        .context("set discovered compose project archived")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::types::ComposeConfig, ids};

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dockrev-discovery-db-{}.sqlite3",
            ulid::Ulid::new()
        ))
    }

    fn now() -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    }

    async fn seed_project(db: &Db, project: &str, archived_reason: Option<&str>) -> String {
        let stack_id = ids::new_stack_id();
        let now = now();
        let stack = crate::api::types::StackRecord {
            id: stack_id.clone(),
            name: project.to_string(),
            archived: false,
            compose: ComposeConfig {
                kind: "path".to_string(),
                compose_files: vec!["/srv/example/compose.yml".to_string()],
                env_file: None,
            },
            backup: crate::api::types::StackBackupConfig::default(),
            services: Vec::new(),
        };
        db.insert_stack(&stack, &[], &now).await.unwrap();
        db.upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
            project: project.to_string(),
            stack_id: Some(stack_id.clone()),
            status: "missing".to_string(),
            last_seen_at: None,
            last_scan_at: now.clone(),
            last_error: Some("compose_files_missing".to_string()),
            last_config_files: Some(vec!["/srv/example/compose.yml".to_string()]),
            unarchive_if_active: false,
        })
        .await
        .unwrap();
        if let Some(reason) = archived_reason {
            db.set_stack_archived(&stack_id, true, Some(reason), &now)
                .await
                .unwrap();
            db.set_discovered_compose_project_archived(project, true, Some(reason), &now)
                .await
                .unwrap();
        }
        stack_id
    }

    #[tokio::test]
    async fn healthy_reconciliation_restores_only_system_archives() {
        let path = temp_db_path();
        let db = Db::open(&path).await.unwrap();
        let automatic_stack = seed_project(&db, "automatic", Some("auto_archive_on_restart")).await;
        let missing_auto_stack = seed_project(
            &db,
            "missing-auto",
            Some("auto_archive_compose_files_missing"),
        )
        .await;
        let user_stack = seed_project(&db, "manual", Some("user_archive")).await;
        let discovery_manual_stack =
            seed_project(&db, "discovery-manual", Some("auto_archive_on_restart")).await;
        db.set_discovered_compose_project_archived(
            "discovery-manual",
            true,
            Some("user_archive"),
            &now(),
        )
        .await
        .unwrap();
        let now = now();

        for project in ["automatic", "missing-auto", "manual", "discovery-manual"] {
            db.upsert_discovered_compose_project(DiscoveredComposeProjectUpsert {
                project: project.to_string(),
                stack_id: None,
                status: "stopped".to_string(),
                last_seen_at: None,
                last_scan_at: now.clone(),
                last_error: None,
                last_config_files: None,
                unarchive_if_active: true,
            })
            .await
            .unwrap();
        }

        assert!(
            !db.get_stack(&automatic_stack)
                .await
                .unwrap()
                .unwrap()
                .archived
        );
        assert!(
            !db.get_stack(&missing_auto_stack)
                .await
                .unwrap()
                .unwrap()
                .archived
        );
        assert!(db.get_stack(&user_stack).await.unwrap().unwrap().archived);
        assert!(
            db.get_stack(&discovery_manual_stack)
                .await
                .unwrap()
                .unwrap()
                .archived
        );
        let visible = db
            .list_discovered_compose_projects(ArchivedFilter::Exclude)
            .await
            .unwrap();
        assert!(visible.iter().any(|project| project.project == "automatic"));
        assert!(
            visible
                .iter()
                .any(|project| project.project == "missing-auto")
        );
        assert!(!visible.iter().any(|project| project.project == "manual"));
        assert!(
            !visible
                .iter()
                .any(|project| project.project == "discovery-manual")
        );

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }

    #[tokio::test]
    async fn missing_compose_files_archive_only_system_discovery_records() {
        let path = temp_db_path();
        let db = Db::open(&path).await.unwrap();
        let automatic_stack = seed_project(&db, "automatic", None).await;
        let user_stack = seed_project(&db, "manual", Some("user_archive")).await;
        let now = now();

        db.archive_discovered_compose_project_for_missing_compose_files("automatic", &now)
            .await
            .unwrap();
        db.archive_discovered_compose_project_for_missing_compose_files("manual", &now)
            .await
            .unwrap();

        assert!(
            db.get_stack(&automatic_stack)
                .await
                .unwrap()
                .unwrap()
                .archived
        );
        assert!(db.get_stack(&user_stack).await.unwrap().unwrap().archived);
        let archive_reasons = db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, archived_reason FROM stacks WHERE id IN (?1, ?2) ORDER BY id",
                )?;
                let rows = stmt.query_map(params![automatic_stack, user_stack], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await
            .unwrap();
        assert!(
            archive_reasons
                .iter()
                .any(|(_, reason)| reason.as_deref() == Some("auto_archive_compose_files_missing"))
        );
        assert!(
            archive_reasons
                .iter()
                .any(|(_, reason)| reason.as_deref() == Some("user_archive"))
        );
        let archived = db
            .list_discovered_compose_projects(ArchivedFilter::Only)
            .await
            .unwrap();
        assert_eq!(archived.len(), 2);

        drop(db);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }
}
