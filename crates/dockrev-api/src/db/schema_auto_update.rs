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
"#,
    )?;
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
    tx.execute(
        r#"
INSERT OR IGNORE INTO auto_update_candidates (
  id, stack_id, service_id, image_ref, raw_tag, candidate_digest,
  resolved_version, status, reason, attempts, discovered_at, source_job_id, source,
  current_tag, current_display_tag, current_digest, created_at, updated_at
)
SELECT
  p.service_id || ':' || p.candidate_digest,
  p.stack_id,
  p.service_id,
  COALESCE(NULLIF(json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{}' END, '$.imageRef'), ''), ''),
  p.candidate_tag,
  p.candidate_digest,
  NULL,
  'awaiting_inference',
  'migration_pending_history',
  0,
  p.first_seen_at,
  p.source_check_job_id,
  CASE WHEN LOWER(j.reason) = 'schedule' THEN 'schedule' ELSE 'github_webhook' END,
  COALESCE(NULLIF(json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{}' END, '$.currentTag'), ''), p.current_display_tag),
  p.current_display_tag,
  json_extract(CASE WHEN json_valid(p.summary_json) THEN p.summary_json ELSE '{}' END, '$.currentDigest'),
  ?1,
  ?1
FROM auto_update_pending p
JOIN jobs j ON j.id = p.source_check_job_id
JOIN services s ON s.id = p.service_id AND s.candidate_digest = p.candidate_digest
WHERE p.status IN ('pending', 'enqueuing', 'enqueued')
  AND COALESCE(TRIM(p.candidate_digest), '') <> ''
  AND COALESCE(TRIM(p.first_seen_at), '') <> ''
  AND (
    LOWER(j.reason) = 'schedule'
    OR LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook'
  )
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = (
  SELECT id FROM auto_update_candidates c
  WHERE c.service_id = auto_update_pending.service_id
    AND c.candidate_digest = auto_update_pending.candidate_digest
)
WHERE candidate_id IS NULL
  AND EXISTS (
    SELECT 1 FROM jobs j
    WHERE j.id = auto_update_pending.source_check_job_id
      AND (
        LOWER(j.reason) = 'schedule'
        OR LOWER(COALESCE(json_extract(CASE WHEN json_valid(j.summary_json) THEN j.summary_json ELSE '{}' END, '$.source'), '')) = 'github_webhook'
      )
  )
"#,
        [],
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
    tx.execute(
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
      AND s.candidate_digest = auto_update_candidates.candidate_digest
  )
"#,
        params![&now],
    )?;
    tx.execute(
        r#"
UPDATE auto_update_pending
SET candidate_id = NULL
WHERE candidate_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM services s
    WHERE s.id = auto_update_pending.service_id
      AND s.candidate_digest = auto_update_pending.candidate_digest
  )
  AND status IN ('pending', 'enqueuing', 'enqueued')
"#,
        [],
    )?;
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
