use anyhow::Context as _;
use rusqlite::{OptionalExtension, Transaction};

use super::MetricsStore;

const ROLLUP_INTEGRITY_JSON: &str = r#"json_array(
    service_id, resolution_seconds, bucket_start, bucket_end, sample_count, cpu_avg, cpu_peak,
    mem_used_avg, mem_used_peak, mem_limit_avg, mem_limit_peak, net_rx_first, net_rx_last,
    net_tx_first, net_tx_last, block_read_first, block_read_last, block_write_first,
    block_write_last, pids_avg, pids_peak, container_count_avg, container_count_peak,
    net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
    net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
)"#;

impl MetricsStore {
    pub(super) async fn rollups_are_intact(&self) -> anyhow::Result<bool> {
        self.reader_call(|conn| {
            let expected_count = conn
                .query_row(
                    "SELECT row_count FROM metrics_rollup_integrity WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(expected_count) = expected_count else {
                return Ok(false);
            };
            let actual_count = conn.query_row(
                "SELECT COUNT(*) FROM service_resource_rollups",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            if expected_count != actual_count {
                return Ok(false);
            }
            let integrity_query = format!(
                "SELECT EXISTS(SELECT 1 FROM service_resource_rollups WHERE integrity_json != {ROLLUP_INTEGRITY_JSON})"
            );
            let has_mismatch = conn.query_row(&integrity_query, [], |row| row.get::<_, i64>(0))?;
            Ok(has_mismatch == 0)
        })
        .await
        .context("verify metrics rollup integrity")
    }
}

pub(super) fn refresh_rollup_integrity_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        r#"INSERT OR REPLACE INTO metrics_rollup_integrity (id, row_count)
           SELECT 1, COUNT(*) FROM service_resource_rollups"#,
        [],
    )?;
    Ok(())
}
