use std::collections::BTreeSet;

use super::SCHEMA;

pub(super) fn ensure_sample_schema(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_samples)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !columns.contains("legacy_id") {
        conn.execute_batch("DROP TABLE service_resource_samples;")?;
        conn.execute_batch(SCHEMA)?;
        return Ok(());
    }
    if !columns.contains("legacy_signature") {
        conn.execute(
            "ALTER TABLE service_resource_samples ADD COLUMN legacy_signature TEXT",
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
    Ok(())
}
