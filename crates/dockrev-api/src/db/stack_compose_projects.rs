use super::*;

impl Db {
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

    pub async fn is_stack_missing(&self, stack_id: &str) -> anyhow::Result<bool> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            Ok(conn.query_row(
                r#"
SELECT EXISTS (
  SELECT 1 FROM discovered_compose_projects d
  WHERE d.stack_id = ?1 AND d.status = 'missing'
) AND NOT EXISTS (
  SELECT 1 FROM discovered_compose_projects d
  WHERE d.stack_id = ?1 AND d.status <> 'missing'
)
"#,
                params![stack_id],
                |row| Ok(row.get::<_, i64>(0)? != 0),
            )?)
        })
        .await
        .context("check stack missing state")
    }

    pub async fn get_stack_compose_projects_for_cleanup(
        &self,
        stack_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let stack_id = stack_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT project
FROM discovered_compose_projects
WHERE stack_id = ?1
ORDER BY last_scan_at DESC, project ASC
"#,
            )?;
            Ok(stmt
                .query_map(params![stack_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("get stack compose projects for cleanup")
    }
}
