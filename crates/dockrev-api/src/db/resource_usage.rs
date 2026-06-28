use super::*;

impl Db {
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
                    block_read_bytes: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                    block_write_bytes: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
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
    ) -> anyhow::Result<u64> {
        let older_than = older_than.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                r#"
DELETE FROM service_resource_samples
WHERE sampled_at < ?1
"#,
                params![older_than],
            )? as u64)
        })
        .await
        .context("delete expired service resource samples")
    }
}
