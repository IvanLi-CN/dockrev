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

pub(super) fn apply_migration_0024_normalize_auto_update_digest_identity(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let id = "0024_normalize_auto_update_digest_identity";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = now_rfc3339()?;
    tx.execute_batch("DROP INDEX IF EXISTS idx_auto_update_pending_active_candidate")?;
    deduplicate_auto_update_pending_digests_tx(&tx, &now)?;
    for table in ["services", "auto_update_pending"] {
        let digest = super::canonical_digest_sql("candidate_digest");
        let sql = format!(
            "UPDATE {table} SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> ''"
        );
        tx.execute(&sql, [])?;
    }
    deduplicate_auto_update_candidate_digests_tx(&tx, &now)?;
    let digest = super::canonical_digest_sql("candidate_digest");
    tx.execute(
        &format!(
            "UPDATE auto_update_candidates SET candidate_digest = {digest} WHERE candidate_digest IS NOT NULL AND TRIM(candidate_digest) <> ''"
        ),
        [],
    )?;
    tx.execute_batch(
        "CREATE UNIQUE INDEX idx_auto_update_pending_active_candidate\n         ON auto_update_pending(\n           service_id, rule_id, policy_scope_type, policy_scope_id, candidate_digest\n         )\n         WHERE status IN ('pending', 'enqueuing', 'enqueued')",
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
