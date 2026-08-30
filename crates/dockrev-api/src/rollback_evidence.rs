use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    backup_helper,
    docker_runner::{self, DockerRunnerConfig},
    runner::CommandRunner,
};

pub const MAX_LOG_BYTES: usize = 1024 * 1024;
const SPOOL_DIR_NAME: &str = "rollback-evidence-spool";

#[derive(Clone)]
pub struct RollbackEvidenceContext {
    job_id: String,
    root: PathBuf,
    records: Arc<Mutex<BTreeMap<String, EvidenceMetadata>>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceMetadata {
    pub service_id: String,
    pub candidate_id: String,
    pub health_status: String,
    pub health_policy: Option<HealthPolicy>,
    pub health_policy_deadline_seconds: Option<u64>,
    pub state_status: Option<String>,
    pub state_error: Option<String>,
    pub exit_code: Option<i64>,
    pub restart_count: Option<i64>,
    pub logs_bytes: usize,
    pub logs_truncated: bool,
    pub capture_errors: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthPolicy {
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub start_period_seconds: u64,
    pub start_interval_seconds: u64,
    pub retries: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub status: &'static str,
    pub failed_candidates: usize,
    pub archive_format: &'static str,
    pub compression: &'static str,
    pub archive_size_bytes: Option<u64>,
    pub services: Vec<EvidenceMetadata>,
    pub errors: Vec<String>,
}

impl RollbackEvidenceContext {
    pub fn new(job_id: impl Into<String>, db_path: &Path) -> anyhow::Result<Self> {
        let root = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(SPOOL_DIR_NAME);
        fs::create_dir_all(&root).with_context(|| format!("create evidence spool {:?}", root))?;
        set_owner_only(&root)?;
        Ok(Self {
            job_id: job_id.into(),
            root,
            records: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn job_spool_path(&self) -> PathBuf {
        self.root.join(&self.job_id)
    }

    pub fn metadata(&self) -> Vec<EvidenceMetadata> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn capture_failure(
        &self,
        runner: &dyn CommandRunner,
        docker_cfg: &DockerRunnerConfig,
        service_id: &str,
        candidate_id: &str,
        health_status: &str,
        health_policy: Option<HealthPolicy>,
        deadline: Option<Duration>,
    ) -> EvidenceMetadata {
        let state_future = runner.run_raw(
            docker_runner::inspect_candidate_state(docker_cfg, candidate_id),
            Duration::from_secs(10),
        );
        let logs_future = runner.run_raw_bounded(
            docker_runner::logs_with_timestamps(docker_cfg, candidate_id),
            Duration::from_secs(10),
            MAX_LOG_BYTES,
        );
        let (state_result, logs_result) = tokio::join!(state_future, logs_future);

        let mut metadata = EvidenceMetadata {
            service_id: service_id.to_string(),
            candidate_id: candidate_id.to_string(),
            health_status: health_status.to_string(),
            health_policy,
            health_policy_deadline_seconds: deadline.map(|value| value.as_secs()),
            ..Default::default()
        };
        let mut state_json = serde_json::json!({});
        let mut health_log = Value::Array(Vec::new());
        match state_result {
            Ok(output) if output.status == 0 => {
                let raw_state =
                    serde_json::from_slice::<Value>(&output.stdout).unwrap_or_else(|error| {
                        metadata
                            .capture_errors
                            .push(format!("state parse: {error}"));
                        serde_json::json!({})
                    });
                metadata.state_status = raw_state
                    .get("Status")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                metadata.state_error = raw_state
                    .get("Error")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                metadata.exit_code = raw_state.get("ExitCode").and_then(Value::as_i64);
                metadata.restart_count = raw_state.get("RestartCount").and_then(Value::as_i64);
                health_log = raw_state
                    .get("Health")
                    .and_then(|health| health.get("Log"))
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new()));
                state_json = serde_json::json!({
                    "Status": metadata.state_status,
                    "Error": metadata.state_error,
                    "ExitCode": metadata.exit_code,
                    "RestartCount": metadata.restart_count,
                });
            }
            Ok(output) => metadata
                .capture_errors
                .push(format!("state command exited with {}", output.status)),
            Err(error) => metadata
                .capture_errors
                .push(format!("state command: {error}")),
        }

        let (log_bytes, truncated) = match logs_result {
            Ok(output) if output.status == 0 => truncate_complete_lines(&output.stdout),
            Ok(output) => {
                metadata
                    .capture_errors
                    .push(format!("logs command exited with {}", output.status));
                truncate_complete_lines(&output.stdout)
            }
            Err(error) => {
                metadata
                    .capture_errors
                    .push(format!("logs command: {error}"));
                (Vec::new(), false)
            }
        };
        metadata.logs_bytes = log_bytes.len();
        metadata.logs_truncated = truncated;

        let key = format!("{}\0{}", service_id, candidate_id);
        let service_dir = self
            .job_spool_path()
            .join(path_component(service_id))
            .join(path_component(candidate_id));
        if let Err(error) = write_capture(&service_dir, &state_json, &health_log, &log_bytes).await
        {
            metadata
                .capture_errors
                .push(format!("spool write: {error}"));
        }
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, metadata.clone());
        metadata
    }

    pub async fn finalize(&self) -> EvidenceSummary {
        let records = self.metadata();
        if records.is_empty() {
            return EvidenceSummary {
                status: "absent",
                failed_candidates: 0,
                archive_format: "tar",
                compression: "zstd",
                archive_size_bytes: None,
                services: Vec::new(),
                errors: Vec::new(),
            };
        }
        let spool = self.job_spool_path();
        let mut errors = records
            .iter()
            .flat_map(|record| record.capture_errors.iter().cloned())
            .collect::<Vec<_>>();
        if let Err(error) = write_manifest(&spool, &records).await {
            errors.push(format!("manifest: {error}"));
        }
        let archive_path = spool.with_extension("tar.zst");
        let archive_part = spool.with_extension("tar.zst.part");
        let archive_size_bytes = match archive_dir(&spool, &archive_part, &archive_path).await {
            Ok(()) => tokio::fs::metadata(&archive_path)
                .await
                .ok()
                .map(|m| m.len()),
            Err(error) => {
                errors.push(format!("archive: {error}"));
                None
            }
        };
        EvidenceSummary {
            status: if archive_size_bytes.is_some() {
                "available"
            } else {
                "incomplete"
            },
            failed_candidates: records.len(),
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes,
            services: records,
            errors,
        }
    }

    pub fn archive_path(&self) -> PathBuf {
        self.job_spool_path().with_extension("tar.zst")
    }

    pub async fn cleanup_after_commit(&self) {
        let _ = tokio::fs::remove_dir_all(self.job_spool_path()).await;
        let _ = tokio::fs::remove_file(self.archive_path()).await;
        let _ = tokio::fs::remove_file(self.job_spool_path().with_extension("tar.zst.part")).await;
    }
}

pub fn spool_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SPOOL_DIR_NAME)
}

