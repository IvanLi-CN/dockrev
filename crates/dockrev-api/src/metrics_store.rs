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
use rollup_integrity::refresh_rollup_integrity_tx;
#[path = "metrics_store_schema.rs"]
mod schema;
use schema::{
    ensure_latest_schema, ensure_migration_manifest_schema, ensure_rollup_schema_columns,
    ensure_sample_schema,
};
#[path = "metrics_store_target_integrity.rs"]
mod target_integrity;
use target_integrity::trust_metrics_target_tx;

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

CREATE TABLE IF NOT EXISTS metrics_rollup_integrity (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  row_count INTEGER NOT NULL
);

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
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_raw_update
  AFTER UPDATE ON service_resource_samples
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_raw_delete
  AFTER DELETE ON service_resource_samples
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_insert
  AFTER INSERT ON service_resource_latest_samples
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_update
  AFTER UPDATE ON service_resource_latest_samples
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_latest_delete
  AFTER DELETE ON service_resource_latest_samples
  BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_insert
  AFTER INSERT ON service_resource_rollups
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_update
  AFTER UPDATE ON service_resource_rollups
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_rollup_delete
  AFTER DELETE ON service_resource_rollups
  BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_insert
  AFTER INSERT ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_delete
  AFTER DELETE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS metrics_target_pruned_legacy_update
  AFTER UPDATE ON metrics_migration_pruned_legacy_ids
  BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
