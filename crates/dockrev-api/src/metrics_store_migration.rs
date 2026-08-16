use std::path::Path;

use crate::db::Db;

use super::{
    FIVE_MINUTE_ROLLUP_RETENTION_SECONDS, MINUTE_ROLLUP_RETENTION_SECONDS, MetricsStore,
    MigrationManifest, RAW_RETENTION_SECONDS, format_time,
};

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
        let now = time::OffsetDateTime::now_utc();
        let raw_cutoff = format_time(now - time::Duration::seconds(RAW_RETENTION_SECONDS))?;
        let minute_cutoff =
            format_time(now - time::Duration::seconds(MINUTE_ROLLUP_RETENTION_SECONDS))?;
        let five_minute_cutoff =
            format_time(now - time::Duration::seconds(FIVE_MINUTE_ROLLUP_RETENTION_SECONDS))?;

        if migration_complete && let Some(manifest) = self.migration_manifest().await? {
            let fingerprint = db.legacy_metric_fingerprint().await?;
            let target_matches_source_count =
                self.migrated_legacy_sample_count().await? == fingerprint.sample_count;
            let target_integrity_matches = !target_matches_source_count
                || (
                    manifest.source_sample_count,
                    manifest.source_sample_hash.clone(),
                ) == self.migrated_legacy_integrity().await?;
            if manifest.source_sample_count == fingerprint.sample_count
                && manifest.source_max_id == Some(fingerprint.max_id)
                && target_integrity_matches
            {
                let source_coverage = db.legacy_metric_coverage(&raw_cutoff).await?;
                let source_rollups = db
                    .legacy_metric_rollup_coverage(&minute_cutoff, &five_minute_cutoff)
                    .await?;
                if self
                    .legacy_read_models_cover(&source_coverage, &source_rollups, &raw_cutoff)
                    .await?
                {
                    return Ok(());
                }
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

        let mut after_id = 0_i64;
        loop {
            let batch = db.list_legacy_metric_samples_after(after_id, 2_000).await?;
            if batch.is_empty() {
                break;
            }
            after_id = batch.last().map(|row| row.id).unwrap_or(after_id);
            self.insert_legacy_samples(batch).await?;
        }
        self.rebuild_latest_samples().await?;

        let target = self.migrated_legacy_integrity().await?;
        if (source.sample_count, source.sample_hash.clone()) != target {
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
