use std::collections::BTreeSet;

pub(super) fn ensure_sample_schema(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_samples)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !columns.contains("legacy_id") {
        conn.execute(
            "ALTER TABLE service_resource_samples ADD COLUMN legacy_id INTEGER",
            [],
        )?;
    }
    if !columns.contains("legacy_signature") {
        conn.execute(
            "ALTER TABLE service_resource_samples ADD COLUMN legacy_signature TEXT",
            [],
        )?;
    }
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_metrics_samples_legacy_id ON service_resource_samples(legacy_id)",
        [],
    )?;
    Ok(())
}

pub(super) fn ensure_latest_schema(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_latest_samples)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !columns.contains("legacy_source") {
        // Existing rows predate provenance tracking. Keep them as unknown until raw evidence or
        // a legacy projection identifies their source; dropping them would lose stale native data.
        conn.execute(
            "ALTER TABLE service_resource_latest_samples ADD COLUMN legacy_source INTEGER NOT NULL DEFAULT 2",
            [],
        )?;
    }
    Ok(())
}

pub(super) fn ensure_rollup_schema_columns(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_rollups)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    let needs_integrity_backfill = !columns.contains("integrity_json");
    for (name, kind) in [
        ("net_rx_rate_avg", "REAL"),
        ("net_tx_rate_avg", "REAL"),
        ("block_read_rate_avg", "REAL"),
        ("block_write_rate_avg", "REAL"),
        ("integrity_json", "TEXT NOT NULL DEFAULT ''"),
    ] {
        if !columns.contains(name) {
            conn.execute(
                &format!("ALTER TABLE service_resource_rollups ADD COLUMN {name} {kind}"),
                [],
            )?;
        }
    }
    if needs_integrity_backfill {
        conn.execute_batch(
            r#"UPDATE service_resource_rollups
               SET integrity_json = json_array(
                 service_id, resolution_seconds, bucket_start, bucket_end, sample_count, cpu_avg, cpu_peak,
                 mem_used_avg, mem_used_peak, mem_limit_avg, mem_limit_peak, net_rx_first, net_rx_last,
                 net_tx_first, net_tx_last, block_read_first, block_read_last, block_write_first,
                 block_write_last, pids_avg, pids_peak, container_count_avg, container_count_peak,
                 net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
                 net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
               );
               INSERT OR REPLACE INTO metrics_rollup_integrity (id, row_count)
               SELECT 1, COUNT(*) FROM service_resource_rollups;"#,
        )?;
    }
    Ok(())
}

pub(super) fn ensure_migration_manifest_schema(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(metrics_migration_manifest)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !columns.contains("source_max_id") {
        conn.execute(
            "ALTER TABLE metrics_migration_manifest ADD COLUMN source_max_id INTEGER",
            [],
        )?;
    }
    if !columns.contains("source_latest_count") {
        conn.execute(
            "ALTER TABLE metrics_migration_manifest ADD COLUMN source_latest_count INTEGER",
            [],
        )?;
    }
    if !columns.contains("source_latest_hash") {
        conn.execute(
            "ALTER TABLE metrics_migration_manifest ADD COLUMN source_latest_hash TEXT",
            [],
        )?;
    }
    if !columns.contains("source_raw_revision") {
        conn.execute(
            "ALTER TABLE metrics_migration_manifest ADD COLUMN source_raw_revision INTEGER",
            [],
        )?;
    }
    if !columns.contains("source_latest_revision") {
        conn.execute(
            "ALTER TABLE metrics_migration_manifest ADD COLUMN source_latest_revision INTEGER",
            [],
        )?;
    }
    Ok(())
}
