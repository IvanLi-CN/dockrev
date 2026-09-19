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
  ON auto_update_pending(
    service_id, rule_id, policy_scope_type, policy_scope_id, candidate_digest
  )
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

include!("schema_auto_update_migrations.rs");

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
    let candidate_digest = super::strict_canonical_digest_sql("auto_update_candidates.candidate_digest");
    let pending_digest = super::strict_canonical_digest_sql("p.candidate_digest");
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
    let pending_digest = super::strict_canonical_digest_sql("p.candidate_digest");
    let comparison_digest = super::strict_canonical_digest_sql("p2.candidate_digest");
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
      AND p2.policy_scope_type = p.policy_scope_type
      AND p2.policy_scope_id = p.policy_scope_id
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

fn deduplicate_auto_update_candidate_digests_tx(
    tx: &rusqlite::Transaction<'_>,
    now: &str,
) -> anyhow::Result<()> {
    let candidate_digest = super::strict_canonical_digest_sql("candidate.candidate_digest");
    let other_digest = super::strict_canonical_digest_sql("other.candidate_digest");
    let sql = format!(
        r#"
CREATE TEMP TABLE migration_duplicate_auto_update_candidates AS
SELECT
  candidate.id AS duplicate_id,
  (
    SELECT other.id
    FROM auto_update_candidates other
    WHERE other.service_id = candidate.service_id
      AND {other_digest} = {candidate_digest}
    ORDER BY
      CASE WHEN lower(trim(other.candidate_digest)) LIKE 'sha256:%' THEN 1 ELSE 0 END DESC,
      CASE WHEN other.status = 'superseded' THEN 0 ELSE 1 END DESC,
      CASE WHEN EXISTS (
        SELECT 1
        FROM auto_update_pending active_pending
        WHERE active_pending.candidate_id = other.id
          AND active_pending.status IN ('pending', 'enqueuing', 'enqueued')
      ) THEN 1 ELSE 0 END DESC,
      CASE WHEN other.hydration_origin = 'discovery_history' THEN 1 ELSE 0 END DESC,
      CASE other.status
        WHEN 'ready' THEN 3
        WHEN 'awaiting_inference' THEN 2
        WHEN 'unresolved' THEN 1
        ELSE 0
      END DESC,
      CASE WHEN COALESCE(TRIM(other.image_ref), '') <> '' THEN 1 ELSE 0 END DESC,
      CASE WHEN COALESCE(TRIM(other.discovered_at), '') <> '' THEN 1 ELSE 0 END DESC,
      other.discovered_at ASC,
      other.created_at ASC,
      other.id ASC
    LIMIT 1
  ) AS keeper_id
FROM auto_update_candidates candidate
WHERE NULLIF(TRIM(candidate.candidate_digest), '') IS NOT NULL
"#
    );
    tx.execute(&sql, [])?;
    tx.execute(
        r#"
UPDATE jobs
SET status = 'cancelled',
    finished_at = ?1
WHERE id IN (
  SELECT duplicate.update_job_id
  FROM migration_duplicate_auto_update_candidates mapping
  JOIN auto_update_candidates duplicate ON duplicate.id = mapping.duplicate_id
  WHERE mapping.duplicate_id <> mapping.keeper_id
    AND duplicate.update_job_id IS NOT NULL
    AND NOT EXISTS (
      SELECT 1
      FROM auto_update_pending active_pending
      WHERE active_pending.update_job_id = duplicate.update_job_id
        AND active_pending.status IN ('pending', 'enqueuing', 'enqueued')
    )
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
SELECT duplicate.update_job_id, ?1, 'migration-canonical-digest', ?1
FROM migration_duplicate_auto_update_candidates mapping
JOIN auto_update_candidates duplicate ON duplicate.id = mapping.duplicate_id
JOIN jobs j ON j.id = duplicate.update_job_id
WHERE mapping.duplicate_id <> mapping.keeper_id
  AND duplicate.update_job_id IS NOT NULL
  AND j.status = 'running'
  AND NOT EXISTS (
    SELECT 1
    FROM auto_update_pending active_pending
    WHERE active_pending.update_job_id = duplicate.update_job_id
      AND active_pending.status IN ('pending', 'enqueuing', 'enqueued')
  )
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
UPDATE auto_update_candidates AS keeper
SET source_job_id = COALESCE(
      (SELECT NULLIF(TRIM(other.source_job_id), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id
         AND LOWER(TRIM(other.source)) IN ('schedule', 'github_webhook')
         AND NULLIF(TRIM(other.source_job_id), '') IS NOT NULL
         AND NULLIF(TRIM(other.discovered_at), '') IS NOT NULL
       ORDER BY other.discovered_at ASC, other.created_at ASC, other.id ASC LIMIT 1),
      NULLIF(TRIM(keeper.source_job_id), ''), keeper.source_job_id),
    source = COALESCE(
      (SELECT other.source
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id
         AND LOWER(TRIM(other.source)) IN ('schedule', 'github_webhook')
         AND NULLIF(TRIM(other.source_job_id), '') IS NOT NULL
         AND NULLIF(TRIM(other.discovered_at), '') IS NOT NULL
       ORDER BY other.discovered_at ASC, other.created_at ASC, other.id ASC LIMIT 1),
      NULLIF(TRIM(keeper.source), ''), keeper.source),
    discovered_at = COALESCE(
      (SELECT NULLIF(TRIM(other.discovered_at), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id
         AND LOWER(TRIM(other.source)) IN ('schedule', 'github_webhook')
         AND NULLIF(TRIM(other.source_job_id), '') IS NOT NULL
         AND NULLIF(TRIM(other.discovered_at), '') IS NOT NULL
      ORDER BY other.discovered_at ASC, other.created_at ASC, other.id ASC LIMIT 1),
      NULLIF(TRIM(keeper.discovered_at), ''), keeper.discovered_at),
    resolved_tags = COALESCE(
      NULLIF(TRIM(keeper.resolved_tags), ''),
      (SELECT NULLIF(TRIM(other.resolved_tags), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.resolved_tags), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    last_error = COALESCE(
      NULLIF(TRIM(keeper.last_error), ''),
      (SELECT NULLIF(TRIM(other.last_error), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.last_error), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    retry_at = COALESCE(
      NULLIF(TRIM(keeper.retry_at), ''),
      (SELECT NULLIF(TRIM(other.retry_at), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.retry_at), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    policy_reason = COALESCE(
      NULLIF(TRIM(keeper.policy_reason), ''),
      (SELECT NULLIF(TRIM(other.policy_reason), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.policy_reason), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    policy_rule_id = COALESCE(
      NULLIF(TRIM(keeper.policy_rule_id), ''),
      (SELECT NULLIF(TRIM(other.policy_rule_id), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.policy_rule_id), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    policy_evaluated_at = COALESCE(
      NULLIF(TRIM(keeper.policy_evaluated_at), ''),
      (SELECT NULLIF(TRIM(other.policy_evaluated_at), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.policy_evaluated_at), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    policy_scope_type = COALESCE(
      NULLIF(TRIM(keeper.policy_scope_type), ''),
      (SELECT NULLIF(TRIM(other.policy_scope_type), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.policy_scope_type), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    policy_scope_id = COALESCE(
      NULLIF(TRIM(keeper.policy_scope_id), ''),
      (SELECT NULLIF(TRIM(other.policy_scope_id), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.policy_scope_id), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    superseded_at = COALESCE(
      NULLIF(TRIM(keeper.superseded_at), ''),
      (SELECT NULLIF(TRIM(other.superseded_at), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.superseded_at), '') IS NOT NULL
       ORDER BY other.superseded_at ASC, other.created_at ASC, other.id ASC LIMIT 1)),
    superseded_by_candidate_id = COALESCE(
      NULLIF(TRIM(keeper.superseded_by_candidate_id), ''),
      (SELECT NULLIF(TRIM(other.superseded_by_candidate_id), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.superseded_by_candidate_id), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1)),
    resolved_version = COALESCE(
      NULLIF(TRIM(keeper.resolved_version), ''),
      (SELECT NULLIF(TRIM(other.resolved_version), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.resolved_version), '') IS NOT NULL
       ORDER BY CASE other.status WHEN 'ready' THEN 2 WHEN 'awaiting_inference' THEN 1 ELSE 0 END DESC,
                other.created_at ASC, other.id ASC LIMIT 1)),
    status = CASE
      WHEN keeper.status = 'superseded' THEN keeper.status
      WHEN EXISTS (SELECT 1 FROM migration_duplicate_auto_update_candidates mapping
                   JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
                   WHERE mapping.keeper_id = keeper.id AND other.status = 'ready') THEN 'ready'
      WHEN EXISTS (SELECT 1 FROM migration_duplicate_auto_update_candidates mapping
                   JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
                   WHERE mapping.keeper_id = keeper.id AND other.status = 'awaiting_inference') THEN 'awaiting_inference'
      ELSE keeper.status END,
    reason = COALESCE(
      (SELECT NULLIF(TRIM(other.reason), '')
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.reason), '') IS NOT NULL
       ORDER BY CASE other.status WHEN 'ready' THEN 3 WHEN 'awaiting_inference' THEN 2 WHEN 'unresolved' THEN 1 ELSE 0 END DESC,
                other.created_at ASC, other.id ASC LIMIT 1), keeper.reason),
    attempts = MAX(keeper.attempts, COALESCE(
      (SELECT MAX(other.attempts)
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id), 0)),
    settled_at = COALESCE(keeper.settled_at,
      (SELECT other.settled_at
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND other.settled_at IS NOT NULL
       ORDER BY other.settled_at ASC, other.created_at ASC, other.id ASC LIMIT 1)),
    policy_status = COALESCE(
      (SELECT other.policy_status
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND other.policy_status IS NOT NULL
         AND EXISTS (SELECT 1 FROM auto_update_pending active_pending
                     WHERE active_pending.candidate_id = other.id
                       AND active_pending.status IN ('pending', 'enqueuing', 'enqueued'))
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1), keeper.policy_status),
    update_job_id = COALESCE(
      (SELECT other.update_job_id
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND other.update_job_id IS NOT NULL
         AND EXISTS (SELECT 1 FROM auto_update_pending active_pending
                     WHERE active_pending.candidate_id = other.id
                       AND active_pending.status IN ('pending', 'enqueuing', 'enqueued'))
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1), keeper.update_job_id),
    hydration_origin = CASE
      WHEN EXISTS (SELECT 1 FROM migration_duplicate_auto_update_candidates mapping
                   JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
                   WHERE mapping.keeper_id = keeper.id AND other.hydration_origin = 'discovery_history')
        THEN 'discovery_history'
      ELSE COALESCE(keeper.hydration_origin, (SELECT other.hydration_origin
        FROM migration_duplicate_auto_update_candidates mapping
        JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
        WHERE mapping.keeper_id = keeper.id AND other.hydration_origin IS NOT NULL
        ORDER BY other.created_at ASC, other.id ASC LIMIT 1)) END,
    settlement_generation = MAX(keeper.settlement_generation, COALESCE(
      (SELECT MAX(other.settlement_generation)
       FROM migration_duplicate_auto_update_candidates mapping
       JOIN auto_update_candidates other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id), 0)),
    updated_at = ?1
WHERE keeper.id IN (SELECT keeper_id FROM migration_duplicate_auto_update_candidates WHERE duplicate_id <> keeper_id)
"#,
        rusqlite::params![now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_candidates
SET superseded_by_candidate_id = (
  SELECT mapping.keeper_id
  FROM migration_duplicate_auto_update_candidates mapping
  WHERE mapping.duplicate_id = auto_update_candidates.superseded_by_candidate_id
)
WHERE superseded_by_candidate_id IN (
  SELECT duplicate_id
  FROM migration_duplicate_auto_update_candidates
  WHERE duplicate_id <> keeper_id
)
"#,
        [],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = (
  SELECT mapping.keeper_id
  FROM migration_duplicate_auto_update_candidates mapping
  WHERE mapping.duplicate_id = auto_update_pending.candidate_id
)
WHERE candidate_id IN (
  SELECT duplicate_id
  FROM migration_duplicate_auto_update_candidates
  WHERE duplicate_id <> keeper_id
)
"#,
        [],
    )?;
    tx.execute(
        r#"
DELETE FROM auto_update_candidates
WHERE id IN (
  SELECT duplicate_id
  FROM migration_duplicate_auto_update_candidates
  WHERE duplicate_id <> keeper_id
)
"#,
        [],
    )?;
    tx.execute_batch("DROP TABLE migration_duplicate_auto_update_candidates")?;
    Ok(())
}

pub(super) fn apply_migration_0025_normalize_new_version_notification_digest_identity(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0025_normalize_new_version_notification_digest_identity";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    let candidate_digest = super::canonical_digest_sql("notification.candidate_digest");
    let other_digest = super::canonical_digest_sql("other.candidate_digest");
    let mapping_sql = format!(
        r#"
CREATE TEMP TABLE migration_duplicate_new_version_notifications AS
SELECT
  notification.id AS duplicate_id,
  (
    SELECT other.id
    FROM new_version_notifications other
    WHERE other.service_id = notification.service_id
      AND other.status IN ('pending', 'sent')
      AND {other_digest} = {candidate_digest}
    ORDER BY
      CASE other.status WHEN 'sent' THEN 2 WHEN 'pending' THEN 1 ELSE 0 END DESC,
      other.created_at ASC,
      other.id ASC
    LIMIT 1
  ) AS keeper_id
FROM new_version_notifications notification
WHERE notification.status IN ('pending', 'sent')
  AND NULLIF(TRIM(notification.candidate_digest), '') IS NOT NULL
"#
    );
    tx.execute(&mapping_sql, [])?;
    tx.execute(
        r#"
UPDATE new_version_notifications AS keeper
SET sent_channels_json = COALESCE(
      (SELECT json_group_array(channel)
       FROM (
         SELECT value AS channel
         FROM json_each(CASE
           WHEN json_valid(keeper.sent_channels_json)
             AND json_type(keeper.sent_channels_json) = 'array'
             THEN keeper.sent_channels_json ELSE '[]' END)
         WHERE type = 'text'
         UNION
         SELECT channel.value
         FROM migration_duplicate_new_version_notifications mapping
         JOIN new_version_notifications other ON other.id = mapping.duplicate_id
         JOIN json_each(CASE
           WHEN json_valid(other.sent_channels_json)
             AND json_type(other.sent_channels_json) = 'array'
             THEN other.sent_channels_json ELSE '[]' END) AS channel
         WHERE mapping.keeper_id = keeper.id
           AND mapping.duplicate_id <> mapping.keeper_id
           AND channel.type = 'text'
         ORDER BY channel
       )), '[]'),
    sent_at = COALESCE(
      keeper.sent_at,
      (SELECT MIN(other.sent_at)
       FROM migration_duplicate_new_version_notifications mapping
       JOIN new_version_notifications other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND other.sent_at IS NOT NULL)),
    last_error = COALESCE(
      NULLIF(TRIM(keeper.last_error), ''),
      (SELECT NULLIF(TRIM(other.last_error), '')
       FROM migration_duplicate_new_version_notifications mapping
       JOIN new_version_notifications other ON other.id = mapping.duplicate_id
       WHERE mapping.keeper_id = keeper.id AND NULLIF(TRIM(other.last_error), '') IS NOT NULL
       ORDER BY other.created_at ASC, other.id ASC LIMIT 1))
WHERE keeper.id IN (
  SELECT keeper_id
  FROM migration_duplicate_new_version_notifications
  WHERE duplicate_id <> keeper_id
)
"#,
        [],
    )?;
    tx.execute(
        r#"
UPDATE new_version_notifications
SET
  status = 'superseded',
  superseded_at = COALESCE(superseded_at, ?1),
  last_error = COALESCE(last_error, 'migration_canonical_digest')
WHERE id IN (
  SELECT duplicate_id
  FROM migration_duplicate_new_version_notifications
  WHERE duplicate_id <> keeper_id
)
"#,
        rusqlite::params![&now],
    )?;
    let digest = super::canonical_digest_sql("candidate_digest");
    tx.execute(
        &format!(
            "UPDATE new_version_notifications SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> ''"
        ),
        [],
    )?;
    tx.execute_batch("DROP TABLE migration_duplicate_new_version_notifications")?;
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
    let candidate_digest = super::strict_canonical_digest_sql("c.candidate_digest");
    let pending_digest = super::strict_canonical_digest_sql("p.candidate_digest");
    let service_digest = super::strict_canonical_digest_sql("candidate_service.candidate_digest");
    let candidate_current_digest = super::strict_canonical_digest_sql("c.current_digest");
    let pending_current_digest = super::strict_canonical_digest_sql("p.current_digest");
    let service_current_digest =
        super::strict_canonical_digest_sql("candidate_service.current_digest");
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
      AND {candidate_current_digest} = {pending_current_digest}
      AND {pending_current_digest} = {service_current_digest}
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
          AND LOWER(TRIM(candidate_source_job.stack_id)) = LOWER(TRIM(c.stack_id))
          AND LOWER(TRIM(candidate_source_job.service_id)) = LOWER(TRIM(c.service_id)))
        OR (LOWER(candidate_source_job.scope) = 'stack'
          AND LOWER(TRIM(candidate_source_job.stack_id)) = LOWER(TRIM(c.stack_id))
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
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
              AND LOWER(TRIM(source_job.service_id)) = LOWER(TRIM(p.service_id)))
            OR (LOWER(source_job.scope) = 'stack'
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
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
        let digest = super::strict_canonical_digest_sql("candidate_digest");
        let sql = format!(
            "UPDATE {table} SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> '' AND ({digest}) IS NOT NULL"
        );
        tx.execute(&sql, [])?;
    }
    let pending_digest = super::strict_canonical_digest_sql("p.candidate_digest");
    let service_digest = super::strict_canonical_digest_sql("s.candidate_digest");
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
  AND LOWER(j.type) = 'check'
  AND LOWER(j.status) = 'success'
  AND s.stack_id = p.stack_id
  AND (
    (
      LOWER(j.reason) = 'schedule'
      AND LOWER(j.created_by) = 'schedule'
    )
    OR (
      LOWER(j.created_by) IN ('webhook', 'github')
      AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook'
    )
  )
  AND (
    (LOWER(j.scope) = 'service'
      AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(p.stack_id))
      AND LOWER(TRIM(j.service_id)) = LOWER(TRIM(p.service_id)))
    OR (LOWER(j.scope) = 'stack'
      AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(p.stack_id))
      AND j.service_id IS NULL)
    OR (LOWER(j.scope) = 'all'
      AND j.stack_id IS NULL
      AND j.service_id IS NULL)
  )
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
    let pending_digest = super::strict_canonical_digest_sql("auto_update_pending.candidate_digest");
    let candidate_digest = super::strict_canonical_digest_sql("c.candidate_digest");
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
      AND LOWER(j.type) = 'check'
      AND LOWER(j.status) = 'success'
      AND (
        (
          LOWER(j.reason) = 'schedule'
          AND LOWER(j.created_by) = 'schedule'
        )
        OR (
          LOWER(j.created_by) IN ('webhook', 'github')
          AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook'
        )
      )
      AND (
        (LOWER(j.scope) = 'service'
          AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(auto_update_pending.stack_id))
          AND LOWER(TRIM(j.service_id)) = LOWER(TRIM(auto_update_pending.service_id)))
        OR (LOWER(j.scope) = 'stack'
          AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(auto_update_pending.stack_id))
          AND j.service_id IS NULL)
        OR (LOWER(j.scope) = 'all'
          AND j.stack_id IS NULL
          AND j.service_id IS NULL)
      )
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
UPDATE jobs
SET status = 'cancelled',
    finished_at = ?1
WHERE id IN (
  SELECT update_job_id
  FROM auto_update_pending
  WHERE candidate_id IS NULL
    AND status IN ('pending', 'enqueuing', 'enqueued')
    AND update_job_id IS NOT NULL
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
WHERE p.candidate_id IS NULL
  AND p.status IN ('pending', 'enqueuing', 'enqueued')
  AND p.update_job_id IS NOT NULL
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
    let candidate_digest = super::strict_canonical_digest_sql("c.candidate_digest");
    let pending_digest = super::strict_canonical_digest_sql("p.candidate_digest");
    let sql = format!(
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
      OR {candidate_digest} IS NULL
      OR {pending_digest} IS NULL
      OR {candidate_digest} <> {pending_digest}
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
              AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook'
            )
          )
          AND (
            (LOWER(source_job.scope) = 'service'
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
              AND LOWER(TRIM(source_job.service_id)) = LOWER(TRIM(p.service_id)))
            OR (LOWER(source_job.scope) = 'stack'
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
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
"#
    );
    tx.execute(&sql, params![&now])?;
    let valid_candidate_digest =
        super::strict_canonical_digest_sql("valid_candidate.candidate_digest");
    let candidate_service_digest =
        super::strict_canonical_digest_sql("candidate_service.candidate_digest");
    let sql = format!(
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
      AND {valid_candidate_digest} = {pending_digest}
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
          AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(candidate_source_job.summary_json) THEN candidate_source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
      )
      AND (
        (LOWER(candidate_source_job.scope) = 'service'
          AND LOWER(TRIM(candidate_source_job.stack_id)) = LOWER(TRIM(valid_candidate.stack_id))
          AND LOWER(TRIM(candidate_source_job.service_id)) = LOWER(TRIM(valid_candidate.service_id)))
        OR (LOWER(candidate_source_job.scope) = 'stack'
          AND LOWER(TRIM(candidate_source_job.stack_id)) = LOWER(TRIM(valid_candidate.stack_id))
          AND candidate_source_job.service_id IS NULL)
        OR (LOWER(candidate_source_job.scope) = 'all'
          AND candidate_source_job.stack_id IS NULL
          AND candidate_source_job.service_id IS NULL)
      )
      AND candidate_service.stack_id = valid_candidate.stack_id
      AND {candidate_service_digest} = {valid_candidate_digest}
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
              AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
          )
          AND (
            (LOWER(source_job.scope) = 'service'
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
              AND LOWER(TRIM(source_job.service_id)) = LOWER(TRIM(p.service_id)))
            OR (LOWER(source_job.scope) = 'stack'
              AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(p.stack_id))
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
"#
    );
    tx.execute(&sql, params![&now])?;
    let candidate_digest = super::strict_canonical_digest_sql("c.candidate_digest");
    let pending_digest = super::strict_canonical_digest_sql("auto_update_pending.candidate_digest");
    let service_digest = super::strict_canonical_digest_sql("s.candidate_digest");
    let sql = format!(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL,
    status = 'skipped',
    summary_json = CASE
      WHEN json_valid(summary_json)
        AND json_type(CASE WHEN json_valid(summary_json) THEN summary_json ELSE '{{}}' END) = 'object'
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
        AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
        )
        AND (
          (LOWER(j.scope) = 'service'
            AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(c.stack_id))
            AND LOWER(TRIM(j.service_id)) = LOWER(TRIM(c.service_id)))
          OR (LOWER(j.scope) = 'stack'
            AND LOWER(TRIM(j.stack_id)) = LOWER(TRIM(c.stack_id))
            AND j.service_id IS NULL)
          OR (LOWER(j.scope) = 'all'
            AND j.stack_id IS NULL
            AND j.service_id IS NULL)
        )
        AND s.stack_id = c.stack_id
        AND {service_digest} = {candidate_digest}
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
                AND LOWER(COALESCE(json_extract(CASE WHEN json_valid(source_job.summary_json) THEN source_job.summary_json ELSE '{{}}' END, '$.source'), '')) = 'github_webhook')
            )
            AND (
              (LOWER(source_job.scope) = 'service'
                AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(auto_update_pending.stack_id))
                AND LOWER(TRIM(source_job.service_id)) = LOWER(TRIM(auto_update_pending.service_id)))
              OR (LOWER(source_job.scope) = 'stack'
                AND LOWER(TRIM(source_job.stack_id)) = LOWER(TRIM(auto_update_pending.stack_id))
                AND source_job.service_id IS NULL)
              OR (LOWER(source_job.scope) = 'all'
                AND source_job.stack_id IS NULL
                AND source_job.service_id IS NULL)
            )
        )
        AND c.service_id = auto_update_pending.service_id
        AND {candidate_digest} = {pending_digest}
        AND c.stack_id = auto_update_pending.stack_id
      )
  )
"#
    );
    tx.execute(&sql, params![&now])?;
    let candidate_digest =
        super::strict_canonical_digest_sql("auto_update_candidates.candidate_digest");
    let service_digest = super::strict_canonical_digest_sql("s.candidate_digest");
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
