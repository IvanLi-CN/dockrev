use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tokio_rusqlite::Connection;

#[cfg(test)]
use crate::db::Db;
use crate::{api::types::ServiceResourceSample, db::ServiceResourceSampleInput};

#[path = "metrics_store_integrity.rs"]
mod integrity;
#[cfg(test)]
use integrity::metrics_integrity_from_connection;
pub(crate) use integrity::stable_table_hash;
#[path = "metrics_store_legacy.rs"]
mod legacy;
use legacy::legacy_sample_signature;
#[path = "metrics_store_latest.rs"]
mod latest;
#[path = "metrics_store_migration.rs"]
mod migration;
use migration::metrics_target_identity;
#[path = "metrics_store_rollup_integrity.rs"]
mod rollup_integrity;
#[path = "metrics_store_schema.rs"]
mod schema;
use schema::{
    ensure_latest_schema, ensure_migration_manifest_schema, ensure_native_integrity_schema,
    ensure_pruned_legacy_integrity_schema, ensure_rollup_schema_columns, ensure_sample_schema,
    ensure_target_write_guard_schema,
};
#[path = "metrics_store_target_integrity.rs"]
mod target_integrity;
use target_integrity::{
    adjust_native_raw_count_tx, begin_managed_metrics_write_tx, end_managed_metrics_write_tx,
    mark_native_raw_pruned_tx, trust_metrics_target_tx, trust_pruned_legacy_integrity_tx,
};
#[path = "metrics_store_rollups.rs"]
mod rollups;
use rollups::{gc_batch_tx, rebuild_rollup_tx, write_samples_tx};

pub const RAW_RETENTION_SECONDS: i64 = 24 * 60 * 60;
pub const MINUTE_ROLLUP_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const FIVE_MINUTE_ROLLUP_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MINUTE_RESOLUTION_SECONDS: u32 = 60;
pub const FIVE_MINUTE_RESOLUTION_SECONDS: u32 = 5 * 60;

const READ_CONNECTION_COUNT: usize = 2;
const GC_BATCH_SIZE: usize = 10_000;
const GC_MAX_BATCHES: usize = 10;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS service_resource_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  legacy_id INTEGER UNIQUE,
  legacy_signature TEXT,
  service_id TEXT NOT NULL,
  sampled_at TEXT NOT NULL,
  cpu_percent REAL NOT NULL,
  mem_used_bytes INTEGER,
  mem_limit_bytes INTEGER,
  net_rx_bytes INTEGER,
  net_tx_bytes INTEGER,
  block_read_bytes INTEGER,
  block_write_bytes INTEGER,
  pids INTEGER,
  container_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_metrics_samples_service_time
  ON service_resource_samples(service_id, sampled_at);
CREATE INDEX IF NOT EXISTS idx_metrics_samples_time ON service_resource_samples(sampled_at);

