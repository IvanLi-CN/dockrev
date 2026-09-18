use rusqlite::{Transaction, params};
use std::collections::BTreeMap;

const HYDRATION_ORIGIN_DISCOVERY_HISTORY: &str = "discovery_history";
const HYDRATION_ORIGIN_AMBIGUOUS_HISTORY: &str = "discovery_history_ambiguous";

#[derive(Clone, Debug)]
pub(crate) struct HydratedCandidateRow {
    pub service_id: String,
    pub candidate_digest: String,
    pub candidate_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CandidateHydrationDiagnosticRow {
    pub service_id: String,
    pub candidate_digest: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub source_job_id: Option<String>,
    pub hydration_origin: Option<String>,
    pub discovered_at: Option<String>,
}

#[derive(Clone, Debug)]
struct DiscoveryHistoryRow {
    #[allow(dead_code)]
    id: i64,
    service_id: String,
    stack_id: String,
    image_ref: String,
    source_job_id: String,
    discovered_at: String,
    current_digest: String,
    current_display_tag: String,
    current_tag: String,
    candidate_tag: String,
    candidate_digest: String,
    candidate_display_tag: String,
    service_candidate_digest: Option<String>,
    job_type: Option<String>,
    job_status: Option<String>,
    job_created_by: Option<String>,
    job_reason: Option<String>,
    job_summary_json: Option<String>,
    job_scope: Option<String>,
    job_stack_id: Option<String>,
    job_service_id: Option<String>,
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn canonical_candidate_digest(value: &str) -> String {
    crate::snapshot_worker::normalize_digest(value)
        .unwrap_or_else(|| value.trim().to_ascii_lowercase())
}

fn reuse_equivalent_candidate_id(
    tx: &Transaction<'_>,
    service_id: &str,
    candidate_digest: &str,
) -> anyhow::Result<Option<String>> {
    let mut stmt = tx.prepare(
        "SELECT id, candidate_digest FROM auto_update_candidates WHERE service_id = ?1 ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![service_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut equivalent = None;
    for row in rows {
        let (id, digest) = row?;
        if digest == candidate_digest {
            return Ok(Some(id));
        }
        if equivalent.is_none() && canonical_candidate_digest(&digest) == candidate_digest {
            equivalent = Some((id, digest));
        }
    }
    if let Some((id, _)) = equivalent {
        tx.execute(
            "UPDATE auto_update_candidates SET candidate_digest = ?1 WHERE id = ?2",
            params![candidate_digest, id],
        )?;
        return Ok(Some(id));
    }
    Ok(None)
}

fn job_summary(row: &DiscoveryHistoryRow) -> serde_json::Value {
    row.job_summary_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn source_job_matches_discovery(row: &DiscoveryHistoryRow) -> bool {
    match row.job_scope.as_deref() {
        Some(scope) if scope.eq_ignore_ascii_case("service") => {
            row.job_stack_id.as_deref() == Some(row.stack_id.as_str())
                && row.job_service_id.as_deref() == Some(row.service_id.as_str())
        }
        Some(scope) if scope.eq_ignore_ascii_case("stack") => {
            row.job_stack_id.as_deref() == Some(row.stack_id.as_str())
                && row.job_service_id.is_none()
        }
        Some(scope) if scope.eq_ignore_ascii_case("all") => {
            row.job_stack_id.is_none() && row.job_service_id.is_none()
        }
        _ => false,
    }
}

fn qualified_source(row: &DiscoveryHistoryRow) -> Option<&'static str> {
    if !source_job_matches_discovery(row) {
        return None;
    }
    if row
        .job_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("check"))
        && row
            .job_status
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("success"))
        && row
            .job_reason
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("schedule"))
        && row
            .job_created_by
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("schedule"))
    {
        return Some("schedule");
    }

    if row
        .job_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("check"))
        && row
            .job_status
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("success"))
        && row.job_created_by.as_deref().is_some_and(|value| {
            value.eq_ignore_ascii_case("webhook") || value.eq_ignore_ascii_case("github")
        })
        && job_summary(row)
            .get("source")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("github_webhook"))
    {
        return Some("github_webhook");
    }

    None
}

