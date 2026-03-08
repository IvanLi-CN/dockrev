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
  last_config_files_json = excluded.last_config_files_json
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

            if input.unarchive_if_active && input.status == "active" {
                tx.execute(
                    r#"
UPDATE discovered_compose_projects
SET archived = 0, archived_at = NULL, archived_reason = NULL
WHERE project = ?1
"#,
                    params![input.project],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert discovered compose project")
    }

    pub async fn mark_discovered_compose_projects_missing_except(
        &self,
        seen_projects: &[String],
        now: &str,
    ) -> anyhow::Result<Vec<String>> {
        let seen_projects = seen_projects.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let newly_missing = if seen_projects.is_empty() {
                let mut stmt = tx.prepare(
                    r#"
	SELECT project
	FROM discovered_compose_projects
	WHERE status != 'missing'
	"#,
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                let newly_missing = rows.collect::<Result<Vec<_>, _>>()?;
                tx.execute(
                    r#"
	UPDATE discovered_compose_projects
	SET status = 'missing', last_scan_at = ?1
	WHERE status != 'missing'
	"#,
                    params![now],
                )?;
                newly_missing
            } else {
                let placeholders = seen_projects.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql_select = format!(
                    "SELECT project FROM discovered_compose_projects WHERE status != 'missing' AND project NOT IN ({placeholders})"
                );
                let mut params: Vec<&dyn rusqlite::ToSql> =
                    Vec::with_capacity(seen_projects.len());
                for p in &seen_projects {
                    params.push(p);
                }
                let mut stmt = tx.prepare(&sql_select)?;
                let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
                let newly_missing = rows.collect::<Result<Vec<_>, _>>()?;

                let sql_update = format!(
                    "UPDATE discovered_compose_projects SET status = 'missing', last_scan_at = ? WHERE status != 'missing' AND project NOT IN ({placeholders})"
                );
                let mut params2: Vec<&dyn rusqlite::ToSql> =
                    Vec::with_capacity(1 + seen_projects.len());
                params2.push(&now);
                for p in &seen_projects {
                    params2.push(p);
                }
                tx.execute(&sql_update, params2.as_slice())?;
                newly_missing
            };

            tx.commit()?;
            Ok(newly_missing)
        })
        .await
        .context("mark discovered compose projects missing")
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
