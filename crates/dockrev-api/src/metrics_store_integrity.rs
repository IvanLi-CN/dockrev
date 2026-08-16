#[cfg(test)]
use super::MetricsIntegrity;

#[cfg(test)]
pub(super) fn metrics_integrity_from_connection(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<MetricsIntegrity> {
    let (sample_count, sample_hash) = stable_table_hash(
        conn,
        r#"SELECT service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes, net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count FROM service_resource_samples ORDER BY service_id, sampled_at, id"#,
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
