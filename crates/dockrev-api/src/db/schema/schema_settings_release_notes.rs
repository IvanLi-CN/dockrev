pub(super) fn ensure_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let desired = [
        (
            "release_notes_octo_rill_enabled",
            "ALTER TABLE settings ADD COLUMN release_notes_octo_rill_enabled INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "release_notes_octo_rill_api_base_url",
            "ALTER TABLE settings ADD COLUMN release_notes_octo_rill_api_base_url TEXT",
        ),
        (
            "release_notes_octo_rill_api_key",
            "ALTER TABLE settings ADD COLUMN release_notes_octo_rill_api_key TEXT",
        ),
        (
            "release_notes_octo_rill_default_view",
            "ALTER TABLE settings ADD COLUMN release_notes_octo_rill_default_view TEXT NOT NULL DEFAULT 'smart'",
        ),
    ];

    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;

    for (name, ddl) in desired {
        if existing.iter().any(|c| c == name) {
            continue;
        }
        conn.execute_batch(ddl)?;
    }

    conn.execute(
        r#"
UPDATE settings
SET release_notes_octo_rill_default_view = 'smart'
WHERE release_notes_octo_rill_default_view NOT IN ('original', 'translated', 'smart')
"#,
        [],
    )?;

    Ok(())
}