pub async fn cleanup_orphaned_spools(db: &crate::db::Db, db_path: &Path) {
    let root = spool_root(db_path);
    let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
        return;
    };
    let _ = set_owner_only(&root);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if file_type.is_file() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let job_id = name
                .strip_suffix(".tar.zst")
                .or_else(|| name.strip_suffix(".tar.zst.part"));
            if let Some(job_id) = job_id
                && db.get_job(job_id).await.ok().flatten().is_none()
            {
                let _ = tokio::fs::remove_file(path).await;
            }
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let Some(job_id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if db.get_job(job_id).await.ok().flatten().is_none() {
            let _ = tokio::fs::remove_dir_all(&path).await;
            let _ = tokio::fs::remove_file(path.with_extension("tar.zst")).await;
        } else {
            let _ = set_owner_only(&path);
        }
    }
}

pub async fn recover_orphaned_evidence(db: &crate::db::Db, db_path: &Path) {
    let root = spool_root(db_path);
    let Ok(mut entries) = tokio::fs::read_dir(&root).await else {
        return;
    };
    let _ = set_owner_only(&root);
    while let Ok(Some(entry)) = entries.next_entry().await {
        let spool = entry.path();
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(job_id) = spool.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let _ = set_owner_only(&spool);
        let Some(job) = db.get_job(job_id).await.ok().flatten() else {
            continue;
        };
        if !matches!(
            job.status.as_str(),
            "success" | "failed" | "rolled_back" | "cancelled"
        ) {
            continue;
        }
        let manifest = spool.join("manifest.json");
        let Ok(manifest_bytes) = tokio::fs::read(&manifest).await else {
            continue;
        };
        let Ok(records) = serde_json::from_slice::<Vec<EvidenceMetadata>>(&manifest_bytes) else {
            continue;
        };
        if db
            .get_rollback_evidence_archive(job_id)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            let _ = tokio::fs::remove_dir_all(&spool).await;
            let _ = tokio::fs::remove_file(spool.with_extension("tar.zst")).await;
            let _ = tokio::fs::remove_file(spool.with_extension("tar.zst.part")).await;
            continue;
        }
        let archive_path = spool.with_extension("tar.zst");
        let part_path = spool.with_extension("tar.zst.part");
        if archive_dir(&spool, &part_path, &archive_path)
            .await
            .is_err()
        {
            continue;
        }
        let Ok(archive) = tokio::fs::read(&archive_path).await else {
            continue;
        };
        let summary = EvidenceSummary {
            status: "available",
            failed_candidates: records.len(),
            archive_format: "tar",
            compression: "zstd",
            archive_size_bytes: Some(archive.len() as u64),
            services: records,
            errors: Vec::new(),
        };
        if db
            .attach_rollback_evidence_archive(
                job_id,
                archive,
                &serde_json::to_value(&summary).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await
            .unwrap_or(false)
        {
            let _ = tokio::fs::remove_dir_all(&spool).await;
            let _ = tokio::fs::remove_file(archive_path).await;
            let _ = tokio::fs::remove_file(part_path).await;
        }
    }
}

