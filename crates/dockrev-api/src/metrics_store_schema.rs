use std::collections::BTreeSet;

use rusqlite::OptionalExtension as _;

pub(super) fn ensure_sample_schema(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(service_resource_samples)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    drop(stmt);
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
    drop(stmt);
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
    let mut metadata_stmt = conn.prepare("PRAGMA table_info(metrics_rollup_integrity)")?;
    let metadata_columns = metadata_stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    drop(metadata_stmt);
    let added_trusted_count = !metadata_columns.contains("trusted_row_count");
    if added_trusted_count {
        conn.execute(
            "ALTER TABLE metrics_rollup_integrity ADD COLUMN trusted_row_count INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    let metadata_exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM metrics_rollup_integrity WHERE id = 1)",
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if needs_integrity_backfill || !metadata_exists {
        let (row_count, empty_count, mismatch_count) = conn.query_row(
            r#"SELECT
                 COUNT(*),
                 COALESCE(SUM(CASE WHEN integrity_json = '' THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN integrity_json != json_array(
                   service_id, resolution_seconds, bucket_start, bucket_end, sample_count, cpu_avg, cpu_peak,
                   mem_used_avg, mem_used_peak, mem_limit_avg, mem_limit_peak, net_rx_first, net_rx_last,
                   net_tx_first, net_tx_last, block_read_first, block_read_last, block_write_first,
                   block_write_last, pids_avg, pids_peak, container_count_avg, container_count_peak,
                   net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
                   net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
                 ) THEN 1 ELSE 0 END), 0)
               FROM service_resource_rollups"#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?;
        if empty_count == row_count || mismatch_count == 0 {
            let tx = conn.transaction()?;
            if empty_count > 0 {
                tx.execute(
                    r#"UPDATE service_resource_rollups
                       SET integrity_json = json_array(
                         service_id, resolution_seconds, bucket_start, bucket_end, sample_count, cpu_avg, cpu_peak,
                         mem_used_avg, mem_used_peak, mem_limit_avg, mem_limit_peak, net_rx_first, net_rx_last,
                         net_tx_first, net_tx_last, block_read_first, block_read_last, block_write_first,
                         block_write_last, pids_avg, pids_peak, container_count_avg, container_count_peak,
                         net_rx_rate_avg, net_tx_rate_avg, block_read_rate_avg, block_write_rate_avg,
                         net_rx_rate_peak, net_tx_rate_peak, block_read_rate_peak, block_write_rate_peak
                       )
                       WHERE integrity_json = ''"#,
                    [],
                )?;
            }
            tx.execute(
                r#"INSERT OR REPLACE INTO metrics_rollup_integrity (id, row_count, trusted_row_count)
                   SELECT 1, COUNT(*), COUNT(*) FROM service_resource_rollups"#,
                [],
            )?;
            tx.commit()?;
        }
    } else if added_trusted_count {
        conn.execute(
            r#"UPDATE metrics_rollup_integrity
               SET trusted_row_count = row_count
               WHERE id = 1"#,
            [],
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

pub(super) fn ensure_pruned_legacy_integrity_schema(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(metrics_pruned_legacy_integrity)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    let needs_trusted_backfill = !columns.contains("trusted_row_count");
    for name in [
        "trusted_row_count",
        "trusted_id_sum",
        "trusted_id_square_sum",
    ] {
        if !columns.contains(name) {
            conn.execute(
                &format!(
                    "ALTER TABLE metrics_pruned_legacy_integrity ADD COLUMN {name} INTEGER NOT NULL DEFAULT 0"
                ),
                [],
            )?;
        }
    }
    let metadata_count = conn.query_row(
        "SELECT COUNT(*) FROM metrics_pruned_legacy_integrity WHERE id = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if metadata_count == 0 {
        conn.execute(
            r#"INSERT INTO metrics_pruned_legacy_integrity (
                   id, row_count, id_sum, id_square_sum,
                   trusted_row_count, trusted_id_sum, trusted_id_square_sum
               )
               SELECT 1,
                      COUNT(*),
                      COALESCE(SUM(legacy_id % 65521), 0),
                      COALESCE(SUM((legacy_id % 65521) * (legacy_id % 65521)), 0),
                      COUNT(*),
                      COALESCE(SUM(legacy_id % 65521), 0),
                      COALESCE(SUM((legacy_id % 65521) * (legacy_id % 65521)), 0)
               FROM metrics_migration_pruned_legacy_ids"#,
            [],
        )?;
    } else if needs_trusted_backfill {
        conn.execute(
            r#"UPDATE metrics_pruned_legacy_integrity
               SET trusted_row_count = row_count,
                   trusted_id_sum = id_sum,
                   trusted_id_square_sum = id_square_sum
               WHERE id = 1"#,
            [],
        )?;
    }
    Ok(())
}

pub(super) fn ensure_native_integrity_schema(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let raw_triggers_exist = conn.query_row(
        r#"SELECT EXISTS(
             SELECT 1 FROM sqlite_master
             WHERE type = 'trigger' AND name = 'metrics_native_raw_insert'
           )"#,
        [],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if raw_triggers_exist {
        conn.execute_batch(
            r#"DROP TRIGGER metrics_native_raw_insert;
               DROP TRIGGER IF EXISTS metrics_native_raw_delete;
               DROP TRIGGER IF EXISTS metrics_native_raw_update_to_legacy;
               DROP TRIGGER IF EXISTS metrics_native_raw_update_to_native;"#,
        )?;
    }
    conn.execute_batch(
        r#"CREATE TRIGGER IF NOT EXISTS metrics_native_latest_insert
              AFTER INSERT ON service_resource_latest_samples WHEN NEW.legacy_source != 1
              BEGIN UPDATE metrics_native_integrity SET latest_row_count = latest_row_count + 1 WHERE id = 1; END;
           CREATE TRIGGER IF NOT EXISTS metrics_native_latest_delete
              AFTER DELETE ON service_resource_latest_samples WHEN OLD.legacy_source != 1
              BEGIN UPDATE metrics_native_integrity SET latest_row_count = latest_row_count - 1 WHERE id = 1; END;
           CREATE TRIGGER IF NOT EXISTS metrics_native_latest_update_to_legacy
              AFTER UPDATE ON service_resource_latest_samples
              WHEN OLD.legacy_source != 1 AND NEW.legacy_source = 1
              BEGIN UPDATE metrics_native_integrity SET latest_row_count = latest_row_count - 1 WHERE id = 1; END;
           CREATE TRIGGER IF NOT EXISTS metrics_native_latest_update_to_native
              AFTER UPDATE ON service_resource_latest_samples
              WHEN OLD.legacy_source = 1 AND NEW.legacy_source != 1
              BEGIN UPDATE metrics_native_integrity SET latest_row_count = latest_row_count + 1 WHERE id = 1; END;"#,
    )?;
    let initialized = conn.query_row(
        "SELECT initialized FROM metrics_native_integrity WHERE id = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if initialized != 0 {
        return Ok(());
    }
    conn.execute(
        r#"UPDATE metrics_native_integrity
           SET initialized = 1,
               raw_row_count = (
                 SELECT COUNT(*) FROM service_resource_samples WHERE legacy_id IS NULL
               ),
               latest_row_count = (
                 SELECT COUNT(*) FROM service_resource_latest_samples WHERE legacy_source != 1
               ),
               trusted_raw_row_count = (
                 SELECT COUNT(*) FROM service_resource_samples WHERE legacy_id IS NULL
               ),
               trusted_latest_row_count = (
                 SELECT COUNT(*) FROM service_resource_latest_samples WHERE legacy_source != 1
               ),
               has_pruned_raw = EXISTS(
                 SELECT 1 FROM service_resource_rollups AS rollup
                 WHERE NOT EXISTS (
                   SELECT 1 FROM service_resource_samples AS sample
                   WHERE sample.service_id = rollup.service_id
                     AND sample.sampled_at >= rollup.bucket_start
                     AND sample.sampled_at < rollup.bucket_end
                 )
               )
           WHERE id = 1"#,
        [],
    )?;
    Ok(())
}

pub(super) fn ensure_target_write_guard_schema(
    conn: &mut rusqlite::Connection,
) -> anyhow::Result<()> {
    let current_trigger = conn
        .query_row(
            r#"SELECT sql FROM sqlite_master
               WHERE type = 'trigger' AND name = 'metrics_target_raw_insert'"#,
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current_trigger
        .as_deref()
        .is_some_and(|sql| sql.contains("metrics_target_write_guard"))
    {
        return Ok(());
    }
    conn.execute_batch(
        r#"DROP TRIGGER IF EXISTS metrics_target_raw_insert;
           DROP TRIGGER IF EXISTS metrics_target_raw_update;
           DROP TRIGGER IF EXISTS metrics_target_raw_delete;
           DROP TRIGGER IF EXISTS metrics_target_latest_insert;
           DROP TRIGGER IF EXISTS metrics_target_latest_update;
           DROP TRIGGER IF EXISTS metrics_target_latest_delete;
           DROP TRIGGER IF EXISTS metrics_target_rollup_insert;
           DROP TRIGGER IF EXISTS metrics_target_rollup_update;
           DROP TRIGGER IF EXISTS metrics_target_rollup_delete;
           CREATE TRIGGER metrics_target_raw_insert
             AFTER INSERT ON service_resource_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_raw_update
             AFTER UPDATE ON service_resource_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_raw_delete
             AFTER DELETE ON service_resource_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET raw_revision = raw_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_latest_insert
             AFTER INSERT ON service_resource_latest_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_latest_update
             AFTER UPDATE ON service_resource_latest_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_latest_delete
             AFTER DELETE ON service_resource_latest_samples
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET latest_revision = latest_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_rollup_insert
             AFTER INSERT ON service_resource_rollups
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_rollup_update
             AFTER UPDATE ON service_resource_rollups
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;
           CREATE TRIGGER metrics_target_rollup_delete
             AFTER DELETE ON service_resource_rollups
             WHEN (SELECT managed FROM metrics_target_write_guard WHERE id = 1) = 0
             BEGIN UPDATE metrics_target_revision SET rollup_revision = rollup_revision + 1 WHERE id = 1; END;"#,
    )?;
    Ok(())
}
