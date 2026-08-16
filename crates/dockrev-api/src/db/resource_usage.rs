use super::*;

#[allow(dead_code)]
impl Db {
    pub async fn metrics_migration_state(&self) -> anyhow::Result<Option<MetricsMigrationState>> {
        self.call(|conn| {
            conn.execute_batch(
                r#"CREATE TABLE IF NOT EXISTS metrics_store_migration (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    state TEXT NOT NULL,
                    target_identity TEXT,
                    last_error TEXT,
                    updated_at TEXT NOT NULL
                );"#,
            )?;
            let _ = conn.execute(
                "ALTER TABLE metrics_store_migration ADD COLUMN target_identity TEXT",
                [],
            );
            Ok(conn
                .query_row(
                    "SELECT state, target_identity FROM metrics_store_migration WHERE id = 1",
                    [],
                    |row| {
                        Ok(MetricsMigrationState {
                            state: row.get(0)?,
                            target_identity: row.get(1)?,
                        })
                    },
                )
                .optional()?)
        })
        .await
        .context("get metrics migration state")
    }

    pub async fn set_metrics_migration_state(
        &self,
        state: &str,
        target_identity: Option<&str>,
        last_error: Option<&str>,
    ) -> anyhow::Result<()> {
        let state = state.to_string();
        let target_identity = target_identity.map(ToString::to_string);
        let last_error = last_error.map(ToString::to_string);
        let updated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?;
        self.call(move |conn| {
            conn.execute_batch(
                r#"CREATE TABLE IF NOT EXISTS metrics_store_migration (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    state TEXT NOT NULL,
                    target_identity TEXT,
                    last_error TEXT,
                    updated_at TEXT NOT NULL
                );"#,
            )?;
            let _ = conn.execute("ALTER TABLE metrics_store_migration ADD COLUMN target_identity TEXT", []);
            conn.execute(
                r#"INSERT INTO metrics_store_migration (id, state, target_identity, last_error, updated_at)
                   VALUES (1, ?1, ?2, ?3, ?4)
                   ON CONFLICT(id) DO UPDATE SET
                     state = excluded.state,
                     target_identity = excluded.target_identity,
                     last_error = excluded.last_error,
                     updated_at = excluded.updated_at"#,
                params![state, target_identity, last_error, updated_at],
            )?;
            Ok(())
        })
        .await
        .context("set metrics migration state")
    }

    pub async fn list_legacy_metric_samples_after(
        &self,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<LegacyMetricSampleRow>> {
        let limit = limit.clamp(1, 10_000) as i64;
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT id, service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                   FROM service_resource_samples WHERE id > ?1 ORDER BY id ASC LIMIT ?2"#,
            )?;
            let rows = stmt.query_map(params![after_id, limit], |row| {
                Ok(LegacyMetricSampleRow {
                    id: row.get(0)?,
                    sample: ServiceResourceSampleInput {
                        service_id: row.get(1)?,
                        sampled_at: row.get(2)?,
                        cpu_percent: row.get(3)?,
                        mem_used_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                        mem_limit_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                        net_rx_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                        net_tx_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                        block_read_bytes: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                        block_write_bytes: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                        pids: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
                        container_count: row.get::<_, i64>(11)? as u32,
                    },
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list legacy metric sample batch")
    }

    pub async fn legacy_metrics_integrity(
        &self,
    ) -> anyhow::Result<crate::metrics_store::MetricsIntegrity> {
        self.call(|conn| {
            let (sample_count, sample_hash) = crate::metrics_store::stable_table_hash(
                conn,
                r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                   FROM service_resource_samples ORDER BY service_id, sampled_at, id"#,
            )?;
            let (latest_count, latest_hash) = crate::metrics_store::stable_table_hash(
                conn,
                r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                    net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count,
                    prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes
                   FROM service_resource_latest_samples ORDER BY service_id"#,
            )?;
            Ok(crate::metrics_store::MetricsIntegrity {
                sample_count,
                sample_hash,
                latest_count,
                latest_hash,
            })
        })
        .await
        .context("verify legacy metrics")
    }

    pub async fn legacy_metric_fingerprint(&self) -> anyhow::Result<LegacyMetricFingerprint> {
        self.call(|conn| {
            conn.query_row(
                "SELECT COUNT(*), COALESCE(MAX(id), 0) FROM service_resource_samples",
                [],
                |row| {
                    Ok(LegacyMetricFingerprint {
                        sample_count: row.get::<_, i64>(0)? as u64,
                        max_id: row.get(1)?,
                    })
                },
            )
            .map_err(Into::into)
        })
        .await
        .context("fingerprint legacy metrics")
    }

    pub async fn legacy_metric_coverage(
        &self,
        raw_cutoff: &str,
    ) -> anyhow::Result<Vec<LegacyMetricCoverageRow>> {
        let raw_cutoff = raw_cutoff.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT service_id,
                           COUNT(CASE WHEN sampled_at >= ?1 THEN 1 END),
                           MAX(sampled_at)
                    FROM service_resource_samples
                    GROUP BY service_id
                    ORDER BY service_id"#,
            )?;
            let rows = stmt.query_map(params![raw_cutoff], |row| {
                Ok(LegacyMetricCoverageRow {
                    service_id: row.get(0)?,
                    raw_sample_count: row.get::<_, i64>(1)? as u64,
                    latest_sampled_at: row.get(2)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("summarize legacy metrics coverage")
    }

    pub async fn legacy_metric_rollup_coverage(
        &self,
        minute_cutoff: &str,
        five_minute_cutoff: &str,
    ) -> anyhow::Result<Vec<LegacyMetricRollupCoverageRow>> {
        let minute_cutoff = minute_cutoff.to_string();
        let five_minute_cutoff = five_minute_cutoff.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT service_id, resolution_seconds, bucket_start, sample_count
                    FROM (
                      SELECT service_id,
                             60 AS resolution_seconds,
                             strftime(
                               '%Y-%m-%dT%H:%M:%SZ',
                               CAST(strftime('%s', sampled_at) AS INTEGER)
                                 - (CAST(strftime('%s', sampled_at) AS INTEGER) % 60),
                               'unixepoch'
                             ) AS bucket_start,
                             COUNT(*) AS sample_count
                      FROM service_resource_samples
                      WHERE sampled_at >= ?1
                      GROUP BY service_id, bucket_start
                      UNION ALL
                      SELECT service_id,
                             300 AS resolution_seconds,
                             strftime(
                               '%Y-%m-%dT%H:%M:%SZ',
                               CAST(strftime('%s', sampled_at) AS INTEGER)
                                 - (CAST(strftime('%s', sampled_at) AS INTEGER) % 300),
                               'unixepoch'
                             ) AS bucket_start,
                             COUNT(*) AS sample_count
                      FROM service_resource_samples
                      WHERE sampled_at >= ?2
                      GROUP BY service_id, bucket_start
                    )
                    ORDER BY service_id, resolution_seconds, bucket_start"#,
            )?;
            let rows = stmt.query_map(params![minute_cutoff, five_minute_cutoff], |row| {
                Ok(LegacyMetricRollupCoverageRow {
                    service_id: row.get(0)?,
                    resolution_seconds: row.get::<_, i64>(1)? as u32,
                    bucket_start: row.get(2)?,
                    sample_count: row.get::<_, i64>(3)? as u64,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("summarize legacy metric rollup coverage")
    }

    pub async fn legacy_metrics_latest_sampled_at(&self) -> anyhow::Result<Option<String>> {
        self.call(|conn| {
            conn.query_row(
                "SELECT MAX(sampled_at) FROM service_resource_samples",
                [],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(Into::into)
        })
        .await
        .context("get latest legacy metric sample")
    }

    #[cfg(test)]
    pub async fn insert_legacy_metric_fixture(
        &self,
        rows: &[ServiceResourceSampleInput],
    ) -> anyhow::Result<()> {
        let rows = rows.to_vec();
        self.call(move |conn| {
            conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for row in rows {
                tx.execute(
                    r#"INSERT INTO service_resource_samples (
                        service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
                    params![
                        row.service_id, row.sampled_at, row.cpu_percent,
                        row.mem_used_bytes.map(|value| value as i64), row.mem_limit_bytes.map(|value| value as i64),
                        row.net_rx_bytes.map(|value| value as i64), row.net_tx_bytes.map(|value| value as i64),
                        row.block_read_bytes.map(|value| value as i64), row.block_write_bytes.map(|value| value as i64),
                        row.pids.map(|value| value as i64), row.container_count as i64,
                    ],
                )?;
                tx.execute(
                    r#"INSERT INTO service_resource_latest_samples (
                        service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count,
                        prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL, NULL)
                    ON CONFLICT(service_id) DO UPDATE SET
                        sampled_at=excluded.sampled_at, cpu_percent=excluded.cpu_percent,
                        mem_used_bytes=excluded.mem_used_bytes, mem_limit_bytes=excluded.mem_limit_bytes,
                        net_rx_bytes=excluded.net_rx_bytes, net_tx_bytes=excluded.net_tx_bytes,
                        block_read_bytes=excluded.block_read_bytes, block_write_bytes=excluded.block_write_bytes,
                        pids=excluded.pids, container_count=excluded.container_count
                    WHERE excluded.sampled_at >= service_resource_latest_samples.sampled_at"#,
                    params![
                        row.service_id, row.sampled_at, row.cpu_percent,
                        row.mem_used_bytes.map(|value| value as i64), row.mem_limit_bytes.map(|value| value as i64),
                        row.net_rx_bytes.map(|value| value as i64), row.net_tx_bytes.map(|value| value as i64),
                        row.block_read_bytes.map(|value| value as i64), row.block_write_bytes.map(|value| value as i64),
                        row.pids.map(|value| value as i64), row.container_count as i64,
                    ],
                )?;
            }
            tx.commit()?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            Ok(())
        })
        .await
    }

    #[cfg(test)]
    pub async fn delete_legacy_metric_fixture_service(
        &self,
        service_id: &str,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM service_resource_samples WHERE service_id = ?1",
                params![service_id],
            )?;
            conn.execute(
                "DELETE FROM service_resource_latest_samples WHERE service_id = ?1",
                params![service_id],
            )?;
            Ok(())
        })
        .await
    }

    #[cfg(test)]
    pub async fn update_legacy_metric_fixture_cpu(
        &self,
        service_id: &str,
        cpu_percent: f64,
    ) -> anyhow::Result<()> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE service_resource_samples SET cpu_percent = ?2 WHERE service_id = ?1",
                params![service_id, cpu_percent],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn list_service_resource_targets(
        &self,
    ) -> anyhow::Result<Vec<ServiceResourceTarget>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sv.id,
  sv.name,
  (
    SELECT d.project
    FROM discovered_compose_projects d
    WHERE
      d.stack_id = sv.stack_id
      AND d.archived = 0
      AND d.status != 'missing'
    ORDER BY d.last_scan_at DESC
    LIMIT 1
  ) AS compose_project
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY sv.stack_id ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                let compose_project: Option<String> = row.get(2)?;
                let service_id: String = row.get(0)?;
                let service_name: String = row.get(1)?;
                Ok(compose_project.map(|project| ServiceResourceTarget {
                    service_id,
                    service_name,
                    compose_project: project,
                }))
            })?;
            let mut out = Vec::new();
            for row in rows {
                if let Some(item) = row? {
                    out.push(item);
                }
            }
            Ok(out)
        })
        .await
        .context("list service resource targets")
    }

    pub async fn list_active_service_ids_for_metrics(&self) -> anyhow::Result<BTreeSet<String>> {
        self.call(|conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT sv.id
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY sv.id ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<BTreeSet<_>, _>>()?)
        })
        .await
        .context("list active services for metrics")
    }

    pub async fn get_service_resource_target(
        &self,
        service_id: &str,
    ) -> anyhow::Result<Option<ServiceResourceTarget>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    r#"
