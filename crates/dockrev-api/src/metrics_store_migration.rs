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
    pub async fn migrate_from_legacy(&self, db: &Db) -> anyhow::Result<()> {
        let state = db.metrics_migration_state().await?;
        let migration_complete = state.as_ref().is_some_and(|state| {
            state.state == "complete"
                && state.target_identity.as_deref() == Some(self.target_identity.as_str())
        });
        let mut retained_pruned_legacy_ids = BTreeSet::new();
        let mut source_matches_manifest = false;
        if migration_complete && let Some(manifest) = self.migration_manifest().await? {
            let fingerprint = db.legacy_metric_fingerprint().await?;
            source_matches_manifest = manifest.source_sample_count == fingerprint.sample_count
                && manifest.source_max_id == Some(fingerprint.max_id);
            if source_matches_manifest {
                let target_matches_source_count =
                    self.migrated_legacy_sample_count().await? == fingerprint.sample_count;
                let target_integrity_matches = !target_matches_source_count
                    || (
                        manifest.source_sample_count,
                        manifest.source_sample_hash.clone(),
                    ) == self.migrated_legacy_integrity().await?;
                let raw_is_intact = self.retained_legacy_samples_match_signatures().await?;
                if target_integrity_matches
                    && raw_is_intact
                    && self
                        .legacy_sample_coverage_is_complete(fingerprint.sample_count)
                        .await?
                {
                    self.rebuild_latest_samples().await?;
                    self.rebuild_rollups().await?;
                    return Ok(());
                }
                retained_pruned_legacy_ids = self.pruned_legacy_ids().await?;
            }
        }

        let source = db.legacy_metrics_integrity().await?;
        let fingerprint = db.legacy_metric_fingerprint().await?;
        let manifest = MigrationManifest {
            source_sample_count: source.sample_count,
            source_sample_hash: source.sample_hash.clone(),
            source_max_id: Some(fingerprint.max_id),
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
        self.rebuild_latest_samples().await?;

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
        self.rebuild_rollups().await?;
        self.set_migration_manifest(&manifest).await?;
        db.set_metrics_migration_state("complete", Some(&self.target_identity), None)
            .await?;
        Ok(())
    }

    async fn migrated_legacy_sample_count(&self) -> anyhow::Result<u64> {
        self.reader_call(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM service_resource_samples WHERE legacy_id IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0).map(|value| value as u64),
            )?)
        })
        .await
    }
}
