use super::*;

pub(super) fn project_backup_target_overrides(
    policies: &[crate::db::ServiceBackupTargetPolicyRow],
    is_volume: bool,
) -> BTreeMap<String, crate::api::types::TernaryChoice> {
    policies
        .iter()
        .map(|row| {
            let choice = match row.policy {
                crate::api::types::BackupTargetPolicy::Disabled => {
                    crate::api::types::TernaryChoice::Skip
                }
                crate::api::types::BackupTargetPolicy::StopRelatedServices => {
                    crate::api::types::TernaryChoice::Force
                }
                crate::api::types::BackupTargetPolicy::LiveBackup => {
                    crate::api::types::TernaryChoice::Inherit
                }
            };
            (row.key.clone(), choice)
        })
        .filter(|(key, _)| {
            if is_volume {
                !key.starts_with('/')
            } else {
                key.starts_with('/')
            }
        })
        .collect()
}

pub(super) fn prune_service_backup_target_policies_tx(
    tx: &rusqlite::Transaction<'_>,
    service_id: &str,
    declared_bind_paths: &BTreeSet<String>,
    declared_volume_names: &BTreeSet<String>,
) -> anyhow::Result<()> {
    let mut stmt = tx.prepare(
        r#"
SELECT target_kind, target_key
FROM service_backup_target_policies
WHERE service_id = ?1
"#,
    )?;
    let rows = stmt.query_map(params![service_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let existing = rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (target_kind, target_key) in existing {
        let keep = if target_kind == "volume" {
            declared_volume_names.contains(&target_key)
        } else {
            declared_bind_paths.contains(&target_key)
        };
        if keep {
            continue;
        }
        tx.execute(
            r#"
DELETE FROM service_backup_target_policies
WHERE service_id = ?1 AND target_kind = ?2 AND target_key = ?3
"#,
            params![service_id, target_kind, target_key],
        )?;
    }
    Ok(())
}

pub(super) fn put_service_backup_targets_tx(
    conn: &mut rusqlite::Connection,
    service_id: &str,
    update: &crate::db::ServiceBackupTargetsUpdate,
    now: &str,
) -> anyhow::Result<bool> {
    let service_id = service_id.to_string();
    let update = update.clone();
    let now = now.to_string();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let stack_id = tx
        .query_row(
            "SELECT stack_id FROM services WHERE id = ?1",
            params![service_id.clone()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(stack_id) = stack_id else {
        return Ok(false);
    };

    tx.execute(
        "DELETE FROM service_backup_target_policies WHERE service_id = ?1",
        params![service_id.clone()],
    )?;
    for row in &update.bind_paths {
        tx.execute(
            r#"
INSERT INTO service_backup_target_policies (
  service_id,
  target_kind,
  target_key,
  policy,
  created_at,
  updated_at
) VALUES (?1, 'bind', ?2, ?3, ?4, ?4)
"#,
            params![
                service_id.clone(),
                row.key,
                row.policy.as_str(),
                now.clone()
            ],
        )?;
    }
    for row in &update.volume_names {
        tx.execute(
            r#"
INSERT INTO service_backup_target_policies (
  service_id,
  target_kind,
  target_key,
  policy,
  created_at,
  updated_at
) VALUES (?1, 'volume', ?2, ?3, ?4, ?4)
"#,
            params![
                service_id.clone(),
                row.key,
                row.policy.as_str(),
                now.clone()
            ],
        )?;
    }

    let mut stack_targets = Vec::<crate::api::types::BackupTarget>::new();
    let mut seen = BTreeSet::<(String, String)>::new();
    let mut stmt = tx.prepare(
        r#"
SELECT p.target_kind, p.target_key
FROM service_backup_target_policies p
JOIN services s ON s.id = p.service_id
WHERE s.stack_id = ?1 AND p.policy != 'disabled'
ORDER BY p.target_kind ASC, p.target_key ASC
"#,
    )?;
    let rows = stmt.query_map(params![stack_id.clone()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (target_kind, target_key) = row?;
        if !seen.insert((target_kind.clone(), target_key.clone())) {
            continue;
        }
        if target_kind == "volume" {
            stack_targets.push(crate::api::types::BackupTarget::DockerVolume { name: target_key });
        } else {
            stack_targets.push(crate::api::types::BackupTarget::BindMount { path: target_key });
        }
    }
    drop(stmt);

    tx.execute(
        r#"
UPDATE stacks
SET backup_targets_json = ?2, updated_at = ?3
WHERE id = ?1
"#,
        params![
            stack_id,
            serde_json::to_string(&stack_targets)?,
            now.clone()
        ],
    )?;

    let changed = tx.execute(
        r#"
UPDATE services
SET
  backup_targets_bind_paths_json = ?2,
  backup_targets_volume_names_json = ?3,
  updated_at = ?4
WHERE id = ?1
"#,
        params![
            service_id,
            serde_json::to_string(&project_backup_target_overrides(&update.bind_paths, false))?,
            serde_json::to_string(&project_backup_target_overrides(&update.volume_names, true))?,
            now
        ],
    )?;

    tx.commit()?;
    Ok(changed > 0)
}
