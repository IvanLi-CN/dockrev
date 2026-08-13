use super::*;

impl Db {
    pub async fn get_github_packages_settings(&self) -> anyhow::Result<GitHubPackagesSettingsDb> {
        self.call(|conn| {
            Ok(conn.query_row(
                r#"
SELECT
  enabled,
  callback_url,
  pat,
  webhook_secret,
  updated_at
FROM github_packages_settings
WHERE id = 'default'
"#,
                [],
                |row| {
                    Ok(GitHubPackagesSettingsDb {
                        enabled: row.get::<_, i64>(0)? != 0,
                        callback_url: row.get(1)?,
                        pat: row.get(2)?,
                        webhook_secret: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )?)
        })
        .await
        .context("get github packages settings")
    }

    pub async fn put_github_packages_settings(
        &self,
        settings: &GitHubPackagesSettingsDb,
        now: &str,
    ) -> anyhow::Result<()> {
        let settings = settings.clone();
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_settings
SET
  enabled = ?1,
  callback_url = ?2,
  pat = ?3,
  webhook_secret = ?4,
  updated_at = ?5
WHERE id = 'default'
"#,
                params![
                    settings.enabled as i64,
                    settings.callback_url,
                    settings.pat,
                    settings.webhook_secret,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("put github packages settings")
    }

    pub async fn list_github_packages_targets(
        &self,
    ) -> anyhow::Result<Vec<GitHubPackagesTargetDb>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
FROM github_packages_targets
ORDER BY owner ASC, input ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                let warnings_json: String = row.get(4)?;
                let warnings: Vec<String> =
                    serde_json::from_str(&warnings_json).unwrap_or_else(|_| Vec::new());
                Ok(GitHubPackagesTargetDb {
                    id: row.get(0)?,
                    input: row.get(1)?,
                    kind: row.get(2)?,
                    owner: row.get(3)?,
                    warnings,
                    updated_at: row.get(5)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages targets")
    }

    pub async fn put_github_packages_targets(
        &self,
        targets: &[GitHubPackagesTargetDb],
        now: &str,
    ) -> anyhow::Result<()> {
        let targets = targets.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute("DELETE FROM github_packages_targets", [])?;
            for t in targets {
                tx.execute(
                    r#"
INSERT INTO github_packages_targets (
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
                    params![
                        t.id,
                        t.input,
                        t.kind,
                        t.owner,
                        serde_json::to_string(&t.warnings).unwrap_or_else(|_| "[]".to_string()),
                        now
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .context("put github packages targets")
    }

    pub async fn upsert_github_packages_target_by_input(
        &self,
        input: &str,
        kind: &str,
        owner: &str,
        warnings: &[String],
        now: &str,
    ) -> anyhow::Result<()> {
        let id = ulid::Ulid::new().to_string();
        let input = input.to_string();
        let kind = kind.to_string();
        let owner = owner.to_string();
        let warnings_json = serde_json::to_string(warnings).unwrap_or_else(|_| "[]".to_string());
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "DELETE FROM github_packages_targets WHERE input = ?1",
                params![input],
            )?;
            tx.execute(
                r#"
INSERT INTO github_packages_targets (
  id,
  input,
  kind,
  owner,
  warnings_json,
  updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
                params![id, input, kind, owner, warnings_json, now],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert github packages target by input")
    }

    pub async fn delete_github_packages_target_by_input(&self, input: &str) -> anyhow::Result<u32> {
        let input = input.to_string();
        self.call(move |conn| {
            let n = conn.execute(
                "DELETE FROM github_packages_targets WHERE input = ?1",
                params![input],
            )?;
            Ok(n as u32)
        })
        .await
        .context("delete github packages target by input")
    }

    pub async fn list_github_packages_repos(&self) -> anyhow::Result<Vec<GitHubPackagesRepoDb>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
ORDER BY owner ASC, repo ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(GitHubPackagesRepoDb {
                    owner: row.get(0)?,
                    repo: row.get(1)?,
                    selected: row.get::<_, i64>(2)? != 0,
                    webhook_state: row.get(3)?,
                    webhook_job_id: row.get(4)?,
                    hook_id: row.get(5)?,
                    last_sync_at: row.get(6)?,
                    last_audit_at: row.get(7)?,
                    last_op: row.get(8)?,
                    last_error: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos")
    }

    pub async fn upsert_github_packages_repos_default_selected(
        &self,
        repos: &[(String, String)],
        now: &str,
    ) -> anyhow::Result<u32> {
        let repos: Vec<(String, String)> = repos.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut inserted: u32 = 0;
            for (owner, repo) in repos {
                // The DB treats repo keys case-insensitively in several read paths (via `lower()`),
                // but the primary key is case-sensitive. Avoid creating case-variant duplicates by
                // skipping inserts when a case-insensitive match already exists.
                let exists: Option<i64> = tx
                    .query_row(
                        r#"
SELECT 1
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
                        params![&owner, &repo],
                        |row| row.get(0),
                    )
                    .optional()?;
                if exists.is_some() {
                    continue;
                }

                let n = tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, 1, ?3)
ON CONFLICT(owner, repo) DO NOTHING
"#,
                    params![owner, repo, now],
                )?;
                inserted += n as u32;
            }
            tx.commit()?;
            Ok(inserted)
        })
        .await
        .context("upsert github packages repos default selected")
    }

    pub async fn count_github_packages_repos_total(&self) -> anyhow::Result<u32> {
        self.call(|conn| {
            Ok(
                conn.query_row("SELECT COUNT(*) FROM github_packages_repos", [], |row| {
                    row.get::<_, i64>(0).map(|v| v as u32)
                })?,
            )
        })
        .await
        .context("count github packages repos total")
    }

    pub async fn count_github_packages_repos_selected_total(&self) -> anyhow::Result<u32> {
        self.call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM github_packages_repos WHERE selected = 1",
                [],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages repos selected total")
    }

    pub async fn count_github_packages_repos_filtered(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
    ) -> anyhow::Result<u32> {
        let q = q.map(|s| s.to_string());
        self.call(move |conn| {
            let mut sql = "SELECT COUNT(*) FROM github_packages_repos".to_string();
            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            Ok(conn.query_row(&sql, params.as_slice(), |row| {
                row.get::<_, i64>(0).map(|v| v as u32)
            })?)
        })
        .await
        .context("count github packages repos filtered")
    }

    pub async fn list_github_packages_repos_page(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<GitHubPackagesRepoDb>> {
        let q = q.map(|s| s.to_string());
        self.call(move |conn| {
            let mut sql = r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
"#
            .to_string();

            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            sql.push_str(" ORDER BY owner ASC, repo ASC LIMIT ? OFFSET ?");
            values.push(rusqlite::types::Value::from(limit as i64));
            values.push(rusqlite::types::Value::from(offset as i64));

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok(GitHubPackagesRepoDb {
                    owner: row.get(0)?,
                    repo: row.get(1)?,
                    selected: row.get::<_, i64>(2)? != 0,
                    webhook_state: row.get(3)?,
                    webhook_job_id: row.get(4)?,
                    hook_id: row.get(5)?,
                    last_sync_at: row.get(6)?,
                    last_audit_at: row.get(7)?,
                    last_op: row.get(8)?,
                    last_error: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos page")
    }

    pub async fn upsert_github_packages_repo_selected(
        &self,
        owner: &str,
        repo: &str,
        selected: bool,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.trim().to_string();
        let repo = repo.trim().to_string();
        let now = now.to_string();
        self.call(move |conn| {
            // Reads treat owner/repo case-insensitively (via `lower()`), but the primary key is
            // case-sensitive. Prefer updating an existing row that matches case-insensitively to
            // avoid creating case-variant duplicates. If duplicates already exist, keep the "best"
            // row (favoring ones with sync state) and delete the rest.
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            let canonical: Option<(String, String)> = tx
                .query_row(
                    r#"
SELECT owner, repo
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
ORDER BY
  (hook_id IS NOT NULL) DESC,
  (last_sync_at IS NOT NULL) DESC,
  updated_at DESC
LIMIT 1
"#,
                    params![&owner, &repo],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if let Some((canon_owner, canon_repo)) = canonical {
                tx.execute(
                    r#"
UPDATE github_packages_repos
SET selected = ?3, updated_at = ?4
WHERE owner = ?1 AND repo = ?2
"#,
                    params![&canon_owner, &canon_repo, selected as i64, &now],
                )?;

                // Remove case-variant duplicates (keep the canonical row above).
                tx.execute(
                    r#"
DELETE FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
  AND NOT (owner = ?3 AND repo = ?4)
"#,
                    params![&owner, &repo, &canon_owner, &canon_repo],
                )?;
            } else {
                tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, ?3, ?4)
"#,
                    params![&owner, &repo, selected as i64, &now],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("upsert github packages repo selected")
    }

    pub async fn get_github_packages_repo_selected(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Option<bool>> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT selected
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
            )?;
            let mut rows = stmt.query(params![owner, repo])?;
            if let Some(row) = rows.next()? {
                let selected = row.get::<_, i64>(0)? != 0;
                Ok(Some(selected))
            } else {
                Ok(None)
            }
        })
        .await
        .context("get github packages repo selected")
    }

    pub async fn get_github_packages_repo(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Option<GitHubPackagesRepoDb>> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  owner,
  repo,
  selected,
  webhook_state,
  webhook_job_id,
  hook_id,
  last_sync_at,
  last_audit_at,
  last_op,
  last_error,
  updated_at
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
LIMIT 1
"#,
            )?;
            let row = stmt
                .query_row(params![owner, repo], |row| {
                    Ok(GitHubPackagesRepoDb {
                        owner: row.get(0)?,
                        repo: row.get(1)?,
                        selected: row.get::<_, i64>(2)? != 0,
                        webhook_state: row.get(3)?,
                        webhook_job_id: row.get(4)?,
                        hook_id: row.get(5)?,
                        last_sync_at: row.get(6)?,
                        last_audit_at: row.get(7)?,
                        last_op: row.get(8)?,
                        last_error: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                })
                .optional()?;
            Ok(row)
        })
        .await
        .context("get github packages repo")
    }

    pub async fn list_github_packages_repos_selected_by_owner(
        &self,
        owner: &str,
    ) -> anyhow::Result<Vec<(String, bool)>> {
        let owner = owner.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT repo, selected
FROM github_packages_repos
WHERE lower(owner) = lower(?1)
ORDER BY repo ASC
"#,
            )?;
            let rows = stmt.query_map(params![owner], |row| {
                let repo: String = row.get(0)?;
                let selected = row.get::<_, i64>(1)? != 0;
                Ok((repo, selected))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos selected by owner")
    }

    pub async fn delete_github_packages_repo(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<bool> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let query_owner = owner.clone();
        let query_repo = repo.clone();
        let deleted = self
            .call(move |conn| {
                let n = conn.execute(
                    r#"
DELETE FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                    params![query_owner, query_repo],
                )?;
                Ok(n > 0)
            })
            .await
            .context("delete github packages repo")?;
        if deleted {
            self.management_events
                .publish_change(
                    "github_packages",
                    "repo",
                    format!("{owner}/{repo}"),
                    serde_json::json!({ "operation": "repo_removed" }),
                )
                .await;
        }
        Ok(deleted)
    }

    pub async fn bulk_set_github_packages_repos_selected(
        &self,
        q: Option<&str>,
        selected_filter: Option<bool>,
        selected: bool,
        now: &str,
    ) -> anyhow::Result<u32> {
        let q = q.map(|s| s.to_string());
        let now = now.to_string();
        self.call(move |conn| {
            let mut sql =
                "UPDATE github_packages_repos SET selected = ?, updated_at = ?".to_string();
            let mut clauses: Vec<String> = Vec::new();
            let mut values: Vec<rusqlite::types::Value> = Vec::new();

            values.push(rusqlite::types::Value::from(selected as i64));
            values.push(rusqlite::types::Value::from(now));

            if let Some(sel) = selected_filter {
                clauses.push("selected = ?".to_string());
                values.push(rusqlite::types::Value::from(sel as i64));
            }
            if let Some(q) = &q
                && !q.trim().is_empty()
            {
                clauses.push("lower(owner || '/' || repo) LIKE '%' || lower(?) || '%'".to_string());
                values.push(rusqlite::types::Value::from(q.trim().to_string()));
            }

            if !clauses.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&clauses.join(" AND "));
            }

            let params: Vec<&dyn rusqlite::ToSql> =
                values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            let n = conn.execute(&sql, params.as_slice())?;
            Ok(n as u32)
        })
        .await
        .context("bulk set github packages repos selected")
    }

    pub async fn put_github_packages_repos(
        &self,
        repos: &[(String, String, bool)],
        now: &str,
    ) -> anyhow::Result<()> {
        let repos: Vec<(String, String, bool)> = repos.to_vec();
        let now = now.to_string();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

            // See `upsert_github_packages_repo_selected`: avoid creating case-variant duplicates by
            // reusing the canonical casing of any existing row that matches case-insensitively.
            let mut canonical: Vec<(String, String, bool)> = Vec::with_capacity(repos.len());
            for (owner, repo, selected) in &repos {
                let owner = owner.trim();
                let repo = repo.trim();
                if owner.is_empty() || repo.is_empty() {
                    continue;
                }

                let existing: Option<(String, String)> = tx
                    .query_row(
                        r#"
SELECT owner, repo
FROM github_packages_repos
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
ORDER BY
  (hook_id IS NOT NULL) DESC,
  (last_sync_at IS NOT NULL) DESC,
  updated_at DESC
LIMIT 1
"#,
                        params![owner, repo],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                let (owner, repo) = existing.unwrap_or((owner.to_string(), repo.to_string()));
                canonical.push((owner, repo, *selected));
            }

            for (owner, repo, selected) in &canonical {
                tx.execute(
                    r#"
INSERT INTO github_packages_repos (owner, repo, selected, updated_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(owner, repo) DO UPDATE SET
  selected = excluded.selected,
  updated_at = excluded.updated_at
"#,
                    params![owner, repo, *selected as i64, now],
                )?;
            }

            if canonical.is_empty() {
                tx.execute("DELETE FROM github_packages_repos", [])?;
            } else {
                // Avoid hitting SQLite's SQL-variable limit (commonly 999) by using a temp table
                // instead of `NOT IN (?, ?, ...)` with one placeholder per repo.
                tx.execute(
                    "CREATE TEMP TABLE IF NOT EXISTS tmp_github_packages_keep (full_name TEXT PRIMARY KEY)",
                    [],
                )?;
                tx.execute("DELETE FROM tmp_github_packages_keep", [])?;
                for (owner, repo, _) in &canonical {
                    let full_name = format!("{owner}/{repo}");
                    tx.execute(
                        "INSERT OR IGNORE INTO tmp_github_packages_keep (full_name) VALUES (?1)",
                        params![full_name],
                    )?;
                }
                tx.execute(
                    "DELETE FROM github_packages_repos WHERE (owner || '/' || repo) NOT IN (SELECT full_name FROM tmp_github_packages_keep)",
                    [],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .context("put github packages repos")
    }

    #[cfg(test)]
    pub async fn set_github_packages_repo_sync_result(
        &self,
        owner: &str,
        repo: &str,
        hook_id: Option<i64>,
        last_sync_at: Option<&str>,
        last_error: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let last_sync_at = last_sync_at.map(|s| s.to_string());
        let last_error = last_error.map(|s| s.to_string());
        let webhook_state = if last_error.is_some() {
            "error".to_string()
        } else if hook_id.is_some() {
            "ok".to_string()
        } else {
            "unknown".to_string()
        };
        let now = now.to_string();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  last_op = 'register',
  webhook_job_id = NULL,
  hook_id = ?4,
  last_sync_at = ?5,
  last_error = ?6,
  updated_at = ?7
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![
                    owner,
                    repo,
                    webhook_state,
                    hook_id,
                    last_sync_at,
                    last_error,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo sync result")
    }

    pub async fn set_github_packages_repo_webhook_job_state(
        &self,
        owner: &str,
        repo: &str,
        webhook_state: &str,
        webhook_job_id: Option<&str>,
        last_op: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let webhook_state = webhook_state.to_string();
        let webhook_job_id = webhook_job_id.map(|s| s.to_string());
        let last_op = last_op.map(|s| s.to_string());
        let now = now.to_string();
        let event_owner = owner.clone();
        let event_repo = repo.clone();
        let event_webhook_state = webhook_state.clone();
        let event_job_id = webhook_job_id.clone();
        let event_last_op = last_op.clone();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  webhook_job_id = ?4,
  last_op = ?5,
  updated_at = ?6
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![owner, repo, webhook_state, webhook_job_id, last_op, now],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo webhook job state")?;
        self.management_events
            .publish_change(
                "github_packages",
                "repo",
                format!("{event_owner}/{event_repo}"),
                serde_json::json!({
                    "operation": "repo_webhook_job_state_updated",
                    "webhookState": event_webhook_state,
                    "jobId": event_job_id,
                    "lastOp": event_last_op,
                }),
            )
            .await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn set_github_packages_repo_webhook_result(
        &self,
        owner: &str,
        repo: &str,
        webhook_state: &str,
        hook_id: Option<i64>,
        last_sync_at: Option<&str>,
        last_audit_at: Option<&str>,
        last_error: Option<&str>,
        webhook_job_id: Option<&str>,
        last_op: Option<&str>,
        now: &str,
    ) -> anyhow::Result<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();
        let webhook_state = webhook_state.to_string();
        let last_sync_at = last_sync_at.map(|s| s.to_string());
        let last_audit_at = last_audit_at.map(|s| s.to_string());
        let last_error = last_error.map(|s| s.to_string());
        let webhook_job_id = webhook_job_id.map(|s| s.to_string());
        let last_op = last_op.map(|s| s.to_string());
        let now = now.to_string();
        let event_owner = owner.clone();
        let event_repo = repo.clone();
        let event_webhook_state = webhook_state.clone();
        let event_job_id = webhook_job_id.clone();
        let event_last_op = last_op.clone();
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_repos
SET
  webhook_state = ?3,
  hook_id = ?4,
  last_sync_at = ?5,
  last_audit_at = ?6,
  last_error = ?7,
  webhook_job_id = ?8,
  last_op = ?9,
  updated_at = ?10
WHERE lower(owner) = lower(?1) AND lower(repo) = lower(?2)
"#,
                params![
                    owner,
                    repo,
                    webhook_state,
                    hook_id,
                    last_sync_at,
                    last_audit_at,
                    last_error,
                    webhook_job_id,
                    last_op,
                    now
                ],
            )?;
            Ok(())
        })
        .await
        .context("set github packages repo webhook result")?;
        self.management_events
            .publish_change(
                "github_packages",
                "repo",
                format!("{event_owner}/{event_repo}"),
                serde_json::json!({
                    "operation": "repo_webhook_state_updated",
                    "webhookState": event_webhook_state,
                    "jobId": event_job_id,
                    "lastOp": event_last_op,
                }),
            )
            .await;
        Ok(())
    }

    pub async fn list_github_packages_repos_for_job_state_summary(
        &self,
    ) -> anyhow::Result<Vec<(String, Option<String>)>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT webhook_state, last_audit_at
FROM github_packages_repos
WHERE selected = 1
"#,
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages repos state summary")
    }

    pub async fn count_github_packages_deliveries_total(&self) -> anyhow::Result<u32> {
        self.call(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM github_packages_deliveries",
                [],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages deliveries total")
    }

    pub async fn summarize_github_packages_deliveries(
        &self,
    ) -> anyhow::Result<GitHubPackagesWebhookDeliverySummary> {
        self.call(move |conn| {
            Ok(conn.query_row(
                r#"
SELECT
  SUM(CASE WHEN decision = 'processed' THEN 1 ELSE 0 END),
  SUM(CASE WHEN decision = 'ignored' THEN 1 ELSE 0 END),
  SUM(CASE WHEN decision = 'rejected' THEN 1 ELSE 0 END)
FROM github_packages_deliveries
"#,
                [],
                |row| {
                    Ok(GitHubPackagesWebhookDeliverySummary {
                        processed: row.get::<_, Option<i64>>(0)?.unwrap_or(0) as u32,
                        ignored: row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u32,
                        rejected: row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u32,
                    })
                },
            )?)
        })
        .await
        .context("summarize github packages deliveries")
    }

    pub async fn count_github_packages_deliveries_filtered(
        &self,
        decision: Option<&str>,
        q: Option<&str>,
    ) -> anyhow::Result<u32> {
        let decision = decision.map(|s| s.to_string());
        let q_like = q.map(|s| format!("%{}%", s.trim().to_ascii_lowercase()));
        self.call(move |conn| {
            Ok(conn.query_row(
                r#"
SELECT COUNT(*)
FROM github_packages_deliveries
WHERE (?1 IS NULL OR decision = ?1)
  AND (
    ?2 IS NULL
    OR lower(delivery_id) LIKE ?2
    OR lower(COALESCE(owner, '')) LIKE ?2
    OR lower(COALESCE(repo, '')) LIKE ?2
    OR lower(COALESCE(event, '')) LIKE ?2
    OR lower(COALESCE(action, '')) LIKE ?2
    OR lower(COALESCE(reason, '')) LIKE ?2
    OR lower(COALESCE(job_id, '')) LIKE ?2
    OR lower(COALESCE(job_ids_json, '')) LIKE ?2
  )
"#,
                params![decision, q_like],
                |row| row.get::<_, i64>(0).map(|v| v as u32),
            )?)
        })
        .await
        .context("count github packages deliveries filtered")
    }

    pub async fn list_github_packages_deliveries_page(
        &self,
        decision: Option<&str>,
        q: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<GitHubPackagesWebhookDeliveryDb>> {
        let decision = decision.map(|s| s.to_string());
        let q_like = q.map(|s| format!("%{}%", s.trim().to_ascii_lowercase()));
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  reason,
  response_status,
  job_id,
  job_ids_json,
  attempt_count
FROM github_packages_deliveries
WHERE (?1 IS NULL OR decision = ?1)
  AND (
    ?2 IS NULL
    OR lower(delivery_id) LIKE ?2
    OR lower(COALESCE(owner, '')) LIKE ?2
    OR lower(COALESCE(repo, '')) LIKE ?2
    OR lower(COALESCE(event, '')) LIKE ?2
    OR lower(COALESCE(action, '')) LIKE ?2
    OR lower(COALESCE(reason, '')) LIKE ?2
    OR lower(COALESCE(job_id, '')) LIKE ?2
    OR lower(COALESCE(job_ids_json, '')) LIKE ?2
  )
ORDER BY received_at DESC, delivery_id DESC
LIMIT ?3 OFFSET ?4
"#,
            )?;
            let rows = stmt.query_map(params![decision, q_like, limit, offset], |row| {
                let job_id: Option<String> = row.get(10)?;
                let job_ids_json: Option<String> = row.get(11)?;
                Ok(GitHubPackagesWebhookDeliveryDb {
                    delivery_id: row.get(0)?,
                    received_at: row.get(1)?,
                    first_received_at: row.get(2)?,
                    owner: row.get(3)?,
                    repo: row.get(4)?,
                    event: row.get(5)?,
                    action: row.get(6)?,
                    decision: row.get(7)?,
                    reason: row.get(8)?,
                    response_status: row
                        .get::<_, Option<i64>>(9)?
                        .and_then(|value| u16::try_from(value).ok()),
                    job_ids: parse_github_packages_delivery_job_ids(
                        job_id.as_deref(),
                        job_ids_json.as_deref(),
                    ),
                    job_id,
                    attempt_count: row.get::<_, i64>(12)?.max(1) as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages deliveries page")
    }

    pub async fn get_github_packages_delivery(
        &self,
        delivery_id: &str,
    ) -> anyhow::Result<Option<GitHubPackagesWebhookDeliveryDb>> {
        let delivery_id = delivery_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  reason,
  response_status,
  job_id,
  job_ids_json,
  attempt_count
FROM github_packages_deliveries
WHERE delivery_id = ?1
"#,
                    params![delivery_id],
                    |row| {
                        let job_id: Option<String> = row.get(10)?;
                        let job_ids_json: Option<String> = row.get(11)?;
                        Ok(GitHubPackagesWebhookDeliveryDb {
                            delivery_id: row.get(0)?,
                            received_at: row.get(1)?,
                            first_received_at: row.get(2)?,
                            owner: row.get(3)?,
                            repo: row.get(4)?,
                            event: row.get(5)?,
                            action: row.get(6)?,
                            decision: row.get(7)?,
                            reason: row.get(8)?,
                            response_status: row
                                .get::<_, Option<i64>>(9)?
                                .and_then(|value| u16::try_from(value).ok()),
                            job_ids: parse_github_packages_delivery_job_ids(
                                job_id.as_deref(),
                                job_ids_json.as_deref(),
                            ),
                            job_id,
                            attempt_count: row.get::<_, i64>(12)?.max(1) as u32,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get github packages delivery")
    }

    pub async fn insert_github_packages_delivery_event(
        &self,
        delivery_id: &str,
        received_at: &str,
        payload_json: &str,
    ) -> anyhow::Result<i64> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let payload_json = payload_json.to_string();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO github_packages_delivery_events (delivery_id, received_at, payload_json) VALUES (?1, ?2, ?3)",
                params![delivery_id, received_at, payload_json],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .context("insert github packages delivery event")
    }

    pub async fn list_github_packages_delivery_events_since(
        &self,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<GitHubPackagesDeliveryEventRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, payload_json
FROM github_packages_delivery_events
WHERE id > ?1
ORDER BY id ASC
LIMIT ?2
"#,
            )?;

            let rows = stmt.query_map(params![after_id, limit as i64], |row| {
                Ok(GitHubPackagesDeliveryEventRow {
                    id: row.get(0)?,
                    payload_json: row.get(1)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list github packages delivery events since")
    }

    pub async fn get_github_packages_delivery_events_last_id(&self) -> anyhow::Result<i64> {
        self.call(move |conn| {
            let v: i64 = conn.query_row(
                "SELECT COALESCE(MAX(id), 0) FROM github_packages_delivery_events",
                [],
                |row| row.get(0),
            )?;
            Ok(v)
        })
        .await
        .context("get github packages delivery events last id")
    }

    pub async fn record_github_packages_delivery(
        &self,
        input: GitHubPackagesWebhookDeliveryRecordInput,
    ) -> anyhow::Result<u32> {
        self.call(move |conn| {
            let delivery_id = input.delivery_id.clone();
            let job_ids_json = serde_json::to_string(&input.job_ids)?;
            conn.execute(
                r#"
INSERT INTO github_packages_deliveries (
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  reason,
  response_status,
  job_id,
  job_ids_json,
  attempt_count
)
VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1)
ON CONFLICT(delivery_id) DO UPDATE SET
  received_at = excluded.received_at,
  owner = COALESCE(excluded.owner, github_packages_deliveries.owner),
  repo = COALESCE(excluded.repo, github_packages_deliveries.repo),
  event = COALESCE(excluded.event, github_packages_deliveries.event),
  action = COALESCE(excluded.action, github_packages_deliveries.action),
  decision = excluded.decision,
  reason = excluded.reason,
  response_status = excluded.response_status,
  job_id = COALESCE(excluded.job_id, github_packages_deliveries.job_id),
  job_ids_json = COALESCE(excluded.job_ids_json, github_packages_deliveries.job_ids_json),
  attempt_count = github_packages_deliveries.attempt_count + 1
"#,
                params![
                    delivery_id,
                    input.received_at,
                    input.owner,
                    input.repo,
                    input.event,
                    input.action,
                    input.decision,
                    input.reason,
                    input.response_status.map(i64::from),
                    input.job_id,
                    job_ids_json,
                ],
            )?;
            conn.query_row(
                "SELECT attempt_count FROM github_packages_deliveries WHERE delivery_id = ?1",
                params![input.delivery_id],
                |row| row.get::<_, i64>(0).map(|value| value.max(1) as u32),
            )
            .context("load github packages delivery attempt count")
        })
        .await
        .context("record github packages delivery")
    }

    pub async fn increment_github_packages_delivery_attempt(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
        event: Option<&str>,
        action: Option<&str>,
    ) -> anyhow::Result<u32> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        let event = event.map(|s| s.to_string());
        let action = action.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
UPDATE github_packages_deliveries
SET
  received_at = ?2,
  owner = COALESCE(?3, owner),
  repo = COALESCE(?4, repo),
  event = COALESCE(?5, event),
  action = COALESCE(?6, action),
  attempt_count = attempt_count + 1
WHERE delivery_id = ?1
"#,
                params![&delivery_id, &received_at, owner, repo, event, action],
            )?;
            if changed == 0 {
                return Err(anyhow::anyhow!(
                    "github packages delivery not found for duplicate attempt"
                ));
            }
            conn.query_row(
                "SELECT attempt_count FROM github_packages_deliveries WHERE delivery_id = ?1",
                params![delivery_id],
                |row| row.get::<_, i64>(0).map(|value| value.max(1) as u32),
            )
            .context("load github packages delivery attempt count")
        })
        .await
        .context("increment github packages delivery attempt")
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_github_packages_delivery_outcome(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
        event: Option<&str>,
        action: Option<&str>,
        decision: &str,
        reason: Option<&str>,
        response_status: Option<u16>,
        job_id: Option<&str>,
        job_ids: &[String],
    ) -> anyhow::Result<()> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        let event = event.map(|s| s.to_string());
        let action = action.map(|s| s.to_string());
        let decision = decision.to_string();
        let reason = reason.map(|s| s.to_string());
        let response_status = response_status.map(i64::from);
        let job_id = job_id.map(|s| s.to_string());
        let job_ids = if job_ids.is_empty() {
            None
        } else {
            Some(serde_json::to_string(job_ids)?)
        };
        self.call(move |conn| {
            conn.execute(
                r#"
UPDATE github_packages_deliveries
SET
  received_at = ?2,
  owner = COALESCE(?3, owner),
  repo = COALESCE(?4, repo),
  event = COALESCE(?5, event),
  action = COALESCE(?6, action),
  decision = ?7,
  reason = ?8,
  response_status = ?9,
  job_id = COALESCE(?10, job_id),
  job_ids_json = COALESCE(?11, job_ids_json)
WHERE delivery_id = ?1
"#,
                params![
                    delivery_id,
                    received_at,
                    owner,
                    repo,
                    event,
                    action,
                    decision,
                    reason,
                    response_status,
                    job_id,
                    job_ids,
                ],
            )?;
            Ok(())
        })
        .await
        .context("update github packages delivery outcome")
    }

    pub async fn insert_github_packages_delivery_if_new(
        &self,
        delivery_id: &str,
        received_at: &str,
        owner: Option<&str>,
        repo: Option<&str>,
    ) -> anyhow::Result<bool> {
        let delivery_id = delivery_id.to_string();
        let received_at = received_at.to_string();
        let owner = owner.map(|s| s.to_string());
        let repo = repo.map(|s| s.to_string());
        self.call(move |conn| {
            let changed = conn.execute(
                r#"
INSERT OR IGNORE INTO github_packages_deliveries (
  delivery_id,
  received_at,
  first_received_at,
  owner,
  repo,
  event,
  action,
  decision,
  response_status,
  attempt_count
)
VALUES (?1, ?2, ?2, ?3, ?4, 'package', 'published', 'processed', 200, 1)
"#,
                params![delivery_id, received_at, owner, repo],
            )?;
            Ok(changed > 0)
        })
        .await
        .context("insert github packages delivery")
    }

    pub async fn delete_github_packages_delivery(&self, delivery_id: &str) -> anyhow::Result<()> {
        let delivery_id = delivery_id.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM github_packages_deliveries WHERE delivery_id = ?1",
                params![delivery_id],
            )?;
            Ok(())
        })
        .await
        .context("delete github packages delivery")
    }

    pub async fn github_packages_delivery_exists(&self, delivery_id: &str) -> anyhow::Result<bool> {
        let delivery_id = delivery_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT 1 FROM github_packages_deliveries WHERE delivery_id = ?1 LIMIT 1",
            )?;
            let mut rows = stmt.query(params![delivery_id])?;
            Ok(rows.next()?.is_some())
        })
        .await
        .context("check github packages delivery exists")
    }

    pub async fn list_active_github_webhook_service_targets(
        &self,
    ) -> anyhow::Result<Vec<GithubWebhookServiceTarget>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  st.id,
  sv.id,
  sv.image_ref
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(GithubWebhookServiceTarget {
                    stack_id: row.get(0)?,
                    service_id: row.get(1)?,
                    image_ref: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list active github webhook service targets")
    }
}