"#;

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
                "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; {SCHEMA}"
            ))?;
            ensure_sample_schema(conn)?;
            ensure_latest_schema(conn)?;
            ensure_rollup_schema_columns(conn)?;
            ensure_migration_manifest_schema(conn)?;
            Ok(())
        })
        .await?;
        for reader in &self.readers {
            reader
                .call(|conn| {
                    conn.execute_batch("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000;")?;
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
            conn.execute(
                "DELETE FROM service_resource_samples WHERE legacy_id IS NOT NULL",
                [],
            )?;
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
        self.reader_call(move |conn| {
            if let Some(resolution_seconds) = resolution_seconds {
                let mut stmt = conn.prepare(
                    r#"SELECT bucket_end, cpu_avg, mem_used_avg, mem_limit_avg, net_rx_last,
                        net_tx_last, block_read_last, block_write_last, pids_avg, container_count_avg,
                        net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
                        cpu_peak, mem_used_peak, mem_limit_peak, pids_peak, container_count_peak,
                        net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
                    FROM service_resource_rollups
                    WHERE service_id = ?1 AND resolution_seconds = ?2 AND bucket_end >= ?3
                    ORDER BY bucket_start ASC"#,
                )?;
                let rows = stmt.query_map(params![service_id, resolution_seconds, since], |row| {
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
                })?;
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

    /// Build rollups after a full legacy copy or an integrity failure. Older rollups can outlive
    /// the raw samples needed to reconstruct them, so this only touches buckets represented by
    /// retained raw samples.
    async fn reconcile_rollups_from_raw(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            for (service_id, resolution, bucket_start) in buckets {
                rebuild_rollup_tx(&tx, &service_id, resolution, bucket_start)?;
            }
            refresh_rollup_integrity_tx(&tx)?;
            trust_metrics_target_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

#[derive(Clone)]
struct RollupAccumulator {
    bucket_start: i64,
    resolution_seconds: u32,
    count: u64,
    cpu_sum: f64,
    cpu_peak: f64,
    mem_used_sum: u128,
    mem_used_count: u64,
    mem_used_peak: Option<u64>,
    mem_limit_sum: u128,
    mem_limit_count: u64,
    mem_limit_peak: Option<u64>,
    net_rx_first: Option<u64>,
    net_rx_last: Option<u64>,
    net_tx_first: Option<u64>,
    net_tx_last: Option<u64>,
    block_read_first: Option<u64>,
    block_read_last: Option<u64>,
    block_write_first: Option<u64>,
    block_write_last: Option<u64>,
    pids_sum: u128,
    pids_count: u64,
    pids_peak: Option<u64>,
    container_sum: u64,
    container_peak: u32,
    net_rx_rate_sum: f64,
    net_rx_rate_count: u64,
    net_tx_rate_sum: f64,
    net_tx_rate_count: u64,
    block_read_rate_sum: f64,
    block_read_rate_count: u64,
    block_write_rate_sum: f64,
    block_write_rate_count: u64,
    net_rx_rate_peak: Option<f64>,
    net_tx_rate_peak: Option<f64>,
    block_read_rate_peak: Option<f64>,
    block_write_rate_peak: Option<f64>,
}

impl RollupAccumulator {
    fn new(bucket_start: i64, resolution_seconds: u32) -> Self {
        Self {
            bucket_start,
            resolution_seconds,
            count: 0,
            cpu_sum: 0.0,
            cpu_peak: 0.0,
            mem_used_sum: 0,
            mem_used_count: 0,
            mem_used_peak: None,
            mem_limit_sum: 0,
            mem_limit_count: 0,
            mem_limit_peak: None,
            net_rx_first: None,
            net_rx_last: None,
            net_tx_first: None,
            net_tx_last: None,
            block_read_first: None,
            block_read_last: None,
            block_write_first: None,
            block_write_last: None,
            pids_sum: 0,
            pids_count: 0,
            pids_peak: None,
            container_sum: 0,
            container_peak: 0,
            net_rx_rate_sum: 0.0,
            net_rx_rate_count: 0,
            net_tx_rate_sum: 0.0,
            net_tx_rate_count: 0,
            block_read_rate_sum: 0.0,
            block_read_rate_count: 0,
            block_write_rate_sum: 0.0,
            block_write_rate_count: 0,
            net_rx_rate_peak: None,
            net_tx_rate_peak: None,
            block_read_rate_peak: None,
            block_write_rate_peak: None,
        }
    }

    fn push(&mut self, sample: &ServiceResourceSample, previous: Option<&ServiceResourceSample>) {
        self.count += 1;
        self.cpu_sum += sample.cpu_percent;
        self.cpu_peak = self.cpu_peak.max(sample.cpu_percent);
        self.container_sum += sample.container_count as u64;
        self.container_peak = self.container_peak.max(sample.container_count);
        accumulate_option(
            &mut self.mem_used_sum,
            &mut self.mem_used_count,
            &mut self.mem_used_peak,
            sample.mem_used_bytes,
        );
        accumulate_option(
            &mut self.mem_limit_sum,
            &mut self.mem_limit_count,
            &mut self.mem_limit_peak,
            sample.mem_limit_bytes,
        );
        accumulate_option(
            &mut self.pids_sum,
            &mut self.pids_count,
            &mut self.pids_peak,
            sample.pids,
        );
        first_last(
            &mut self.net_rx_first,
            &mut self.net_rx_last,
            sample.net_rx_bytes,
        );
        first_last(
            &mut self.net_tx_first,
            &mut self.net_tx_last,
            sample.net_tx_bytes,
        );
        first_last(
            &mut self.block_read_first,
            &mut self.block_read_last,
            sample.block_read_bytes,
        );
        first_last(
            &mut self.block_write_first,
            &mut self.block_write_last,
            sample.block_write_bytes,
        );
        if let Some(previous) = previous {
            let dt = parse_epoch(&sample.sampled_at)
                .ok()
                .zip(parse_epoch(&previous.sampled_at).ok())
                .map(|(next, prev)| (next - prev) as f64)
                .filter(|seconds| *seconds > 0.0);
            if let Some(dt) = dt {
                accumulate_rate(
                    &mut self.net_rx_rate_sum,
                    &mut self.net_rx_rate_count,
                    &mut self.net_rx_rate_peak,
                    rate(previous.net_rx_bytes, sample.net_rx_bytes, dt),
                );
                accumulate_rate(
                    &mut self.net_tx_rate_sum,
                    &mut self.net_tx_rate_count,
                    &mut self.net_tx_rate_peak,
                    rate(previous.net_tx_bytes, sample.net_tx_bytes, dt),
                );
                accumulate_rate(
                    &mut self.block_read_rate_sum,
                    &mut self.block_read_rate_count,
                    &mut self.block_read_rate_peak,
                    rate(previous.block_read_bytes, sample.block_read_bytes, dt),
                );
                accumulate_rate(
                    &mut self.block_write_rate_sum,
                    &mut self.block_write_rate_count,
                    &mut self.block_write_rate_peak,
                    rate(previous.block_write_bytes, sample.block_write_bytes, dt),
                );
            }
        }
    }
}

fn gc_batch_tx(
    conn: &mut rusqlite::Connection,
    raw_cutoff: &str,
    minute_cutoff: &str,
    five_minute_cutoff: &str,
    active_service_ids: &BTreeSet<String>,
) -> anyhow::Result<bool> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT OR IGNORE INTO metrics_migration_pruned_legacy_ids (legacy_id) SELECT legacy_id FROM service_resource_samples WHERE rowid IN (SELECT rowid FROM service_resource_samples WHERE sampled_at < ?1 LIMIT ?2) AND legacy_id IS NOT NULL",
        params![raw_cutoff, GC_BATCH_SIZE as i64],
    )?;
    let raw_deleted = tx.execute(
        "DELETE FROM service_resource_samples WHERE rowid IN (SELECT rowid FROM service_resource_samples WHERE sampled_at < ?1 LIMIT ?2)",
        params![raw_cutoff, GC_BATCH_SIZE as i64],
    )?;
    if raw_deleted > 0 {
        trust_metrics_target_tx(&tx)?;
        tx.commit()?;
        return Ok(true);
    }
    let minute_deleted = tx.execute(
        "DELETE FROM service_resource_rollups WHERE rowid IN (SELECT rowid FROM service_resource_rollups WHERE resolution_seconds = ?1 AND bucket_end < ?2 LIMIT ?3)",
        params![MINUTE_RESOLUTION_SECONDS, minute_cutoff, GC_BATCH_SIZE as i64],
    )?;
    if minute_deleted > 0 {
        refresh_rollup_integrity_tx(&tx)?;
        trust_metrics_target_tx(&tx)?;
        tx.commit()?;
        return Ok(true);
    }
    let five_minute_deleted = tx.execute(
        "DELETE FROM service_resource_rollups WHERE rowid IN (SELECT rowid FROM service_resource_rollups WHERE resolution_seconds = ?1 AND bucket_end < ?2 LIMIT ?3)",
        params![FIVE_MINUTE_RESOLUTION_SECONDS, five_minute_cutoff, GC_BATCH_SIZE as i64],
    )?;
    if five_minute_deleted > 0 {
        refresh_rollup_integrity_tx(&tx)?;
        trust_metrics_target_tx(&tx)?;
        tx.commit()?;
        return Ok(true);
    }

    if active_service_ids.is_empty() {
        for table in [
            "service_resource_samples",
            "service_resource_latest_samples",
            "service_resource_rollups",
        ] {
            let sql =
                format!("DELETE FROM {table} WHERE rowid IN (SELECT rowid FROM {table} LIMIT ?1)");
            if table == "service_resource_samples" {
                tx.execute(
                    "INSERT OR IGNORE INTO metrics_migration_pruned_legacy_ids (legacy_id) SELECT legacy_id FROM service_resource_samples WHERE rowid IN (SELECT rowid FROM service_resource_samples LIMIT ?1) AND legacy_id IS NOT NULL",
                    params![GC_BATCH_SIZE as i64],
                )?;
            }
            if tx.execute(&sql, params![GC_BATCH_SIZE as i64])? > 0 {
                if table == "service_resource_rollups" {
                    refresh_rollup_integrity_tx(&tx)?;
                }
                trust_metrics_target_tx(&tx)?;
                tx.commit()?;
                return Ok(true);
            }
        }
    } else {
        let placeholders = std::iter::repeat_n("?", active_service_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        for table in [
            "service_resource_samples",
            "service_resource_latest_samples",
            "service_resource_rollups",
        ] {
            let limit = GC_BATCH_SIZE as i64;
            let sql = format!(
                "DELETE FROM {table} WHERE rowid IN (SELECT rowid FROM {table} WHERE service_id NOT IN ({placeholders}) LIMIT ?{})",
                active_service_ids.len() + 1
            );
            let mut values = active_service_ids
                .iter()
                .map(|value| value as &dyn rusqlite::ToSql)
                .collect::<Vec<_>>();
            values.push(&limit);
            if table == "service_resource_samples" {
                let legacy_sql = format!(
                    "INSERT OR IGNORE INTO metrics_migration_pruned_legacy_ids (legacy_id) SELECT legacy_id FROM service_resource_samples WHERE rowid IN (SELECT rowid FROM service_resource_samples WHERE service_id NOT IN ({placeholders}) LIMIT ?{}) AND legacy_id IS NOT NULL",
                    active_service_ids.len() + 1
                );
                tx.execute(&legacy_sql, values.as_slice())?;
            }
            if tx.execute(&sql, values.as_slice())? > 0 {
                if table == "service_resource_rollups" {
                    refresh_rollup_integrity_tx(&tx)?;
                }
                trust_metrics_target_tx(&tx)?;
                tx.commit()?;
                return Ok(true);
            }
        }
    }
    tx.commit()?;
    Ok(false)
}

fn write_samples_tx(
    conn: &mut rusqlite::Connection,
    rows: &[ServiceResourceSampleInput],
    rollups: bool,
) -> anyhow::Result<usize> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut inserted = 0;
    let mut touched_rollups = BTreeSet::new();
    for row in rows {
        tx.execute(
            r#"INSERT INTO service_resource_samples (
              service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes,
              net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                row.service_id,
                row.sampled_at,
                row.cpu_percent,
                row.mem_used_bytes.map(|value| value as i64),
                row.mem_limit_bytes.map(|value| value as i64),
                row.net_rx_bytes.map(|value| value as i64),
                row.net_tx_bytes.map(|value| value as i64),
                row.block_read_bytes.map(|value| value as i64),
                row.block_write_bytes.map(|value| value as i64),
                row.pids.map(|value| value as i64),
                row.container_count as i64,
            ],
        )?;
        inserted += 1;
        let previous = tx.query_row(
            "SELECT sampled_at, net_rx_bytes, net_tx_bytes, legacy_source FROM service_resource_latest_samples WHERE service_id = ?1",
            params![row.service_id],
            |current| Ok((
                current.get::<_, String>(0)?, current.get::<_, Option<i64>>(1)?.map(|value| value as u64),
                current.get::<_, Option<i64>>(2)?.map(|value| value as u64),
                current.get::<_, i64>(3)?,
            )),
        ).optional()?;
        let current_is_newer = previous
            .as_ref()
            .is_none_or(|(sampled_at, _, _, legacy_source)| {
                *legacy_source == 1 || row.sampled_at >= *sampled_at
            });
        if current_is_newer {
            tx.execute(
                r#"INSERT INTO service_resource_latest_samples (
                  service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes, net_tx_bytes,
                  block_read_bytes, block_write_bytes, pids, container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes, legacy_source
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0)
                ON CONFLICT(service_id) DO UPDATE SET
                  sampled_at=excluded.sampled_at, cpu_percent=excluded.cpu_percent, mem_used_bytes=excluded.mem_used_bytes,
                  mem_limit_bytes=excluded.mem_limit_bytes, net_rx_bytes=excluded.net_rx_bytes, net_tx_bytes=excluded.net_tx_bytes,
                  block_read_bytes=excluded.block_read_bytes, block_write_bytes=excluded.block_write_bytes, pids=excluded.pids,
                  container_count=excluded.container_count, prev_sampled_at=excluded.prev_sampled_at,
                  prev_net_rx_bytes=excluded.prev_net_rx_bytes, prev_net_tx_bytes=excluded.prev_net_tx_bytes,
                  legacy_source=0"#,
                params![
                    row.service_id, row.sampled_at, row.cpu_percent,
                    row.mem_used_bytes.map(|value| value as i64), row.mem_limit_bytes.map(|value| value as i64),
                    row.net_rx_bytes.map(|value| value as i64), row.net_tx_bytes.map(|value| value as i64),
                    row.block_read_bytes.map(|value| value as i64), row.block_write_bytes.map(|value| value as i64),
                    row.pids.map(|value| value as i64), row.container_count as i64,
                    previous.as_ref().map(|(sampled_at, _, _, _)| sampled_at),
                    previous.as_ref().and_then(|(_, value, _, _)| *value).map(|value| value as i64),
                    previous.as_ref().and_then(|(_, _, value, _)| *value).map(|value| value as i64),
                ],
            )?;
        }
        if rollups {
            for resolution in [MINUTE_RESOLUTION_SECONDS, FIVE_MINUTE_RESOLUTION_SECONDS] {
                let epoch = parse_epoch(&row.sampled_at)?;
                let start = epoch - epoch.rem_euclid(resolution as i64);
                touched_rollups.insert((row.service_id.clone(), resolution, start));
            }
        }
    }
    for (service_id, resolution, bucket_start) in touched_rollups {
        rebuild_rollup_tx(&tx, &service_id, resolution, bucket_start)?;
    }
    if rollups {
        refresh_rollup_integrity_tx(&tx)?;
    }
    trust_metrics_target_tx(&tx)?;
    tx.commit()?;
    Ok(inserted)
}

fn rebuild_rollup_tx(
    tx: &rusqlite::Transaction<'_>,
    service_id: &str,
    resolution: u32,
    bucket_start: i64,
) -> anyhow::Result<()> {
    let start = format_epoch(bucket_start)?;
    let end = format_epoch(bucket_start + resolution as i64)?;
    let previous = tx
        .query_row(
            r#"SELECT sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes,
                net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
               FROM service_resource_samples
               WHERE service_id = ?1 AND sampled_at < ?2
               ORDER BY sampled_at DESC, id DESC LIMIT 1"#,
            params![service_id, start],
            map_sample_row,
        )
        .optional()?;
    let mut stmt = tx.prepare(
        r#"SELECT sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes,
            net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
           FROM service_resource_samples
           WHERE service_id = ?1 AND sampled_at >= ?2 AND sampled_at < ?3
           ORDER BY sampled_at ASC, id ASC"#,
    )?;
    let samples = stmt
        .query_map(params![service_id, start, end], map_sample_row)?
        .collect::<Result<Vec<_>, _>>()?;
    if samples.is_empty() {
        tx.execute(
            "DELETE FROM service_resource_rollups WHERE service_id = ?1 AND resolution_seconds = ?2 AND bucket_start = ?3",
            params![service_id, resolution as i64, format_epoch(bucket_start)?],
        )?;
        return Ok(());
    }
    let mut bucket = RollupAccumulator::new(bucket_start, resolution);
    let mut previous = previous;
    for sample in samples {
        bucket.push(&sample, previous.as_ref());
        previous = Some(sample);
    }
    insert_rollup_tx(tx, service_id, resolution, &bucket)
}

#[allow(dead_code)]
fn upsert_single_rollup_tx(
    tx: &rusqlite::Transaction<'_>,
    service_id: &str,
    resolution: u32,
    accumulator: &RollupAccumulator,
) -> anyhow::Result<()> {
    let existing = tx.query_row(
        r#"SELECT sample_count, cpu_avg, cpu_peak, mem_used_avg, mem_used_peak, mem_limit_avg, mem_limit_peak,
          net_rx_first, net_rx_last, net_tx_first, net_tx_last, block_read_first, block_read_last, block_write_first,
          block_write_last, pids_avg, pids_peak, container_count_avg, container_count_peak, net_rx_rate_peak,
          net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
        FROM service_resource_rollups WHERE service_id = ?1 AND resolution_seconds = ?2 AND bucket_start = ?3"#,
        params![service_id, resolution, format_epoch(accumulator.bucket_start)?],
        ExistingRollup::from_row,
    ).optional()?;
    let merged = existing
        .map(|existing| existing.merge(accumulator))
        .unwrap_or_else(|| ExistingRollup::from_accumulator(accumulator));
    insert_rollup_tx(
        tx,
        service_id,
        resolution,
        &merged.into_accumulator(accumulator.bucket_start, resolution),
    )
}

#[allow(dead_code)]
#[derive(Clone)]
struct ExistingRollup {
    count: u64,
    cpu_avg: f64,
    cpu_peak: f64,
    mem_used_avg: Option<f64>,
    mem_used_peak: Option<u64>,
    mem_limit_avg: Option<f64>,
    mem_limit_peak: Option<u64>,
    net_rx_first: Option<u64>,
    net_rx_last: Option<u64>,
    net_tx_first: Option<u64>,
    net_tx_last: Option<u64>,
    block_read_first: Option<u64>,
    block_read_last: Option<u64>,
    block_write_first: Option<u64>,
    block_write_last: Option<u64>,
    pids_avg: Option<f64>,
    pids_peak: Option<u64>,
    container_avg: f64,
    container_peak: u32,
    net_rx_rate_peak: Option<f64>,
    net_tx_rate_peak: Option<f64>,
    block_read_rate_peak: Option<f64>,
    block_write_rate_peak: Option<f64>,
}

#[allow(dead_code)]
impl ExistingRollup {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            count: row.get::<_, i64>(0)? as u64,
            cpu_avg: row.get(1)?,
            cpu_peak: row.get(2)?,
            mem_used_avg: row.get(3)?,
            mem_used_peak: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
            mem_limit_avg: row.get(5)?,
            mem_limit_peak: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
            net_rx_first: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
            net_rx_last: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
            net_tx_first: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
            net_tx_last: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
            block_read_first: row.get::<_, Option<i64>>(11)?.map(|value| value as u64),
            block_read_last: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
            block_write_first: row.get::<_, Option<i64>>(13)?.map(|value| value as u64),
            block_write_last: row.get::<_, Option<i64>>(14)?.map(|value| value as u64),
            pids_avg: row.get(15)?,
            pids_peak: row.get::<_, Option<i64>>(16)?.map(|value| value as u64),
            container_avg: row.get(17)?,
            container_peak: row.get::<_, i64>(18)? as u32,
            net_rx_rate_peak: row.get(19)?,
            net_tx_rate_peak: row.get(20)?,
            block_read_rate_peak: row.get(21)?,
            block_write_rate_peak: row.get(22)?,
        })
    }
    fn from_accumulator(value: &RollupAccumulator) -> Self {
        Self {
            count: 0,
            cpu_avg: 0.0,
            cpu_peak: 0.0,
            mem_used_avg: None,
            mem_used_peak: None,
            mem_limit_avg: None,
            mem_limit_peak: None,
            net_rx_first: None,
            net_rx_last: None,
            net_tx_first: None,
            net_tx_last: None,
            block_read_first: None,
            block_read_last: None,
            block_write_first: None,
            block_write_last: None,
            pids_avg: None,
            pids_peak: None,
            container_avg: 0.0,
            container_peak: 0,
            net_rx_rate_peak: None,
            net_tx_rate_peak: None,
            block_read_rate_peak: None,
            block_write_rate_peak: None,
        }
        .merge(value)
    }
    fn merge(mut self, value: &RollupAccumulator) -> Self {
        let total = self.count + value.count;
        self.cpu_avg = weighted_average(
            Some(self.cpu_avg),
            self.count,
            Some(value.cpu_sum / value.count as f64),
            value.count,
        )
        .unwrap_or(0.0);
        self.cpu_peak = self.cpu_peak.max(value.cpu_peak);
        self.mem_used_avg = weighted_average(
            self.mem_used_avg,
            self.count,
            average(value.mem_used_sum, value.mem_used_count),
            value.count,
        );
        self.mem_used_peak = max_option(self.mem_used_peak, value.mem_used_peak);
        self.mem_limit_avg = weighted_average(
            self.mem_limit_avg,
            self.count,
            average(value.mem_limit_sum, value.mem_limit_count),
            value.count,
        );
        self.mem_limit_peak = max_option(self.mem_limit_peak, value.mem_limit_peak);
        self.net_rx_first = self.net_rx_first.or(value.net_rx_first);
        self.net_rx_last = value.net_rx_last.or(self.net_rx_last);
        self.net_tx_first = self.net_tx_first.or(value.net_tx_first);
        self.net_tx_last = value.net_tx_last.or(self.net_tx_last);
        self.block_read_first = self.block_read_first.or(value.block_read_first);
        self.block_read_last = value.block_read_last.or(self.block_read_last);
        self.block_write_first = self.block_write_first.or(value.block_write_first);
        self.block_write_last = value.block_write_last.or(self.block_write_last);
        self.pids_avg = weighted_average(
            self.pids_avg,
            self.count,
            average(value.pids_sum, value.pids_count),
            value.count,
        );
        self.pids_peak = max_option(self.pids_peak, value.pids_peak);
        self.container_avg = weighted_average(
            Some(self.container_avg),
            self.count,
            Some(value.container_sum as f64 / value.count as f64),
            value.count,
        )
        .unwrap_or(0.0);
        self.container_peak = self.container_peak.max(value.container_peak);
        self.net_rx_rate_peak = max_option_f64(self.net_rx_rate_peak, value.net_rx_rate_peak);
        self.net_tx_rate_peak = max_option_f64(self.net_tx_rate_peak, value.net_tx_rate_peak);
        self.block_read_rate_peak =
            max_option_f64(self.block_read_rate_peak, value.block_read_rate_peak);
        self.block_write_rate_peak =
            max_option_f64(self.block_write_rate_peak, value.block_write_rate_peak);
        self.count = total;
        self
    }
    fn into_accumulator(self, bucket_start: i64, resolution_seconds: u32) -> RollupAccumulator {
        RollupAccumulator {
            bucket_start,
            resolution_seconds,
            count: self.count,
            cpu_sum: self.cpu_avg * self.count as f64,
            cpu_peak: self.cpu_peak,
            mem_used_sum: self.mem_used_avg.unwrap_or(0.0).round() as u128 * self.count as u128,
            mem_used_count: if self.mem_used_avg.is_some() {
                self.count
            } else {
                0
            },
            mem_used_peak: self.mem_used_peak,
            mem_limit_sum: self.mem_limit_avg.unwrap_or(0.0).round() as u128 * self.count as u128,
            mem_limit_count: if self.mem_limit_avg.is_some() {
                self.count
            } else {
                0
            },
            mem_limit_peak: self.mem_limit_peak,
            net_rx_first: self.net_rx_first,
            net_rx_last: self.net_rx_last,
            net_tx_first: self.net_tx_first,
            net_tx_last: self.net_tx_last,
            block_read_first: self.block_read_first,
            block_read_last: self.block_read_last,
            block_write_first: self.block_write_first,
            block_write_last: self.block_write_last,
            pids_sum: self.pids_avg.unwrap_or(0.0).round() as u128 * self.count as u128,
            pids_count: if self.pids_avg.is_some() {
                self.count
            } else {
                0
            },
            pids_peak: self.pids_peak,
            container_sum: (self.container_avg * self.count as f64).round() as u64,
            container_peak: self.container_peak,
            net_rx_rate_sum: 0.0,
            net_rx_rate_count: 0,
            net_tx_rate_sum: 0.0,
            net_tx_rate_count: 0,
            block_read_rate_sum: 0.0,
            block_read_rate_count: 0,
            block_write_rate_sum: 0.0,
            block_write_rate_count: 0,
            net_rx_rate_peak: self.net_rx_rate_peak,
            net_tx_rate_peak: self.net_tx_rate_peak,
            block_read_rate_peak: self.block_read_rate_peak,
            block_write_rate_peak: self.block_write_rate_peak,
        }
    }
}

