use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

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
        let mut source_matches_manifest = false;
        let revision = db.legacy_metric_revision().await?;
        let legacy_latest = db.list_legacy_metric_latest_samples().await?;
        let previous_manifest = self.migration_manifest().await?;
        let mut raw_source_matches_manifest = false;
        if let Some(manifest) = previous_manifest.as_ref() {
            raw_source_matches_manifest =
                manifest.source_raw_revision == Some(revision.raw_revision);
            source_matches_manifest = raw_source_matches_manifest
                && manifest.source_latest_revision == Some(revision.latest_revision);
            if source_matches_manifest && migration_complete {
                if self.target_is_trusted().await? {
                    let pruned_legacy_ids = if active_service_ids.is_some() {
                        BTreeSet::new()
                    } else {
                        self.pruned_legacy_ids().await?
                    };
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
                    return Ok(());
                }
            }
        }

        let native_target = self.native_target_state().await?;
        if native_target.raw_is_untrusted {
            let message = "native raw metrics target changed and cannot recover".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        if native_target.latest_is_untrusted {
            let message = "native latest metrics target changed and cannot recover".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }

        if !self.pruned_legacy_ids_are_intact().await? {
            let message = "metrics migration tombstone integrity verification failed".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        let retained_pruned_legacy_ids = self.pruned_legacy_ids().await?;
        if !raw_source_matches_manifest && !retained_pruned_legacy_ids.is_empty() {
            let message =
                "legacy raw source changed after retention and cannot rebuild long-window rollups"
                    .to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        if source_matches_manifest
            && !db
                .legacy_metric_ids_exist(&retained_pruned_legacy_ids)
                .await?
        {
            let message = "legacy metric tombstones no longer match the source".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        let rollups_are_intact = self.rollups_are_intact().await?;
        if !rollups_are_intact
            && (!retained_pruned_legacy_ids.is_empty() || native_target.has_pruned_raw)
        {
            let message = "retained rollups cannot be recovered after raw retention".to_string();
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }

        let source = db.legacy_metrics_integrity().await?;
        let fingerprint = db.legacy_metric_fingerprint().await?;
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

        let previous_legacy_rollup_buckets = if !raw_source_matches_manifest || !rollups_are_intact
        {
            self.legacy_rollup_buckets().await?
        } else {
            BTreeSet::new()
        };
        self.clear_legacy_samples().await?;

        let mut retained_legacy_samples = BTreeMap::new();
        let mut after_id = 0_i64;
        loop {
            let batch = db.list_legacy_metric_samples_after(after_id, 2_000).await?;
            if batch.is_empty() {
                break;
            }
            after_id = batch.last().map(|row| row.id).unwrap_or(after_id);
            for row in &batch {
                if retained_pruned_legacy_ids.contains(&row.id) {
                    retained_legacy_samples.insert(row.id, row.sample.clone());
                }
            }
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
                && self
                    .retained_legacy_samples_match(&retained_legacy_samples)
                    .await?
        };
        if !target_is_verified {
            let message =
                format!("legacy metrics verification failed: source={source:?} target={target:?}");
            db.set_metrics_migration_state("copying", Some(&self.target_identity), Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        if !raw_source_matches_manifest || !rollups_are_intact {
            self.reconcile_rollups_from_raw(&previous_legacy_rollup_buckets)
                .await?;
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
