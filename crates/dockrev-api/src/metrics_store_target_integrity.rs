use anyhow::Context as _;
use rusqlite::Transaction;

use super::MetricsStore;

pub(super) struct NativeTargetState {
    pub raw_is_untrusted: bool,
    pub latest_is_untrusted: bool,
    pub has_pruned_raw: bool,
}

impl MetricsStore {
    pub(super) async fn target_is_trusted(&self) -> anyhow::Result<bool> {
        self.reader_call(|conn| {
            conn.query_row(
                r#"SELECT raw_revision = trusted_raw_revision
                         AND latest_revision = trusted_latest_revision
                         AND rollup_revision = trusted_rollup_revision
                         AND EXISTS (
                           SELECT 1 FROM metrics_pruned_legacy_integrity
                           WHERE id = 1
                             AND row_count = trusted_row_count
                             AND id_sum = trusted_id_sum
                             AND id_square_sum = trusted_id_square_sum
                         )
                         AND EXISTS (
                           SELECT 1 FROM metrics_native_integrity
                           WHERE id = 1
                             AND raw_row_count = trusted_raw_row_count
                             AND latest_row_count = trusted_latest_row_count
                         )
                         AND EXISTS (
                           SELECT 1 FROM metrics_rollup_integrity
                           WHERE id = 1
                             AND row_count = trusted_row_count
                         )
                    FROM metrics_target_revision WHERE id = 1"#,
                [],
                |row| row.get::<_, i64>(0).map(|value| value != 0),
            )
            .map_err(Into::into)
        })
        .await
        .context("verify metrics target revision")
    }

    pub(super) async fn native_target_state(&self) -> anyhow::Result<NativeTargetState> {
        self.reader_call(|conn| {
            conn.query_row(
                r#"SELECT
                     (target.raw_revision != target.trusted_raw_revision
                        OR native.raw_row_count != native.trusted_raw_row_count)
                       AND (native.raw_row_count != 0 OR native.trusted_raw_row_count != 0),
                     (target.latest_revision != target.trusted_latest_revision
                        OR native.latest_row_count != native.trusted_latest_row_count)
                       AND (native.latest_row_count != 0 OR native.trusted_latest_row_count != 0),
                     native.has_pruned_raw
                   FROM metrics_target_revision AS target
                   JOIN metrics_native_integrity AS native ON native.id = target.id
                   WHERE target.id = 1"#,
                [],
                |row| {
                    Ok(NativeTargetState {
                        raw_is_untrusted: row.get::<_, i64>(0)? != 0,
                        latest_is_untrusted: row.get::<_, i64>(1)? != 0,
                        has_pruned_raw: row.get::<_, i64>(2)? != 0,
                    })
                },
            )
            .map_err(Into::into)
        })
        .await
        .context("verify native metrics target integrity")
    }

    pub(super) async fn trust_target(&self) -> anyhow::Result<()> {
        self.writer_call(|conn| {
            let tx = conn.transaction()?;
            trust_metrics_target_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .context("trust metrics target revision")
    }
}

pub(super) fn trust_metrics_target_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        r#"UPDATE metrics_target_revision
           SET trusted_raw_revision = raw_revision,
               trusted_latest_revision = latest_revision,
               trusted_rollup_revision = rollup_revision
           WHERE id = 1"#,
        [],
    )?;
    tx.execute(
        r#"UPDATE metrics_rollup_integrity
           SET trusted_row_count = row_count
           WHERE id = 1"#,
        [],
    )?;
    tx.execute(
        r#"UPDATE metrics_native_integrity
           SET trusted_raw_row_count = raw_row_count,
               trusted_latest_row_count = latest_row_count
           WHERE id = 1"#,
        [],
    )?;
    Ok(())
}

pub(super) fn mark_native_raw_pruned_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE metrics_native_integrity SET has_pruned_raw = 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

pub(super) fn adjust_native_raw_count_tx(tx: &Transaction<'_>, delta: i64) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE metrics_native_integrity SET raw_row_count = raw_row_count + ?1 WHERE id = 1",
        [delta],
    )?;
    Ok(())
}

pub(super) fn begin_managed_metrics_write_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE metrics_target_write_guard SET managed = 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

pub(super) fn end_managed_metrics_write_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        "UPDATE metrics_target_write_guard SET managed = 0 WHERE id = 1",
        [],
    )?;
    Ok(())
}

pub(super) fn trust_pruned_legacy_integrity_tx(tx: &Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        r#"UPDATE metrics_pruned_legacy_integrity
           SET trusted_row_count = row_count,
               trusted_id_sum = id_sum,
               trusted_id_square_sum = id_square_sum
           WHERE id = 1"#,
        [],
    )?;
    Ok(())
}
