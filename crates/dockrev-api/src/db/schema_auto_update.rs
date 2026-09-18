fn ensure_auto_update_schema(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS auto_update_policies (
  scope_type TEXT NOT NULL,
  scope_id TEXT NOT NULL,
  mode TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  rules_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (scope_type, scope_id)
);
CREATE INDEX IF NOT EXISTS idx_auto_update_policies_scope
  ON auto_update_policies(scope_type, scope_id);

CREATE TABLE IF NOT EXISTS auto_update_pending (
  id TEXT PRIMARY KEY NOT NULL,
  policy_scope_type TEXT NOT NULL,
  policy_scope_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  source_check_job_id TEXT NOT NULL,
  candidate_tag TEXT NOT NULL,
  candidate_display_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  current_display_tag TEXT NOT NULL,
  current_digest TEXT,
  first_seen_at TEXT NOT NULL,
  due_at TEXT NOT NULL,
  min_age_seconds INTEGER NOT NULL,
  min_version_lag INTEGER NOT NULL,
  status TEXT NOT NULL,
  update_job_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  summary_json TEXT NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_auto_update_pending_active_candidate
  ON auto_update_pending(service_id, rule_id, candidate_digest)
  WHERE status IN ('pending', 'enqueuing', 'enqueued');
CREATE INDEX IF NOT EXISTS idx_auto_update_pending_due
  ON auto_update_pending(status, due_at);

CREATE TABLE IF NOT EXISTS auto_update_candidates (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  raw_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  resolved_version TEXT,
  resolved_tags TEXT,
  status TEXT NOT NULL,
  reason TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  retry_at TEXT,
  discovered_at TEXT NOT NULL,
  source_job_id TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'unknown',
  current_tag TEXT NOT NULL DEFAULT '',
  current_display_tag TEXT NOT NULL DEFAULT '',
  current_digest TEXT,
  settled_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  policy_status TEXT,
  policy_reason TEXT,
  policy_rule_id TEXT,
  policy_evaluated_at TEXT,
  policy_scope_type TEXT,
  policy_scope_id TEXT,
  update_job_id TEXT,
  last_error TEXT,
  superseded_at TEXT,
  superseded_by_candidate_id TEXT,
  hydration_origin TEXT,
  UNIQUE(service_id, candidate_digest)
);
CREATE INDEX IF NOT EXISTS idx_auto_update_candidates_status_retry
  ON auto_update_candidates(status, retry_at);
CREATE INDEX IF NOT EXISTS idx_auto_update_candidates_service_discovered
  ON auto_update_candidates(service_id, discovered_at DESC);
"#,
    )?;
    Ok(())
}

