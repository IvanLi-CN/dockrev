use super::*;
use std::path::PathBuf;

fn temporary_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("dockrev-schema-test-{}.sqlite3", ulid::Ulid::new()))
}

#[tokio::test]
async fn startup_reconciles_missing_discovery_projects_with_active_stacks() {
    let db_path = temporary_db_path();
    let initial_db = Db::open(&db_path).await.unwrap();
    drop(initial_db);
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    for stack_id in ["legacy_missing", "active_project", "user_archived"] {
        conn.execute(
            r#"
INSERT INTO stacks (
  id, name, compose_type, compose_files_json, backup_targets_json,
  backup_retention_keep_last, backup_retention_delete_after_stable_seconds,
  created_at, updated_at, last_check_at
) VALUES (?1, ?1, 'path', '[]', '[]', 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
"#,
            [stack_id],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE stacks SET archived = 1, archived_reason = 'user_archive' WHERE id = 'user_archived'",
        [],
    )
    .unwrap();

    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (
  project, stack_id, status, archived, archived_reason
) VALUES ('legacy-missing', 'legacy_missing', 'missing', 1, 'user_archive')
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (project, stack_id, status)
VALUES ('active-project', 'active_project', 'active')
"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
INSERT INTO discovered_compose_projects (project, stack_id, status)
VALUES ('missing-user-archive', 'user_archived', 'missing')
"#,
        [],
    )
    .unwrap();
    drop(conn);

    let reopened_db = Db::open(&db_path).await.unwrap();
    drop(reopened_db);
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let states = conn
        .prepare("SELECT id, archived, archived_reason FROM stacks ORDER BY id")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        states,
        vec![
            ("active_project".to_string(), 0, None),
            (
                "legacy_missing".to_string(),
                1,
                Some("auto_archive_on_restart".to_string()),
            ),
            (
                "user_archived".to_string(),
                1,
                Some("user_archive".to_string()),
            ),
        ]
    );

    let discovery_state = conn
        .query_row(
            "SELECT archived, archived_reason FROM discovered_compose_projects WHERE project = 'legacy-missing'",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .unwrap();
    assert_eq!(discovery_state, (1, Some("user_archive".to_string())));

    drop(conn);
    std::fs::remove_file(&db_path).unwrap();
    let wal_path = db_path.with_extension("sqlite3-wal");
    let shm_path = db_path.with_extension("sqlite3-shm");
    let _ = std::fs::remove_file(wal_path);
    let _ = std::fs::remove_file(shm_path);
}