fn is_complete_hydration(row: &DiscoveryHistoryRow) -> bool {
    qualified_source(row).is_some()
        && non_empty(&row.image_ref)
        && non_empty(&row.source_job_id)
        && non_empty(&row.discovered_at)
        && non_empty(&row.current_digest)
        && non_empty(&row.current_tag)
        && non_empty(&row.candidate_digest)
        && (non_empty(&row.candidate_tag) || non_empty(&row.candidate_display_tag))
}

fn candidate_values(row: &DiscoveryHistoryRow) -> (String, String, String, String) {
    let raw_tag = if non_empty(&row.candidate_tag) {
        row.candidate_tag.trim().to_string()
    } else {
        row.candidate_display_tag.trim().to_string()
    };
    let current_tag = if non_empty(&row.current_tag) {
        row.current_tag.trim().to_string()
    } else {
        String::new()
    };
    let current_display_tag = if non_empty(&row.current_display_tag) {
        row.current_display_tag.trim().to_string()
    } else if non_empty(&current_tag) {
        current_tag.clone()
    } else {
        String::new()
    };
    let image_ref = crate::snapshot_worker::image_repo_from_image_ref(&row.image_ref)
        .unwrap_or_else(|| row.image_ref.trim().to_string());
    (image_ref, raw_tag, current_tag, current_display_tag)
}

fn select_discovery_history(tx: &Transaction<'_>) -> rusqlite::Result<Vec<DiscoveryHistoryRow>> {
    let mut stmt = tx.prepare(
        r#"
SELECT
  d.id,
  d.service_id,
  s.stack_id,
  d.image_ref,
  d.source_job_id,
  d.discovered_at,
  d.current_digest,
  d.current_display_tag,
  d.current_tag,
  d.candidate_tag,
  d.candidate_digest,
  d.candidate_display_tag,
  s.candidate_digest,
  j.type,
  j.status,
  j.created_by,
  j.reason,
  j.summary_json,
  j.scope,
  j.stack_id,
  j.service_id
FROM service_new_version_discoveries d
JOIN services s
  ON s.id = d.service_id
LEFT JOIN jobs j ON j.id = d.source_job_id
WHERE TRIM(d.candidate_digest) <> ''
ORDER BY d.service_id, d.discovered_at ASC, d.id ASC
"#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DiscoveryHistoryRow {
            id: row.get(0)?,
            service_id: row.get(1)?,
            stack_id: row.get(2)?,
            image_ref: row.get(3)?,
            source_job_id: row.get(4)?,
            discovered_at: row.get(5)?,
            current_digest: row.get(6)?,
            current_display_tag: row.get(7)?,
            current_tag: row.get(8)?,
            candidate_tag: row.get(9)?,
            candidate_digest: row.get(10)?,
            candidate_display_tag: row.get(11)?,
            service_candidate_digest: row.get(12)?,
            job_type: row.get(13)?,
            job_status: row.get(14)?,
            job_created_by: row.get(15)?,
            job_reason: row.get(16)?,
            job_summary_json: row.get(17)?,
            job_scope: row.get(18)?,
            job_stack_id: row.get(19)?,
            job_service_id: row.get(20)?,
        })
    })?;
    rows.collect()
}