fn apply_migration_0017_add_auto_update_candidate_projection_context(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0017_add_auto_update_candidate_projection_context";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing_columns = {
        let mut stmt = tx.prepare("PRAGMA table_info(auto_update_candidates)")?;
        stmt.query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<BTreeSet<_>, _>>()?
    };
    for (name, ddl) in [
        (
            "policy_scope_type",
            "ALTER TABLE auto_update_candidates ADD COLUMN policy_scope_type TEXT",
        ),
        (
            "policy_scope_id",
            "ALTER TABLE auto_update_candidates ADD COLUMN policy_scope_id TEXT",
        ),
        (
            "update_job_id",
            "ALTER TABLE auto_update_candidates ADD COLUMN update_job_id TEXT",
        ),
    ] {
        if !existing_columns.contains(name) {
            tx.execute(ddl, [])?;
        }
    }
    let candidate_digest = super::canonical_digest_sql("auto_update_candidates.candidate_digest");
    let pending_digest = super::canonical_digest_sql("p.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_candidates
SET policy_scope_type = (
      SELECT p.policy_scope_type
      FROM auto_update_pending p
      WHERE p.service_id = auto_update_candidates.service_id
        AND {pending_digest} = {candidate_digest}
      ORDER BY p.updated_at DESC, p.id DESC
      LIMIT 1
    ),
    policy_scope_id = (
      SELECT p.policy_scope_id
      FROM auto_update_pending p
      WHERE p.service_id = auto_update_candidates.service_id
        AND {pending_digest} = {candidate_digest}
      ORDER BY p.updated_at DESC, p.id DESC
      LIMIT 1
    ),
    update_job_id = (
      SELECT p.update_job_id
      FROM auto_update_pending p
      WHERE p.service_id = auto_update_candidates.service_id
        AND {pending_digest} = {candidate_digest}
      ORDER BY p.updated_at DESC, p.id DESC
      LIMIT 1
    )
WHERE EXISTS (
  SELECT 1
  FROM auto_update_pending p
  WHERE p.service_id = auto_update_candidates.service_id
    AND {pending_digest} = {candidate_digest}
);
"#
    );
    tx.execute(&sql, [])?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0018_add_auto_update_candidate_audit_fields(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0018_add_auto_update_candidate_audit_fields";
    if migration_applied(conn, id)? {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing_columns = {
        let mut stmt = tx.prepare("PRAGMA table_info(auto_update_candidates)")?;
        stmt.query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<BTreeSet<_>, _>>()?
    };
    for (name, ddl) in [
        (
            "last_error",
            "ALTER TABLE auto_update_candidates ADD COLUMN last_error TEXT",
        ),
        (
            "superseded_at",
            "ALTER TABLE auto_update_candidates ADD COLUMN superseded_at TEXT",
        ),
        (
            "superseded_by_candidate_id",
            "ALTER TABLE auto_update_candidates ADD COLUMN superseded_by_candidate_id TEXT",
        ),
    ] {
        if !existing_columns.contains(name) {
            tx.execute(ddl, [])?;
        }
    }
    tx.execute(
        "UPDATE auto_update_candidates SET superseded_at = COALESCE(superseded_at, settled_at) WHERE status = 'superseded'",
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0019_add_auto_update_candidate_resolved_tags(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0019_add_auto_update_candidate_resolved_tags";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let has_column = tx
        .prepare("PRAGMA table_info(auto_update_candidates)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?
        .contains("resolved_tags");
    if !has_column {
        tx.execute(
            "ALTER TABLE auto_update_candidates ADD COLUMN resolved_tags TEXT",
            [],
        )?;
    }
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0020_hydrate_auto_update_candidates_from_discoveries(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0020_hydrate_auto_update_candidates_from_discoveries";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let has_column = tx
        .prepare("PRAGMA table_info(auto_update_candidates)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?
        .contains("hydration_origin");
    if !has_column {
        tx.execute(
            "ALTER TABLE auto_update_candidates ADD COLUMN hydration_origin TEXT",
            [],
        )?;
    }
    super::auto_update_hydration::hydrate_auto_update_candidates_tx(
        &tx,
        &now_rfc3339()?,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn deduplicate_auto_update_pending_digests_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &str,
) -> anyhow::Result<()> {
    let pending_digest = super::canonical_digest_sql("p.candidate_digest");
    let comparison_digest = super::canonical_digest_sql("p2.candidate_digest");
    let sql = format!(
        r#"
CREATE TEMP TABLE migration_duplicate_auto_update_pending AS
SELECT
  p.id,
  p.update_job_id,
  (
    SELECT p2.id
    FROM auto_update_pending p2
    WHERE p2.service_id = p.service_id
      AND p2.rule_id = p.rule_id
      AND {comparison_digest} = {pending_digest}
      AND p2.status IN ('pending', 'enqueuing', 'enqueued')
    ORDER BY
      CASE p2.status WHEN 'enqueued' THEN 3 WHEN 'enqueuing' THEN 2 ELSE 1 END DESC,
      CASE WHEN p2.update_job_id IS NULL THEN 0 ELSE 1 END DESC,
      p2.created_at ASC,
      p2.id ASC
    LIMIT 1
  ) AS keeper_id
FROM auto_update_pending p
WHERE p.status IN ('pending', 'enqueuing', 'enqueued')
"#
    );
    tx.execute(&sql, [])?;
    tx.execute(
        r#"
UPDATE jobs
SET status = 'cancelled',
    finished_at = ?1
WHERE id IN (
  SELECT update_job_id
  FROM migration_duplicate_auto_update_pending
  WHERE id <> keeper_id
    AND update_job_id IS NOT NULL
)
  AND status = 'queued'
  AND created_by = 'auto-policy'
"#,
        rusqlite::params![now],
    )?;
    tx.execute(
        r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT p.update_job_id, ?1, 'migration-canonical-digest', ?1
FROM migration_duplicate_auto_update_pending p
JOIN jobs j ON j.id = p.update_job_id
WHERE p.id <> p.keeper_id
  AND p.update_job_id IS NOT NULL
  AND j.status = 'running'
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
        rusqlite::params![now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL,
    status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json)
        AND json_type(CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END) = 'object'
        THEN json_set(summary_json, '$.skipReason', 'migration_duplicate_candidate_digest', '$.skippedAt', ?1)
      ELSE json_object('skipReason', 'migration_duplicate_candidate_digest', 'skippedAt', ?1)
    END,
    updated_at = ?1
WHERE id IN (
  SELECT id
  FROM migration_duplicate_auto_update_pending
  WHERE id <> keeper_id
)
"#,
        rusqlite::params![now],
    )?;
    tx.execute_batch("DROP TABLE migration_duplicate_auto_update_pending")?;
    Ok(())
}

pub(super) fn apply_migration_0024_normalize_auto_update_digest_identity(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0024_normalize_auto_update_digest_identity";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    deduplicate_auto_update_pending_digests_tx(&tx, &now)?;
    for table in ["services", "auto_update_pending"] {
        let digest = super::canonical_digest_sql("candidate_digest");
        let sql = format!(
            "UPDATE {table} SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> ''"
        );
        tx.execute(&sql, [])?;
    }
    let candidate_digest = super::canonical_digest_sql("auto_update_candidates.candidate_digest");
    let other_digest = super::canonical_digest_sql("other.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_candidates
SET candidate_digest = {candidate_digest}
WHERE candidate_digest IS NOT NULL
  AND TRIM(candidate_digest) <> ''
  AND NOT EXISTS (
    SELECT 1
    FROM auto_update_candidates other
    WHERE other.id <> auto_update_candidates.id
      AND other.service_id = auto_update_candidates.service_id
      AND {other_digest} = {candidate_digest}
  )
"#
    );
    tx.execute(&sql, [])?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0021_add_auto_update_pending_current_digest(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0021_add_auto_update_pending_current_digest";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let has_column = tx
        .prepare("PRAGMA table_info(auto_update_pending)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?
        .contains("current_digest");
    if !has_column {
        tx.execute(
            "ALTER TABLE auto_update_pending ADD COLUMN current_digest TEXT",
            [],
        )?;
    }
    tx.execute(
        r#"
UPDATE auto_update_pending
SET current_digest = NULLIF(TRIM(json_extract(
  CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END,
  '$.currentDigest'
)), '')
WHERE current_digest IS NULL
  AND NULLIF(TRIM(json_extract(
    CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END,
    '$.currentDigest'
  )), '') IS NOT NULL
"#,
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0022_harden_auto_update_source_provenance(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0022_harden_auto_update_source_provenance";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    tx.execute_batch(
        "CREATE TEMP TABLE migration_invalid_auto_update_pending (id TEXT PRIMARY KEY NOT NULL)",
    )?;
    let candidate_digest = super::canonical_digest_sql("c.candidate_digest");
    let pending_digest = super::canonical_digest_sql("p.candidate_digest");
    let service_digest = super::canonical_digest_sql("candidate_service.candidate_digest");
    let sql = format!(
        r#"
INSERT INTO migration_invalid_auto_update_pending (id)
SELECT p.id
FROM auto_update_pending p
WHERE p.status IN ('pending', 'enqueuing', 'enqueued')
  AND NOT EXISTS (
    SELECT 1
    FROM auto_update_candidates c
    JOIN jobs candidate_source_job ON candidate_source_job.id = c.source_job_id
    JOIN services candidate_service ON candidate_service.id = c.service_id
    WHERE c.id = p.candidate_id
      AND c.service_id = p.service_id
      AND {candidate_digest} = {pending_digest}
      AND c.stack_id = p.stack_id
      AND c.source_job_id = p.source_check_job_id
      AND COALESCE(TRIM(c.image_ref), '') <> ''
      AND COALESCE(TRIM(c.discovered_at), '') <> ''
      AND LOWER(c.source) IN ('schedule', 'github_webhook')
      AND candidate_service.stack_id = c.stack_id
      AND {service_digest} = {candidate_digest}
      AND LOWER(candidate_source_job.type) = 'check'
      AND LOWER(candidate_source_job.status) = 'success'
      AND (
        (LOWER(c.source) = 'schedule'
          AND LOWER(candidate_source_job.reason) = 'schedule'
          AND LOWER(candidate_source_job.created_by) = 'schedule')
        OR (LOWER(c.source) = 'github_webhook'
          AND LOWER(candidate_source_job.created_by) IN ('webhook', 'github')
          AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(candidate_source_job.summary_json) THEN candidate_source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
      )
      AND (
        (LOWER(candidate_source_job.scope) = 'service'
          AND candidate_source_job.stack_id = c.stack_id
          AND candidate_source_job.service_id = c.service_id)
        OR (LOWER(candidate_source_job.scope) = 'stack'
          AND candidate_source_job.stack_id = c.stack_id
          AND candidate_source_job.service_id IS NULL)
        OR (LOWER(candidate_source_job.scope) = 'all'
          AND candidate_source_job.stack_id IS NULL
          AND candidate_source_job.service_id IS NULL)
      )
      AND EXISTS (
        SELECT 1
        FROM jobs source_job
        WHERE source_job.id = p.source_check_job_id
          AND LOWER(source_job.type) = 'check'
          AND LOWER(source_job.status) = 'success'
          AND (
            (LOWER(c.source) = 'schedule'
              AND LOWER(source_job.reason) = 'schedule'
              AND LOWER(source_job.created_by) = 'schedule')
            OR (LOWER(c.source) = 'github_webhook'
              AND LOWER(source_job.created_by) IN ('webhook', 'github')
              AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
          )
          AND (
            (LOWER(source_job.scope) = 'service'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id = p.service_id)
            OR (LOWER(source_job.scope) = 'stack'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id IS NULL)
            OR (LOWER(source_job.scope) = 'all'
              AND source_job.stack_id IS NULL
              AND source_job.service_id IS NULL)
          )
      )
  )
"#
    );
    tx.execute(&sql, [])?;
    tx.execute(
        r#"
UPDATE jobs
SET status = 'cancelled',
    finished_at = ?1
WHERE id IN (
  SELECT p.update_job_id
  FROM auto_update_pending p
  JOIN migration_invalid_auto_update_pending invalid ON invalid.id = p.id
  WHERE p.update_job_id IS NOT NULL
)
  AND status = 'queued'
  AND created_by = 'auto-policy'
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT p.update_job_id, ?1, 'migration-ambiguous-history', ?1
FROM auto_update_pending p
JOIN migration_invalid_auto_update_pending invalid ON invalid.id = p.id
JOIN jobs j ON j.id = p.update_job_id
WHERE p.update_job_id IS NOT NULL
  AND j.status = 'running'
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL,
    status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json)
        AND json_type(CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END) = 'object'
        THEN json_set(summary_json, '$.skipReason', 'migration_ambiguous_history', '$.skippedAt', ?1)
      ELSE json_object('skipReason', 'migration_ambiguous_history', 'skippedAt', ?1)
    END,
    updated_at = ?1
WHERE id IN (SELECT id FROM migration_invalid_auto_update_pending)
"#,
        params![&now],
    )?;
    tx.execute_batch("DROP TABLE migration_invalid_auto_update_pending")?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_migration_0023_add_auto_update_candidate_settlement_generation(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0023_add_auto_update_candidate_settlement_generation";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let columns = tx
        .prepare("PRAGMA table_info(auto_update_candidates)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !columns.contains("settlement_generation") {
        tx.execute(
            "ALTER TABLE auto_update_candidates ADD COLUMN settlement_generation INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0014_add_auto_update_candidates(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0014_add_auto_update_candidates";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS auto_update_candidates (
  id TEXT PRIMARY KEY NOT NULL,
  stack_id TEXT NOT NULL,
  service_id TEXT NOT NULL,
  image_ref TEXT NOT NULL,
  raw_tag TEXT NOT NULL,
  candidate_digest TEXT NOT NULL,
  resolved_version TEXT,
  status TEXT NOT NULL,
  reason TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  retry_at TEXT,
  discovered_at TEXT NOT NULL,
  source_job_id TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'unknown',
  current_tag TEXT NOT NULL DEFAULT '',
  current_display_tag TEXT NOT NULL DEFAULT '',
  current_digest TEXT,
  settled_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  policy_status TEXT,
  policy_reason TEXT,
  policy_rule_id TEXT,
  policy_evaluated_at TEXT,
  UNIQUE(service_id, candidate_digest)
);
CREATE INDEX IF NOT EXISTS idx_auto_update_candidates_status_retry
  ON auto_update_candidates(status, retry_at);
CREATE INDEX IF NOT EXISTS idx_auto_update_candidates_service_discovered
  ON auto_update_candidates(service_id, discovered_at DESC);
ALTER TABLE auto_update_pending ADD COLUMN candidate_id TEXT;
"#,
    )?;
    let now = now_rfc3339()?;
    deduplicate_auto_update_pending_digests_tx(&tx, &now)?;
    for table in ["services", "auto_update_pending", "auto_update_candidates"] {
        let digest = super::canonical_digest_sql("candidate_digest");
        let sql = format!(
            "UPDATE {table} SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> ''"
        );
        tx.execute(&sql, [])?;
    }
    let pending_digest = super::canonical_digest_sql("p.candidate_digest");
    let service_digest = super::canonical_digest_sql("s.candidate_digest");
    let sql = format!(
        r#"
INSERT OR IGNORE INTO auto_update_candidates (
  id, stack_id, service_id, image_ref, raw_tag, candidate_digest,
  resolved_version, status, reason, attempts, discovered_at, source_job_id, source,
  current_tag, current_display_tag, current_digest, created_at, updated_at
)
SELECT
  p.service_id || ':' || {pending_digest},
  p.stack_id,
  p.service_id,
  COALESCE(NULLIF(json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{{}}' END, '$.imageRef'), ''), ''),
  p.candidate_tag,
  {pending_digest},
  NULL,
  'awaiting_inference',
  'migration_pending_history',
  0,
  p.first_seen_at,
  p.source_check_job_id,
  CASE WHEN LOWER(j.reason) = 'schedule' THEN 'schedule' ELSE 'github_webhook' END,
  COALESCE(NULLIF(json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{{}}' END, '$.currentTag'), ''), p.current_display_tag),
  p.current_display_tag,
  json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{{}}' END, '$.currentDigest'),
  ?1,
  ?1
FROM auto_update_pending p
JOIN jobs j ON j.id = p.source_check_job_id
JOIN services s ON s.id = p.service_id AND {service_digest} = {pending_digest}
WHERE p.status IN ('pending', 'enqueuing', 'enqueued')
  AND COALESCE(TRIM(p.candidate_digest), '') <> ''
  AND COALESCE(TRIM(p.first_seen_at), '') <> ''
  AND COALESCE(TRIM(json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{{}}' END, '$.imageRef')), '') <> ''
  AND LOWER(j.status) = 'success'
  AND s.stack_id = p.stack_id
  AND (
    LOWER(j.reason) = 'schedule'
    OR LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook'
  )
"#
    );
    tx.execute(&sql, params![&now])?;
    let migrated_candidates = {
        let mut stmt = tx.prepare(
            "SELECT id, image_ref, raw_tag FROM auto_update_candidates WHERE reason = 'migration_pending_history'",
        )?;
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    for (candidate_id, image_ref, raw_tag) in migrated_candidates {
        let image_ref = crate::snapshot_worker::image_repo_from_image_ref(&image_ref)
            .unwrap_or(image_ref);
        let resolved_version = dockrev_common::normalized_semver_from_oci_version(&raw_tag);
        tx.execute(
            r#"
UPDATE auto_update_candidates
SET image_ref = ?1,
    resolved_version = COALESCE(?2, resolved_version),
    status = CASE WHEN ?2 IS NULL THEN status ELSE 'ready' END,
    reason = CASE WHEN ?2 IS NULL THEN reason ELSE 'digest_bound_version' END,
    settled_at = CASE WHEN ?2 IS NULL THEN settled_at ELSE ?3 END,
    updated_at = ?3
WHERE id = ?4
"#,
            params![&image_ref, resolved_version, &now, &candidate_id],
        )?;
    }
    let pending_digest = super::canonical_digest_sql("auto_update_pending.candidate_digest");
    let candidate_digest = super::canonical_digest_sql("c.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_pending
SET candidate_id = (
  SELECT id FROM auto_update_candidates c
  WHERE c.service_id = auto_update_pending.service_id
    AND {candidate_digest} = {pending_digest}
)
WHERE candidate_id IS NULL
  AND EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = auto_update_pending.source_check_job_id
      AND LOWER(j.status) = 'success'
      AND (
        LOWER(j.reason) = 'schedule'
        OR LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook'
      )
  )
"#
    );
    tx.execute(&sql, [])?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json)
        AND json_type(CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END) = 'object'
        THEN json_set(summary_json, '$.skipReason', 'migration_ambiguous_history', '$.skippedAt', ?1)
      ELSE json_object('skipReason', 'migration_ambiguous_history', 'skippedAt', ?1)
    END,
    updated_at = ?1
WHERE candidate_id IS NULL
  AND status IN ('pending', 'enqueuing', 'enqueued')
"#,
        params![&now],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0016_harden_auto_update_candidate_backfill(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0016_harden_auto_update_candidate_backfill";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    tx.execute(
        r#"
UPDATE jobs
SET status = 'cancelled',
    finished_at = ?1
WHERE id IN (
  SELECT p.update_job_id
  FROM auto_update_pending p
  LEFT JOIN auto_update_candidates c ON c.id = p.candidate_id
  WHERE p.status IN ('pending', 'enqueuing', 'enqueued')
    AND p.update_job_id IS NOT NULL
    AND (
      p.candidate_id IS NULL
      OR c.service_id <> p.service_id
      OR c.candidate_digest <> p.candidate_digest
      OR c.stack_id <> p.stack_id
      OR COALESCE(TRIM(c.image_ref), '') = ''
      OR COALESCE(TRIM(c.discovered_at), '') = ''
      OR c.source NOT IN ('schedule', 'github_webhook')
      OR NOT EXISTS (
        SELECT 1
        FROM jobs source_job
        WHERE source_job.id = p.source_check_job_id
          AND LOWER(source_job.type) = 'check'
          AND LOWER(source_job.status) = 'success'
          AND (
            (
              LOWER(c.source) = 'schedule'
              AND LOWER(source_job.reason) = 'schedule'
              AND LOWER(source_job.created_by) = 'schedule'
            )
            OR (
              LOWER(c.source) = 'github_webhook'
              AND LOWER(source_job.created_by) IN ('webhook', 'github')
              AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook'
            )
          )
          AND (
            (LOWER(source_job.scope) = 'service'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id = p.service_id)
            OR (LOWER(source_job.scope) = 'stack'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id IS NULL)
            OR (LOWER(source_job.scope) = 'all'
              AND source_job.stack_id IS NULL
              AND source_job.service_id IS NULL)
          )
      )
    )
)
  AND status = 'queued'
  AND created_by = 'auto-policy'
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT p.update_job_id, ?1, 'migration-ambiguous-history', ?1
FROM auto_update_pending p
JOIN jobs j ON j.id = p.update_job_id
WHERE (
    p.status IN ('pending', 'enqueuing', 'enqueued')
    OR (
      p.status = 'skipped'
      AND json_valid(p.summary_json)
      AND json_extract(p.summary_json, '$.skipReason') = 'migration_ambiguous_history'
    )
  )
  AND p.update_job_id IS NOT NULL
  AND j.status = 'running'
  AND NOT EXISTS (
    SELECT 1
    FROM auto_update_candidates valid_candidate
    JOIN jobs candidate_source_job ON candidate_source_job.id = valid_candidate.source_job_id
    JOIN services candidate_service ON candidate_service.id = valid_candidate.service_id
    WHERE valid_candidate.id = p.candidate_id
      AND valid_candidate.service_id = p.service_id
      AND valid_candidate.candidate_digest = p.candidate_digest
      AND valid_candidate.stack_id = p.stack_id
      AND COALESCE(TRIM(valid_candidate.image_ref), '') <> ''
      AND COALESCE(TRIM(valid_candidate.discovered_at), '') <> ''
      AND valid_candidate.source IN ('schedule', 'github_webhook')
      AND LOWER(candidate_source_job.type) = 'check'
      AND LOWER(candidate_source_job.status) = 'success'
      AND (
        (LOWER(valid_candidate.source) = 'schedule'
          AND LOWER(candidate_source_job.reason) = 'schedule'
          AND LOWER(candidate_source_job.created_by) = 'schedule')
        OR (LOWER(valid_candidate.source) = 'github_webhook'
          AND LOWER(candidate_source_job.created_by) IN ('webhook', 'github')
          AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(candidate_source_job.summary_json) THEN candidate_source_job.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook')
      )
      AND (
        (LOWER(candidate_source_job.scope) = 'service'
          AND candidate_source_job.stack_id = valid_candidate.stack_id
          AND candidate_source_job.service_id = valid_candidate.service_id)
        OR (LOWER(candidate_source_job.scope) = 'stack'
          AND candidate_source_job.stack_id = valid_candidate.stack_id
          AND candidate_source_job.service_id IS NULL)
        OR (LOWER(candidate_source_job.scope) = 'all'
          AND candidate_source_job.stack_id IS NULL
          AND candidate_source_job.service_id IS NULL)
      )
      AND candidate_service.stack_id = valid_candidate.stack_id
      AND candidate_service.candidate_digest = valid_candidate.candidate_digest
      AND EXISTS (
        SELECT 1
        FROM jobs source_job
        WHERE source_job.id = p.source_check_job_id
          AND LOWER(source_job.type) = 'check'
          AND LOWER(source_job.status) = 'success'
          AND (
            (LOWER(valid_candidate.source) = 'schedule'
              AND LOWER(source_job.reason) = 'schedule'
              AND LOWER(source_job.created_by) = 'schedule')
            OR (LOWER(valid_candidate.source) = 'github_webhook'
              AND LOWER(source_job.created_by) IN ('webhook', 'github')
              AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook')
          )
          AND (
            (LOWER(source_job.scope) = 'service'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id = p.service_id)
            OR (LOWER(source_job.scope) = 'stack'
              AND source_job.stack_id = p.stack_id
              AND source_job.service_id IS NULL)
            OR (LOWER(source_job.scope) = 'all'
              AND source_job.stack_id IS NULL
              AND source_job.service_id IS NULL)
          )
      )
  )
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL,
    status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json)
        AND json_type(CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{}' END) = 'object'
        THEN json_set(summary_json, '$.skipReason', 'migration_ambiguous_history', '$.skippedAt', ?1)
      ELSE json_object('skipReason', 'migration_ambiguous_history', 'skippedAt', ?1)
    END,
    updated_at = ?1
WHERE status IN ('pending', 'enqueuing', 'enqueued')
  AND (
    candidate_id IS NULL
    OR NOT EXISTS (
      SELECT 1
      FROM auto_update_candidates c
      JOIN jobs j ON j.id = c.source_job_id
      JOIN services s ON s.id = c.service_id
      WHERE c.id = auto_update_pending.candidate_id
        AND COALESCE(TRIM(c.image_ref), '') <> ''
        AND COALESCE(TRIM(c.discovered_at), '') <> ''
        AND LOWER(j.type) = 'check'
        AND LOWER(j.status) = 'success'
        AND c.source IN ('schedule', 'github_webhook')
        AND (
          (LOWER(c.source) = 'schedule'
            AND LOWER(j.reason) = 'schedule'
            AND LOWER(j.created_by) = 'schedule')
          OR (LOWER(c.source) = 'github_webhook'
            AND LOWER(j.created_by) IN ('webhook', 'github')
            AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook')
        )
        AND (
          (LOWER(j.scope) = 'service'
            AND j.stack_id = c.stack_id
            AND j.service_id = c.service_id)
          OR (LOWER(j.scope) = 'stack'
            AND j.stack_id = c.stack_id
            AND j.service_id IS NULL)
          OR (LOWER(j.scope) = 'all'
            AND j.stack_id IS NULL
            AND j.service_id IS NULL)
        )
        AND s.stack_id = c.stack_id
        AND s.candidate_digest = c.candidate_digest
        AND EXISTS (
          SELECT 1
          FROM jobs source_job
          WHERE source_job.id = auto_update_pending.source_check_job_id
            AND LOWER(source_job.type) = 'check'
            AND LOWER(source_job.status) = 'success'
            AND (
              (LOWER(c.source) = 'schedule'
                AND LOWER(source_job.reason) = 'schedule'
                AND LOWER(source_job.created_by) = 'schedule')
              OR (LOWER(c.source) = 'github_webhook'
                AND LOWER(source_job.created_by) IN ('webhook', 'github')
                AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook')
            )
            AND (
              (LOWER(source_job.scope) = 'service'
                AND source_job.stack_id = auto_update_pending.stack_id
                AND source_job.service_id = auto_update_pending.service_id)
              OR (LOWER(source_job.scope) = 'stack'
                AND source_job.stack_id = auto_update_pending.stack_id
                AND source_job.service_id IS NULL)
              OR (LOWER(source_job.scope) = 'all'
                AND source_job.stack_id IS NULL
                AND source_job.service_id IS NULL)
            )
        )
        AND c.service_id = auto_update_pending.service_id
        AND c.candidate_digest = auto_update_pending.candidate_digest
        AND c.stack_id = auto_update_pending.stack_id
      )
  )
"#,
        params![&now],
    )?;
    let candidate_digest = super::canonical_digest_sql("auto_update_candidates.candidate_digest");
    let service_digest = super::canonical_digest_sql("s.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_candidates
SET status = 'superseded',
    reason = 'migration_ambiguous_history',
    settled_at = ?1,
    policy_status = 'skipped',
    policy_reason = 'migration_ambiguous_history',
    policy_evaluated_at = ?1,
    updated_at = ?1
WHERE reason = 'migration_pending_history'
  AND NOT EXISTS (
    SELECT 1
    FROM jobs j
    JOIN services s ON s.id = auto_update_candidates.service_id
    WHERE j.id = auto_update_candidates.source_job_id
      AND COALESCE(TRIM(auto_update_candidates.image_ref), '') <> ''
      AND LOWER(j.status) = 'success'
      AND auto_update_candidates.source IN ('schedule', 'github_webhook')
      AND s.stack_id = auto_update_candidates.stack_id
      AND {service_digest} = {candidate_digest}
  )
"#
    );
    tx.execute(&sql, params![&now])?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}

fn apply_migration_0015_add_auto_update_candidate_provenance(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0015_add_auto_update_candidate_provenance";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    let mut columns = tx.prepare("PRAGMA table_info(auto_update_candidates)")?;
    let existing = columns
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(columns);
    if !existing.iter().any(|column| column == "source") {
        tx.execute_batch(
            "ALTER TABLE auto_update_candidates ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown'",
        )?;
    }
    tx.execute(
        r#"
UPDATE auto_update_candidates
SET resolved_version = NULL,
    status = 'awaiting_inference',
    reason = 'migration_pending_history'
WHERE auto_update_candidates.reason = 'migration_pending_history'
  AND auto_update_candidates.resolved_version IS NULL
  AND EXISTS (
    SELECT 1 FROM auto_update_pending p
    WHERE p.candidate_id = auto_update_candidates.id
  )
"#,
        [],
    )?;
    let candidate_digest = super::canonical_digest_sql("auto_update_candidates.candidate_digest");
    let service_digest = super::canonical_digest_sql("s.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_candidates
SET status = 'superseded',
    reason = 'migration_candidate_not_current',
    settled_at = ?1,
    policy_status = 'skipped',
    policy_reason = 'migration_candidate_not_current',
    policy_evaluated_at = ?1,
    updated_at = ?1
WHERE reason = 'migration_pending_history'
  AND NOT EXISTS (
    SELECT 1 FROM services s
    WHERE s.id = auto_update_candidates.service_id
      AND {service_digest} = {candidate_digest}
  )
"#
    );
    tx.execute(&sql, params![&now])?;
    let pending_digest = super::canonical_digest_sql("auto_update_pending.candidate_digest");
    let service_digest = super::canonical_digest_sql("s.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL
WHERE candidate_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM services s
    WHERE s.id = auto_update_pending.service_id
      AND {service_digest} = {pending_digest}
  )
  AND status IN ('pending', 'enqueuing', 'enqueued')
"#
    );
    tx.execute(&sql, [])?;
    tx.execute(
        r#"
UPDATE auto_update_candidates
SET source = CASE
  WHEN EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = auto_update_candidates.source_job_id
      AND LOWER(j.reason) = 'schedule'
  ) THEN 'schedule'
  WHEN EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = auto_update_candidates.source_job_id
      AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook'
  ) THEN 'github_webhook'
  ELSE 'unknown'
END
WHERE source = 'unknown'
"#,
        [],
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