CREATE TABLE IF NOT EXISTS service_resource_latest_samples (
  service_id TEXT PRIMARY KEY NOT NULL,
  sampled_at TEXT NOT NULL,
  cpu_percent REAL NOT NULL,
  mem_used_bytes INTEGER,
  mem_limit_bytes INTEGER,
  net_rx_bytes INTEGER,
  net_tx_bytes INTEGER,
  block_read_bytes INTEGER,
  block_write_bytes INTEGER,
  pids INTEGER,
  container_count INTEGER NOT NULL DEFAULT 1,
  prev_sampled_at TEXT,
  prev_net_rx_bytes INTEGER,
  prev_net_tx_bytes INTEGER,
  legacy_source INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_metrics_latest_samples_sampled_at
  ON service_resource_latest_samples(sampled_at);

CREATE TABLE IF NOT EXISTS metrics_migration_manifest (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  source_sample_count INTEGER NOT NULL,
  source_sample_hash TEXT NOT NULL,
  source_max_id INTEGER,
  source_latest_count INTEGER NOT NULL,
  source_latest_hash TEXT NOT NULL,
  source_raw_revision INTEGER,
  source_latest_revision INTEGER
);

CREATE TABLE IF NOT EXISTS metrics_migration_pruned_legacy_ids (
  legacy_id INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS metrics_pruned_legacy_integrity (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  row_count INTEGER NOT NULL,
  id_sum INTEGER NOT NULL,
  id_square_sum INTEGER NOT NULL,
  trusted_row_count INTEGER NOT NULL,
  trusted_id_sum INTEGER NOT NULL,
  trusted_id_square_sum INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS metrics_rollup_integrity (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  row_count INTEGER NOT NULL,
  trusted_row_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS metrics_native_integrity (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  initialized INTEGER NOT NULL DEFAULT 0,
  raw_row_count INTEGER NOT NULL DEFAULT 0,
  latest_row_count INTEGER NOT NULL DEFAULT 0,
  trusted_raw_row_count INTEGER NOT NULL DEFAULT 0,
  trusted_latest_row_count INTEGER NOT NULL DEFAULT 0,
  has_pruned_raw INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO metrics_native_integrity (id) VALUES (1);

CREATE TABLE IF NOT EXISTS metrics_target_revision (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  raw_revision INTEGER NOT NULL,
  latest_revision INTEGER NOT NULL,
  rollup_revision INTEGER NOT NULL,
  trusted_raw_revision INTEGER NOT NULL,
  trusted_latest_revision INTEGER NOT NULL,
  trusted_rollup_revision INTEGER NOT NULL
);
INSERT OR IGNORE INTO metrics_target_revision VALUES (1, 0, 0, 0, 0, 0, 0);

CREATE TABLE IF NOT EXISTS metrics_target_write_guard (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  managed INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO metrics_target_write_guard VALUES (1, 0);

CREATE TABLE IF NOT EXISTS service_resource_rollups (
  service_id TEXT NOT NULL,
  resolution_seconds INTEGER NOT NULL,
  bucket_start TEXT NOT NULL,
  bucket_end TEXT NOT NULL,
  sample_count INTEGER NOT NULL,
  cpu_avg REAL NOT NULL,
  cpu_peak REAL NOT NULL,
  mem_used_avg REAL,
  mem_used_peak INTEGER,
  mem_limit_avg REAL,
  mem_limit_peak INTEGER,
  net_rx_first INTEGER,
  net_rx_last INTEGER,
  net_tx_first INTEGER,
  net_tx_last INTEGER,
  block_read_first INTEGER,
  block_read_last INTEGER,
  block_write_first INTEGER,
  block_write_last INTEGER,
  pids_avg REAL,
  pids_peak INTEGER,
  container_count_avg REAL NOT NULL,
  container_count_peak INTEGER NOT NULL,
  net_rx_rate_avg REAL,
  net_tx_rate_avg REAL,
  block_read_rate_avg REAL,
  block_write_rate_avg REAL,
  net_rx_rate_peak REAL,
  net_tx_rate_peak REAL,
  block_read_rate_peak REAL,
  block_write_rate_peak REAL,
  integrity_json TEXT NOT NULL DEFAULT '',
  PRIMARY KEY(service_id, resolution_seconds, bucket_start)
);
CREATE INDEX IF NOT EXISTS idx_metrics_rollups_lookup
  ON service_resource_rollups(service_id, resolution_seconds, bucket_start);
CREATE INDEX IF NOT EXISTS idx_metrics_rollups_expiry
  ON service_resource_rollups(resolution_seconds, bucket_end);

CREATE TRIGGER IF NOT EXISTS metrics_target_raw_insert
  AFTER INSERT ON service_resource_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_raw_update
  AFTER UPDATE ON service_resource_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_raw_delete
  AFTER DELETE ON service_resource_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_insert
  AFTER INSERT ON service_resource_latest_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_update
  AFTER UPDATE ON service_resource_latest_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_delete
  AFTER DELETE ON service_resource_latest_samples
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_insert
  AFTER INSERT ON service_resource_rollups
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_update
  AFTER UPDATE ON service_resource_rollups
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_delete
  AFTER DELETE ON service_resource_rollups
  WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_rollup_integrity_insert
  AFTER INSERT ON service_resource_rollups
  BEGIN UPDATE metrics_rollup_integrity SET row_count = row_count + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_rollup_integrity_delete
  AFTER DELETE ON service_resource_rollups
  BEGIN UPDATE metrics_rollup_integrity SET row_count = row_count - 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_insert
  AFTER INSERT ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_delete
  AFTER DELETE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_update
  AFTER UPDATE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_pruned_legacy_integrity_insert
  AFTER INSERT ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_pruned_legacy_integrity
    SET row_count = row_count + 1,
        id_sum = id_sum + (NEW.legacy_id % 65521),
        id_square_sum = id_square_sum + ((NEW.legacy_id % 65521) * (NEW.legacy_id % 65521))
    WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_pruned_legacy_integrity_delete
  AFTER DELETE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_pruned_legacy_integrity
    SET row_count = row_count - 1,
        id_sum = id_sum - (OLD.legacy_id % 65521),
        id_square_sum = id_square_sum - ((OLD.legacy_id % 65521) * (OLD.legacy_id % 65521))
    WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_pruned_legacy_integrity_update
  AFTER UPDATE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_pruned_legacy_integrity
    SET id_sum = id_sum - (OLD.legacy_id % 65521) + (NEW.legacy_id % 65521),
        id_square_sum = id_square_sum - ((OLD.legacy_id % 65521) * (OLD.legacy_id % 65521))
          + ((NEW.legacy_id % 65521) * (NEW.legacy_id % 65521))
    WHERE id = 1; END;
"#;

const ROLLUP_HISTORY_QUERY: &str = r#"SELECT bucket_end, cpu_avg, mem_used_avg, mem_limit_avg, net_rx_last,
    net_tx_last, block_read_last, block_write_last, pids_avg, container_count_avg,
    net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
    cpu_peak, mem_used_peak, mem_limit_peak, pids_peak, container_count_peak,
    net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
FROM service_resource_rollups
WHERE service_id = ?1 AND resolution_seconds = ?2 AND bucket_start >= ?3
ORDER BY bucket_start ASC"#;

#[derive(Clone)]
pub struct MetricsStore {
    writer: Connection,
    readers: Vec<Connection>,
    next_reader: Arc<AtomicUsize>,
    target_identity: String,
}

#[derive(Clone, Debug)]
pub struct MetricsLatestSampleRow {
    pub service_id: String,
    pub sampled_at: Option<String>,
    pub cpu_percent: Option<f64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_limit_bytes: Option<u64>,
    pub net_rx_bytes: Option<u64>,
    pub net_tx_bytes: Option<u64>,
    pub prev_sampled_at: Option<String>,
    pub prev_net_rx_bytes: Option<u64>,
    pub prev_net_tx_bytes: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct MetricsRecentCountRow {
    pub service_id: String,
    pub sample_count: u32,
}

#[derive(Clone, Debug)]
pub struct MetricsHistory {
    pub resolution_seconds: Option<u32>,
    pub samples: Vec<ServiceResourceSample>,
    pub peaks: Vec<ServiceResourcePeak>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceResourcePeak {
    pub sampled_at: String,
    pub cpu_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
    pub container_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_rx_rate_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_tx_rate_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_read_rate_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_write_rate_bps: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricsIntegrity {
    pub sample_count: u64,
    pub sample_hash: String,
    pub latest_count: u64,
    pub latest_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationManifest {
    source_sample_count: u64,
    source_sample_hash: String,
    source_max_id: Option<i64>,
    source_latest_count: Option<u64>,
    source_latest_hash: Option<String>,
    source_raw_revision: Option<u64>,
    source_latest_revision: Option<u64>,
}

impl MetricsStore {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create metrics database parent directory {}",
                    parent.display()
                )
            })?;
        }
        let writer = Connection::open(path).await?;
        let mut readers = Vec::with_capacity(READ_CONNECTION_COUNT);
        if path != Path::new(":memory:") {
            for _ in 0..READ_CONNECTION_COUNT {
                readers.push(Connection::open(path).await?);
            }
        }
        let store = Self {
            writer,
            readers,
            next_reader: Arc::new(AtomicUsize::new(0)),
            target_identity: metrics_target_identity(path),
        };
        store.init().await?;
        Ok(store)
    }

    async fn init(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            conn.execute_batch(&format!(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; {SCHEMA}"
            ))?;
            ensure_sample_schema(conn)?;
            ensure_latest_schema(conn)?;
            ensure_rollup_schema_columns(conn)?;
            ensure_migration_manifest_schema(conn)?;
            ensure_pruned_legacy_integrity_schema(conn)?;
            ensure_native_integrity_schema(conn)?;
            ensure_target_write_guard_schema(conn)?;
            Ok(())
        })
        .await?;
        for reader in &self.readers {
            reader
                .call(|conn| {
                    conn.execute_batch(
                        "PRAGMA foreign_keys = ON; PRAGMA query_only = ON; PRAGMA busy_timeout = 5000;",
                    )?;
                    Ok::<(), anyhow::Error>(())
                })
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        }
        Ok(())
    }

    async fn writer_call<R, F>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        self.writer
            .call(f)
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))
    }

    async fn reader_call<R, F>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut rusqlite::Connection) -> anyhow::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        if self.readers.is_empty() {
            return self
                .writer
                .call(f)
                .await
                .map_err(|err| anyhow::anyhow!(err.to_string()));
        }
        let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        self.readers[index]
            .call(f)
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))
    }

    pub async fn insert_samples(
        &self,
        rows: &[ServiceResourceSampleInput],
    ) -> anyhow::Result<usize> {
        let rows = rows.to_vec();
        self.writer_call(move |conn| write_samples_tx(conn, &rows, true))
            .await
            .context("insert metric samples")
    }

    async fn insert_legacy_samples(
        &self,
        rows: Vec<crate::db::LegacyMetricSampleRow>,
    ) -> anyhow::Result<usize> {
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin_managed_metrics_write_tx(&tx)?;
            let mut inserted = 0;
            for row in rows {
                inserted += tx.execute(
                    r#"INSERT INTO service_resource_samples (
                        legacy_id, legacy_signature, service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                    ON CONFLICT(legacy_id) DO NOTHING"#,
                    params![
                        row.id,
                        legacy_sample_signature(&row.sample),
                        row.sample.service_id,
                        row.sample.sampled_at,
                        row.sample.cpu_percent,
                        row.sample.mem_used_bytes.map(|value| value as i64),
                        row.sample.mem_limit_bytes.map(|value| value as i64),
                        row.sample.net_rx_bytes.map(|value| value as i64),
                        row.sample.net_tx_bytes.map(|value| value as i64),
                        row.sample.block_read_bytes.map(|value| value as i64),
                        row.sample.block_write_bytes.map(|value| value as i64),
                        row.sample.pids.map(|value| value as i64),
                        row.sample.container_count as i64,
                    ],
                )?;
            }
            end_managed_metrics_write_tx(&tx)?;
            tx.commit()?;
            Ok(inserted)
        })
        .await
        .context("copy legacy metric samples")
    }

    async fn migrated_legacy_integrity(&self) -> anyhow::Result<(u64, String)> {
        self.reader_call(move |conn| {
            stable_table_hash(
                conn,
                r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                   FROM service_resource_samples
                   WHERE legacy_id IS NOT NULL
                   ORDER BY service_id, sampled_at, legacy_id"#,
            )
        })
        .await
    }

    async fn clear_legacy_samples(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin_managed_metrics_write_tx(&tx)?;
            tx.execute(
                "DELETE FROM service_resource_samples WHERE legacy_id IS NOT NULL",
                [],
            )?;
            end_managed_metrics_write_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn migration_manifest(&self) -> anyhow::Result<Option<MigrationManifest>> {
        self.reader_call(|conn| {
            conn.query_row(
                "SELECT source_sample_count, source_sample_hash, source_max_id, source_latest_count, source_latest_hash, source_raw_revision, source_latest_revision FROM metrics_migration_manifest WHERE id = 1",
                [],
                |row| {
                    Ok(MigrationManifest {
                        source_sample_count: row.get::<_, i64>(0)? as u64,
                        source_sample_hash: row.get(1)?,
                        source_max_id: row.get(2)?,
                        source_latest_count: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
                        source_latest_hash: row.get(4)?,
                        source_raw_revision: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                        source_latest_revision: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
        .await
    }

    async fn set_migration_manifest(&self, manifest: &MigrationManifest) -> anyhow::Result<()> {
        let manifest = manifest.clone();
        self.writer_call(move |conn| {
            conn.execute(
                r#"INSERT INTO metrics_migration_manifest (
                       id, source_sample_count, source_sample_hash, source_max_id,
                       source_latest_count, source_latest_hash, source_raw_revision, source_latest_revision
                   ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
                   ON CONFLICT(id) DO UPDATE SET source_sample_count=excluded.source_sample_count,
                     source_sample_hash=excluded.source_sample_hash,
                     source_max_id=excluded.source_max_id,
                     source_latest_count=excluded.source_latest_count,
                     source_latest_hash=excluded.source_latest_hash,
                     source_raw_revision=excluded.source_raw_revision,
                     source_latest_revision=excluded.source_latest_revision"#,
                params![
                    manifest.source_sample_count as i64,
                    manifest.source_sample_hash,
                    manifest.source_max_id,
                    manifest.source_latest_count.map(|value| value as i64),
                    manifest.source_latest_hash,
                    manifest.source_raw_revision.map(|value| value as i64),
                    manifest.source_latest_revision.map(|value| value as i64),
                ],
            )?;
            Ok(())
        })
        .await
    }

    /// Reconcile only services that still have raw samples. `latest` intentionally outlives raw
    /// retention, so clearing the table here would make a restart erase a valid stale summary.
    async fn reconcile_latest_samples_from_raw(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin_managed_metrics_write_tx(&tx)?;
            tx.execute_batch(
                r#"INSERT INTO service_resource_latest_samples (
                    service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids,
                    container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes,
                    legacy_source
                )
                SELECT
                    latest.service_id,
                    latest.sampled_at,
                    latest.cpu_percent,
                    latest.mem_used_bytes,
                    latest.mem_limit_bytes,
                    latest.net_rx_bytes,
                    latest.net_tx_bytes,
                    latest.block_read_bytes,
                    latest.block_write_bytes,
                    latest.pids,
                    latest.container_count,
                    previous.sampled_at,
                    previous.net_rx_bytes,
                    previous.net_tx_bytes,
                    CASE WHEN latest.legacy_id IS NULL THEN 0 ELSE 1 END
                FROM service_resource_samples latest
                LEFT JOIN service_resource_samples previous ON previous.id = (
                    SELECT candidate.id
                    FROM service_resource_samples candidate
                    WHERE candidate.service_id = latest.service_id
                      AND (candidate.sampled_at < latest.sampled_at
                        OR (candidate.sampled_at = latest.sampled_at AND candidate.id < latest.id))
                    ORDER BY candidate.sampled_at DESC, candidate.id DESC
                    LIMIT 1
                )
                WHERE latest.id = (
                    SELECT candidate.id
                    FROM service_resource_samples candidate
                    WHERE candidate.service_id = latest.service_id
                    ORDER BY candidate.sampled_at DESC, candidate.id DESC
                    LIMIT 1
                )
                ON CONFLICT(service_id) DO UPDATE SET
                    sampled_at=excluded.sampled_at,
                    cpu_percent=excluded.cpu_percent,
                    mem_used_bytes=excluded.mem_used_bytes,
                    mem_limit_bytes=excluded.mem_limit_bytes,
                    net_rx_bytes=excluded.net_rx_bytes,
                    net_tx_bytes=excluded.net_tx_bytes,
                    block_read_bytes=excluded.block_read_bytes,
                    block_write_bytes=excluded.block_write_bytes,
                    pids=excluded.pids,
                    container_count=excluded.container_count,
                    prev_sampled_at=excluded.prev_sampled_at,
                    prev_net_rx_bytes=excluded.prev_net_rx_bytes,
                    prev_net_tx_bytes=excluded.prev_net_tx_bytes,
                    legacy_source=excluded.legacy_source
                WHERE service_resource_latest_samples.legacy_source = 1
                   OR excluded.sampled_at >= service_resource_latest_samples.sampled_at;"#,
            )?;
            end_managed_metrics_write_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn list_latest_samples(&self) -> anyhow::Result<Vec<MetricsLatestSampleRow>> {
        self.reader_call(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes
                   FROM service_resource_latest_samples ORDER BY service_id"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(MetricsLatestSampleRow {
                    service_id: row.get(0)?,
                    sampled_at: row.get(1)?,
                    cpu_percent: row.get(2)?,
                    mem_used_bytes: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
                    mem_limit_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                    net_rx_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                    net_tx_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                    prev_sampled_at: row.get(7)?,
                    prev_net_rx_bytes: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                    prev_net_tx_bytes: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn list_recent_counts_since(
        &self,
        since: &str,
        resolution_seconds: Option<u32>,
    ) -> anyhow::Result<Vec<MetricsRecentCountRow>> {
        let since = since.to_string();
        self.reader_call(move |conn| {
            let mut rows = if let Some(resolution_seconds) = resolution_seconds {
                let mut stmt = conn.prepare(
                    "SELECT service_id, SUM(sample_count) FROM service_resource_rollups WHERE resolution_seconds = ?1 AND bucket_end >= ?2 GROUP BY service_id ORDER BY service_id",
                )?;
                stmt.query_map(params![resolution_seconds as i64, since], |row| {
                    Ok(MetricsRecentCountRow {
                        service_id: row.get(0)?,
                        sample_count: row.get::<_, i64>(1)? as u32,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
            } else {
                let mut stmt = conn.prepare(
                    "SELECT service_id, COUNT(*) FROM service_resource_samples WHERE sampled_at >= ?1 GROUP BY service_id ORDER BY service_id",
                )?;
                stmt.query_map(params![since], |row| {
                    Ok(MetricsRecentCountRow {
                        service_id: row.get(0)?,
                        sample_count: row.get::<_, i64>(1)? as u32,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
            };
            rows.sort_by(|left, right| left.service_id.cmp(&right.service_id));
            Ok(rows)
        })
        .await
    }

    pub async fn history_since(
        &self,
        service_id: &str,
        since: &str,
        resolution_seconds: Option<u32>,
    ) -> anyhow::Result<MetricsHistory> {
        let service_id = service_id.to_string();
        let since = since.to_string();
        let bucket_start_since = resolution_seconds
            .map(|resolution_seconds| rollup_bucket_start_cutoff(&since, resolution_seconds))
            .transpose()?;
        self.reader_call(move |conn| {
            if let Some(resolution_seconds) = resolution_seconds {
                let mut stmt = conn.prepare(ROLLUP_HISTORY_QUERY)?;
                let rows = stmt.query_map(
                    params![service_id, resolution_seconds, bucket_start_since],
                    |row| {
                    let sampled_at: String = row.get(0)?;
                    Ok((
                        ServiceResourceSample {
                            sampled_at: sampled_at.clone(),
                            cpu_percent: row.get(1)?,
                            mem_used_bytes: round_opt(row.get(2)?),
                            mem_limit_bytes: round_opt(row.get(3)?),
                            net_rx_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                            net_tx_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                            net_rx_rate_bps: row.get(10)?,
                            net_tx_rate_bps: row.get(11)?,
                            block_read_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                            block_write_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                            block_read_rate_bps: row.get(12)?,
                            block_write_rate_bps: row.get(13)?,
                            pids: round_opt(row.get(8)?),
                            container_count: round_opt(row.get(9)?).unwrap_or(0) as u32,
                        },
                        ServiceResourcePeak {
                            sampled_at,
                            cpu_percent: row.get(14)?,
                            mem_used_bytes: row.get::<_, Option<i64>>(15)?.map(|value| value as u64),
                            mem_limit_bytes: row.get::<_, Option<i64>>(16)?.map(|value| value as u64),
                            pids: row.get::<_, Option<i64>>(17)?.map(|value| value as u64),
                            container_count: row.get::<_, i64>(18)? as u32,
                            net_rx_rate_bps: row.get(19)?,
                            net_tx_rate_bps: row.get(20)?,
                            block_read_rate_bps: row.get(21)?,
                            block_write_rate_bps: row.get(22)?,
                        },
                    ))
                    },
                )?;
                let rows = rows.collect::<Result<Vec<_>, _>>()?;
                Ok(MetricsHistory {
                    resolution_seconds: Some(resolution_seconds),
                    samples: rows.iter().map(|(sample, _)| sample.clone()).collect(),
                    peaks: rows.into_iter().map(|(_, peak)| peak).collect(),
                })
            } else {
                let mut stmt = conn.prepare(
                    r#"SELECT sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes,
                        net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                    FROM service_resource_samples WHERE service_id = ?1 AND sampled_at >= ?2
                    ORDER BY sampled_at ASC, id ASC"#,
                )?;
                let samples = stmt
                    .query_map(params![service_id, since], map_sample_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(MetricsHistory {
                    resolution_seconds: None,
                    samples,
                    peaks: Vec::new(),
                })
            }
        })
        .await
    }

    pub async fn gc(&self, active_service_ids: &BTreeSet<String>) -> anyhow::Result<()> {
        let now = time::OffsetDateTime::now_utc();
        let raw_cutoff = format_time(now - time::Duration::seconds(RAW_RETENTION_SECONDS))?;
        let minute_cutoff =
            format_time(now - time::Duration::seconds(MINUTE_ROLLUP_RETENTION_SECONDS))?;
        let five_minute_cutoff =
            format_time(now - time::Duration::seconds(FIVE_MINUTE_ROLLUP_RETENTION_SECONDS))?;
        for _ in 0..GC_MAX_BATCHES {
            let raw_cutoff = raw_cutoff.clone();
            let minute_cutoff = minute_cutoff.clone();
            let five_minute_cutoff = five_minute_cutoff.clone();
            let active_service_ids = active_service_ids.clone();
            let deleted = self
                .writer_call(move |conn| {
                    gc_batch_tx(
                        conn,
                        &raw_cutoff,
                        &minute_cutoff,
                        &five_minute_cutoff,
                        &active_service_ids,
                    )
                })
                .await?;
            if !deleted {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn integrity(&self) -> anyhow::Result<MetricsIntegrity> {
        self.reader_call(metrics_integrity_from_connection).await
    }

    /// Build rollups after a full legacy copy or an integrity failure. Older native rollups can
    /// outlive raw retention, so only buckets represented by retained raw samples or by the
    /// pre-copy legacy projection are touched.
    async fn reconcile_rollups_from_raw(
        &self,
        previous_legacy_buckets: &BTreeSet<(String, u32, i64)>,
    ) -> anyhow::Result<()> {
        let previous_legacy_buckets = previous_legacy_buckets.clone();
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin_managed_metrics_write_tx(&tx)?;
            let buckets = {
                let mut stmt = tx.prepare(
                    "SELECT service_id, sampled_at FROM service_resource_samples ORDER BY service_id ASC, sampled_at ASC, id ASC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                let mut buckets = BTreeSet::new();
                for row in rows {
                    let (service_id, sampled_at) = row?;
                    let epoch = parse_epoch(&sampled_at)?;
                    for resolution in [MINUTE_RESOLUTION_SECONDS, FIVE_MINUTE_RESOLUTION_SECONDS] {
                        let start = epoch - epoch.rem_euclid(resolution as i64);
                        buckets.insert((service_id.clone(), resolution, start));
                    }
                }
                buckets
            };
            let mut buckets = buckets;
            buckets.extend(previous_legacy_buckets);
            for (service_id, resolution, bucket_start) in buckets {
                rebuild_rollup_tx(&tx, &service_id, resolution, bucket_start)?;
            }
            end_managed_metrics_write_tx(&tx)?;
            trust_metrics_target_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    async fn legacy_rollup_buckets(&self) -> anyhow::Result<BTreeSet<(String, u32, i64)>> {
        self.reader_call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT service_id, sampled_at FROM service_resource_samples WHERE legacy_id IS NOT NULL ORDER BY service_id ASC, sampled_at ASC, id ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut buckets = BTreeSet::new();
            for row in rows {
                let (service_id, sampled_at) = row?;
                let epoch = parse_epoch(&sampled_at)?;
                for resolution in [MINUTE_RESOLUTION_SECONDS, FIVE_MINUTE_RESOLUTION_SECONDS] {
                    buckets.insert((
                        service_id.clone(),
                        resolution,
                        epoch - epoch.rem_euclid(resolution as i64),
                    ));
                }
            }
            Ok(buckets)
        })
        .await
    }
}

fn map_sample_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServiceResourceSample> {
    Ok(ServiceResourceSample {
        sampled_at: row.get(0)?,
        cpu_percent: row.get(1)?,
        mem_used_bytes: row.get::<_, Option<i64>>(2)?.map(|value| value as u64),
        mem_limit_bytes: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
        net_rx_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
        net_tx_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
        net_rx_rate_bps: None,
        net_tx_rate_bps: None,
        block_read_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
        block_write_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
        block_read_rate_bps: None,
        block_write_rate_bps: None,
        pids: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
        container_count: row.get::<_, i64>(9)? as u32,
    })
}

#[allow(dead_code)]
fn sample_from_input(input: &ServiceResourceSampleInput) -> ServiceResourceSample {
    ServiceResourceSample {
        sampled_at: input.sampled_at.clone(),
        cpu_percent: input.cpu_percent,
        mem_used_bytes: input.mem_used_bytes,
        mem_limit_bytes: input.mem_limit_bytes,
        net_rx_bytes: input.net_rx_bytes,
        net_tx_bytes: input.net_tx_bytes,
        net_rx_rate_bps: None,
        net_tx_rate_bps: None,
        block_read_bytes: input.block_read_bytes,
        block_write_bytes: input.block_write_bytes,
        block_read_rate_bps: None,
        block_write_rate_bps: None,
        pids: input.pids,
        container_count: input.container_count,
    }
}
fn round_opt(value: Option<f64>) -> Option<u64> {
    value.map(|value| value.round().max(0.0) as u64)
}
fn average(sum: u128, count: u64) -> Option<f64> {
    (count > 0).then_some(sum as f64 / count as f64)
}
fn average_f64(sum: f64, count: u64) -> Option<f64> {
    (count > 0).then_some(sum / count as f64)
}
fn weighted_average(a: Option<f64>, a_count: u64, b: Option<f64>, b_count: u64) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => {
            Some((a * a_count as f64 + b * b_count as f64) / (a_count + b_count) as f64)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
fn max_option(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
fn max_option_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}
fn first_last(first: &mut Option<u64>, last: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        if first.is_none() {
            *first = Some(value);
        }
        *last = Some(value);
    }
}
fn accumulate_option(sum: &mut u128, count: &mut u64, peak: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *sum += value as u128;
        *count += 1;
        *peak = max_option(*peak, Some(value));
    }
}
fn rate(previous: Option<u64>, current: Option<u64>, seconds: f64) -> Option<f64> {
    previous
        .zip(current)
        .filter(|(previous, current)| current >= previous)
        .map(|(previous, current)| (current - previous) as f64 / seconds)
}
fn accumulate_rate(sum: &mut f64, count: &mut u64, peak: &mut Option<f64>, candidate: Option<f64>) {
    if let Some(candidate) = candidate {
        *sum += candidate;
        *count += 1;
        *peak = max_option_f64(*peak, Some(candidate));
    }
}
fn parse_epoch(value: &str) -> anyhow::Result<i64> {
    Ok(
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)?
            .unix_timestamp(),
    )
}
fn rollup_bucket_start_cutoff(since: &str, resolution_seconds: u32) -> anyhow::Result<String> {
    let cutoff =
        time::OffsetDateTime::parse(since, &time::format_description::well_known::Rfc3339)?
            - time::Duration::seconds(resolution_seconds as i64);
    let bucket_start = cutoff.unix_timestamp() + i64::from(cutoff.nanosecond() > 0);
    format_epoch(bucket_start)
}
fn format_epoch(value: i64) -> anyhow::Result<String> {
    format_time(time::OffsetDateTime::from_unix_timestamp(value)?)
}
fn format_time(value: time::OffsetDateTime) -> anyhow::Result<String> {
    Ok(value.format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
#[path = "metrics_store_tests.rs"]
mod tests;
