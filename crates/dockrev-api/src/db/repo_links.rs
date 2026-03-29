use super::*;

impl Db {
    pub async fn count_repo_link_backfill_candidates(
        &self,
        stack_id: Option<&str>,
    ) -> anyhow::Result<u32> {
        let stack_id = stack_id.map(ToString::to_string);
        self.call(move |conn| {
            let count = match stack_id {
                Some(stack_id) => conn.query_row(
                    r#"
SELECT COUNT(*)
FROM services sv
JOIN stacks s ON s.id = sv.stack_id
WHERE
  sv.stack_id = ?1
  AND s.archived = 0
  AND sv.archived = 0
  AND sv.repo_url IS NULL
  AND sv.repo_url_auto_disabled = 0
"#,
                    params![stack_id],
                    |row| row.get::<_, i64>(0),
                )?,
                None => conn.query_row(
                    r#"
SELECT COUNT(*)
FROM services sv
JOIN stacks s ON s.id = sv.stack_id
WHERE
  s.archived = 0
  AND sv.archived = 0
  AND sv.repo_url IS NULL
  AND sv.repo_url_auto_disabled = 0
"#,
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
            };
            Ok(count as u32)
        })
        .await
        .context("count repo link backfill candidates")
    }

    pub async fn list_repo_link_backfill_targets(
        &self,
        stack_id: Option<&str>,
    ) -> anyhow::Result<Vec<RepoLinkBackfillTarget>> {
        let stack_id = stack_id.map(ToString::to_string);
        self.call(move |conn| {
            let sql = if stack_id.is_some() {
                r#"
SELECT
  sv.id,
  sv.stack_id,
  s.name,
  sv.name,
  sv.image_ref,
  sv.image_tag,
  sv.current_digest,
  sv.candidate_digest,
  sv.repo_url_auto_disabled
FROM services sv
JOIN stacks s ON s.id = sv.stack_id
WHERE
  sv.stack_id = ?1
  AND s.archived = 0
  AND sv.archived = 0
  AND sv.repo_url IS NULL
ORDER BY s.name ASC, sv.name ASC
"#
            } else {
                r#"
SELECT
  sv.id,
  sv.stack_id,
  s.name,
  sv.name,
  sv.image_ref,
  sv.image_tag,
  sv.current_digest,
  sv.candidate_digest,
  sv.repo_url_auto_disabled
FROM services sv
JOIN stacks s ON s.id = sv.stack_id
WHERE
  s.archived = 0
  AND sv.archived = 0
  AND sv.repo_url IS NULL
ORDER BY s.name ASC, sv.name ASC
"#
            };
            let mut stmt = conn.prepare(sql)?;
            let mut rows = if let Some(stack_id) = stack_id {
                stmt.query(params![stack_id])?
            } else {
                stmt.query([])?
            };

            let mut targets = Vec::new();
            while let Some(row) = rows.next()? {
                targets.push(RepoLinkBackfillTarget {
                    service_id: row.get(0)?,
                    stack_id: row.get(1)?,
                    stack_name: row.get(2)?,
                    service_name: row.get(3)?,
                    snapshot_target: ServiceSnapshotTarget {
                        image_ref: row.get(4)?,
                        current_tag: row.get(5)?,
                        current_digest: row.get(6)?,
                        candidate_digest: row.get(7)?,
                    },
                    repo_url_auto_disabled: row.get::<_, i64>(8)? != 0,
                });
            }

            Ok(targets)
        })
        .await
        .context("list repo link backfill targets")
    }

    pub async fn set_service_repo_url_if_empty(
        &self,
        service_id: &str,
        repo_url: &str,
        now: &str,
    ) -> anyhow::Result<bool> {
        let service_id = service_id.to_string();
        let repo_url = repo_url.to_string();
        let now = now.to_string();
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE services
SET repo_url = ?2, updated_at = ?3
WHERE id = ?1 AND repo_url IS NULL AND repo_url_auto_disabled = 0
"#,
                params![service_id, repo_url, now],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("set service repo url if empty")
    }
}