fn truncate_complete_lines(input: &[u8]) -> (Vec<u8>, bool) {
    if input.len() <= MAX_LOG_BYTES {
        return (input.to_vec(), false);
    }
    let mut end = 0;
    for line in input.split_inclusive(|byte| *byte == b'\n') {
        if end + line.len() > MAX_LOG_BYTES {
            return (input[..end].to_vec(), true);
        }
        end += line.len();
    }
    (input[..end].to_vec(), end < input.len())
}

async fn write_capture(
    dir: &Path,
    state: &Value,
    health_log: &Value,
    logs: &[u8],
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    set_owner_only(dir)?;
    atomic_write(dir.join("state.json"), serde_json::to_vec(state)?).await?;
    atomic_write(dir.join("health.log"), serde_json::to_vec(health_log)?).await?;
    atomic_write(dir.join("container.log"), logs.to_vec()).await?;
    Ok(())
}

async fn write_manifest(dir: &Path, records: &[EvidenceMetadata]) -> anyhow::Result<()> {
    atomic_write(dir.join("manifest.json"), serde_json::to_vec(records)?).await
}

async fn atomic_write(path: PathBuf, bytes: Vec<u8>) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    set_owner_only(&tmp)?;
    tokio::fs::rename(tmp, path).await?;
    Ok(())
}

async fn archive_dir(source: &Path, part: &Path, final_path: &Path) -> anyhow::Result<()> {
    if !source.is_dir() {
        anyhow::bail!("evidence spool missing: {}", source.display());
    }
    let _ = tokio::fs::remove_file(part).await;
    let _ = tokio::fs::remove_file(final_path).await;
    let total_bytes = directory_size(source).await.unwrap_or(0);
    backup_helper::archive_directory(source, part, final_path, total_bytes).await
}

async fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut entries = tokio::fs::read_dir(current).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Ok(total)
}

fn path_component(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty() {
        result.push('_');
    }
    result
}

