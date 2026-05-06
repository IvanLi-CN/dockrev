use super::*;

impl Db {
    pub async fn upsert_service_tag_history(
        &self,
        service_id: &str,
        image_repo: &str,
        tag: &str,
        source: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        let image_repo = image_repo.to_string();
        let tag = tag.to_string();
        let source = source.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
INSERT INTO service_tag_history (
  service_id,
  image_repo,
  tag,
  last_used_at,
  use_count,
  source
) VALUES (?1, ?2, ?3, ?4, 1, ?5)
ON CONFLICT(service_id, image_repo, tag) DO UPDATE SET
  last_used_at = excluded.last_used_at,
  use_count = service_tag_history.use_count + 1,
  source = excluded.source
"#,
                params![service_id, image_repo, tag, now, source],
            )?;
            Ok(())
        })
        .await
        .context("upsert service tag history")
    }

    pub async fn list_service_tag_suggestions(
        &self,
        service_id: &str,
        image_repo: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<ServiceTagSuggestion>> {
        let service_id = service_id.to_string();
        let image_repo = image_repo.to_string();
        let limit = i64::from(limit.clamp(1, 20));
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT tag, last_used_at, source, use_count
FROM service_tag_history
WHERE service_id = ?1 AND image_repo = ?2
ORDER BY last_used_at DESC, tag DESC
LIMIT ?3
"#,
            )?;
            let rows = stmt.query_map(params![service_id, image_repo, limit], |row| {
                Ok(ServiceTagSuggestion {
                    tag: row.get(0)?,
                    last_used_at: row.get(1)?,
                    source: row.get(2)?,
                    use_count: row.get::<_, i64>(3)?.max(0) as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service tag suggestions")
    }
}
