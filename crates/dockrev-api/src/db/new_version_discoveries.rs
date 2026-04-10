use super::*;

const JOB_TYPE_CHECK: &str = "check";
const JOB_STATUS_SUCCESS: &str = "success";

#[derive(Clone, Debug, PartialEq, Eq)]
struct NewVersionDiscoveryInput {
    service_id: String,
    image_ref: String,
    source_job_id: String,
    discovered_at: String,
    current_digest: String,
    current_display_tag: String,
    current_tag: String,
    candidate_tag: String,
    candidate_digest: String,
    candidate_display_tag: String,
}

pub(super) struct NewVersionDiscoveryBaseline {
    pub service_id: String,
    pub current_image_ref: String,
    pub current_digest: Option<String>,
    pub current_display_tag: Option<String>,
    pub current_tag: String,
}

pub(crate) fn normalize_discovery_key(input: Option<&str>) -> String {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn stable_candidate_display_tag<'a>(
    candidate_tag: &str,
    candidate_display_tag: &'a str,
) -> Option<&'a str> {
    let candidate_display_tag = candidate_display_tag.trim();
    if candidate_display_tag.is_empty() {
        return None;
    }

    if candidate_display_tag
        .to_ascii_lowercase()
        .starts_with("sha256:")
    {
        return None;
    }

    let candidate_tag = candidate_tag.trim();
    if !candidate_tag.is_empty() {
        if crate::notify::notification_tag_requires_settle(candidate_tag, candidate_display_tag) {
            return None;
        }
        return Some(candidate_display_tag);
    }

    crate::ignore::is_strict_semver(candidate_display_tag).then_some(candidate_display_tag)
}

pub(crate) fn candidate_tag_allows_settled_fallback(raw_candidate_tag: &str) -> bool {
    let raw_candidate_tag = raw_candidate_tag.trim();
    raw_candidate_tag.is_empty()
        || crate::notify::notification_tag_requires_settle(raw_candidate_tag, raw_candidate_tag)
}

pub(crate) fn canonical_visible_version_tag(tag: &str) -> String {
    let tag = tag.trim();
    if let Some(normalized) = dockrev_common::normalized_semver_from_oci_version(tag) {
        return normalized;
    }

    let numeric = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);
    let parts = numeric.split('.').collect::<Vec<_>>();
    if parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return match parts.as_slice() {
            [major] => format!("{major}.0.0"),
            [major, minor] => format!("{major}.{minor}.0"),
            _ => tag.to_string(),
        };
    }

    tag.to_string()
}

fn semver_core_version_tag(tag: &str) -> Option<String> {
    crate::ignore::parse_version(tag)
        .map(|version| format!("{}.{}.{}", version.major, version.minor, version.patch))
}

fn is_floating_alias_tag(tag: &str) -> bool {
    matches!(
        tag.trim().to_ascii_lowercase().as_str(),
        "latest"
            | "edge"
            | "stable"
            | "main"
            | "master"
            | "head"
            | "dev"
            | "nightly"
            | "rolling"
            | "canary"
            | "snapshot"
            | "beta"
            | "alpha"
    )
}

pub(crate) fn canonical_candidate_identity_tag(
    raw_candidate_tag: &str,
    display_tag: &str,
) -> String {
    if is_floating_alias_tag(raw_candidate_tag)
        && let Some(core) = semver_core_version_tag(display_tag)
    {
        return core;
    }
    canonical_visible_version_tag(display_tag)
}

pub(crate) fn stable_candidate_display_tag_from_tags(
    raw_candidate_tag: &str,
    tags: &std::collections::BTreeSet<String>,
) -> Option<String> {
    let raw_candidate_tag = raw_candidate_tag.trim();
    if tags.len() == 1 {
        let tag = tags.iter().next().cloned()?;
        if candidate_tag_allows_settled_fallback(raw_candidate_tag) {
            return Some(tag);
        }
        if !raw_candidate_tag.is_empty()
            && canonical_visible_version_tag(raw_candidate_tag)
                == canonical_visible_version_tag(&tag)
        {
            return Some(tag);
        }
        return None;
    }
    if is_floating_alias_tag(raw_candidate_tag) {
        let semver_cores = tags
            .iter()
            .filter_map(|tag| semver_core_version_tag(tag))
            .collect::<std::collections::BTreeSet<_>>();
        if semver_cores.len() == 1 {
            return semver_cores.iter().next().cloned();
        }
    }
    None
}

