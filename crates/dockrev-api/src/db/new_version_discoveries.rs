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
    current_digest: &str,
    current_display_tag: &str,
    current_tag: &str,
) -> bool {
    if !current_digest.is_empty() {
        row.current_digest == current_digest
            || (row.current_digest.is_empty() && row.current_display_tag == current_display_tag)
            || (row.current_digest.is_empty()
                && row.current_display_tag == current_tag
                && row.current_tag == current_tag)
            || (row.current_digest.is_empty()
                && row.current_display_tag.is_empty()
                && row.current_tag == current_tag)
    } else if !current_display_tag.is_empty() {
        (row.current_digest.is_empty() && row.current_display_tag == current_display_tag)
            || (row.current_digest.is_empty()
                && row.current_display_tag == current_tag
                && row.current_tag == current_tag)
            || (row.current_digest.is_empty()
                && row.current_display_tag.is_empty()
                && row.current_tag == current_tag)
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
    if let Some(tag) = stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
    {
        return Some(format!("tag:{}", canonical_visible_version_tag(tag)));
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

    if let Some(tags) = stable_tags_by_provenance.get(&key)
        && tags.len() == 1
    {
        return tags.iter().next().cloned().map(|tag| format!("tag:{tag}"));
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
    if let Some(tag) = stable_candidate_display_tag(&row.candidate_tag, &row.candidate_display_tag)
    {
        return Some(canonical_visible_version_tag(tag));
    }

    let digest = row.candidate_digest.trim();
    if !digest.is_empty() {
        let key = (
            row.service_id.clone(),
            row.image_ref.clone(),
            row.current_tag.clone(),
            digest.to_string(),
        );
        if let Some(tags) = stable_tags_by_provenance.get(&key)
            && tags.len() == 1
        {
            return tags.iter().next().cloned();
        }
    }

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

fn build_stable_tags_by_provenance<'a>(
    matched_rows: &[&'a NewVersionDiscoveryRow],
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
            discovery_matches_baseline(row, current_digest, current_display_tag, current_tag)
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
                {
                    if discovered_at.is_some() {
                        existing.first_discovered_at = discovered_at;
                    }
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
        current_digest,
        current_display_tag,
        current_tag,
        effective_stable_tags_by_provenance,
    )
    .len() as u32
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
mod tests {
    use std::{collections::BTreeMap, path::Path};

    use super::*;
    use crate::{
        api::types::{BackupRetention, ComposeConfig, StackBackupConfig},
        models::{JobRecord, ServiceSeed, StackRecord},
    };

    fn make_summary(
        service_id: &str,
        current_tag: &str,
        current_display_tag: Option<&str>,
        current_digest: Option<&str>,
        candidate_digest: &str,
        candidate_display_tag: Option<&str>,
    ) -> serde_json::Value {
        make_summary_with_candidate_tag(
            service_id,
            current_tag,
            current_display_tag,
            current_digest,
            "latest",
            candidate_digest,
            candidate_display_tag,
        )
    }

    fn make_summary_with_candidate_tag(
        service_id: &str,
        current_tag: &str,
        current_display_tag: Option<&str>,
        current_digest: Option<&str>,
        candidate_tag: &str,
        candidate_digest: &str,
        candidate_display_tag: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "newVersions": {
                "count": 1,
                "services": [{
                    "stackId": "stack_1",
                    "serviceId": service_id,
                    "serviceName": "web",
                    "imageRef": "ghcr.io/acme/web",
                    "currentTag": current_tag,
                    "currentDigest": current_digest,
                    "currentDisplayTag": current_display_tag.unwrap_or(current_tag),
                    "candidateTag": candidate_tag,
                    "candidateDisplayTag": candidate_display_tag.unwrap_or(candidate_tag),
                    "candidateDigest": candidate_digest,
                }],
            }
        })
    }

    async fn seed_service(
        db: &Db,
        service_id: &str,
        current_digest: Option<&str>,
        current_resolved_tag: Option<&str>,
        image_tag: &str,
        candidate_digest: Option<&str>,
    ) {
        let now = "2026-03-19T00:00:00Z";
        let stack = StackRecord {
            id: "stack_1".to_string(),
            name: "demo".to_string(),
            archived: false,
            compose: ComposeConfig {
                kind: "compose".to_string(),
                compose_files: vec!["/tmp/demo.yml".to_string()],
                env_file: None,
            },
            backup: StackBackupConfig {
                targets: Vec::new(),
                retention: BackupRetention::default(),
            },
            services: Vec::new(),
        };
        let seeds = vec![ServiceSeed {
            id: service_id.to_string(),
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: image_tag.to_string(),
            auto_rollback: false,
            backup_bind_paths: BTreeMap::new(),
            backup_volume_names: BTreeMap::new(),
        }];
        db.insert_stack(&stack, &seeds, now).await.unwrap();
        db.update_service_check_result(
            service_id,
            current_digest.map(ToString::to_string),
            current_resolved_tag.map(ToString::to_string),
            current_resolved_tag.map(|value| format!("[\"{value}\"]")),
            candidate_digest.map(|_| image_tag.to_string()),
            candidate_digest.map(|_| "1.1.0".to_string()),
            candidate_digest.map(ToString::to_string),
            candidate_digest.map(|_| "match".to_string()),
            candidate_digest.map(|_| "[\"linux/amd64\"]".to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
    }

    async fn insert_successful_check_job(db: &Db, job_id: &str, summary: serde_json::Value) {
        let now = "2026-03-19T00:00:00Z";
        let mut job = JobRecord::new_running(
            job_id.to_string(),
            JobType::Check,
            JobScope::Service,
            Some("stack_1".to_string()),
            Some("svc_1".to_string()),
            now,
        )
        .to_db();
        job.created_by = "test".to_string();
        job.reason = "ui".to_string();
        db.insert_job(job).await.unwrap();
        db.finish_job(job_id, "success", now, &summary)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn finish_job_counts_unique_visible_versions_for_current_baseline() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("v1.1.0"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_2",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-b",
                Some("1.1.0"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_3",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_4",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-c",
                Some("1.2.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
    }

    #[tokio::test]
    async fn finish_job_canonicalizes_semver_equivalent_visible_versions() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("5.2"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_2",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-b",
                Some("5.2.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
    }

    #[tokio::test]
    async fn finish_job_uses_digest_when_candidate_display_tag_is_floating_alias() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("latest"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_2",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-b",
                Some("latest"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
    }

    #[tokio::test]
    async fn finish_job_uses_digest_when_candidate_display_tag_is_unresolved_non_semver_tag() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary_with_candidate_tag(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "15-alpine",
                "sha256:candidate-a",
                Some("15-alpine"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_2",
            make_summary_with_candidate_tag(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "15-alpine",
                "sha256:candidate-b",
                Some("15-alpine"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
    }

    #[tokio::test]
    async fn finish_job_falls_back_to_display_tag_when_historical_current_digest_is_missing() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                None,
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
    }

    #[tokio::test]
    async fn finish_job_matches_unresolved_alias_history_after_display_tag_resolves() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            None,
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("latest"),
                None,
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
    }

    #[tokio::test]
    async fn finish_job_ignores_floating_candidate_alias_once_same_digest_resolves() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("latest"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_2",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
    }

    #[tokio::test]
    async fn current_baseline_change_drops_old_discovery_counts() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_1",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;

        db.update_service_check_result(
            "svc_1",
            Some("sha256:current-v2".to_string()),
            Some("2.0.0".to_string()),
            Some("[\"2.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("2.1.0".to_string()),
            Some("sha256:live-candidate-v2".to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            "2026-03-19T00:10:00Z",
            "2026-03-19T00:10:00Z",
        )
        .await
        .unwrap();

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, None);
    }

    #[tokio::test]
    async fn backfill_rebuilds_discoveries_from_successful_check_history() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        let now = "2026-03-19T00:00:00Z";
        let mut job = JobRecord::new_running(
            "job_backfill".to_string(),
            JobType::Check,
            JobScope::Service,
            Some("stack_1".to_string()),
            Some("svc_1".to_string()),
            now,
        )
        .to_db();
        job.status = "success".to_string();
        job.created_by = "test".to_string();
        job.reason = "schedule".to_string();
        job.finished_at = Some(now.to_string());
        job.summary_json = make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-backfill",
            Some("1.1.0"),
        );
        db.insert_job(job).await.unwrap();
        insert_successful_check_job(
            &db,
            "job_backfill_same_visible",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-backfill-2",
                Some("1.1.0"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_backfill_new_visible",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-backfill-3",
                Some("1.2.0"),
            ),
        )
        .await;

        db.call(|conn| {
            conn.execute("DELETE FROM service_new_version_discoveries", [])?;
            Ok(())
        })
        .await
        .unwrap();

        let inserted = db
            .rebuild_new_version_discoveries_from_successful_checks()
            .await
            .unwrap();
        assert_eq!(inserted, 3);

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
    }

    #[tokio::test]
    async fn backfill_rebuild_preserves_image_ref_provenance() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_backfill_image_ref",
            serde_json::json!({
                "newVersions": {
                    "count": 1,
                    "services": [{
                        "stackId": "stack_1",
                        "serviceId": "svc_1",
                        "serviceName": "web",
                        "imageRef": "ghcr.io/acme/legacy-web",
                        "currentTag": "latest",
                        "currentDigest": "sha256:current-v1",
                        "currentDisplayTag": "1.0.0",
                        "candidateTag": "latest",
                        "candidateDisplayTag": "latest",
                        "candidateDigest": "sha256:candidate-a"
                    }],
                }
            }),
        )
        .await;

        db.call(|conn| {
            conn.execute("DELETE FROM service_new_version_discoveries", [])?;
            Ok(())
        })
        .await
        .unwrap();

        db.rebuild_new_version_discoveries_from_successful_checks()
            .await
            .unwrap();

        let rows = db
            .list_new_version_discoveries_for_services(&["svc_1".to_string()])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].image_ref, "ghcr.io/acme/legacy-web");
    }

    #[tokio::test]
    async fn finish_job_records_discoveries_even_when_notifications_are_disabled() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.0.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        let mut settings = db.get_notification_settings().await.unwrap();
        settings.email_enabled = false;
        settings.webhook_enabled = false;
        settings.telegram_enabled = false;
        settings.webpush_enabled = false;
        settings.event_new_version_enabled = false;
        db.put_notification_settings(&settings, "2026-03-19T00:00:00Z")
            .await
            .unwrap();

        insert_successful_check_job(
            &db,
            "job_notifications_disabled",
            make_summary(
                "svc_1",
                "latest",
                Some("1.0.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("1.1.0"),
            ),
        )
        .await;

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
    }

    #[tokio::test]
    async fn get_stack_normalizes_unsettled_discovery_history_from_notifications() {
        let db = Db::open(Path::new(":memory:")).await.unwrap();
        seed_service(
            &db,
            "svc_1",
            Some("sha256:current-v1"),
            Some("1.16.0"),
            "latest",
            Some("sha256:live-candidate"),
        )
        .await;

        insert_successful_check_job(
            &db,
            "job_unsettled_a",
            make_summary(
                "svc_1",
                "latest",
                Some("1.16.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-a",
                Some("latest"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_unsettled_b",
            make_summary(
                "svc_1",
                "latest",
                Some("1.16.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-b",
                Some("latest"),
            ),
        )
        .await;
        insert_successful_check_job(
            &db,
            "job_unsettled_c",
            make_summary(
                "svc_1",
                "latest",
                Some("1.16.0"),
                Some("sha256:current-v1"),
                "sha256:candidate-c",
                Some("latest"),
            ),
        )
        .await;

        for (id, digest, display_tag) in [
            ("nvn_a", "sha256:candidate-a", "1.16.2"),
            ("nvn_b", "sha256:candidate-b", "1.16.2"),
            ("nvn_c", "sha256:candidate-c", "1.17.0"),
        ] {
            db.reserve_new_version_notification(&crate::db::NewVersionNotificationPending {
                id: id.to_string(),
                service_id: "svc_1".to_string(),
                job_id: "job_notifications".to_string(),
                reason: "schedule".to_string(),
                image_ref: "ghcr.io/acme/web".to_string(),
                image_tag: "latest".to_string(),
                current_tag: "latest".to_string(),
                current_display_tag: "1.16.0".to_string(),
                candidate_tag: "latest".to_string(),
                candidate_display_tag: display_tag.to_string(),
                candidate_digest: digest.to_string(),
                created_at: "2026-03-19T00:03:00Z".to_string(),
            })
            .await
            .unwrap();
        }

        let stack = db.get_stack("stack_1").await.unwrap().unwrap();
        assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
    }
}