fn insert_rollup_tx(
    tx: &rusqlite::Transaction<'_>,
    service_id: &str,
    resolution: u32,
    bucket: &RollupAccumulator,
) -> anyhow::Result<()> {
    tx.execute(
        r#"INSERT OR REPLACE INTO service_resource_rollups (
          service_id, resolution_seconds, bucket_start, bucket_end, sample_count, cpu_avg, cpu_peak, mem_used_avg,
          mem_used_peak, mem_limit_avg, mem_limit_peak, net_rx_first, net_rx_last, net_tx_first, net_tx_last,
          block_read_first, block_read_last, block_write_first, block_write_last, pids_avg, pids_peak,
          container_count_avg, container_count_peak, net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg,
          block_write_rate_avg, net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak,
          integrity_json
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
          ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31,
          json_array(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                     ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)
        )"#,
        params![
            service_id, resolution as i64, format_epoch(bucket.bucket_start)?, format_epoch(bucket.bucket_start + bucket.resolution_seconds as i64)?, bucket.count as i64,
            bucket.cpu_sum / bucket.count as f64, bucket.cpu_peak, average(bucket.mem_used_sum, bucket.mem_used_count), bucket.mem_used_peak.map(|value| value as i64),
            average(bucket.mem_limit_sum, bucket.mem_limit_count), bucket.mem_limit_peak.map(|value| value as i64), bucket.net_rx_first.map(|value| value as i64), bucket.net_rx_last.map(|value| value as i64),
            bucket.net_tx_first.map(|value| value as i64), bucket.net_tx_last.map(|value| value as i64), bucket.block_read_first.map(|value| value as i64), bucket.block_read_last.map(|value| value as i64),
            bucket.block_write_first.map(|value| value as i64), bucket.block_write_last.map(|value| value as i64), average(bucket.pids_sum, bucket.pids_count), bucket.pids_peak.map(|value| value as i64),
            bucket.container_sum as f64 / bucket.count as f64, bucket.container_peak as i64,
            average_f64(bucket.net_rx_rate_sum, bucket.net_rx_rate_count),
            average_f64(bucket.net_tx_rate_sum, bucket.net_tx_rate_count),
            average_f64(bucket.block_read_rate_sum, bucket.block_read_rate_count),
            average_f64(bucket.block_write_rate_sum, bucket.block_write_rate_count),
            bucket.net_rx_rate_peak, bucket.net_tx_rate_peak, bucket.block_read_rate_peak, bucket.block_write_rate_peak,
        ],
    )?;
    Ok(())
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
fn format_epoch(value: i64) -> anyhow::Result<String> {
    format_time(time::OffsetDateTime::from_unix_timestamp(value)?)
}
fn format_time(value: time::OffsetDateTime) -> anyhow::Result<String> {
    Ok(value.format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
#[path = "metrics_store_tests.rs"]
mod tests;