fn stable_current_baseline_tag(current_tag: &str, current_display_tag: &str) -> Option<String> {
    let current_tag = current_tag.trim();
    let current_display_tag = current_display_tag.trim();
    let effective_display_tag = if current_display_tag.is_empty() {
        current_tag
    } else {
        current_display_tag
    };
    stable_candidate_display_tag(current_tag, effective_display_tag)
        .map(canonical_visible_version_tag)
}

fn current_baseline_pinned_digest(current_image_ref: &str) -> Option<String> {
    let (_, digest) = current_image_ref.trim().split_once('@')?;
    let digest = normalize_discovery_key(Some(digest));
    (!digest.is_empty() && digest.to_ascii_lowercase().starts_with("sha256:")).then_some(digest)
}

fn unresolved_current_baseline_match(
    row: &NewVersionDiscoveryRow,
    current_image_ref: &str,
    current_display_tag: &str,
    current_tag: &str,
) -> bool {
    let current_raw_tag = current_baseline_pinned_digest(current_image_ref)
        .unwrap_or_else(|| current_tag.trim().to_string());
    row.current_digest.is_empty()
        && ((row.current_display_tag == current_display_tag)
            || (row.current_display_tag == current_raw_tag && row.current_tag == current_raw_tag)
            || (row.current_display_tag.is_empty() && row.current_tag == current_raw_tag))
}

fn current_baseline_is_digest_pinned(
    current_image_ref: &str,
    current_display_tag: &str,
    current_tag: &str,
) -> bool {
    let current_image_ref = current_image_ref.trim();
    let current_display_tag = current_display_tag.trim();
    let current_tag = current_tag.trim();
    current_image_ref.to_ascii_lowercase().contains("@sha256:")
        || current_display_tag
            .to_ascii_lowercase()
            .starts_with("sha256:")
        || current_tag.to_ascii_lowercase().starts_with("sha256:")
}

fn discovery_inputs_from_summary(
    summary: &serde_json::Value,
    source_job_id: &str,
    discovered_at: &str,
) -> Vec<NewVersionDiscoveryInput> {
    let Some(items) = summary
        .get("newVersions")
        .and_then(|value| value.get("services"))
        .and_then(|value| value.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(service_id) = item.get("serviceId").and_then(|value| value.as_str()) else {
            continue;
        };
        let image_ref = item
            .get("imageRef")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let Some(current_tag) = item.get("currentTag").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(candidate_digest) = item.get("candidateDigest").and_then(|value| value.as_str())
        else {
            continue;
        };
        let current_display_tag = item
            .get("currentDisplayTag")
            .and_then(|value| value.as_str())
            .unwrap_or(current_tag);
        let candidate_display_tag = item
            .get("candidateDisplayTag")
            .and_then(|value| value.as_str())
            .or_else(|| item.get("candidateTag").and_then(|value| value.as_str()));
        let candidate_tag = item
            .get("candidateTag")
            .and_then(|value| value.as_str())
            .or(candidate_display_tag);

        out.push(NewVersionDiscoveryInput {
            service_id: service_id.to_string(),
            image_ref: normalize_discovery_key(Some(image_ref)),
            source_job_id: source_job_id.to_string(),
            discovered_at: discovered_at.to_string(),
            current_digest: normalize_discovery_key(
                item.get("currentDigest").and_then(|value| value.as_str()),
            ),
            current_display_tag: normalize_discovery_key(Some(current_display_tag)),
            current_tag: normalize_discovery_key(Some(current_tag)),
            candidate_tag: normalize_discovery_key(candidate_tag),
            candidate_digest: normalize_discovery_key(Some(candidate_digest)),
            candidate_display_tag: normalize_discovery_key(candidate_display_tag),
        });
    }
    out
}

