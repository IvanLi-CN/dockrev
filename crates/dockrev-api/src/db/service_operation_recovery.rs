use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ServiceOperationRecoveryTarget {
    pub(crate) service_id: String,
    pub(crate) stack_id: String,
    pub(crate) service_name: String,
    pub(crate) image_ref: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoverySnapshot {
    schema_version: u32,
    #[serde(flatten)]
    state: ServiceAcceptedState,
}

impl Db {
    /// Loads immutable accepted-state baselines acquired by a mutation.
    pub(crate) async fn baseline_service_operation_settlements(
        &self,
        job_id: &str,
    ) -> anyhow::Result<Vec<ServiceAcceptedStateSettlement>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut statement = conn.prepare(
                "SELECT service_id, opened_generation, baseline_snapshot_json FROM job_service_targets WHERE job_id = ?1 AND opened_generation IS NOT NULL ORDER BY service_id",
            )?;
            let rows = statement.query_map([job_id], |row| {
                let service_id: String = row.get(0)?;
                let opened_generation: i64 = row.get(1)?;
                let baseline = row
                    .get::<_, Option<String>>(2)?
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(
                        2,
                        "baseline_snapshot_json".to_string(),
                        rusqlite::types::Type::Null,
                    ))?;
                let snapshot: RecoverySnapshot = serde_json::from_str(&baseline).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                if snapshot.schema_version != 1 {
                    return Err(rusqlite::Error::InvalidColumnType(
                        2,
                        "unsupported baseline schema".to_string(),
                        rusqlite::types::Type::Text,
                    ));
                }
                Ok(ServiceAcceptedStateSettlement {
                    service_id,
                    opened_generation,
                    state: snapshot.state,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
        .context("load service operation baselines")
    }

    pub(crate) async fn list_service_operation_recovery_targets(
        &self,
        job_id: &str,
    ) -> anyhow::Result<Vec<ServiceOperationRecoveryTarget>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut statement = conn.prepare(
                r#"
SELECT service.id, service.stack_id, service.name, service.image_ref
FROM job_service_targets target
JOIN services service ON service.id = target.service_id
WHERE target.job_id = ?1 AND target.opened_generation IS NOT NULL
ORDER BY service.id
"#,
            )?;
            let rows = statement.query_map([job_id], |row| {
                Ok(ServiceOperationRecoveryTarget {
                    service_id: row.get(0)?,
                    stack_id: row.get(1)?,
                    service_name: row.get(2)?,
                    image_ref: row.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
        .await
        .context("list service operation recovery targets")
    }
}