SELECT
  sv.id,
  sv.name,
  (
    SELECT d.project
    FROM discovered_compose_projects d
    WHERE
      d.stack_id = sv.stack_id
      AND d.archived = 0
      AND d.status != 'missing'
    ORDER BY d.last_scan_at DESC
    LIMIT 1
  ) AS compose_project
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
WHERE sv.id = ?1 AND st.archived = 0 AND sv.archived = 0
"#,
                    params![service_id],
                    |row| {
                        let compose_project: Option<String> = row.get(2)?;
                        let service_id: String = row.get(0)?;
                        let service_name: String = row.get(1)?;
                        Ok(compose_project.map(|project| ServiceResourceTarget {
                            service_id,
                            service_name,
                            compose_project: project,
                        }))
                    },
                )
                .optional()?
                .flatten())
        })
        .await
        .context("get service resource target")
    }

    pub async fn insert_service_resource_samples(
        &self,
        rows: &[ServiceResourceSampleInput],
    ) -> anyhow::Result<usize> {
        let rows = rows.to_vec();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut inserted = 0usize;
            for row in rows {
                let previous = tx
                    .query_row(
                        r#"
SELECT sampled_at, net_rx_bytes, net_tx_bytes
FROM service_resource_latest_samples
WHERE service_id = ?1
"#,
                        params![row.service_id],
                        |query_row| {
                            Ok((
                                query_row.get::<_, String>(0)?,
                                query_row
                                    .get::<_, Option<i64>>(1)?
                                    .map(|value| value as u64),
                                query_row
                                    .get::<_, Option<i64>>(2)?
                                    .map(|value| value as u64),
                            ))
                        },
                    )
                    .optional()?;
                tx.execute(
                    r#"
INSERT INTO service_resource_samples (
  service_id,
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
"#,
                    params![
                        row.service_id,
                        row.sampled_at,
                        row.cpu_percent,
                        row.mem_used_bytes.map(|v| v as i64),
                        row.mem_limit_bytes.map(|v| v as i64),
                        row.net_rx_bytes.map(|v| v as i64),
                        row.net_tx_bytes.map(|v| v as i64),
                        row.block_read_bytes.map(|v| v as i64),
                        row.block_write_bytes.map(|v| v as i64),
                        row.pids.map(|v| v as i64),
                        row.container_count as i64,
                    ],
                )?;
                let (prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes) = previous
                    .map(|(sampled_at, net_rx_bytes, net_tx_bytes)| {
                        (
                            Some(sampled_at),
                            net_rx_bytes.map(|value| value as i64),
                            net_tx_bytes.map(|value| value as i64),
                        )
                    })
                    .unwrap_or((None, None, None));
                tx.execute(
                    r#"
INSERT INTO service_resource_latest_samples (
  service_id,
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count,
  prev_sampled_at,
  prev_net_rx_bytes,
  prev_net_tx_bytes
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
WHERE excluded.sampled_at >= service_resource_latest_samples.sampled_at
"#,
                    params![
                        row.service_id,
                        row.sampled_at,
                        row.cpu_percent,
                        row.mem_used_bytes.map(|v| v as i64),
                        row.mem_limit_bytes.map(|v| v as i64),
                        row.net_rx_bytes.map(|v| v as i64),
                        row.net_tx_bytes.map(|v| v as i64),
                        row.block_read_bytes.map(|v| v as i64),
                        row.block_write_bytes.map(|v| v as i64),
                        row.pids.map(|v| v as i64),
                        row.container_count as i64,
                        prev_sampled_at,
                        prev_net_rx_bytes,
                        prev_net_tx_bytes,
                    ],
                )?;
                inserted = inserted.saturating_add(1);
            }
            tx.commit()?;
            Ok(inserted)
        })
        .await
        .context("insert service resource samples")
    }

    pub async fn list_service_resource_samples_since(
        &self,
        service_id: &str,
        since: &str,
    ) -> anyhow::Result<Vec<ServiceResourceSample>> {
        let service_id = service_id.to_string();
        let since = since.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count
FROM service_resource_samples
WHERE service_id = ?1 AND sampled_at >= ?2
ORDER BY sampled_at ASC
"#,
            )?;
            let rows = stmt.query_map(params![service_id, since], |row| {
                Ok(ServiceResourceSample {
                    sampled_at: row.get(0)?,
                    cpu_percent: row.get(1)?,
                    mem_used_bytes: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                    mem_limit_bytes: row.get::<_, Option<i64>>(3)?.map(|v| v as u64),
                    net_rx_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    net_tx_bytes: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                    net_rx_rate_bps: None,
                    net_tx_rate_bps: None,
                    block_read_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                    block_write_bytes: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                    block_read_rate_bps: None,
                    block_write_rate_bps: None,
                    pids: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                    container_count: row.get::<_, i64>(9)? as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service resource samples since")
    }

    pub async fn list_service_resource_latest_samples(
        &self,
    ) -> anyhow::Result<Vec<ServiceResourceLatestSampleRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sv.id,
  latest.sampled_at,
  latest.cpu_percent,
  latest.mem_used_bytes,
  latest.mem_limit_bytes,
  latest.net_rx_bytes,
  latest.net_tx_bytes,
  latest.prev_sampled_at,
  latest.prev_net_rx_bytes,
  latest.prev_net_tx_bytes
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
LEFT JOIN service_resource_latest_samples latest
  ON latest.service_id = sv.id
WHERE st.archived = 0 AND sv.archived = 0
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(ServiceResourceLatestSampleRow {
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
        .context("list service resource latest samples")
    }

    pub async fn list_service_resource_recent_counts_since(
        &self,
        since: &str,
    ) -> anyhow::Result<Vec<ServiceResourceRecentCountRow>> {
        let since = since.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT
  sv.id,
  COUNT(s.id)
FROM services sv
JOIN stacks st ON st.id = sv.stack_id
LEFT JOIN service_resource_samples s
  ON s.service_id = sv.id
  AND s.sampled_at >= ?1
WHERE st.archived = 0 AND sv.archived = 0
GROUP BY sv.id
ORDER BY st.name ASC, sv.name ASC
"#,
            )?;
            let rows = stmt.query_map(params![since], |row| {
                Ok(ServiceResourceRecentCountRow {
                    service_id: row.get(0)?,
                    sample_count: row.get::<_, i64>(1)? as u32,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list service resource recent counts since")
    }

    pub async fn delete_expired_service_resource_samples(
        &self,
        older_than: &str,
        batch_size: u32,
    ) -> anyhow::Result<u64> {
        let older_than = older_than.to_string();
        let batch_size = batch_size.clamp(1, 10_000) as i64;
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
DELETE FROM service_resource_samples
WHERE rowid IN (
  SELECT rowid
  FROM service_resource_samples
  WHERE sampled_at < ?1
  ORDER BY sampled_at ASC
  LIMIT ?2
)
"#,
                params![older_than, batch_size],
            )? as u64)
        })
        .await
        .context("delete expired service resource samples")
    }
}
