use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Context as _;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tokio_rusqlite::Connection;

use crate::{
    api::types::ServiceResourceSample,
    db::{Db, ServiceResourceSampleInput},
};

pub const RAW_RETENTION_SECONDS: i64 = 24 * 60 * 60;
pub const MINUTE_ROLLUP_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const FIVE_MINUTE_ROLLUP_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MINUTE_RESOLUTION_SECONDS: u32 = 60;
pub const FIVE_MINUTE_RESOLUTION_SECONDS: u32 = 5 * 60;

const READ_CONNECTION_COUNT: usize = 2;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS service_resource_samples (
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
  container_count INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY(service_id, sampled_at)
);
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
  prev_net_tx_bytes INTEGER
);

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
  PRIMARY KEY(service_id, resolution_seconds, bucket_start)
);
CREATE INDEX IF NOT EXISTS idx_metrics_rollups_lookup
  ON service_resource_rollups(service_id, resolution_seconds, bucket_start);
CREATE INDEX IF NOT EXISTS idx_metrics_rollups_expiry
  ON service_resource_rollups(resolution_seconds, bucket_end);
"#;

#[derive(Clone)]
pub struct MetricsStore {
    writer: Connection,
    readers: Vec<Connection>,
    next_reader: Arc<AtomicUsize>,
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
        for _ in 0..READ_CONNECTION_COUNT {
            readers.push(Connection::open(path).await?);
        }
        let store = Self {
            writer,
            readers,
            next_reader: Arc::new(AtomicUsize::new(0)),
        };
        store.init().await?;
        Ok(store)
    }

    async fn init(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            conn.execute_batch(&format!(
                "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; {SCHEMA}"
            ))?;
            ensure_rollup_schema_columns(conn)?;
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
        let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        self.readers[index]
            .call(f)
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))
    }

    pub async fn migrate_from_legacy(&self, db: &Db) -> anyhow::Result<()> {
        let state = db.metrics_migration_state().await?;
        if state.as_deref() == Some("complete") {
            return Ok(());
        }
        db.set_metrics_migration_state("copying", None).await?;

        let mut after_id = 0_i64;
        loop {
            let batch = db.list_legacy_metric_samples_after(after_id, 2_000).await?;
            if batch.is_empty() {
                break;
            }
            after_id = batch.last().map(|row| row.id).unwrap_or(after_id);
            self.insert_samples_without_rollups(
                &batch.into_iter().map(|row| row.sample).collect::<Vec<_>>(),
            )
            .await?;
        }
        self.upsert_latest_samples(&db.list_legacy_metric_latest_samples().await?)
            .await?;

        let source = db.legacy_metrics_integrity().await?;
        let target = self.integrity().await?;
        if source != target {
            let message =
                format!("legacy metrics verification failed: source={source:?} target={target:?}");
            db.set_metrics_migration_state("copying", Some(&message))
                .await?;
            anyhow::bail!(message);
        }
        self.rebuild_rollups().await?;
        db.set_metrics_migration_state("complete", None).await?;
        Ok(())
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

    async fn insert_samples_without_rollups(
        &self,
        rows: &[ServiceResourceSampleInput],
    ) -> anyhow::Result<usize> {
        let rows = rows.to_vec();
        self.writer_call(move |conn| write_samples_tx(conn, &rows, false))
            .await
            .context("copy legacy metric samples")
    }

    async fn upsert_latest_samples(
        &self,
        rows: &[crate::db::LegacyMetricLatestSampleRow],
    ) -> anyhow::Result<()> {
        let rows = rows.to_vec();
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for row in rows {
                tx.execute(
                    r#"INSERT INTO service_resource_latest_samples (
                        service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids,
                        container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                    ON CONFLICT(service_id) DO UPDATE SET
                        sampled_at = excluded.sampled_at,
                        cpu_percent = excluded.cpu_percent,
                        mem_used_bytes = excluded.mem_used_bytes,
                        mem_limit_bytes = excluded.mem_limit_bytes,
                        net_rx_bytes = excluded.net_rx_bytes,
                        net_tx_bytes = excluded.net_tx_bytes,
                        block_read_bytes = excluded.block_read_bytes,
                        block_write_bytes = excluded.block_write_bytes,
                        pids = excluded.pids,
                        container_count = excluded.container_count,
                        prev_sampled_at = excluded.prev_sampled_at,
                        prev_net_rx_bytes = excluded.prev_net_rx_bytes,
                        prev_net_tx_bytes = excluded.prev_net_tx_bytes
                    WHERE excluded.sampled_at >= service_resource_latest_samples.sampled_at"#,
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
                        row.prev_sampled_at,
                        row.prev_net_rx_bytes.map(|value| value as i64),
                        row.prev_net_tx_bytes.map(|value| value as i64),
                    ],
                )?;
            }
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
    ) -> anyhow::Result<Vec<MetricsRecentCountRow>> {
        let since = since.to_string();
        self.reader_call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT service_id, COUNT(*) FROM service_resource_samples WHERE sampled_at >= ?1 GROUP BY service_id ORDER BY service_id",
            )?;
            let rows = stmt.query_map(params![since], |row| {
                Ok(MetricsRecentCountRow {
                    service_id: row.get(0)?,
                    sample_count: row.get::<_, i64>(1)? as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
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
                    ORDER BY sampled_at ASC"#,
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
        let active_service_ids = active_service_ids.clone();
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute("DELETE FROM service_resource_samples WHERE sampled_at < ?1", params![raw_cutoff])?;
            tx.execute(
                "DELETE FROM service_resource_rollups WHERE resolution_seconds = ?1 AND bucket_end < ?2",
                params![MINUTE_RESOLUTION_SECONDS, minute_cutoff],
            )?;
            tx.execute(
                "DELETE FROM service_resource_rollups WHERE resolution_seconds = ?1 AND bucket_end < ?2",
                params![FIVE_MINUTE_RESOLUTION_SECONDS, five_minute_cutoff],
            )?;
            if active_service_ids.is_empty() {
                tx.execute("DELETE FROM service_resource_samples", [])?;
                tx.execute("DELETE FROM service_resource_latest_samples", [])?;
                tx.execute("DELETE FROM service_resource_rollups", [])?;
            } else {
                let placeholders = std::iter::repeat_n("?", active_service_ids.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sqls = [
                    format!("DELETE FROM service_resource_samples WHERE service_id NOT IN ({placeholders})"),
                    format!("DELETE FROM service_resource_latest_samples WHERE service_id NOT IN ({placeholders})"),
                    format!("DELETE FROM service_resource_rollups WHERE service_id NOT IN ({placeholders})"),
                ];
                let values = active_service_ids
                    .iter()
                    .map(|value| value as &dyn rusqlite::ToSql)
                    .collect::<Vec<_>>();
                for sql in sqls {
                    tx.execute(&sql, values.as_slice())?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn integrity(&self) -> anyhow::Result<MetricsIntegrity> {
        self.reader_call(metrics_integrity_from_connection).await
    }

    async fn rebuild_rollups(&self) -> anyhow::Result<()> {
        let samples = self
            .reader_call(|conn| {
                let mut stmt = conn.prepare(
                    r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                    FROM service_resource_samples ORDER BY service_id ASC, sampled_at ASC"#,
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(SampleRecord {
                        service_id: row.get(0)?,
                        sample: ServiceResourceSample {
                            sampled_at: row.get(1)?,
                            cpu_percent: row.get(2)?,
                            mem_used_bytes: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
                            mem_limit_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                            net_rx_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                            net_tx_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                            net_rx_rate_bps: None,
                            net_tx_rate_bps: None,
                            block_read_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                            block_write_bytes: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                            block_read_rate_bps: None,
                            block_write_rate_bps: None,
                            pids: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                            container_count: row.get::<_, i64>(10)? as u32,
                        },
                    })
                })?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .await?;
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute("DELETE FROM service_resource_rollups", [])?;
            let mut buckets = BTreeMap::<(String, u32, i64), RollupAccumulator>::new();
            let mut previous = BTreeMap::<String, ServiceResourceSample>::new();
            for record in samples {
                for resolution in [MINUTE_RESOLUTION_SECONDS, FIVE_MINUTE_RESOLUTION_SECONDS] {
                    let epoch = parse_epoch(&record.sample.sampled_at)?;
                    let start = epoch - epoch.rem_euclid(resolution as i64);
                    let key = (record.service_id.clone(), resolution, start);
                    buckets
                        .entry(key)
                        .or_insert_with(|| RollupAccumulator::new(start, resolution))
                        .push(&record.sample, previous.get(&record.service_id));
                }
                previous.insert(record.service_id, record.sample);
            }
            for ((service_id, resolution, _), bucket) in buckets {
                insert_rollup_tx(&tx, &service_id, resolution, &bucket)?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

fn ensure_rollup_schema_columns(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_rollups)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    for (name, kind) in [
        ("net_rx_rate_avg", "REAL"),
        ("net_tx_rate_avg", "REAL"),
        ("block_read_rate_avg", "REAL"),
        ("block_write_rate_avg", "REAL"),
    ] {
        if !columns.contains(name) {
            conn.execute(
                &format!("ALTER TABLE service_resource_rollups ADD COLUMN {name} {kind}"),
                [],
            )?;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct SampleRecord {
    service_id: String,
    sample: ServiceResourceSample,
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

fn write_samples_tx(
    conn: &mut rusqlite::Connection,
    rows: &[ServiceResourceSampleInput],
    rollups: bool,
) -> anyhow::Result<usize> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut inserted = 0;
    let mut touched_rollups = BTreeSet::new();
    for row in rows {
        let changed = tx.execute(
            r#"INSERT OR IGNORE INTO service_resource_samples (
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
        if changed == 0 {
            continue;
        }
        inserted += 1;
        let previous = tx.query_row(
            "SELECT sampled_at, net_rx_bytes, net_tx_bytes FROM service_resource_latest_samples WHERE service_id = ?1",
            params![row.service_id],
            |current| Ok((
                current.get::<_, String>(0)?, current.get::<_, Option<i64>>(1)?.map(|value| value as u64),
                current.get::<_, Option<i64>>(2)?.map(|value| value as u64),
            )),
        ).optional()?;
        let current_is_newer = previous
            .as_ref()
            .is_none_or(|(sampled_at, _, _)| row.sampled_at >= *sampled_at);
        if current_is_newer {
            tx.execute(
                r#"INSERT INTO service_resource_latest_samples (
                  service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes, net_tx_bytes,
                  block_read_bytes, block_write_bytes, pids, container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                ON CONFLICT(service_id) DO UPDATE SET
                  sampled_at=excluded.sampled_at, cpu_percent=excluded.cpu_percent, mem_used_bytes=excluded.mem_used_bytes,
                  mem_limit_bytes=excluded.mem_limit_bytes, net_rx_bytes=excluded.net_rx_bytes, net_tx_bytes=excluded.net_tx_bytes,
                  block_read_bytes=excluded.block_read_bytes, block_write_bytes=excluded.block_write_bytes, pids=excluded.pids,
                  container_count=excluded.container_count, prev_sampled_at=excluded.prev_sampled_at,
                  prev_net_rx_bytes=excluded.prev_net_rx_bytes, prev_net_tx_bytes=excluded.prev_net_tx_bytes"#,
                params![
                    row.service_id, row.sampled_at, row.cpu_percent,
                    row.mem_used_bytes.map(|value| value as i64), row.mem_limit_bytes.map(|value| value as i64),
                    row.net_rx_bytes.map(|value| value as i64), row.net_tx_bytes.map(|value| value as i64),
                    row.block_read_bytes.map(|value| value as i64), row.block_write_bytes.map(|value| value as i64),
                    row.pids.map(|value| value as i64), row.container_count as i64,
                    previous.as_ref().map(|(sampled_at, _, _)| sampled_at),
                    previous.as_ref().and_then(|(_, value, _)| *value).map(|value| value as i64),
                    previous.as_ref().and_then(|(_, _, value)| *value).map(|value| value as i64),
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
               ORDER BY sampled_at DESC LIMIT 1"#,
            params![service_id, start],
            map_sample_row,
        )
        .optional()?;
    let mut stmt = tx.prepare(
        r#"SELECT sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes,
            net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
           FROM service_resource_samples
           WHERE service_id = ?1 AND sampled_at >= ?2 AND sampled_at < ?3
           ORDER BY sampled_at ASC"#,
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
          block_write_rate_avg, net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)"#,
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

fn metrics_integrity_from_connection(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<MetricsIntegrity> {
    let (sample_count, sample_hash) = stable_table_hash(
        conn,
        r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count FROM service_resource_samples ORDER BY service_id, sampled_at"#,
    )?;
    let (latest_count, latest_hash) = stable_table_hash(
        conn,
        r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes FROM service_resource_latest_samples ORDER BY service_id"#,
    )?;
    Ok(MetricsIntegrity {
        sample_count,
        sample_hash,
        latest_count,
        latest_hash,
    })
}

pub(crate) fn stable_table_hash(
    conn: &mut rusqlite::Connection,
    query: &str,
) -> anyhow::Result<(u64, String)> {
    let mut stmt = conn.prepare(query)?;
    let column_count = stmt.column_count();
    let mut rows = stmt.query([])?;
    let mut count = 0_u64;
    let mut hash = 0xcbf29ce484222325_u64;
    while let Some(row) = rows.next()? {
        count += 1;
        for index in 0..column_count {
            let value = row.get_ref(index)?;
            stable_hash_bytes(&mut hash, format!("{value:?}").as_bytes());
            stable_hash_bytes(&mut hash, &[0xff]);
        }
        stable_hash_bytes(&mut hash, &[0xfe]);
    }
    Ok((count, format!("{hash:016x}")))
}

fn stable_hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= *byte as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dockrev-{label}-{}.sqlite3", ulid::Ulid::new()))
    }

    fn sample(
        service_id: &str,
        sampled_at: &str,
        cpu_percent: f64,
        net_rx_bytes: u64,
    ) -> ServiceResourceSampleInput {
        ServiceResourceSampleInput {
            service_id: service_id.to_string(),
            sampled_at: sampled_at.to_string(),
            cpu_percent,
            mem_used_bytes: Some(100),
            mem_limit_bytes: Some(200),
            net_rx_bytes: Some(net_rx_bytes),
            net_tx_bytes: Some(net_rx_bytes / 2),
            block_read_bytes: Some(net_rx_bytes / 4),
            block_write_bytes: Some(net_rx_bytes / 8),
            pids: Some(3),
            container_count: 1,
        }
    }

    #[test]
    fn rollup_bucket_is_stable() {
        let epoch = parse_epoch("2026-08-16T13:12:08Z").unwrap();
        assert_eq!(
            epoch - epoch.rem_euclid(60),
            parse_epoch("2026-08-16T13:12:00Z").unwrap()
        );
    }

    #[tokio::test]
    async fn metrics_store_migration_is_idempotent_and_keeps_legacy_rows() {
        let main_path = temp_path("metrics-migration-main");
        let metrics_path = temp_path("metrics-migration-target");
        let db = Db::open(&main_path).await.unwrap();
        let rows = vec![sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)];
        db.insert_legacy_metric_fixture(&rows).await.unwrap();
        let source_before = db.legacy_metrics_integrity().await.unwrap();
        let metrics = MetricsStore::open(&metrics_path).await.unwrap();

        metrics.migrate_from_legacy(&db).await.unwrap();
        assert_eq!(
            db.metrics_migration_state().await.unwrap().as_deref(),
            Some("complete")
        );
        assert_eq!(metrics.integrity().await.unwrap(), source_before);
        assert_eq!(db.legacy_metrics_integrity().await.unwrap(), source_before);

        metrics.migrate_from_legacy(&db).await.unwrap();
        assert_eq!(metrics.integrity().await.unwrap(), source_before);
    }

    #[tokio::test]
    async fn metrics_store_rollup_preserves_average_peak_and_terminal_counters() {
        let metrics = MetricsStore::open(&temp_path("metrics-rollup"))
            .await
            .unwrap();
        metrics
            .insert_samples(&[
                sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
                sample("svc-a", "2026-08-16T13:10:05Z", 30.0, 1_500),
            ])
            .await
            .unwrap();
        let history = metrics
            .history_since(
                "svc-a",
                "2026-08-16T13:00:00Z",
                Some(MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap();
        assert_eq!(history.resolution_seconds, Some(MINUTE_RESOLUTION_SECONDS));
        assert_eq!(history.samples.len(), 1);
        assert!((history.samples[0].cpu_percent - 20.0).abs() < f64::EPSILON);
        assert_eq!(history.samples[0].net_rx_bytes, Some(1_500));
        assert_eq!(history.samples[0].net_rx_rate_bps, Some(100.0));
        assert_eq!(history.peaks[0].cpu_percent, 30.0);
        assert_eq!(history.peaks[0].net_rx_rate_bps, Some(100.0));
    }
}
