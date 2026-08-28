use anyhow::Result;

pub(super) fn ensure_columns(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(jobs)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;
    if !existing
        .iter()
        .any(|name| name == "rollback_evidence_tar_zstd")
    {
        conn.execute_batch("ALTER TABLE jobs ADD COLUMN rollback_evidence_tar_zstd BLOB")?;
    }
    Ok(())
}
