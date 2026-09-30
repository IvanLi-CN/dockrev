impl Db {
    #[cfg(test)]
    pub async fn get_rollback_evidence_archive(
        &self,
        job_id: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT rollback_evidence_tar_zstd FROM jobs WHERE id = ?1",
                    params![job_id],
                    |row| row.get::<_, Option<Vec<u8>>>(0),
                )
                .optional()?
                .flatten())
        })
        .await
        .context("get rollback evidence archive")
    }

    pub async fn rollback_evidence_archive_size(
        &self,
        job_id: &str,
    ) -> anyhow::Result<Option<u64>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let size = conn
                .query_row(
                    "SELECT length(rollback_evidence_tar_zstd) FROM jobs WHERE id = ?1",
                    params![job_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten();
            size.map(|value| {
                u64::try_from(value)
                    .map_err(|_| anyhow::anyhow!("rollback evidence archive has a negative length"))
            })
            .transpose()
        })
        .await
        .context("get rollback evidence archive size")
    }

    pub async fn mark_rollback_evidence_incomplete_if_archive_absent(
        &self,
        job_id: &str,
        metadata: &serde_json::Value,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let mut metadata = metadata.clone();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let Some((summary_raw, has_archive)) = tx
                .query_row(
                    "SELECT summary_json, rollback_evidence_tar_zstd IS NOT NULL FROM jobs WHERE id = ?1",
                    params![&job_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
                )
                .optional()?
            else {
                tx.commit()?;
                return Ok(false);
            };
            if has_archive {
                tx.commit()?;
                return Ok(false);
            }
            let mut summary: serde_json::Value =
                serde_json::from_str(&summary_raw).unwrap_or_else(|_| serde_json::json!({}));
            if !summary.is_object() {
                summary = serde_json::json!({ "result": summary });
            }
            let previous_evidence = summary["rollbackEvidence"].clone();
            let recorded_candidate_total =
                summary["rollbackEvidence"]["failedCandidates"].as_u64();
            let incoming_candidate_total = metadata["failedCandidates"].as_u64();
            if let Some(recorded_candidate_total) = recorded_candidate_total
                && incoming_candidate_total
                    .is_none_or(|incoming| recorded_candidate_total > incoming)
                && let Some(metadata) = metadata.as_object_mut()
            {
                metadata.insert(
                    "failedCandidates".to_string(),
                    serde_json::json!(recorded_candidate_total),
                );
            }
            if metadata["services"].as_array().is_some_and(Vec::is_empty)
                && let Some(previous_services) = previous_evidence["services"].as_array()
            {
                metadata["services"] = serde_json::json!(previous_services);
            }
            let incoming_errors = metadata["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let previous_errors = previous_evidence["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut errors = Vec::new();
            if let Some(primary_error) = incoming_errors.first() {
                errors.push(primary_error.clone());
            }
            for error in previous_errors
                .iter()
                .filter(|error| error.starts_with("manifest:"))
            {
                if !errors.contains(error) {
                    errors.push(error.clone());
                }
            }
            for error in incoming_errors.iter().skip(1).chain(
                previous_errors
                    .iter()
                    .filter(|error| !error.starts_with("manifest:")),
            ) {
                if !errors.contains(error) {
                    errors.push(error.clone());
                }
            }
            metadata["errors"] = serde_json::json!(
                crate::rollback_evidence::bounded_summary_errors(errors, false)
            );
            summary
                .as_object_mut()
                .expect("summary was normalized to an object")
                .insert("rollbackEvidence".to_string(), metadata);
            let changed = tx.execute(
                "UPDATE jobs SET summary_json = ?2 WHERE id = ?1 AND rollback_evidence_tar_zstd IS NULL",
                params![job_id, serde_json::to_string(&summary)?],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("mark rollback evidence incomplete")
    }

    pub async fn read_rollback_evidence_archive_chunk(
        &self,
        job_id: &str,
        offset: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        const CHUNK_BYTES: usize = 64 * 1024;
        let job_id = job_id.to_string();
        self.call(move |conn| {
            use std::io::{Read as _, Seek as _, SeekFrom};

            let row_id = conn
                .query_row(
                    "SELECT rowid FROM jobs WHERE id = ?1 AND rollback_evidence_tar_zstd IS NOT NULL",
                    params![job_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(row_id) = row_id else {
                return Ok(None);
            };
            let mut blob = conn.blob_open(
                rusqlite::MAIN_DB,
                "jobs",
                "rollback_evidence_tar_zstd",
                row_id,
                true,
            )?;
            let size = blob.len() as u64;
            if offset >= size {
                blob.close()?;
                return Ok(Some(Vec::new()));
            }
            if offset > i32::MAX as u64 {
                anyhow::bail!("rollback evidence archive offset exceeds SQLite BLOB limits");
            }
            let chunk_size = (size - offset).min(CHUNK_BYTES as u64) as usize;
            blob.seek(SeekFrom::Start(offset))?;
            let mut bytes = vec![0; chunk_size];
            blob.read_exact(&mut bytes)?;
            blob.close()?;
            Ok(Some(bytes))
        })
        .await
        .context("read rollback evidence archive chunk")
    }

    pub async fn attach_rollback_evidence_archive_from_file(
        &self,
        job_id: &str,
        archive_path: &Path,
        metadata: &serde_json::Value,
    ) -> anyhow::Result<bool> {
        let job_id = job_id.to_string();
        let archive_path = archive_path.to_path_buf();
        let metadata = metadata.clone();
        self.call(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let Some(summary_raw) = tx
                .query_row(
                    "SELECT summary_json FROM jobs WHERE id = ?1",
                    params![&job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            else {
                tx.commit()?;
                return Ok(false);
            };
            let mut summary: serde_json::Value =
                serde_json::from_str(&summary_raw).unwrap_or_else(|_| serde_json::json!({}));
            if !summary.is_object() {
                summary = serde_json::json!({ "result": summary });
            }
            if let Some(object) = summary.as_object_mut() {
                object.insert("rollbackEvidence".to_string(), metadata);
            }
            if !jobs_finish::write_archive_file_tx_if_absent(&tx, &job_id, &archive_path)? {
                tx.commit()?;
                return Ok(false);
            }
            let changed = tx.execute(
                "UPDATE jobs SET summary_json = ?2 WHERE id = ?1",
                params![job_id, serde_json::to_string(&summary)?],
            )?;
            tx.commit()?;
            Ok(changed > 0)
        })
        .await
        .context("attach rollback evidence archive from file")
    }
}
