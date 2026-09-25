use super::*;
use rusqlite::{OptionalExtension, Transaction};

#[derive(Clone, Debug)]
pub(crate) struct ServiceVersionTagObservation {
    pub digest: String,
    pub version: Option<String>,
    pub observed_at: String,
}

fn canonical_observation_digest(input: &str) -> Option<String> {
    let input = input.trim().to_ascii_lowercase();
    let payload = input.strip_prefix("sha256:").unwrap_or(&input);
    if payload.len() != 64 || !payload.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("sha256:{payload}"))
}

pub(super) fn record_from_successful_check_summary_tx(
    tx: &Transaction<'_>,
    summary: &serde_json::Value,
) -> anyhow::Result<()> {
    let Some(observations) = summary
        .get("configuredTagObservations")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(());
    };

    for observation in observations {
        let Some(service_id) = observation
            .get("serviceId")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(image_repo) = observation
            .get("imageRepo")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(configured_tag) = observation
            .get("configuredTag")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(digest) = observation
            .get("digest")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(digest) = canonical_observation_digest(digest) else {
            continue;
        };
        let Some(observed_at) = observation
            .get("observedAt")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let version = observation
            .get("version")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        tx.execute(
            r#"
INSERT INTO service_version_tag_observations (
  service_id, image_repo, configured_tag, digest, version, observed_at
)
SELECT ?1, ?2, ?3, ?4, ?5, ?6
WHERE EXISTS (SELECT 1 FROM services WHERE id = ?1)
ON CONFLICT(service_id, image_repo, configured_tag, digest) DO UPDATE SET
  version = COALESCE(service_version_tag_observations.version, excluded.version)
"#,
            params![
                service_id,
                image_repo,
                configured_tag,
                digest,
                version,
                observed_at
            ],
        )?;

        let snapshot_json = tx
            .query_row(
                r#"
SELECT snapshot_json
FROM image_digest_tags_snapshots
WHERE image_repo = ?1 AND digest = ?2
ORDER BY checked_at DESC
LIMIT 1
"#,
                params![image_repo, digest],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(snapshot_json) = snapshot_json {
            let snapshot: crate::api::types::ServiceDigestTagsSnapshotResponse =
                serde_json::from_str(&snapshot_json)?;
            bind_observed_versions_from_snapshot_tx(tx, image_repo, &digest, &snapshot.tags)?;
        }
    }
    Ok(())
}

pub(super) fn bind_version_tx(
    tx: &Transaction<'_>,
    service_id: &str,
    image_repo: &str,
    configured_tag: &str,
    digest: &str,
    version: &str,
) -> anyhow::Result<()> {
    let Some(digest) = canonical_observation_digest(digest) else {
        return Ok(());
    };
    let version = version.trim();
    if version.is_empty() {
        return Ok(());
    }
    tx.execute(
        r#"
UPDATE service_version_tag_observations
SET version = ?5
WHERE service_id = ?1 AND image_repo = ?2 AND configured_tag = ?3
  AND digest = ?4 AND version IS NULL
"#,
        params![service_id, image_repo, configured_tag, digest, version],
    )?;
    Ok(())
}

pub(super) fn bind_observed_versions_from_snapshot_tx(
    tx: &Transaction<'_>,
    image_repo: &str,
    digest: &str,
    tags: &[String],
) -> anyhow::Result<()> {
    let Some(digest) = canonical_observation_digest(digest) else {
        return Ok(());
    };
    let unresolved = {
        let mut statement = tx.prepare(
            r#"
SELECT service_id, configured_tag
FROM service_version_tag_observations
WHERE image_repo = ?1 AND digest = ?2 AND version IS NULL
"#,
        )?;
        statement
            .query_map(params![image_repo, digest], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };

    for (service_id, configured_tag) in unresolved {
        let version = tags
            .iter()
            .filter(|tag| tag.trim() != configured_tag.trim())
            .filter_map(|tag| crate::ignore::parse_version(tag).map(|parsed| (parsed, tag)))
            .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)))
            .map(|(_, tag)| tag.clone());
        if let Some(version) = version {
            bind_version_tx(
                tx,
                &service_id,
                image_repo,
                &configured_tag,
                &digest,
                &version,
            )?;
        }
    }
    Ok(())
}

impl Db {
    pub async fn list_service_version_tag_observations(
        &self,
        service_id: &str,
        image_repo: &str,
        configured_tag: &str,
    ) -> anyhow::Result<Vec<ServiceVersionTagObservation>> {
        let service_id = service_id.to_string();
        let image_repo = image_repo.to_string();
        let configured_tag = configured_tag.to_string();
        self.call(move |conn| {
            let mut statement = conn.prepare(
                r#"
SELECT digest, version, observed_at
FROM service_version_tag_observations
WHERE service_id = ?1 AND image_repo = ?2 AND configured_tag = ?3
ORDER BY observed_at DESC, digest ASC
"#,
            )?;
            Ok(statement
                .query_map(params![service_id, image_repo, configured_tag], |row| {
                    Ok(ServiceVersionTagObservation {
                        digest: row.get(0)?,
                        version: row.get(1)?,
                        observed_at: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service version tag observations")
    }
}
