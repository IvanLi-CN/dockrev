use super::*;

pub(super) fn apply(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    let id = "0027_add_service_version_tag_observations";
    if migration_applied(conn, id)? {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS service_version_tag_observations (
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  image_repo TEXT NOT NULL,
  configured_tag TEXT NOT NULL,
  digest TEXT NOT NULL,
  version TEXT,
  observed_at TEXT NOT NULL,
  PRIMARY KEY (service_id, image_repo, configured_tag, digest)
);
CREATE INDEX IF NOT EXISTS idx_service_version_tag_observations_lookup
  ON service_version_tag_observations(service_id, image_repo, configured_tag, version);
"#,
    )?;
    record_migration_tx(&tx, id)?;
    tx.commit()?;
    Ok(())
}
