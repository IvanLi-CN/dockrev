use std::{collections::BTreeSet, path::Path};

use crate::db::Db;

use super::{MetricsStore, MigrationManifest};

pub(super) fn metrics_target_identity(path: &Path) -> String {
    if path == Path::new(":memory:") {
        return "sqlite-memory".to_string();
    }
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let Ok(metadata) = std::fs::metadata(&canonical) {
            return format!(
                "{}:{}:{}",
                canonical.display(),
                metadata.dev(),
                metadata.ino()
            );
        }
    }
    canonical.display().to_string()
}

impl MetricsStore {
    #[cfg(test)]
    pub async fn migrate_from_legacy(&self, db: &Db) -> anyhow::Result<()> {
        self.migrate_from_legacy_inner(db, None).await
    }

    pub async fn migrate_from_legacy_with_active_services(
        &self,
        db: &Db,
        active_service_ids: &BTreeSet<String>,
    ) -> anyhow::Result<()> {
        self.migrate_from_legacy_inner(db, Some(active_service_ids))
            .await
    }

    async fn migrate_from_legacy_inner(
        &self,
        db: &Db,
        active_service_ids: Option<&BTreeSet<String>>,
    ) -> anyhow::Result<()> {
        let state = db.metrics_migration_state().await?;
        let migration_complete = state.as_ref().is_some_and(|state| {
            state.state == "complete"
                && state.target_identity.as_deref() == Some(self.target_identity.as_str())
        });
        let mut retained_pruned_legacy_ids = BTreeSet::new();
        let mut source_matches_manifest = false;
        let fingerprint = db.legacy_metric_fingerprint().await?;
        let revision = db.legacy_metric_revision().await?;
        let legacy_latest = db.list_legacy_metric_latest_samples().await?;
        if migration_complete && let Some(manifest) = self.migration_manifest().await? {
            source_matches_manifest = manifest.source_sample_count == fingerprint.sample_count
                && manifest.source_max_id == Some(fingerprint.max_id)
                && manifest.source_raw_revision == Some(revision.raw_revision)
                && manifest.source_latest_revision == Some(revision.latest_revision);
            if source_matches_manifest {
                let pruned_legacy_ids = self.pruned_legacy_ids().await?;
                if self.target_is_trusted().await? {
                    let expected_latest = filter_pruned_legacy_latest(
                        legacy_latest,
                        &pruned_legacy_ids,
                        active_service_ids,
                    );
                    if !self
                        .legacy_latest_projection_matches(&expected_latest)
                        .await?
                    {
                        self.sync_legacy_latest_samples(expected_latest.clone())
                            .await?;
                        if !self
                            .legacy_latest_projection_matches(&expected_latest)
                            .await?
                        {
                            let message =
                                "legacy latest projection verification failed".to_string();
                            db.set_metrics_migration_state(
                                "copying",
                                Some(&self.target_identity),
                                Some(&message),
                            )
                            .await?;
                            anyhow::bail!(message);
                        }
                    }
                    if !self.rollups_are_intact().await? {
                        self.reconcile_rollups_from_raw().await?;
                    }
                    return Ok(());
                }
                // Repairing a changed target must not re-import legacy samples that its own
                // retention GC had intentionally pruned.
                retained_pruned_legacy_ids = pruned_legacy_ids;
            }
        }

        let source = db.legacy_metrics_integrity().await?;
        let manifest = MigrationManifest {
            source_sample_count: source.sample_count,
            source_sample_hash: source.sample_hash.clone(),
            source_max_id: Some(fingerprint.max_id),
            source_latest_count: Some(source.latest_count),
            source_latest_hash: Some(source.latest_hash.clone()),
            source_raw_revision: Some(revision.raw_revision),
            source_latest_revision: Some(revision.latest_revision),
        };
        db.set_metrics_migration_state("copying", Some(&self.target_identity), None)
            .await?;

        self.clear_legacy_samples().await?;
        if !source_matches_manifest {
            self.clear_pruned_legacy_ids().await?;
            retained_pruned_legacy_ids.clear();
        }

        let mut after_id = 0_i64;
        loop {
            let batch = db.list_legacy_metric_samples_after(after_id, 2_000).await?;
            if batch.is_empty() {
                break;
            }
            after_id = batch.last().map(|row| row.id).unwrap_or(after_id);
            let batch = batch
                .into_iter()
                .filter(|row| !retained_pruned_legacy_ids.contains(&row.id))
                .collect();
            self.insert_legacy_samples(batch).await?;
        }
        self.reconcile_latest_samples_from_raw().await?;
        let expected_latest = filter_pruned_legacy_latest(
            legacy_latest,
            &retained_pruned_legacy_ids,
            active_service_ids,
        );
        self.sync_legacy_latest_samples(expected_latest.clone())
            .await?;
        if !self
            .legacy_latest_projection_matches(&expected_latest)
            .await?
        {
            let message = "legacy latest projection verification failed".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }

        let target = self.migrated_legacy_integrity().await?;
        let target_is_verified = if retained_pruned_legacy_ids.is_empty() {
            (source.sample_count, source.sample_hash.clone()) == target
        } else {
            self.legacy_sample_coverage_is_complete(source.sample_count)
                .await?
                && self.retained_legacy_samples_match_signatures().await?
        };
        if !target_is_verified {
            let message =
                format!("legacy metrics verification failed: source={source:?} target={target:?}");
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        if retained_pruned_legacy_ids.is_empty() {
            self.reconcile_rollups_from_raw().await?;
        }
        self.trust_target().await?;
        self.set_migration_manifest(&manifest).await?;
        db.set_metrics_migration_state("complete", Some(&self.target_identity), None)
            .await?;
        Ok(())
    }
}

fn filter_pruned_legacy_latest(
    rows: Vec<crate::db::LegacyMetricLatestSampleRow>,
    pruned_legacy_ids: &BTreeSet<i64>,
    active_service_ids: Option<&BTreeSet<String>>,
) -> Vec<crate::db::LegacyMetricLatestSampleRow> {
    rows.into_iter()
        .filter(|row| match active_service_ids {
            Some(ids) => ids.contains(&row.service_id),
            None => !row
                .legacy_sample_id
                .is_some_and(|id| pruned_legacy_ids.contains(&id)),
        })
        .collect()
}
