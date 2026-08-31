use anyhow::Context as _;
use rusqlite::{OptionalExtension as _, params};

use super::Db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceLifecycleEventRow {
    pub id: i64,
    pub service_id: String,
    pub stack_id: Option<String>,
    pub operation_group_id: String,
    pub job_id: Option<String>,
    pub origin: String,
    pub transition: String,
    pub observed_at: String,
    pub boundary_precision: String,
    pub evidence_json: String,
    pub details_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct ServiceLifecycleEventInput {
    pub service_id: String,
    pub stack_id: Option<String>,
    pub operation_group_id: String,
    pub job_id: Option<String>,
    pub origin: String,
    pub transition: String,
    pub observed_at: String,
    pub boundary_precision: String,
    pub evidence_json: String,
    pub details_json: String,
    pub created_at: String,
}

impl Db {
    pub async fn get_service_id_by_stack_and_name(
        &self,
        stack_id: &str,
        service_name: &str,
    ) -> anyhow::Result<Option<String>> {
        let stack_id = stack_id.to_string();
        let service_name = service_name.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT id FROM services WHERE stack_id = ?1 AND name = ?2",
                params![stack_id, service_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
        })
        .await
    }

    pub async fn insert_service_lifecycle_event(
        &self,
        input: ServiceLifecycleEventInput,
    ) -> anyhow::Result<Option<ServiceLifecycleEventRow>> {
        self.call(move |conn| {
            let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(30))
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            conn.execute(
                "DELETE FROM service_lifecycle_events WHERE created_at < ?1",
                params![cutoff],
            )?;
            let inserted = conn.execute(
                r#"
INSERT OR IGNORE INTO service_lifecycle_events (
  service_id, stack_id, operation_group_id, job_id, origin, transition,
  observed_at, boundary_precision, evidence_json, details_json, created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
"#,
                params![
                    input.service_id,
                    input.stack_id,
                    input.operation_group_id,
                    input.job_id,
                    input.origin,
                    input.transition,
                    input.observed_at,
                    input.boundary_precision,
                    input.evidence_json,
                    input.details_json,
                    input.created_at,
                ],
            )?;
            if inserted == 0 {
                return Ok(None);
            }
            let id = conn.last_insert_rowid();
            conn.query_row(
                r#"SELECT id, service_id, stack_id, operation_group_id, job_id,
                          origin, transition, observed_at, boundary_precision,
                          evidence_json, details_json, created_at
                   FROM service_lifecycle_events WHERE id = ?1"#,
                params![id],
                row_from_sql,
            )
            .map(Some)
            .context("read inserted lifecycle event")
        })
        .await
    }

    #[allow(dead_code)]
    pub async fn list_service_lifecycle_events(
        &self,
        service_id: &str,
        since: &str,
        until: &str,
    ) -> anyhow::Result<Vec<ServiceLifecycleEventRow>> {
        let service_id = service_id.to_string();
        let since = since.to_string();
        let until = until.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT id, service_id, stack_id, operation_group_id, job_id,
                          origin, transition, observed_at, boundary_precision,
                          evidence_json, details_json, created_at
                   FROM service_lifecycle_events
                   WHERE service_id = ?1 AND observed_at >= ?2 AND observed_at <= ?3
                   ORDER BY observed_at ASC, id ASC"#,
            )?;
            let rows = stmt.query_map(params![service_id, since, until], row_from_sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
    }

    pub async fn list_service_lifecycle_events_with_predecessor(
        &self,
        service_id: &str,
        since: &str,
        until: &str,
    ) -> anyhow::Result<Vec<ServiceLifecycleEventRow>> {
        let service_id = service_id.to_string();
        let since = since.to_string();
        let until = until.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT id, service_id, stack_id, operation_group_id, job_id,
                          origin, transition, observed_at, boundary_precision,
                          evidence_json, details_json, created_at
                   FROM service_lifecycle_events
                   WHERE service_id = ?1
                     AND (observed_at >= ?2 OR id = (
                       SELECT id FROM service_lifecycle_events
                       WHERE service_id = ?1 AND observed_at < ?2
                       ORDER BY observed_at DESC, id DESC LIMIT 1
                     ))
                     AND observed_at <= ?3
                   ORDER BY observed_at ASC, id ASC"#,
            )?;
            let rows = stmt.query_map(params![service_id, since, until], row_from_sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
    }

    pub async fn list_service_lifecycle_events_after(
        &self,
        service_id: &str,
        after_id: i64,
    ) -> anyhow::Result<Vec<ServiceLifecycleEventRow>> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"SELECT id, service_id, stack_id, operation_group_id, job_id,
                          origin, transition, observed_at, boundary_precision,
                          evidence_json, details_json, created_at
                   FROM service_lifecycle_events
                   WHERE service_id = ?1 AND id > ?2 ORDER BY id ASC"#,
            )?;
            let rows = stmt.query_map(params![service_id, after_id], row_from_sql)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
    }

    pub async fn service_lifecycle_event_bounds(
        &self,
        service_id: &str,
    ) -> anyhow::Result<(Option<i64>, Option<i64>)> {
        let service_id = service_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT MIN(id), MAX(id) FROM service_lifecycle_events WHERE service_id = ?1",
                params![service_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .await
    }

    pub async fn delete_expired_service_lifecycle_events(
        &self,
        cutoff: &str,
    ) -> anyhow::Result<u64> {
        let cutoff = cutoff.to_string();
        self.call(move |conn| {
            Ok(conn.execute(
                "DELETE FROM service_lifecycle_events WHERE created_at < ?1",
                params![cutoff],
            )? as u64)
        })
        .await
    }
}

