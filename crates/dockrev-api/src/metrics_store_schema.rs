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
        // Existing rows predate provenance tracking and are treated as imported values for one
        // reconciliation pass. New sampler writes explicitly mark the row as native.
        conn.execute(
            "ALTER TABLE service_resource_latest_samples ADD COLUMN legacy_source INTEGER NOT NULL DEFAULT 1",
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
    Ok(())
}
