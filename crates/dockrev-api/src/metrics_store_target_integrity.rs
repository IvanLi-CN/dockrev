use anyhow::Context as _;
use rusqlite::Transaction;

use super::MetricsStore;

impl MetricsStore {
    pub(super) async fn target_is_trusted(&self) -> anyhow::Result<bool> {
        self.reader_call(|conn| {
            conn.query_row(
                r#"SELECT raw_revision = trusted_raw_revision
                         AND latest_revision = trusted_latest_revision
                         AND rollup_revision = trusted_rollup_revision
                    FROM metrics_target_revision WHERE id = 1"#,
                [],
                |row| row.get::<_, i64>(0).map(|value| value != 0),
            )
            .map_err(Into::into)
        })
        .await
        .context("verify metrics target revision")
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
    Ok(())
}
