use std::collections::BTreeSet;

use crate::db::ServiceResourceSampleInput;

use super::MetricsStore;

impl MetricsStore {
    pub(super) async fn pruned_legacy_ids(&self) -> anyhow::Result<BTreeSet<i64>> {
        self.reader_call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT legacy_id FROM metrics_migration_pruned_legacy_ids ORDER BY legacy_id",
            )?;
            Ok(stmt
                .query_map([], |row| row.get::<_, i64>(0))?
                .collect::<Result<BTreeSet<_>, _>>()?)
        })
        .await
    }

    pub(super) async fn pruned_legacy_ids_are_intact(&self) -> anyhow::Result<bool> {
        self.reader_call(|conn| {
            let expected = conn.query_row(
                "SELECT trusted_row_count, trusted_id_sum, trusted_id_square_sum FROM metrics_pruned_legacy_integrity WHERE id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )?;
            let actual = conn.query_row(
                r#"SELECT COUNT(*),
                          COALESCE(SUM(legacy_id % 65521), 0),
                          COALESCE(SUM((legacy_id % 65521) * (legacy_id % 65521)), 0)
                   FROM metrics_migration_pruned_legacy_ids"#,
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )?;
            Ok(expected == actual)
        })
        .await
    }

    pub(super) async fn legacy_sample_coverage_is_complete(
        &self,
        source_sample_count: u64,
    ) -> anyhow::Result<bool> {
        self.reader_call(move |conn| {
            let target_count = conn.query_row(
                "SELECT COUNT(*) FROM service_resource_samples WHERE legacy_id IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0).map(|value| value as u64),
            )?;
            let pruned_count = conn.query_row(
                "SELECT COUNT(*) FROM metrics_migration_pruned_legacy_ids",
                [],
                |row| row.get::<_, i64>(0).map(|value| value as u64),
            )?;
            Ok(target_count.saturating_add(pruned_count) == source_sample_count)
        })
        .await
    }

    pub(super) async fn retained_legacy_samples_match_signatures(&self) -> anyhow::Result<bool> {
        self.reader_call(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT legacy_signature, service_id, sampled_at, cpu_percent, mem_used_bytes,
                          mem_limit_bytes, net_rx_bytes, net_tx_bytes, block_read_bytes,
                          block_write_bytes, pids, container_count
                   FROM service_resource_samples
                   WHERE legacy_id IS NOT NULL
                   ORDER BY legacy_id"#,
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let signature: Option<String> = row.get(0)?;
                let sample = ServiceResourceSampleInput {
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
                };
                if signature.as_deref() != Some(&legacy_sample_signature(&sample)) {
                    return Ok(false);
                }
            }
            Ok(true)
        })
        .await
    }
}

pub(super) fn legacy_sample_signature(sample: &ServiceResourceSampleInput) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    stable_signature_bytes(&mut hash, sample.service_id.as_bytes());
    stable_signature_bytes(&mut hash, sample.sampled_at.as_bytes());
    stable_signature_bytes(&mut hash, &sample.cpu_percent.to_bits().to_le_bytes());
    for value in [
        sample.mem_used_bytes,
        sample.mem_limit_bytes,
        sample.net_rx_bytes,
        sample.net_tx_bytes,
        sample.block_read_bytes,
        sample.block_write_bytes,
        sample.pids,
    ] {
        match value {
            Some(value) => {
                stable_signature_bytes(&mut hash, &[1]);
                stable_signature_bytes(&mut hash, &value.to_le_bytes());
            }
            None => stable_signature_bytes(&mut hash, &[0]),
        }
    }
    stable_signature_bytes(&mut hash, &sample.container_count.to_le_bytes());
    format!("{hash:016x}")
}

fn stable_signature_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= *byte as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}
