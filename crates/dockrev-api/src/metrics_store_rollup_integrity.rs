use anyhow::Context as _;
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::MetricsStore;

const ROLLUP_INTEGRITY_MATCH: &str = r#"
    json_valid(integrity_json) = 1
    AND json_array_length(integrity_json) = 31
    AND json_extract(integrity_json, '$[0]') IS service_id
    AND json_extract(integrity_json, '$[1]') IS resolution_seconds
    AND json_extract(integrity_json, '$[2]') IS bucket_start
    AND json_extract(integrity_json, '$[3]') IS bucket_end
    AND json_extract(integrity_json, '$[4]') IS sample_count
    AND (
        (json_extract(integrity_json, '$[5]') IS NULL AND cpu_avg IS NULL)
        OR (json_extract(integrity_json, '$[5]') IS NOT NULL AND cpu_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[5]') - cpu_avg)
                <= 1.0e-12 * max(1.0, abs(cpu_avg)))
    )
    AND (
        (json_extract(integrity_json, '$[6]') IS NULL AND cpu_peak IS NULL)
        OR (json_extract(integrity_json, '$[6]') IS NOT NULL AND cpu_peak IS NOT NULL
            AND abs(json_extract(integrity_json, '$[6]') - cpu_peak)
                <= 1.0e-12 * max(1.0, abs(cpu_peak)))
    )
    AND (
        (json_extract(integrity_json, '$[7]') IS NULL AND mem_used_avg IS NULL)
        OR (json_extract(integrity_json, '$[7]') IS NOT NULL AND mem_used_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[7]') - mem_used_avg)
                <= 1.0e-12 * max(1.0, abs(mem_used_avg)))
    )
    AND json_extract(integrity_json, '$[8]') IS mem_used_peak
    AND (
        (json_extract(integrity_json, '$[9]') IS NULL AND mem_limit_avg IS NULL)
        OR (json_extract(integrity_json, '$[9]') IS NOT NULL AND mem_limit_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[9]') - mem_limit_avg)
                <= 1.0e-12 * max(1.0, abs(mem_limit_avg)))
    )
    AND json_extract(integrity_json, '$[10]') IS mem_limit_peak
    AND json_extract(integrity_json, '$[11]') IS net_rx_first
    AND json_extract(integrity_json, '$[12]') IS net_rx_last
    AND json_extract(integrity_json, '$[13]') IS net_tx_first
    AND json_extract(integrity_json, '$[14]') IS net_tx_last
    AND json_extract(integrity_json, '$[15]') IS block_read_first
    AND json_extract(integrity_json, '$[16]') IS block_read_last
    AND json_extract(integrity_json, '$[17]') IS block_write_first
    AND json_extract(integrity_json, '$[18]') IS block_write_last
    AND (
        (json_extract(integrity_json, '$[19]') IS NULL AND pids_avg IS NULL)
        OR (json_extract(integrity_json, '$[19]') IS NOT NULL AND pids_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[19]') - pids_avg)
                <= 1.0e-12 * max(1.0, abs(pids_avg)))
    )
    AND json_extract(integrity_json, '$[20]') IS pids_peak
    AND (
        (json_extract(integrity_json, '$[21]') IS NULL AND container_count_avg IS NULL)
        OR (json_extract(integrity_json, '$[21]') IS NOT NULL AND container_count_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[21]') - container_count_avg)
                <= 1.0e-12 * max(1.0, abs(container_count_avg)))
    )
    AND json_extract(integrity_json, '$[22]') IS container_count_peak
    AND (
        (json_extract(integrity_json, '$[23]') IS NULL AND net_rx_rate_avg IS NULL)
        OR (json_extract(integrity_json, '$[23]') IS NOT NULL AND net_rx_rate_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[23]') - net_rx_rate_avg)
                <= 1.0e-12 * max(1.0, abs(net_rx_rate_avg)))
    )
    AND (
        (json_extract(integrity_json, '$[24]') IS NULL AND net_tx_rate_avg IS NULL)
        OR (json_extract(integrity_json, '$[24]') IS NOT NULL AND net_tx_rate_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[24]') - net_tx_rate_avg)
                <= 1.0e-12 * max(1.0, abs(net_tx_rate_avg)))
    )
    AND (
        (json_extract(integrity_json, '$[25]') IS NULL AND block_read_rate_avg IS NULL)
        OR (json_extract(integrity_json, '$[25]') IS NOT NULL AND block_read_rate_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[25]') - block_read_rate_avg)
                <= 1.0e-12 * max(1.0, abs(block_read_rate_avg)))
    )
    AND (
        (json_extract(integrity_json, '$[26]') IS NULL AND block_write_rate_avg IS NULL)
        OR (json_extract(integrity_json, '$[26]') IS NOT NULL AND block_write_rate_avg IS NOT NULL
            AND abs(json_extract(integrity_json, '$[26]') - block_write_rate_avg)
                <= 1.0e-12 * max(1.0, abs(block_write_rate_avg)))
    )
    AND (
        (json_extract(integrity_json, '$[27]') IS NULL AND net_rx_rate_peak IS NULL)
        OR (json_extract(integrity_json, '$[27]') IS NOT NULL AND net_rx_rate_peak IS NOT NULL
            AND abs(json_extract(integrity_json, '$[27]') - net_rx_rate_peak)
                <= 1.0e-12 * max(1.0, abs(net_rx_rate_peak)))
    )
    AND (
        (json_extract(integrity_json, '$[28]') IS NULL AND net_tx_rate_peak IS NULL)
        OR (json_extract(integrity_json, '$[28]') IS NOT NULL AND net_tx_rate_peak IS NOT NULL
            AND abs(json_extract(integrity_json, '$[28]') - net_tx_rate_peak)
                <= 1.0e-12 * max(1.0, abs(net_tx_rate_peak)))
    )
    AND (
        (json_extract(integrity_json, '$[29]') IS NULL AND block_read_rate_peak IS NULL)
        OR (json_extract(integrity_json, '$[29]') IS NOT NULL AND block_read_rate_peak IS NOT NULL
            AND abs(json_extract(integrity_json, '$[29]') - block_read_rate_peak)
                <= 1.0e-12 * max(1.0, abs(block_read_rate_peak)))
    )
    AND (
        (json_extract(integrity_json, '$[30]') IS NULL AND block_write_rate_peak IS NULL)
        OR (json_extract(integrity_json, '$[30]') IS NOT NULL AND block_write_rate_peak IS NOT NULL
            AND abs(json_extract(integrity_json, '$[30]') - block_write_rate_peak)
                <= 1.0e-12 * max(1.0, abs(block_write_rate_peak)))
    )