fn set_owner_only(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(if path.is_dir() { 0o700 } else { 0o600 });
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

pub fn derive_deadline(policy: &HealthPolicy, poll_interval: Duration) -> Duration {
    let interval = Duration::from_secs(policy.interval_seconds);
    let start_interval = Duration::from_secs(policy.start_interval_seconds);
    let seconds = policy
        .start_period_seconds
        .saturating_add(interval.max(start_interval).as_secs())
        .saturating_add(
            policy
                .retries
                .saturating_mul(interval.as_secs().saturating_add(policy.timeout_seconds)),
        )
        .saturating_add(poll_interval.as_secs());
    Duration::from_secs(seconds)
}

pub fn parse_health_policy(raw: &[u8]) -> Option<HealthPolicy> {
    let value: Value = serde_json::from_slice(raw).ok()?;
    if value.is_null() {
        return None;
    }
    let seconds = |name: &str, default: u64| {
        value
            .get(name)
            .and_then(Value::as_i64)
            .and_then(|n| u64::try_from(n).ok())
            // Docker durations are nanoseconds. Round each component upward so the resulting
            // integer policy can only extend, never shorten, the health-policy deadline.
            // An explicit zero uses Docker's documented default for duration fields.
            .map(|n| {
                if n == 0 {
                    default
                } else {
                    n.div_ceil(1_000_000_000)
                }
            })
            .unwrap_or(default)
    };
    Some(HealthPolicy {
        interval_seconds: seconds("Interval", 30),
        timeout_seconds: seconds("Timeout", 30),
        start_period_seconds: seconds("StartPeriod", 0),
        start_interval_seconds: seconds("StartInterval", 5),
        retries: value
            .get("Retries")
            .and_then(Value::as_i64)
            .and_then(|n| u64::try_from(n).ok())
            .unwrap_or(3),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{CommandOutput, CommandSpec, RawCommandOutput};

    struct EvidenceRunner;

    #[async_trait::async_trait]
    impl CommandRunner for EvidenceRunner {
        async fn run(
            &self,
            _spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<CommandOutput> {
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }

        async fn run_raw(
            &self,
            spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<RawCommandOutput> {
            if spec.args.iter().any(|arg| arg == "--timestamps") {
                let mut logs = b"candidate raw bytes\n".to_vec();
                logs.extend(std::iter::repeat_n(b'a', MAX_LOG_BYTES));
                logs.extend_from_slice(b"\n");
                return Ok(RawCommandOutput {
                    status: 0,
                    stdout: logs,
                    stderr: Vec::new(),
                });
            }
            Ok(RawCommandOutput {
                status: 0,
                stdout: br#"{"Status":"running","Error":"","ExitCode":1,"RestartCount":2,"Extra":["ignored-field"],"Health":{"Status":"starting","Log":[{"ExitCode":1,"Output":"not ready"}]}}"#.to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn truncates_only_complete_lines() {
        let input = vec![b'a'; MAX_LOG_BYTES + 10];
        let (output, truncated) = truncate_complete_lines(&input);
        assert!(output.is_empty());
        assert!(truncated);
    }

    #[test]
    fn derives_docker_policy_deadline() {
        let policy = HealthPolicy {
            interval_seconds: 30,
            timeout_seconds: 5,
            start_period_seconds: 60,
            start_interval_seconds: 5,
            retries: 6,
        };
        assert_eq!(
            derive_deadline(&policy, Duration::from_secs(2)),
            Duration::from_secs(302)
        );
    }

    #[test]
    fn rollback_evidence_health_policy_uses_candidate_effective_values() {
        let policy = parse_health_policy(
            br#"{"Interval":1000000000,"Timeout":2000000000,"StartPeriod":3000000000,"StartInterval":4000000000,"Retries":2}"#,
        )
        .expect("health policy should parse");
        assert_eq!(policy.interval_seconds, 1);
        assert_eq!(policy.timeout_seconds, 2);
        assert_eq!(policy.start_period_seconds, 3);
        assert_eq!(policy.start_interval_seconds, 4);
        assert_eq!(policy.retries, 2);
        assert_eq!(
            derive_deadline(&policy, Duration::from_secs(2)),
            Duration::from_secs(15)
        );

        let fractional = parse_health_policy(
            br#"{"Interval":1100000000,"Timeout":2100000000,"StartPeriod":3100000000,"StartInterval":4100000000,"Retries":2}"#,
        )
        .expect("fractional policy should parse conservatively");
        assert_eq!(fractional.interval_seconds, 2);
        assert_eq!(fractional.timeout_seconds, 3);
        assert_eq!(fractional.start_period_seconds, 4);
        assert_eq!(fractional.start_interval_seconds, 5);
        assert_eq!(
            derive_deadline(&fractional, Duration::from_secs(2)),
            Duration::from_secs(21)
        );

        let zero_values = parse_health_policy(
            br#"{"Interval":0,"Timeout":0,"StartPeriod":0,"StartInterval":0,"Retries":0}"#,
        )
        .expect("zero-valued policy should parse");
        assert_eq!(zero_values.interval_seconds, 30);
        assert_eq!(zero_values.timeout_seconds, 30);
        assert_eq!(zero_values.start_period_seconds, 0);
        assert_eq!(zero_values.start_interval_seconds, 5);
        assert_eq!(zero_values.retries, 0);
    }

    #[tokio::test]
    async fn rollback_evidence_archive_preserves_complete_lines_and_service_layout() {
        let root =
            std::env::temp_dir().join(format!("dockrev-rollback-evidence-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let context = RollbackEvidenceContext::new("job-1", &db_path).expect("spool");
        let metadata = context
            .capture_failure(
                &EvidenceRunner,
                &DockerRunnerConfig::default(),
                "service-a",
                "candidate-a",
                "starting",
                None,
                Some(Duration::from_secs(90)),
            )
            .await;

        assert_eq!(metadata.logs_bytes, b"candidate raw bytes\n".len());
        assert!(metadata.logs_truncated);
        let service_dir = context
            .job_spool_path()
            .join("service-a")
            .join("candidate-a");
        let logs = tokio::fs::read(service_dir.join("container.log"))
            .await
            .expect("raw logs");
        assert_eq!(logs.len(), b"candidate raw bytes\n".len());
        assert!(logs.ends_with(b"\n"));
        assert!(
            logs.windows(b"candidate raw bytes".len())
                .any(|window| window == b"candidate raw bytes")
        );
        let state: Value = serde_json::from_slice(
            &tokio::fs::read(service_dir.join("state.json"))
                .await
                .expect("state"),
        )
        .expect("state json");
        assert!(state.get("Extra").is_none());
        assert_eq!(state["Status"], "running");
        assert_eq!(state["RestartCount"], 2);
        let health_log: Value = serde_json::from_slice(
            &tokio::fs::read(service_dir.join("health.log"))
                .await
                .expect("health log"),
        )
        .expect("health log json");
        assert_eq!(health_log[0]["Output"], "not ready");

        let summary = context.finalize().await;
        assert_eq!(summary.status, "available");
        assert_eq!(summary.failed_candidates, 1);
        assert!(summary.archive_size_bytes.unwrap_or_default() > 0);
        assert!(context.archive_path().exists());

        context.cleanup_after_commit().await;
        assert!(!context.job_spool_path().exists());
        assert!(!context.archive_path().exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn cleanup_removes_archive_only_evidence_for_deleted_jobs() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-rollback-evidence-gc-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let evidence_root = spool_root(&db_path);
        tokio::fs::create_dir_all(&evidence_root)
            .await
            .expect("evidence root");
        tokio::fs::write(evidence_root.join("deleted-job.tar.zst"), b"archive")
            .await
            .expect("archive");
        tokio::fs::write(evidence_root.join("deleted-job.tar.zst.part"), b"partial")
            .await
            .expect("partial archive");

        cleanup_orphaned_spools(&db, &db_path).await;

        assert!(!evidence_root.join("deleted-job.tar.zst").exists());
        assert!(!evidence_root.join("deleted-job.tar.zst.part").exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