fn insert_new_version_discovery_conn(
    conn: &rusqlite::Connection,
    discovery: &NewVersionDiscoveryInput,
) -> rusqlite::Result<usize> {
    conn.execute(
        r#"
INSERT OR IGNORE INTO service_new_version_discoveries (
  service_id,
  image_ref,
  source_job_id,
  discovered_at,
  current_digest,
  current_display_tag,
  current_tag,
  candidate_tag,
  candidate_digest,
  candidate_display_tag
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
"#,
        params![
            discovery.service_id,
            discovery.image_ref,
            discovery.source_job_id,
            discovery.discovered_at,
            discovery.current_digest,
            discovery.current_display_tag,
            discovery.current_tag,
            discovery.candidate_tag,
            discovery.candidate_digest,
            discovery.candidate_display_tag,
        ],
    )
}

fn discovery_matches_baseline(
    row: &NewVersionDiscoveryRow,
    current_image_ref: &str,
    current_digest: &str,
    current_display_tag: &str,
    current_tag: &str,
) -> bool {
    let current_stable_tag = stable_current_baseline_tag(current_tag, current_display_tag);
    let row_stable_tag = stable_current_baseline_tag(&row.current_tag, &row.current_display_tag);

    if !current_digest.is_empty() {
        if row.current_digest == current_digest {
            return true;
        }
        if !row.current_digest.is_empty() {
            return false;
        }
        if current_baseline_is_digest_pinned(current_image_ref, current_display_tag, current_tag)
            && unresolved_current_baseline_match(
                row,
                current_image_ref,
                current_display_tag,
                current_tag,
            )
        {
            return true;
        }
        if let Some(current_stable_tag) = current_stable_tag.as_deref() {
            return row_stable_tag.as_deref() == Some(current_stable_tag);
        }
        false
    } else if let Some(current_stable_tag) = current_stable_tag.as_deref() {
        row.current_digest.is_empty() && row_stable_tag.as_deref() == Some(current_stable_tag)
    } else if !current_display_tag.is_empty() {
        unresolved_current_baseline_match(row, current_image_ref, current_display_tag, current_tag)
    } else {
        row.current_digest.is_empty()
            && row.current_display_tag.is_empty()
            && row.current_tag == current_tag
    }
}

fn candidate_identity_key(
    row: &NewVersionDiscoveryRow,
    stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> Option<String> {
    if let Some(tag) = effective_stable_candidate_display_tag(row, stable_tags_by_provenance) {
        return Some(format!(
            "tag:{}",
            canonical_candidate_identity_tag(&row.candidate_tag, &tag)
        ));
    }

    let candidate_display_tag = normalize_discovery_key(Some(row.candidate_display_tag.as_str()));
    if !candidate_display_tag.is_empty()
        && !candidate_display_tag
            .to_ascii_lowercase()
            .starts_with("sha256:")
    {
        return Some(format!(
            "alias:{}",
            canonical_visible_version_tag(&candidate_display_tag)
        ));
    }

    let candidate_tag = normalize_discovery_key(Some(row.candidate_tag.as_str()));
    if !candidate_tag.is_empty() && !candidate_tag.to_ascii_lowercase().starts_with("sha256:") {
        return Some(format!(
            "alias:{}",
            canonical_visible_version_tag(&candidate_tag)
        ));
    }

    let digest = row.candidate_digest.trim();
    if digest.is_empty() {
        return None;
    }
    Some(format!("digest:{digest}"))
}

fn candidate_display_version(
    row: &NewVersionDiscoveryRow,
    stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> Option<String> {
    if let Some(tag) = effective_stable_candidate_display_tag(row, stable_tags_by_provenance) {
        return Some(tag);
    }

    let digest = row.candidate_digest.trim();
    let candidate_display_tag = normalize_discovery_key(Some(row.candidate_display_tag.as_str()));
    if !candidate_display_tag.is_empty()
        && !candidate_display_tag
            .to_ascii_lowercase()
            .starts_with("sha256:")
    {
        return Some(canonical_visible_version_tag(&candidate_display_tag));
    }

    let candidate_tag = normalize_discovery_key(Some(row.candidate_tag.as_str()));
    if !candidate_tag.is_empty() && !candidate_tag.to_ascii_lowercase().starts_with("sha256:") {
        return Some(canonical_visible_version_tag(&candidate_tag));
    }

    (!digest.is_empty()).then_some(digest.to_string())
}

fn effective_stable_candidate_display_tag(
    row: &NewVersionDiscoveryRow,
    stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> Option<String> {
    if let Some(tag) = stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
    {
        return Some(canonical_visible_version_tag(tag));
    }

    let digest = row.candidate_digest.trim();
    if digest.is_empty() {
        return None;
    }

    let key = (
        row.service_id.clone(),
        row.image_ref.clone(),
        row.current_tag.clone(),
        digest.to_string(),
    );
    let tags = stable_tags_by_provenance.get(&key)?;
    stable_candidate_display_tag_from_tags(&row.candidate_tag, tags)
}

fn build_stable_tags_by_provenance(
    matched_rows: &[&NewVersionDiscoveryRow],
    effective_stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> std::collections::HashMap<(String, String, String, String), std::collections::BTreeSet<String>>
{
    let mut stable_tags_by_provenance = matched_rows.iter().fold(
        std::collections::HashMap::<
            (String, String, String, String),
            std::collections::BTreeSet<String>,
        >::new(),
        |mut acc, row| {
            let Some(tag) =
                stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
            else {
                return acc;
            };
            let digest = row.candidate_digest.trim();
            if digest.is_empty() {
                return acc;
            }
            acc.entry((
                row.service_id.clone(),
                row.image_ref.clone(),
                row.current_tag.clone(),
                digest.to_string(),
            ))
            .or_default()
            .insert(canonical_visible_version_tag(tag));
            acc
        },
    );
    for (key, tags) in effective_stable_tags_by_provenance {
        if key.3.trim().is_empty() || tags.is_empty() {
            continue;
        }
        stable_tags_by_provenance
            .entry(key.clone())
            .or_default()
            .extend(tags.iter().cloned());
    }
    stable_tags_by_provenance
}

pub(crate) fn collect_new_version_discovery_candidates_from_rows<'a>(
    rows: impl Iterator<Item = &'a NewVersionDiscoveryRow>,
    current_image_ref: &str,
    current_digest: &str,
    current_display_tag: &str,
    current_tag: &str,
    effective_stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> Vec<NewVersionDiscoveryCandidate> {
    let matched_rows = rows
        .filter(|row| {
            discovery_matches_baseline(
                row,
                current_image_ref,
                current_digest,
                current_display_tag,
                current_tag,
            )
        })
        .collect::<Vec<_>>();
    let stable_tags_by_provenance =
        build_stable_tags_by_provenance(&matched_rows, effective_stable_tags_by_provenance);
    let mut candidates = std::collections::BTreeMap::<String, NewVersionDiscoveryCandidate>::new();

    for row in matched_rows {
        let Some(identity_key) = candidate_identity_key(row, &stable_tags_by_provenance) else {
            continue;
        };
        let version =
            candidate_display_version(row, &stable_tags_by_provenance).unwrap_or_else(|| {
                identity_key
                    .strip_prefix("tag:")
                    .or_else(|| identity_key.strip_prefix("digest:"))
                    .unwrap_or(identity_key.as_str())
                    .to_string()
            });
        let discovered_at = normalize_discovery_key(Some(row.discovered_at.as_str()));
        let discovered_at = (!discovered_at.is_empty()).then_some(discovered_at);
        match candidates.entry(identity_key.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(NewVersionDiscoveryCandidate {
                    identity_key,
                    version,
                    first_discovered_at: discovered_at,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                if existing
                    .first_discovered_at
                    .as_deref()
                    .is_none_or(|current| {
                        discovered_at
                            .as_deref()
                            .is_some_and(|candidate| candidate < current)
                    })
                    && discovered_at.is_some()
                {
                    existing.first_discovered_at = discovered_at;
                }
                if existing.version.starts_with("sha256:") && !version.starts_with("sha256:") {
                    existing.version = version;
                }
            }
        }
    }

    candidates.into_values().collect()
}

pub(crate) fn count_new_version_discoveries_from_rows<'a>(
    rows: impl Iterator<Item = &'a NewVersionDiscoveryRow>,
    current_image_ref: &str,
    current_digest: &str,
    current_display_tag: &str,
    current_tag: &str,
    effective_stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> u32 {
    collect_new_version_discovery_candidates_from_rows(
        rows,
        current_image_ref,
        current_digest,
        current_display_tag,
        current_tag,
        effective_stable_tags_by_provenance,
    )
    .len() as u32
}

pub(crate) fn infer_stable_candidate_display_tag_from_rows<'a>(
    rows: impl Iterator<Item = &'a NewVersionDiscoveryRow>,
    current_image_ref: &str,
    current_digest: &str,
    current_display_tag: &str,
    current_tag: &str,
    candidate_digest: &str,
    effective_stable_tags_by_provenance: &std::collections::HashMap<
        (String, String, String, String),
        std::collections::BTreeSet<String>,
    >,
) -> Option<String> {
    let candidate_digest = normalize_discovery_key(Some(candidate_digest));
    if candidate_digest.is_empty() {
        return None;
    }

    let matched_rows = rows
        .filter(|row| {
            discovery_matches_baseline(
                row,
                current_image_ref,
                current_digest,
                current_display_tag,
                current_tag,
            )
        })
        .collect::<Vec<_>>();
    if matched_rows.is_empty() {
        return None;
    }

    let stable_tags_by_provenance =
        build_stable_tags_by_provenance(&matched_rows, effective_stable_tags_by_provenance);
    let mut versions = std::collections::BTreeSet::<String>::new();
    for row in matched_rows {
        if normalize_discovery_key(Some(row.candidate_digest.as_str())) != candidate_digest {
            continue;
        }
        let Some(version) = candidate_display_version(row, &stable_tags_by_provenance) else {
            continue;
        };
        if version.to_ascii_lowercase().starts_with("sha256:") {
            continue;
        }
        versions.insert(canonical_visible_version_tag(&version));
    }

    (versions.len() == 1)
        .then(|| versions.iter().next().cloned())
        .flatten()
}

pub(crate) fn new_version_discovery_notification_targets(
    rows: &[NewVersionDiscoveryRow],
) -> Vec<(String, String, String, String)> {
    rows.iter()
        .filter(|row| {
            stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag).is_none()
        })
        .filter_map(|row| {
            crate::snapshot_worker::normalize_digest(&row.candidate_digest).map(|digest| {
                (
                    row.service_id.clone(),
                    row.image_ref.clone(),
                    row.current_tag.clone(),
                    digest,
                )
            })
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn list_new_version_discoveries_for_services_conn(
    conn: &rusqlite::Connection,
    service_ids: &[String],
) -> rusqlite::Result<Vec<NewVersionDiscoveryRow>> {
    let service_ids = service_ids
        .iter()
        .map(|service_id| service_id.trim())
        .filter(|service_id| !service_id.is_empty())
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if service_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = service_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT
  service_id,
  image_ref,
  discovered_at,
  current_digest,
  current_display_tag,
  current_tag,
  candidate_tag,
  candidate_digest,
  candidate_display_tag
FROM service_new_version_discoveries
WHERE service_id IN ({placeholders})
"#,
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(service_ids.len());
    for service_id in &service_ids {
        params.push(service_id);
    }
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok(NewVersionDiscoveryRow {
            service_id: row.get(0)?,
            image_ref: row.get(1)?,
            discovered_at: row.get(2)?,
            current_digest: row.get(3)?,
            current_display_tag: row.get(4)?,
            current_tag: row.get(5)?,
            candidate_tag: row.get(6)?,
            candidate_digest: row.get(7)?,
            candidate_display_tag: row.get(8)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

pub(super) fn record_new_version_discoveries_from_summary_conn(
    conn: &rusqlite::Connection,
    source_job_id: &str,
    discovered_at: &str,
    summary: &serde_json::Value,
) -> anyhow::Result<usize> {
    let mut inserted = 0usize;
    for discovery in discovery_inputs_from_summary(summary, source_job_id, discovered_at) {
        inserted += insert_new_version_discovery_conn(conn, &discovery)?;
    }
    Ok(inserted)
}

pub(super) fn backfill_new_version_discoveries_from_successful_checks_conn(
    conn: &rusqlite::Connection,
) -> anyhow::Result<usize> {
    let mut stmt = conn.prepare(
        r#"
SELECT id, finished_at, summary_json
FROM jobs
WHERE type = ?1 AND status = ?2
ORDER BY COALESCE(finished_at, created_at) ASC, id ASC
"#,
    )?;
    let rows = stmt.query_map(params![JOB_TYPE_CHECK, JOB_STATUS_SUCCESS], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut inserted = 0usize;
    for row in rows {
        let (job_id, finished_at, summary_json) = row?;
        let Ok(summary) = serde_json::from_str::<serde_json::Value>(&summary_json) else {
            continue;
        };
        let discovered_at = finished_at
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default();
        inserted += record_new_version_discoveries_from_summary_conn(
            conn,
            &job_id,
            discovered_at,
            &summary,
        )?;
    }

    Ok(inserted)
}

pub(super) fn count_new_version_discoveries_for_services_conn(
    conn: &rusqlite::Connection,
    baselines: &[NewVersionDiscoveryBaseline],
) -> rusqlite::Result<std::collections::HashMap<String, u32>> {
    use std::collections::BTreeSet;

    let rows = list_new_version_discoveries_for_services_conn(
        conn,
        &baselines
            .iter()
            .map(|baseline| baseline.service_id.clone())
            .collect::<Vec<_>>(),
    )?;
    let rows_by_service = rows.into_iter().fold(
        std::collections::HashMap::<String, Vec<NewVersionDiscoveryRow>>::new(),
        |mut acc, row| {
            acc.entry(row.service_id.clone()).or_default().push(row);
            acc
        },
    );
    let notification_targets = rows_by_service
        .values()
        .flat_map(|rows| new_version_discovery_notification_targets(rows))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let notification_tags =
        super::new_version_notifications::list_stable_candidate_display_tags_for_notification_targets_conn(
            conn,
            &notification_targets,
        )?;

    Ok(baselines
        .iter()
        .filter_map(|baseline| {
            let rows = rows_by_service.get(&baseline.service_id)?;
            let count = count_new_version_discoveries_from_rows(
                rows.iter(),
                &normalize_discovery_key(Some(baseline.current_image_ref.as_str())),
                &normalize_discovery_key(baseline.current_digest.as_deref()),
                &normalize_discovery_key(baseline.current_display_tag.as_deref()),
                &normalize_discovery_key(Some(baseline.current_tag.as_str())),
                &notification_tags,
            );
            (count > 0).then_some((baseline.service_id.clone(), count))
        })
        .collect())
}

impl Db {
    pub async fn list_new_version_discoveries_for_services(
        &self,
        service_ids: &[String],
    ) -> anyhow::Result<Vec<NewVersionDiscoveryRow>> {
        let service_ids = service_ids.to_vec();
        self.call(move |conn| {
            Ok(list_new_version_discoveries_for_services_conn(
                conn,
                &service_ids,
            )?)
        })
        .await
        .context("list new version discoveries for services")
    }
}

#[cfg(test)]
impl Db {
    pub async fn rebuild_new_version_discoveries_from_successful_checks(
        &self,
    ) -> anyhow::Result<usize> {
        self.call(|conn| {
            conn.execute("DELETE FROM service_new_version_discoveries", [])?;
            backfill_new_version_discoveries_from_successful_checks_conn(conn)
        })
        .await
        .context("rebuild new version discoveries from successful checks")
    }
}

#[cfg(test)]
mod tests;