fn row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServiceLifecycleEventRow> {
    Ok(ServiceLifecycleEventRow {
        id: row.get(0)?,
        service_id: row.get(1)?,
        stack_id: row.get(2)?,
        operation_group_id: row.get(3)?,
        job_id: row.get(4)?,
        origin: row.get(5)?,
        transition: row.get(6)?,
        observed_at: row.get(7)?,
        boundary_precision: row.get(8)?,
        evidence_json: row.get(9)?,
        details_json: row.get(10)?,
        created_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (std::path::PathBuf, Db) {
        let path =
            std::env::temp_dir().join(format!("dockrev-lifecycle-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).await.unwrap();
        db.call(|conn| {
            conn.execute("INSERT INTO stacks (id,name,compose_type,compose_files_json,backup_targets_json,backup_retention_keep_last,backup_retention_delete_after_stable_seconds,created_at,updated_at,last_check_at) VALUES ('stack','stack','v2','[]','[]',1,0,'2026-01-01','2026-01-01','2026-01-01')", [])?;
            conn.execute("INSERT INTO services (id,stack_id,name,image_ref,image_tag,auto_rollback,backup_targets_bind_paths_json,backup_targets_volume_names_json,created_at,updated_at) VALUES ('svc','stack','web','nginx','latest',0,'{}','{}','2026-01-01','2026-01-01')", [])?;
            Ok(())
        }).await.unwrap();
        (path, db)
    }

    #[tokio::test]
    async fn lifecycle_events_are_idempotent_and_retained_for_thirty_days() {
        let (path, db) = fixture().await;
        let base_time = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        let format_time = |value: time::OffsetDateTime| {
            value
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap()
        };
        let first_at = format_time(base_time);
        let second_at = format_time(base_time + time::Duration::minutes(1));
        let later_at = format_time(base_time + time::Duration::days(1));
        let range_until =
            format_time(base_time + time::Duration::days(1) + time::Duration::minutes(1));
        let input = |transition: &str, created_at: &str| ServiceLifecycleEventInput {
            service_id: "svc".to_string(),
            stack_id: Some("stack".to_string()),
            operation_group_id: "op".to_string(),
            job_id: None,
            origin: "compose".to_string(),
            transition: transition.to_string(),
            observed_at: created_at.to_string(),
            boundary_precision: "exact".to_string(),
            evidence_json: "{}".to_string(),
            details_json: "{}".to_string(),
            created_at: created_at.to_string(),
        };
        assert!(
            db.insert_service_lifecycle_event(input("stopped", &first_at))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            db.insert_service_lifecycle_event(input("stopped", &first_at))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            db.insert_service_lifecycle_event(input("started", &second_at))
                .await
                .unwrap()
                .is_some()
        );
        let rows = db
            .list_service_lifecycle_events("svc", &first_at, &range_until)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        let mut later = input("stopped", &later_at);
        later.operation_group_id = "op-2".to_string();
        db.insert_service_lifecycle_event(later).await.unwrap();
        assert_eq!(
            db.list_service_lifecycle_events("svc", &first_at, &range_until)
                .await
                .unwrap()
                .len(),
            3
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
