use super::*;

#[test]
fn startup_reconciles_missing_discovery_projects_with_active_stacks() {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA).unwrap();

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
) VALUES ('legacy-missing', 'legacy_missing', 'missing', 1, 'auto_archive_on_restart')
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

    reconcile_missing_discovery_projects_on_startup(&mut conn).unwrap();

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
}
