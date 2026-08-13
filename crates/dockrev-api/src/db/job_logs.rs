use super::*;

impl Db {
    pub async fn list_job_logs(&self, job_id: &str) -> anyhow::Result<Vec<JobLogLine>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT ts, level, msg
FROM job_logs
WHERE job_id = ?1
ORDER BY id DESC
LIMIT 500
"#,
            )?;
            let rows = stmt.query_map(params![job_id], |row| {
                Ok(JobLogLine {
                    ts: row.get(0)?,
                    level: row.get(1)?,
                    msg: row.get(2)?,
                })
            })?;
            let mut out = rows.collect::<Result<Vec<_>, _>>()?;
            out.reverse();
            Ok(out)
        })
        .await
        .context("list job logs")
    }

    pub async fn list_job_logs_since(
        &self,
        job_id: &str,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobLogRow>> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, ts, level, msg
FROM job_logs
WHERE job_id = ?1 AND id > ?2
ORDER BY id ASC
LIMIT ?3
"#,
            )?;
            let rows = stmt.query_map(params![job_id, after_id, limit as i64], |row| {
                Ok(JobLogRow {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    level: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job logs since")
    }

    pub async fn list_job_event_logs_since(
        &self,
        after_id: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<JobEventLogRow>> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
SELECT id, job_id, ts, msg
FROM job_logs
WHERE level = 'event' AND id > ?1
ORDER BY id ASC
LIMIT ?2
"#,
            )?;
            let rows = stmt.query_map(params![after_id, limit as i64], |row| {
                Ok(JobEventLogRow {
                    id: row.get(0)?,
                    job_id: row.get(1)?,
                    ts: row.get(2)?,
                    msg: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .context("list job event logs since")
    }

    pub async fn get_job_logs_last_id(&self, job_id: &str) -> anyhow::Result<i64> {
        let job_id = job_id.to_string();
        self.call(move |conn| {
            Ok(conn.query_row(
                "SELECT COALESCE(MAX(id), 0) FROM job_logs WHERE job_id = ?1",
                params![job_id],
                |row| row.get(0),
            )?)
        })
        .await
        .context("get job logs last id")
    }

    pub async fn get_job_logs_global_last_id(&self) -> anyhow::Result<i64> {
        self.call(move |conn| {
            Ok(conn.query_row(
                "SELECT COALESCE(MAX(id), 0) FROM job_logs WHERE level = 'event'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .context("get global job logs last id")
    }

    pub async fn insert_job_log(&self, job_id: &str, line: &JobLogLine) -> anyhow::Result<()> {
        let job_id = job_id.to_string();
        let line = line.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO job_logs (job_id, ts, level, msg) VALUES (?1, ?2, ?3, ?4)",
                params![job_id, line.ts, line.level, line.msg],
            )?;
            Ok(())
        })
        .await
        .context("insert job log")
    }
}
