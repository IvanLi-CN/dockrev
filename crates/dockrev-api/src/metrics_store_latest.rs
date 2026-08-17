use anyhow::Context as _;
use rusqlite::{TransactionBehavior, params};

use super::MetricsStore;

impl MetricsStore {
    pub(super) async fn sync_legacy_latest_samples(
        &self,
        rows: Vec<crate::db::LegacyMetricLatestSampleRow>,
    ) -> anyhow::Result<()> {
        self.writer_call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "DELETE FROM service_resource_latest_samples WHERE legacy_source = 1",
                [],
            )?;
            for row in rows {
                tx.execute(
                    r#"INSERT INTO service_resource_latest_samples (
                        service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                        net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids,
                        container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes,
                        legacy_source
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 1)
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
                        legacy_source=1
                    WHERE service_resource_latest_samples.legacy_source = 1
                       OR excluded.sampled_at >= service_resource_latest_samples.sampled_at"#,
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
        .context("copy legacy latest metric samples")
    }

    pub(super) async fn legacy_latest_projection_matches(
        &self,
        expected: &[crate::db::LegacyMetricLatestSampleRow],
    ) -> anyhow::Result<bool> {
        let expected = expected.to_vec();
        self.reader_call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
                          net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids,
                          container_count, prev_sampled_at, prev_net_rx_bytes, prev_net_tx_bytes,
                          legacy_source
                   FROM service_resource_latest_samples ORDER BY service_id"#,
            )?;
            let mut actual = std::collections::BTreeMap::new();
            for row in stmt.query_map([], |row| {
                Ok((
                    crate::db::LegacyMetricLatestSampleRow {
                        legacy_sample_id: None,
                        service_id: row.get(0)?,
                        sampled_at: row.get(1)?,
                        cpu_percent: row.get(2)?,
                        mem_used_bytes: row.get::<_, Option<i64>>(3)?.map(|value| value as u64),
                        mem_limit_bytes: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                        net_rx_bytes: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                        net_tx_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                        block_read_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                        block_write_bytes: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
                        pids: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                        container_count: row.get::<_, i64>(10)? as u32,
                        prev_sampled_at: row.get(11)?,
                        prev_net_rx_bytes: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
                        prev_net_tx_bytes: row.get::<_, Option<i64>>(13)?.map(|value| value as u64),
                    },
                    row.get::<_, i64>(14)?,
                ))
            })? {
                let (row, legacy_source) = row?;
                actual.insert(row.service_id.clone(), (row, legacy_source));
            }
            let mut expected = expected
                .into_iter()
                .map(|row| (row.service_id.clone(), row))
                .collect::<std::collections::BTreeMap<_, _>>();
            for (service_id, (actual, legacy_source)) in actual {
                if legacy_source == 1 {
                    let Some(mut expected_row) = expected.remove(&service_id) else {
                        return Ok(false);
                    };
                    expected_row.legacy_sample_id = None;
                    if actual != expected_row {
                        return Ok(false);
                    }
                } else if let Some(expected_row) = expected.get(&service_id) {
                    let mut expected_row = expected_row.clone();
                    expected_row.legacy_sample_id = None;
                    if actual.sampled_at < expected_row.sampled_at
                        || (actual.sampled_at == expected_row.sampled_at && actual != expected_row)
                    {
                        return Ok(false);
                    }
                    expected.remove(&service_id);
                }
            }
            Ok(expected.is_empty())
        })
        .await
    }
}