"#;

impl MetricsStore {
    pub(super) async fn rollups_are_intact(&self) -> anyhow::Result<bool> {
        self.writer_call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let integrity_query = format!(
                "SELECT NOT EXISTS(SELECT 1 FROM service_resource_rollups WHERE NOT ({ROLLUP_INTEGRITY_MATCH}))"
            );
            let content_is_intact = tx.query_row(&integrity_query, [], |row| {
                row.get::<_, i64>(0).map(|value| value != 0)
            })?;
            if !content_is_intact {
                return Ok(false);
            }
            let actual_count = tx.query_row(
                "SELECT COUNT(*) FROM service_resource_rollups",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            let metadata = tx
                .query_row(
                    "SELECT row_count, trusted_row_count FROM metrics_rollup_integrity WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((expected_count, trusted_count)) = metadata else {
                return Ok(false);
            };
            if expected_count != trusted_count {
                return Ok(false);
            }
            if expected_count != actual_count {
                tx.execute(
                    r#"INSERT INTO metrics_rollup_integrity (id, row_count, trusted_row_count)
                       VALUES (1, ?1, ?1)
                       ON CONFLICT(id) DO UPDATE SET
                         row_count = excluded.row_count,
                         trusted_row_count = excluded.trusted_row_count"#,
                    params![actual_count],
                )?;
            }
            tx.commit()?;
            Ok(true)
        })
        .await
        .context("verify and repair metrics rollup integrity")
    }
}