fn supersede_previous_candidates(
    tx: &Transaction<'_>,
    row: &HydratedCandidateRow,
    now: &str,
) -> anyhow::Result<()> {
    let pending_ids = {
        let mut stmt = tx.prepare(
            r#"
SELECT p.id, p.candidate_digest, p.update_job_id, p.summary_json
FROM auto_update_pending p
WHERE p.service_id = ?1
  AND p.status IN ('pending', 'enqueuing', 'enqueued')
"#,
        )?;
        stmt.query_map(params![row.service_id], |pending| {
            Ok((
                pending.get::<_, String>(0)?,
                pending.get::<_, String>(1)?,
                pending.get::<_, Option<String>>(2)?,
                pending.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let pending_ids = pending_ids
        .into_iter()
        .filter(|(_, digest, _, _)| canonical_candidate_digest(digest) != row.candidate_digest)
        .collect::<Vec<_>>();
    let candidate_ids = {
        let mut stmt = tx.prepare(
            r#"
SELECT id, candidate_digest
FROM auto_update_candidates
WHERE service_id = ?1
  AND status IN ('awaiting_inference', 'ready', 'unresolved')
"#,
        )?;
        stmt.query_map(params![row.service_id], |candidate| {
            Ok((
                candidate.get::<_, String>(0)?,
                candidate.get::<_, String>(1)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    for candidate_id in candidate_ids
        .into_iter()
        .filter(|(candidate_id, digest)| {
            candidate_id != &row.candidate_id
                && canonical_candidate_digest(digest) != row.candidate_digest
        })
        .map(|(candidate_id, _)| candidate_id)
    {
        tx.execute(
            r#"
UPDATE auto_update_candidates
SET status = 'superseded',
    reason = 'newer_candidate',
    settled_at = ?2,
    policy_status = 'skipped',
    policy_reason = 'candidate_superseded',
    policy_evaluated_at = ?2,
    updated_at = ?2,
    superseded_at = ?2,
    superseded_by_candidate_id = ?3
WHERE id = ?1
"#,
            params![candidate_id, now, row.candidate_id],
        )?;
    }
    for (pending_id, _, update_job_id, summary_raw) in pending_ids {
        let mut summary = serde_json::from_str::<serde_json::Value>(&summary_raw)
            .unwrap_or_else(|_| serde_json::json!({}));
        if !summary.is_object() {
            summary = serde_json::json!({});
        }
        if let Some(object) = summary.as_object_mut() {
            object.insert(
                "skipReason".to_string(),
                serde_json::json!("candidate_superseded"),
            );
            object.insert("skippedAt".to_string(), serde_json::json!(now));
        }
        tx.execute(
            "UPDATE auto_update_pending SET status = 'skipped', summary_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![pending_id, serde_json::to_string(&summary)?, now],
        )?;
        if let Some(update_job_id) = update_job_id {
            tx.execute(
                "UPDATE jobs SET status = 'cancelled', finished_at = ?2 WHERE id = ?1 AND status = 'queued' AND created_by = 'auto-policy'",
                params![update_job_id, now],
            )?;
            tx.execute(
                r#"
INSERT INTO update_job_stop_controls (
  job_id, stop_requested_at, stop_requested_by, updated_at
)
SELECT ?1, ?2, 'auto-policy-supersession', ?2
WHERE EXISTS (SELECT 1 FROM jobs WHERE id = ?1 AND status = 'running')
ON CONFLICT(job_id) DO UPDATE SET
  stop_requested_at = COALESCE(update_job_stop_controls.stop_requested_at, excluded.stop_requested_at),
  stop_requested_by = COALESCE(update_job_stop_controls.stop_requested_by, excluded.stop_requested_by),
  updated_at = excluded.updated_at
WHERE update_job_stop_controls.apply_committed_at IS NULL
  AND update_job_stop_controls.stop_requested_at IS NULL
"#,
                params![update_job_id, now],
            )?;
        }
    }
    Ok(())
}

pub(super) fn hydrate_auto_update_candidates_tx(
    tx: &Transaction<'_>,
    now: &str,
) -> anyhow::Result<Vec<HydratedCandidateRow>> {
    let history = select_discovery_history(tx)?;
    let mut grouped = BTreeMap::<(String, String), Vec<DiscoveryHistoryRow>>::new();
    for row in history {
        let candidate_digest = canonical_candidate_digest(&row.candidate_digest);
        grouped
            .entry((row.service_id.clone(), candidate_digest))
            .or_default()
            .push(row);
    }

    let mut hydrated = Vec::new();
    let mut current_candidates = BTreeMap::<String, (String, String)>::new();
    for ((service_id, candidate_digest), rows) in grouped {
        let source_row = rows
            .iter()
            .find(|row| is_complete_hydration(row))
            .unwrap_or(&rows[0]);
        let complete = is_complete_hydration(source_row);
        let (image_ref, raw_tag, current_tag, current_display_tag) = candidate_values(source_row);
        let source = if complete {
            qualified_source(source_row).unwrap_or("unknown")
        } else {
            "unknown"
        };
        let source_job_id = if complete {
            source_row.source_job_id.trim().to_string()
        } else {
            String::new()
        };
        let discovered_at = if complete {
            source_row.discovered_at.trim().to_string()
        } else {
            String::new()
        };
        let resolved_version = complete
            .then(|| dockrev_common::normalized_semver_from_oci_version(&raw_tag))
            .flatten();
        let status = if !complete {
            "unresolved"
        } else if resolved_version.is_some() {
            "ready"
        } else {
            "awaiting_inference"
        };
        let reason = if !complete {
            "migration_ambiguous_history"
        } else if resolved_version.is_some() {
            "digest_bound_version"
        } else {
            "version_inference_pending"
        };
        let candidate_id = reuse_equivalent_candidate_id(tx, &service_id, &candidate_digest)?
            .unwrap_or_else(|| format!("{service_id}:{candidate_digest}"));
        let hydration_origin = if complete {
            HYDRATION_ORIGIN_DISCOVERY_HISTORY
        } else {
            HYDRATION_ORIGIN_AMBIGUOUS_HISTORY
        };
        let current_digest =
            non_empty(&source_row.current_digest).then(|| source_row.current_digest.clone());

        tx.execute(
            r#"
INSERT INTO auto_update_candidates (
  id, stack_id, service_id, image_ref, raw_tag, candidate_digest,
  resolved_version, resolved_tags, status, reason, attempts, retry_at,
  discovered_at, source_job_id, source, current_tag, current_display_tag,
  current_digest, settled_at, created_at, updated_at, hydration_origin
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, 0, NULL, ?10, ?11, ?12,
        ?13, ?14, ?15, CASE WHEN ?8 IN ('ready', 'unresolved') THEN ?16 ELSE NULL END,
        ?16, ?16, ?17)
ON CONFLICT(service_id, candidate_digest) DO UPDATE SET
  image_ref = CASE
    WHEN COALESCE(TRIM(auto_update_candidates.image_ref), '') = ''
      THEN excluded.image_ref
    ELSE auto_update_candidates.image_ref
  END,
  raw_tag = CASE
    WHEN auto_update_candidates.raw_tag = '' THEN excluded.raw_tag
    ELSE auto_update_candidates.raw_tag
  END,
  resolved_version = COALESCE(auto_update_candidates.resolved_version, excluded.resolved_version),
  status = CASE
    WHEN auto_update_candidates.status = 'superseded' THEN auto_update_candidates.status
    WHEN auto_update_candidates.status = 'ready' AND excluded.status <> 'ready'
      THEN auto_update_candidates.status
    WHEN auto_update_candidates.status = 'unresolved'
      AND excluded.status = 'awaiting_inference'
      AND auto_update_candidates.hydration_origin <> 'discovery_history_ambiguous'
      THEN auto_update_candidates.status
    ELSE excluded.status
  END,
  reason = CASE
    WHEN auto_update_candidates.status = 'superseded' THEN auto_update_candidates.reason
    WHEN auto_update_candidates.status = 'ready' AND excluded.status <> 'ready'
      THEN auto_update_candidates.reason
    WHEN auto_update_candidates.status = 'unresolved'
      AND excluded.status = 'awaiting_inference'
      AND auto_update_candidates.hydration_origin <> 'discovery_history_ambiguous'
      THEN auto_update_candidates.reason
    ELSE COALESCE(excluded.reason, auto_update_candidates.reason)
  END,
  discovered_at = CASE
    WHEN COALESCE(TRIM(auto_update_candidates.discovered_at), '') = ''
      THEN excluded.discovered_at
    ELSE auto_update_candidates.discovered_at
  END,
  source_job_id = CASE
    WHEN COALESCE(TRIM(auto_update_candidates.source_job_id), '') = ''
      OR auto_update_candidates.hydration_origin IS NULL
      OR auto_update_candidates.source = 'unknown'
      THEN excluded.source_job_id
    ELSE auto_update_candidates.source_job_id
  END,
  source = CASE
    WHEN COALESCE(TRIM(auto_update_candidates.source), '') = ''
      OR auto_update_candidates.hydration_origin IS NULL
      OR auto_update_candidates.source = 'unknown'
      THEN excluded.source
    ELSE auto_update_candidates.source
  END,
  current_tag = CASE
    WHEN auto_update_candidates.current_tag = '' THEN excluded.current_tag
    ELSE auto_update_candidates.current_tag
  END,
  current_display_tag = CASE
    WHEN auto_update_candidates.current_display_tag = '' THEN excluded.current_display_tag
    ELSE auto_update_candidates.current_display_tag
  END,
  current_digest = CASE
    WHEN COALESCE(TRIM(auto_update_candidates.current_digest), '') = ''
      THEN excluded.current_digest
    ELSE auto_update_candidates.current_digest
  END,
  settled_at = CASE
    WHEN auto_update_candidates.settled_at IS NULL
      AND excluded.settled_at IS NOT NULL
      THEN excluded.settled_at
    ELSE auto_update_candidates.settled_at
  END,
  hydration_origin = CASE
    WHEN excluded.hydration_origin = 'discovery_history'
      AND auto_update_candidates.hydration_origin = 'discovery_history_ambiguous'
      THEN excluded.hydration_origin
    ELSE COALESCE(auto_update_candidates.hydration_origin, excluded.hydration_origin)
  END,
  updated_at = CASE
    WHEN auto_update_candidates.hydration_origin IS NULL
      OR (
        auto_update_candidates.source = 'unknown'
        AND excluded.source <> 'unknown'
      )
      OR (
        CASE
          WHEN auto_update_candidates.status = 'superseded' THEN 'superseded'
          WHEN auto_update_candidates.status = 'ready' AND excluded.status <> 'ready'
            THEN 'ready'
          WHEN auto_update_candidates.status = 'unresolved'
            AND excluded.status = 'awaiting_inference'
            AND auto_update_candidates.hydration_origin <> 'discovery_history_ambiguous'
            THEN 'unresolved'
          ELSE excluded.status
        END <> auto_update_candidates.status
      )
      OR COALESCE(TRIM(auto_update_candidates.current_digest), '') = ''
        AND COALESCE(TRIM(excluded.current_digest), '') <> ''
      OR COALESCE(TRIM(auto_update_candidates.discovered_at), '') = ''
        AND COALESCE(TRIM(excluded.discovered_at), '') <> ''
      OR COALESCE(TRIM(auto_update_candidates.image_ref), '') = ''
        AND COALESCE(TRIM(excluded.image_ref), '') <> ''
      OR auto_update_candidates.raw_tag = '' AND excluded.raw_tag <> ''
      THEN excluded.updated_at
    ELSE auto_update_candidates.updated_at
  END
"#,
            params![
                candidate_id,
                source_row.stack_id,
                service_id,
                image_ref,
                raw_tag,
                candidate_digest,
                resolved_version,
                status,
                reason,
                discovered_at,
                source_job_id,
                source,
                current_tag,
                current_display_tag,
                current_digest,
                now,
                hydration_origin,
            ],
        )?;
        let candidate_id = tx.query_row(
            "SELECT id FROM auto_update_candidates WHERE service_id = ?1 AND candidate_digest = ?2",
            params![service_id, candidate_digest],
            |row| row.get::<_, String>(0),
        )?;
        if complete
            && source_row
                .service_candidate_digest
                .as_deref()
                .is_some_and(|digest| canonical_candidate_digest(digest) == candidate_digest)
        {
            current_candidates.insert(
                service_id.clone(),
                (candidate_digest.clone(), candidate_id.clone()),
            );
        }
        let hydrated_row = HydratedCandidateRow {
            service_id,
            candidate_digest,
            candidate_id,
        };
        hydrated.push(hydrated_row);
    }
    for (service_id, (candidate_digest, candidate_id)) in current_candidates {
        supersede_previous_candidates(
            tx,
            &HydratedCandidateRow {
                candidate_id,
                service_id,
                candidate_digest,
            },
            now,
        )?;
    }
    Ok(hydrated)
}

pub(super) fn list_candidate_hydration_diagnostics_conn(
    conn: &rusqlite::Connection,
    service_ids: &[String],
) -> rusqlite::Result<Vec<CandidateHydrationDiagnosticRow>> {
    let service_ids = service_ids
        .iter()
        .filter(|id| !id.trim().is_empty())
        .collect::<Vec<_>>();
    if service_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = service_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let candidate_digest = super::canonical_digest_sql("c.candidate_digest");
    let service_digest = super::canonical_digest_sql("s.candidate_digest");
    let discovery_digest = super::canonical_digest_sql("d.candidate_digest");
    let sql = format!(
        r#"
SELECT
  s.id,
  c.id,
  s.candidate_digest,
  c.status,
  c.reason,
  c.source,
  c.source_job_id,
  c.hydration_origin,
  c.discovered_at
FROM services s
LEFT JOIN auto_update_candidates c
  ON c.service_id = s.id
 AND {candidate_digest} = {service_digest}
WHERE s.id IN ({placeholders})
  AND (
    c.hydration_origin IS NOT NULL
    OR EXISTS (
      SELECT 1 FROM service_new_version_discoveries d
      WHERE c.id IS NULL
        AND d.service_id = s.id
        AND {discovery_digest} = {service_digest}
    )
  )
"#
    );
    let params = service_ids
        .iter()
        .map(|id| *id as &dyn rusqlite::ToSql)
        .collect::<Vec<_>>();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |row| {
        let candidate_id: Option<String> = row.get(1)?;
        let candidate_digest: Option<String> = row.get(2)?;
        let hydration_origin: Option<String> = row.get(7)?;
        let status = if candidate_id.is_none() {
            "candidate_missing"
        } else if hydration_origin.as_deref() == Some(HYDRATION_ORIGIN_AMBIGUOUS_HISTORY) {
            "ambiguous_history"
        } else if hydration_origin.as_deref() == Some(HYDRATION_ORIGIN_DISCOVERY_HISTORY) {
            "hydrated"
        } else {
            "unknown"
        };
        let reason = match status {
            "ambiguous_history" => Some("migration_ambiguous_history".to_string()),
            "candidate_missing" => Some("candidate_row_missing".to_string()),
            _ => None,
        };
        Ok(CandidateHydrationDiagnosticRow {
            service_id: row.get(0)?,
            candidate_digest,
            status: status.to_string(),
            reason,
            source: row.get(5)?,
            source_job_id: row
                .get::<_, Option<String>>(6)?
                .filter(|value| non_empty(value)),
            hydration_origin,
            discovered_at: row
                .get::<_, Option<String>>(8)?
                .filter(|value| non_empty(value)),
        })
    })?;
    rows.collect()
}
