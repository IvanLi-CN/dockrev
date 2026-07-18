pub(super) fn ensure_columns(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let provider_missing = {
        let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let existing = rows.collect::<Result<Vec<_>, _>>()?;
        !existing.iter().any(|c| c == "release_notes_provider")
    };

    let desired = [
        (
            "release_notes_provider",
            "ALTER TABLE settings ADD COLUMN release_notes_provider TEXT NOT NULL DEFAULT 'gitHub'",
        ),
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

    if provider_missing {
        conn.execute(
            r#"
UPDATE settings
SET release_notes_provider = CASE
  WHEN release_notes_octo_rill_enabled != 0 THEN 'octoRill'
  ELSE 'gitHub'
END
"#,
            [],
        )?;
    }

    conn.execute(
        r#"
UPDATE settings
SET release_notes_octo_rill_default_view = 'smart'
WHERE release_notes_octo_rill_default_view NOT IN ('original', 'translated', 'smart')
"#,
        [],
    )?;

    conn.execute(
        r#"
UPDATE settings
SET release_notes_provider = 'gitHub'
WHERE release_notes_provider NOT IN ('gitHub', 'octoRill')
"#,
        [],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_columns;

    fn create_legacy_settings_table(conn: &rusqlite::Connection, enabled: i64) {
        conn.execute_batch(
            r#"
CREATE TABLE settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  release_notes_octo_rill_enabled INTEGER NOT NULL DEFAULT 0,
  release_notes_octo_rill_api_base_url TEXT,
  release_notes_octo_rill_api_key TEXT,
  release_notes_octo_rill_default_view TEXT NOT NULL DEFAULT 'smart'
);
"#,
        )
        .unwrap();
        conn.execute(
            r#"
INSERT INTO settings (
  id,
  release_notes_octo_rill_enabled,
  release_notes_octo_rill_api_base_url,
  release_notes_octo_rill_default_view
) VALUES (?1, ?2, ?3, ?4)
"#,
            (
                1_i64,
                enabled,
                "https://octo.example.com/octo-rill",
                "smart",
            ),
        )
        .unwrap();
    }

    #[test]
    fn migrates_legacy_enabled_true_to_octorill_provider() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_legacy_settings_table(&conn, 1);

        ensure_columns(&conn).unwrap();

        let provider: String = conn
            .query_row(
                "SELECT release_notes_provider FROM settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(provider, "octoRill");
    }

    #[test]
    fn migrates_legacy_enabled_false_to_github_provider() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        create_legacy_settings_table(&conn, 0);

        ensure_columns(&conn).unwrap();

        let provider: String = conn
            .query_row(
                "SELECT release_notes_provider FROM settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(provider, "gitHub");
    }
}
