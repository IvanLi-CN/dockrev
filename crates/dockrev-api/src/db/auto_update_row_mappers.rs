fn canonical_auto_update_digest(digest: &str) -> String {
    crate::snapshot_worker::normalize_digest(digest)
        .unwrap_or_else(|| digest.trim().to_ascii_lowercase())
}

pub(super) fn auto_update_policy_from_row(
    mode: Option<String>,
    enabled: Option<i64>,
    rules_json: Option<String>,
    updated_at: Option<String>,
    default_mode: crate::api::types::AutoUpdatePolicyMode,
) -> rusqlite::Result<crate::api::types::AutoUpdatePolicy> {
    let rules = rules_json
        .as_deref()
        .filter(|raw| !raw.trim().is_empty())
        .map(serde_json::from_str::<Vec<crate::api::types::AutoUpdateRule>>)
        .transpose()
        .map_err(map_serde_error)?
        .unwrap_or_default();
    Ok(crate::api::types::AutoUpdatePolicy {
        mode: mode
            .as_deref()
            .map(crate::api::types::AutoUpdatePolicyMode::from_str)
            .unwrap_or(default_mode),
        enabled: enabled.unwrap_or_default() != 0,
        rules,
        updated_at,
    })
}

fn map_auto_update_pending_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutoUpdatePendingRow> {
    let summary_json: String = row.get(18)?;
    let summary_json = serde_json::from_str(&summary_json).map_err(map_serde_error)?;
    Ok(AutoUpdatePendingRow {
        id: row.get(0)?,
        policy_scope_type: row.get(1)?,
        policy_scope_id: row.get(2)?,
        rule_id: row.get(3)?,
        stack_id: row.get(4)?,
        service_id: row.get(5)?,
        source_check_job_id: row.get(6)?,
        candidate_tag: row.get(7)?,
        candidate_display_tag: row.get(8)?,
        candidate_digest: row.get(9)?,
        current_display_tag: row.get(10)?,
        first_seen_at: row.get(11)?,
        due_at: row.get(12)?,
        min_age_seconds: row.get::<_, i64>(13)?.max(0) as u32,
        min_version_lag: row.get::<_, i64>(14)?.max(0) as u32,
        status: row.get(15)?,
        update_job_id: row.get(16)?,
        candidate_id: row.get(17)?,
        summary_json,
    })
}

fn map_auto_update_candidate_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutoUpdateCandidateRow> {
    Ok(AutoUpdateCandidateRow {
        id: row.get(0)?,
        stack_id: row.get(1)?,
        service_id: row.get(2)?,
        image_ref: row.get(3)?,
        raw_tag: row.get(4)?,
        candidate_digest: row.get(5)?,
        resolved_version: row.get(6)?,
        resolved_tags: row
            .get::<_, Option<String>>(7)?
            .map(|raw| serde_json::from_str(&raw))
            .transpose()
            .map_err(map_serde_error)?,
        status: row.get(8)?,
        reason: row.get(9)?,
        attempts: row.get::<_, i64>(10)?.max(0) as u32,
        retry_at: row.get(11)?,
        discovered_at: row.get(12)?,
        source_job_id: row.get(13)?,
        source: row.get(14)?,
        current_tag: row.get(15)?,
        current_display_tag: row.get(16)?,
        current_digest: row.get(17)?,
        settled_at: row.get(18)?,
        updated_at: row.get(20)?,
        policy_status: row.get(21)?,
        policy_reason: row.get(22)?,
        policy_rule_id: row.get(23)?,
        policy_evaluated_at: row.get(24)?,
        policy_scope_type: row.get(25)?,
        policy_scope_id: row.get(26)?,
        update_job_id: row.get(27)?,
        last_error: row.get(28)?,
        superseded_at: row.get(29)?,
        superseded_by_candidate_id: row.get(30)?,
        hydration_origin: row.get(31)?,
        evidence_generation: row.get(32)?,
    })
}
