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
    docker_runner::{self, DockerRunnerConfig},
    runner::CommandRunner,
};

#[path = "rollback_evidence_archive.rs"]
mod archive;
use archive::{
    archive_dir, path_component, promote_log_file, recover_interrupted_capture, set_owner_only,
    write_capture, write_manifest,
};

#[path = "rollback_evidence_log_status.rs"]
mod log_status;
#[path = "rollback_evidence_summary.rs"]
mod summary;
pub(crate) use summary::{sanitize_evidence_metadata, sanitize_summary_for_api};
#[path = "rollback_evidence_recovery.rs"]
mod recovery;
#[cfg(test)]
pub(crate) use recovery::cleanup_orphaned_entry;
pub(crate) use recovery::cleanup_orphaned_spools;

const SPOOL_DIR_NAME: &str = "rollback-evidence-spool";
const LOG_CAPTURE_TIMEOUT_SECONDS: u64 = 300;
const CAPTURE_INTERRUPTED_REASON: &str = "logs capture interrupted before completion";
const MAX_SUMMARY_SERVICES: usize = 32;
const MAX_SUMMARY_CAPTURE_ERRORS: usize = 4;
const MAX_SUMMARY_FIELD_CHARS: usize = 256;
const MAX_SUMMARY_ERRORS: usize = 16;
const MAX_SUMMARY_ERROR_CHARS: usize = 512;
const SUMMARY_TRUNCATION_NOTE: &str =
    "rollback evidence summary metadata was truncated to remain bounded";

#[cfg(test)]
#[path = "rollback_evidence_privacy_tests.rs"]
mod privacy_tests;
#[cfg(test)]
#[path = "rollback_evidence_test_support.rs"]
mod test_support;

#[derive(Clone)]
pub struct RollbackEvidenceContext {
    job_id: String,
    root: PathBuf,
    records: Arc<Mutex<BTreeMap<String, EvidenceMetadata>>>,
    manifest_lock: Arc<tokio::sync::Mutex<()>>,
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
    pub logs_bytes: u64,
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

fn bounded_summary_services(records: &[EvidenceMetadata]) -> (Vec<EvidenceMetadata>, bool) {
    let mut truncated = records.len() > MAX_SUMMARY_SERVICES;
    let mut services = records
        .iter()
        .take(MAX_SUMMARY_SERVICES)
        .cloned()
        .collect::<Vec<_>>();
    for service in &mut services {
        truncated |= truncate_summary_text(&mut service.service_id, MAX_SUMMARY_FIELD_CHARS);
        truncated |= truncate_summary_text(&mut service.candidate_id, MAX_SUMMARY_FIELD_CHARS);
        truncated |= truncate_summary_text(&mut service.health_status, MAX_SUMMARY_FIELD_CHARS);
        if let Some(value) = service.state_status.as_mut() {
            truncated |= truncate_summary_text(value, MAX_SUMMARY_FIELD_CHARS);
        }
        // Docker state errors can contain runtime details; keep them in the private archive only.
        service.state_error = None;
        if service.capture_errors.len() > MAX_SUMMARY_CAPTURE_ERRORS {
            service.capture_errors.truncate(MAX_SUMMARY_CAPTURE_ERRORS);
            truncated = true;
        }
        for error in &mut service.capture_errors {
            truncated |= truncate_summary_text(error, MAX_SUMMARY_FIELD_CHARS);
        }
    }
    (services, truncated)
}

pub(crate) fn ordered_capture_errors(records: &[EvidenceMetadata]) -> Vec<String> {
    let visible_service_count = records.len().min(MAX_SUMMARY_SERVICES);
    records
        .iter()
        .skip(visible_service_count)
        .chain(records.iter().take(visible_service_count))
        .flat_map(|record| record.capture_errors.iter().cloned())
        .take(MAX_SUMMARY_ERRORS + 1)
        .collect()
}

pub(crate) fn bounded_summary_errors(
    mut errors: Vec<String>,
    metadata_truncated: bool,
) -> Vec<String> {
    let mut truncated = metadata_truncated || errors.len() > MAX_SUMMARY_ERRORS;
    errors.truncate(MAX_SUMMARY_ERRORS);
    for error in &mut errors {
        truncated |= truncate_summary_text(error, MAX_SUMMARY_ERROR_CHARS);
    }
    if truncated {
        if errors.len() == MAX_SUMMARY_ERRORS {
            errors.pop();
        }
        errors.push(SUMMARY_TRUNCATION_NOTE.to_string());
    }
    errors
}

fn truncate_summary_text(value: &mut String, max_chars: usize) -> bool {
    if value.chars().count() <= max_chars {
        return false;
    }
    let prefix = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    *value = format!("{prefix}...");
    true
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
            manifest_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub fn job_spool_path(&self) -> PathBuf {
        self.root.join(path_component(&self.job_id))
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
        let service_dir = self
            .job_spool_path()
            .join(path_component(service_id))
            .join(path_component(candidate_id));
        let log_part_path = service_dir.join("container.log.part");
        let mut metadata = EvidenceMetadata {
            service_id: service_id.to_string(),
            candidate_id: candidate_id.to_string(),
            health_status: health_status.to_string(),
            health_policy,
            health_policy_deadline_seconds: deadline.map(|value| value.as_secs()),
            logs_truncated: true,
            capture_errors: vec![CAPTURE_INTERRUPTED_REASON.to_string()],
            ..Default::default()
        };
        let mut state_json = serde_json::json!({});
        let mut health_log = Value::Array(Vec::new());
        let setup_result = async {
            tokio::fs::create_dir_all(&service_dir).await?;
            set_owner_only(&service_dir)?;
            write_capture(&service_dir, &state_json, &health_log).await
        }
        .await;
        let setup_failed = if let Err(error) = setup_result {
            metadata
                .capture_errors
                .push(format!("spool setup: {error}"));
            true
        } else {
            false
        };
        self.upsert_metadata(service_id, candidate_id, metadata.clone());
        if let Err(error) = self.persist_manifest().await {
            metadata
                .capture_errors
                .push(format!("manifest checkpoint: {error}"));
            self.upsert_metadata(service_id, candidate_id, metadata.clone());
            return metadata;
        }
        if setup_failed {
            return metadata;
        }

        let state_future = runner.run_raw(
            docker_runner::inspect_candidate_state(docker_cfg, candidate_id),
            Duration::from_secs(10),
        );
        let logs_future = runner.run_raw_to_file(
            docker_runner::logs_with_timestamps(docker_cfg, candidate_id),
            Duration::from_secs(LOG_CAPTURE_TIMEOUT_SECONDS),
            &log_part_path,
        );
        let (state_result, logs_result) = tokio::join!(state_future, logs_future);

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

        let logs_complete = match logs_result {
            Ok(output) => {
                metadata.logs_bytes = output.bytes_written;
                log_status::record_log_command_result(&mut metadata, &output)
            }
            Err(error) => {
                metadata
                    .capture_errors
                    .push(format!("logs capture: {error}"));
                false
            }
        };
        let log_file_committed = match tokio::fs::metadata(&log_part_path).await {
            Ok(_) => {
                if let Err(error) = set_owner_only(&log_part_path) {
                    metadata
                        .capture_errors
                        .push(format!("logs file permissions: {error}"));
                    false
                } else {
                    match promote_log_file(&log_part_path, &service_dir.join("container.log")).await
                    {
                        Ok(()) => true,
                        Err(error) => {
                            metadata
                                .capture_errors
                                .push(format!("logs file commit: {error}"));
                            false
                        }
                    }
                }
            }
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    metadata
                        .capture_errors
                        .push(format!("logs file inspect: {error}"));
                }
                false
            }
        };
        let stored_log_path = if log_file_committed {
            service_dir.join("container.log")
        } else {
            log_part_path.clone()
        };
        if let Some(bytes) = tokio::fs::metadata(&stored_log_path)
            .await
            .ok()
            .map(|value| value.len())
        {
            metadata.logs_bytes = bytes;
        }
        metadata.logs_truncated = !(logs_complete && log_file_committed);
        if log_file_committed {
            metadata
                .capture_errors
                .retain(|error| error != CAPTURE_INTERRUPTED_REASON);
        }

        if let Err(error) = write_capture(&service_dir, &state_json, &health_log).await {
            metadata
                .capture_errors
                .push(format!("spool write: {error}"));
        }
        self.upsert_metadata(service_id, candidate_id, metadata.clone());
        if let Err(error) = self.persist_manifest().await {
            metadata
                .capture_errors
                .push(format!("manifest update: {error}"));
            self.upsert_metadata(service_id, candidate_id, metadata.clone());
        }
        metadata
    }

    fn upsert_metadata(&self, service_id: &str, candidate_id: &str, metadata: EvidenceMetadata) {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(format!("{}\0{}", service_id, candidate_id), metadata);
    }

    async fn persist_manifest(&self) -> anyhow::Result<()> {
        let _guard = self.manifest_lock.lock().await;
        write_manifest(&self.job_spool_path(), &self.metadata()).await
    }

    pub async fn finalize(&self) -> EvidenceSummary {
        archive::finalize(self).await
    }

    pub fn archive_path(&self) -> PathBuf {
        self.job_spool_path().with_extension("tar.zst")
    }

    pub async fn cleanup_after_commit(&self) -> anyhow::Result<()> {
        let spool = self.job_spool_path();
        let archive = self.archive_path();
        let part = spool.with_extension("tar.zst.part");
        let mut errors = Vec::new();

        if let Err(error) = tokio::fs::remove_dir_all(&spool).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(format!("remove spool {}: {error}", spool.display()));
        }
        for (label, path) in [("archive", archive), ("partial archive", part)] {
            if let Err(error) = tokio::fs::remove_file(&path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(format!("remove {label} {}: {error}", path.display()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(errors.join("; "))
        }
    }
}

pub fn spool_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SPOOL_DIR_NAME)
}

pub async fn recover_orphaned_evidence(db: &crate::db::Db, db_path: &Path) {
    Box::pin(recovery::recover_evidence(db, db_path, false)).await;
}

pub async fn recover_startup_interrupted_evidence(db: &crate::db::Db, db_path: &Path) {
    Box::pin(recovery::recover_evidence(db, db_path, true)).await;
}

pub fn spawn_startup_interrupted_evidence_recovery(
    db: crate::db::Db,
    db_path: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        recover_startup_interrupted_evidence(&db, &db_path).await;
    })
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
    use crate::runner::{CommandOutput, CommandSpec, RawCommandOutput, RawFileCommandOutput};
    use tokio::io::AsyncWriteExt as _;

    struct EvidenceRunner {
        manifest_path: PathBuf,
        log_bytes: Vec<u8>,
    }

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
            if spec
                .args
                .iter()
                .any(|arg| arg == "--timestamps" || arg.contains("logs --timestamps"))
            {
                let manifest: Vec<EvidenceMetadata> = serde_json::from_slice(
                    &tokio::fs::read(&self.manifest_path)
                        .await
                        .expect("manifest checkpoint exists before logs start"),
                )
                .expect("manifest checkpoint is valid");
                assert!(manifest.iter().any(|record| {
                    record
                        .capture_errors
                        .iter()
                        .any(|error| error == CAPTURE_INTERRUPTED_REASON)
                }));
                return Ok(RawCommandOutput {
                    status: 0,
                    stdout: self.log_bytes.clone(),
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

    struct TimedOutEvidenceRunner;

    #[async_trait::async_trait]
    impl CommandRunner for TimedOutEvidenceRunner {
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
            _spec: CommandSpec,
            _timeout: Duration,
        ) -> anyhow::Result<RawCommandOutput> {
            Ok(RawCommandOutput {
                status: 0,
                stdout: br#"{"Status":"running","Error":"","ExitCode":0,"RestartCount":0,"Health":{"Log":[]}}"#.to_vec(),
                stderr: Vec::new(),
            })
        }

        async fn run_raw_to_file(
            &self,
            _spec: CommandSpec,
            _timeout: Duration,
            output_path: &Path,
        ) -> anyhow::Result<RawFileCommandOutput> {
            let partial = b"partial candidate output";
            tokio::fs::write(output_path, partial).await?;
            Ok(RawFileCommandOutput {
                status: -1,
                bytes_written: partial.len() as u64,
                stderr: Vec::new(),
                eof_reached: false,
                timed_out: true,
            })
        }
    }

    fn candidate_log_fixture() -> Vec<u8> {
        let mut bytes = (0..=250).cycle().take(1_048_593).collect::<Vec<_>>();
        bytes.push(0xff);
        bytes
    }

    #[test]
    fn job_spool_path_stays_beneath_private_root_for_parent_component() {
        assert_eq!(path_component("."), "_");
        assert_eq!(path_component(".."), "_");
        let parent =
            std::env::temp_dir().join(format!("dockrev-path-safety-{}", ulid::Ulid::new()));
        fs::create_dir_all(&parent).expect("create test root");
        let context = RollbackEvidenceContext::new("..", &parent.join("dockrev.sqlite"))
            .expect("create evidence context");
        fs::create_dir_all(context.job_spool_path()).expect("create job spool");

        let private_root = context.root.canonicalize().expect("canonical private root");
        let spool_path = context
            .job_spool_path()
            .canonicalize()
            .expect("canonical spool path");
        assert!(spool_path.starts_with(private_root));

        let _ = fs::remove_dir_all(parent);
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
    async fn rollback_evidence_archive_preserves_raw_bytes_and_service_layout() {
        let root =
            std::env::temp_dir().join(format!("dockrev-rollback-evidence-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let context = RollbackEvidenceContext::new("job-1", &db_path).expect("spool");
        let expected_logs = candidate_log_fixture();
        let metadata = context
            .capture_failure(
                &EvidenceRunner {
                    manifest_path: context.job_spool_path().join("manifest.json"),
                    log_bytes: expected_logs.clone(),
                },
                &DockerRunnerConfig::default(),
                "service-a",
                "candidate-a",
                "starting",
                None,
                Some(Duration::from_secs(90)),
            )
            .await;

        assert_eq!(metadata.logs_bytes, expected_logs.len() as u64);
        assert!(!metadata.logs_truncated);
        let service_dir = context
            .job_spool_path()
            .join("service-a")
            .join("candidate-a");
        let logs = tokio::fs::read(service_dir.join("container.log"))
            .await
            .expect("raw logs");
        assert_eq!(logs, expected_logs);
        assert!(logs.len() > 1024 * 1024);
        assert_eq!(logs.last(), Some(&0xff));
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

        let second_logs = b"service-b candidate bytes\xff".to_vec();
        let second_metadata = context
            .capture_failure(
                &EvidenceRunner {
                    manifest_path: context.job_spool_path().join("manifest.json"),
                    log_bytes: second_logs.clone(),
                },
                &DockerRunnerConfig::default(),
                "service-b",
                "candidate-b",
                "unhealthy",
                None,
                Some(Duration::from_secs(90)),
            )
            .await;
        assert_eq!(second_metadata.logs_bytes, second_logs.len() as u64);
        assert!(!second_metadata.logs_truncated);
        let second_service_dir = context
            .job_spool_path()
            .join("service-b")
            .join("candidate-b");
        assert_eq!(
            tokio::fs::read(second_service_dir.join("container.log"))
                .await
                .expect("second service raw logs"),
            second_logs
        );

        let summary = context.finalize().await;
        assert_eq!(summary.status, "available");
        assert_eq!(summary.failed_candidates, 2);
        assert!(summary.archive_size_bytes.unwrap_or_default() > 0);
        assert!(context.archive_path().exists());
        let manifest: Vec<EvidenceMetadata> = serde_json::from_slice(
            &super::test_support::archive_member(&context.archive_path(), "./manifest.json").await,
        )
        .expect("archive manifest");
        assert_eq!(manifest.len(), 2);
        assert_eq!(manifest[0].service_id, "service-a");
        assert_eq!(manifest[0].health_status, "starting");
        assert!(!manifest[0].logs_truncated);
        assert_eq!(manifest[1].service_id, "service-b");
        assert_eq!(manifest[1].health_status, "unhealthy");
        assert!(!manifest[1].logs_truncated);

        assert_eq!(
            super::test_support::archive_member(
                &context.archive_path(),
                "./service-a/candidate-a/container.log"
            )
            .await,
            expected_logs
        );
        assert_eq!(
            super::test_support::archive_member(
                &context.archive_path(),
                "./service-b/candidate-b/container.log"
            )
            .await,
            second_logs
        );

        context.cleanup_after_commit().await.expect("cleanup");
        assert!(!context.job_spool_path().exists());
        assert!(!context.archive_path().exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn committed_cleanup_reports_failures_and_attempts_remaining_copies() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-evidence-cleanup-error-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let evidence =
            RollbackEvidenceContext::new("job-cleanup-error", &root.join("dockrev.sqlite"))
                .expect("evidence context");
        tokio::fs::write(evidence.job_spool_path(), b"blocks directory cleanup")
            .await
            .expect("spool blocker");
        tokio::fs::write(evidence.archive_path(), b"committed archive copy")
            .await
            .expect("archive copy");
        let part = evidence.job_spool_path().with_extension("tar.zst.part");
        tokio::fs::write(&part, b"partial archive copy")
            .await
            .expect("partial archive copy");

        let error = evidence
            .cleanup_after_commit()
            .await
            .expect_err("spool deletion failure must be reported");

        assert!(error.to_string().contains("remove spool"));
        assert!(evidence.job_spool_path().exists());
        assert!(!evidence.archive_path().exists());
        assert!(!part.exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn finalized_summary_bounds_metadata_without_changing_archive_contents() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-evidence-summary-bounds-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let context =
            RollbackEvidenceContext::new("job-summary-bounds", &root.join("dockrev.sqlite"))
                .expect("evidence context");
        let long = "x".repeat(2_000);
        let raw_state_error = "docker state error: credential=private-marker";
        let records = (0..34)
            .map(|index| {
                let (service_id, candidate_id) = if index == 0 {
                    ("service-00".to_string(), "candidate-00".to_string())
                } else {
                    (
                        format!("service-{index:02}-{long}"),
                        format!("candidate-{index:02}-{long}"),
                    )
                };
                EvidenceMetadata {
                    service_id,
                    candidate_id,
                    health_status: long.clone(),
                    state_status: Some(long.clone()),
                    state_error: Some(if index == 0 {
                        raw_state_error.to_string()
                    } else {
                        long.clone()
                    }),
                    capture_errors: vec![long.clone(); 8],
                    logs_truncated: false,
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        for record in &records {
            context.upsert_metadata(&record.service_id, &record.candidate_id, record.clone());
        }
        let first = &records[0];
        let candidate_dir = context
            .job_spool_path()
            .join(path_component(&first.service_id))
            .join(path_component(&first.candidate_id));
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate directory");
        let expected_logs = b"raw candidate log\0\xff\n";
        tokio::fs::write(candidate_dir.join("container.log"), expected_logs)
            .await
            .expect("candidate log");

        let summary = context.finalize().await;

        assert_eq!(summary.status, "available");
        assert_eq!(summary.failed_candidates, 34);
        assert_eq!(summary.services.len(), 32);
        for service in &summary.services {
            for value in [
                Some(service.service_id.as_str()),
                Some(service.candidate_id.as_str()),
                Some(service.health_status.as_str()),
                service.state_status.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                assert!(value.chars().count() <= 256);
            }
            assert!(service.capture_errors.len() <= 4);
            assert!(
                service
                    .capture_errors
                    .iter()
                    .all(|error| error.chars().count() <= 256)
            );
        }
        assert!(
            summary
                .services
                .iter()
                .all(|service| service.state_error.is_none())
        );
        assert!(
            !serde_json::to_string(&summary)
                .expect("summary JSON")
                .contains(raw_state_error)
        );
        assert!(summary.errors.len() <= 16);
        assert!(
            summary
                .errors
                .iter()
                .any(|error| error == SUMMARY_TRUNCATION_NOTE)
        );
        assert!(
            summary
                .errors
                .iter()
                .all(|error| error.chars().count() <= 512)
        );

        let manifest: Vec<Value> = serde_json::from_slice(
            &test_support::archive_member(&context.archive_path(), "./manifest.json").await,
        )
        .expect("full archive manifest");
        assert_eq!(manifest.len(), records.len());
        assert_eq!(manifest[0], serde_json::to_value(first).unwrap());
        assert_eq!(manifest[0]["stateError"], raw_state_error);
        assert_eq!(
            test_support::archive_member(
                &context.archive_path(),
                &format!(
                    "./{}/{}/container.log",
                    path_component(&first.service_id),
                    path_component(&first.candidate_id)
                )
            )
            .await,
            expected_logs
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn partial_log_promotion_fallback_keeps_the_stable_archive_path() {
        let root =
            std::env::temp_dir().join(format!("dockrev-log-promotion-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let partial = root.join("container.log.part");
        let committed = root.join("container.log");
        let expected = b"partial raw candidate log\xff";
        tokio::fs::write(&partial, expected)
            .await
            .expect("partial log");

        archive::promote_log_file_after_rename(
            &partial,
            &committed,
            Err(std::io::Error::other("injected rename failure")),
        )
        .await
        .expect("fallback promotion");

        assert_eq!(tokio::fs::read(&committed).await.unwrap(), expected);
        assert!(!partial.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                std::fs::metadata(&committed)
                    .expect("promoted log metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn timed_out_log_capture_keeps_partial_file_and_records_incomplete_status() {
        let root =
            std::env::temp_dir().join(format!("dockrev-rollback-timeout-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let context = RollbackEvidenceContext::new("job-timeout", &root.join("dockrev.sqlite"))
            .expect("spool");

        let metadata = context
            .capture_failure(
                &TimedOutEvidenceRunner,
                &DockerRunnerConfig::default(),
                "service-a",
                "candidate-a",
                "unhealthy",
                None,
                None,
            )
            .await;

        assert!(metadata.logs_truncated);
        assert_eq!(
            metadata.logs_bytes,
            b"partial candidate output".len() as u64
        );
        assert!(
            metadata
                .capture_errors
                .iter()
                .any(|error| error.contains("timed out"))
        );
        assert!(
            !metadata
                .capture_errors
                .iter()
                .any(|error| error == CAPTURE_INTERRUPTED_REASON)
        );
        assert_eq!(
            tokio::fs::read(
                context
                    .job_spool_path()
                    .join("service-a/candidate-a/container.log")
            )
            .await
            .expect("partial log should be retained"),
            b"partial candidate output"
        );

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn recovery_archives_partial_logs_from_an_interrupted_capture() {
        let root =
            std::env::temp_dir().join(format!("dockrev-rollback-recovery-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job = crate::api::types::JobRecord::new_running(
            "job-interrupted".to_string(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            None,
            None,
            "2026-08-28T00:00:00Z",
        )
        .to_db();
        db.insert_job(job).await.expect("insert job");
        db.finish_job(
            "job-interrupted",
            "rolled_back",
            "2026-08-28T00:01:00Z",
            &serde_json::json!({"status":"rolled_back"}),
        )
        .await
        .expect("finish interrupted job");

        let spool = spool_root(&db_path).join("job-interrupted");
        let candidate_dir = spool.join("service-a/candidate-a");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate spool");
        tokio::fs::write(
            candidate_dir.join("container.log.part"),
            b"partial raw log\xff",
        )
        .await
        .expect("partial logs");
        tokio::fs::write(candidate_dir.join("state.json"), b"{}")
            .await
            .expect("state");
        tokio::fs::write(candidate_dir.join("health.log"), b"[]")
            .await
            .expect("health log");
        write_manifest(
            &spool,
            &[EvidenceMetadata {
                service_id: "service-a".to_string(),
                candidate_id: "candidate-a".to_string(),
                health_status: "unhealthy".to_string(),
                logs_truncated: true,
                capture_errors: vec![CAPTURE_INTERRUPTED_REASON.to_string()],
                ..Default::default()
            }],
        )
        .await
        .expect("checkpoint manifest");

        let ((), ()) = tokio::join!(
            recover_orphaned_evidence(&db, &db_path),
            recover_orphaned_evidence(&db, &db_path)
        );

        let archive = db
            .get_rollback_evidence_archive("job-interrupted")
            .await
            .expect("load recovered archive")
            .expect("recovered archive is attached");
        let job = db
            .get_job("job-interrupted")
            .await
            .expect("load recovered job")
            .expect("job exists");
        let recovered_service = &job.summary_json["rollbackEvidence"]["services"][0];
        assert_eq!(recovered_service["logsTruncated"], true);
        assert_eq!(recovered_service["logsBytes"], b"partial raw log\xff".len());
        assert!(
            recovered_service["captureErrors"]
                .as_array()
                .expect("capture errors")
                .iter()
                .any(|error| error == CAPTURE_INTERRUPTED_REASON)
        );
        assert!(!spool.exists());

        let archive_path = root.join("recovered.tar.zst");
        tokio::fs::write(&archive_path, archive)
            .await
            .expect("write archive fixture");
        let decompressed = tokio::process::Command::new("zstd")
            .arg("-dc")
            .arg(&archive_path)
            .output()
            .await
            .expect("zstd should read evidence archive");
        assert!(decompressed.status.success());
        let mut tar = tokio::process::Command::new("tar")
            .args(["-xOf", "-", "./service-a/candidate-a/container.log"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("tar should read evidence archive");
        let mut tar_stdin = tar.stdin.take().expect("tar stdin");
        let tar_bytes = decompressed.stdout;
        let write_task = tokio::spawn(async move {
            tar_stdin.write_all(&tar_bytes).await?;
            anyhow::Ok(())
        });
        let extracted = tar
            .wait_with_output()
            .await
            .expect("tar extraction should complete");
        write_task
            .await
            .expect("tar input task")
            .expect("tar input should be written");
        assert!(extracted.status.success());
        assert_eq!(extracted.stdout, b"partial raw log\xff");

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn startup_recovery_attaches_interrupted_partial_logs_after_job_recovery() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-rollback-recovery-running-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job = crate::api::types::JobRecord::new_running(
            "job-interrupted-running".to_string(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            None,
            None,
            "2026-08-28T00:00:00Z",
        )
        .to_db();
        db.insert_job(job).await.expect("insert job");

        let spool = spool_root(&db_path).join("job-interrupted-running");
        let candidate_dir = spool.join("service-a/candidate-a");
        tokio::fs::create_dir_all(&candidate_dir)
            .await
            .expect("candidate spool");
        tokio::fs::write(candidate_dir.join("container.log.part"), b"partial log\xff")
            .await
            .expect("partial logs");
        tokio::fs::write(candidate_dir.join("state.json"), b"{}")
            .await
            .expect("state");
        tokio::fs::write(candidate_dir.join("health.log"), b"[]")
            .await
            .expect("health log");
        write_manifest(
            &spool,
            &[EvidenceMetadata {
                service_id: "service-a".to_string(),
                candidate_id: "candidate-a".to_string(),
                health_status: "unhealthy".to_string(),
                logs_truncated: true,
                capture_errors: vec![CAPTURE_INTERRUPTED_REASON.to_string()],
                ..Default::default()
            }],
        )
        .await
        .expect("checkpoint manifest");

        db.recover_incomplete_jobs("2026-08-28T00:01:00Z", "server_restart")
            .await
            .expect("recover interrupted job");
        recover_startup_interrupted_evidence(&db, &db_path).await;

        let job = db
            .get_job("job-interrupted-running")
            .await
            .expect("load job")
            .expect("job exists");
        assert_eq!(job.status, "failed");
        assert_eq!(job.summary_json["rollbackEvidence"]["status"], "available");
        assert_eq!(
            job.summary_json["rollbackEvidence"]["services"][0]["logsTruncated"],
            true
        );
        assert!(
            db.get_rollback_evidence_archive("job-interrupted-running")
                .await
                .expect("load archive")
                .is_some()
        );
        assert!(!spool.exists());
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn successful_job_finalization_creates_no_job_evidence_files() {
        let root =
            std::env::temp_dir().join(format!("dockrev-rollback-absent-{}", ulid::Ulid::new()));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        let job = crate::api::types::JobRecord::new_running(
            "job-success".to_string(),
            crate::api::types::JobType::Update,
            crate::api::types::JobScope::Service,
            None,
            None,
            "2026-08-28T00:00:00Z",
        )
        .to_db();
        db.insert_job(job).await.expect("insert job");
        let context = RollbackEvidenceContext::new("job-success", &db_path).expect("spool context");

        let summary = context.finalize().await;
        db.finish_job_with_archive(
            "job-success",
            "success",
            "2026-08-28T00:01:00Z",
            &serde_json::json!({"status":"success"}),
            None,
        )
        .await
        .expect("finish successful job without evidence");

        assert_eq!(summary.status, "absent");
        assert!(context.metadata().is_empty());
        assert!(!context.job_spool_path().exists());
        assert!(!context.archive_path().exists());
        let job = db
            .get_job("job-success")
            .await
            .expect("load job")
            .expect("job exists");
        assert!(job.summary_json.get("rollbackEvidence").is_none());
        assert!(
            db.get_rollback_evidence_archive("job-success")
                .await
                .expect("load archive")
                .is_none()
        );
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

    #[tokio::test]
    async fn startup_recovery_removes_archive_only_files_for_committed_jobs() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-rollback-committed-archive-only-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        db.insert_job(
            crate::api::types::JobRecord::new_running(
                "job-committed-archive-only".to_string(),
                crate::api::types::JobType::Update,
                crate::api::types::JobScope::Service,
                None,
                None,
                "2026-08-28T00:00:00Z",
            )
            .to_db(),
        )
        .await
        .expect("insert job");
        let committed_archive = b"committed evidence archive";
        db.finish_job_with_archive(
            "job-committed-archive-only",
            "rolled_back",
            "2026-08-28T00:05:00Z",
            &serde_json::json!({
                "rollbackEvidence": {
                    "status": "available",
                    "archiveSizeBytes": committed_archive.len()
                }
            }),
            Some(committed_archive.to_vec()),
        )
        .await
        .expect("commit archive");
        let evidence_root = spool_root(&db_path);
        tokio::fs::create_dir_all(&evidence_root)
            .await
            .expect("evidence root");
        let archive_path = evidence_root.join("job-committed-archive-only.tar.zst");
        let part_path = evidence_root.join("job-committed-archive-only.tar.zst.part");
        tokio::fs::write(&archive_path, b"duplicate local archive")
            .await
            .expect("local archive");
        tokio::fs::write(&part_path, b"partial archive")
            .await
            .expect("partial archive");

        recover_startup_interrupted_evidence(&db, &db_path).await;

        assert!(!archive_path.exists());
        assert!(!part_path.exists());
        assert_eq!(
            db.get_rollback_evidence_archive("job-committed-archive-only")
                .await
                .expect("load archive")
                .expect("committed archive"),
            committed_archive
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn recovery_archive_failure_records_incomplete_and_preserves_spool() {
        let root = std::env::temp_dir().join(format!(
            "dockrev-rollback-recovery-failure-{}",
            ulid::Ulid::new()
        ));
        fs::create_dir_all(&root).expect("test root");
        let db_path = root.join("dockrev.sqlite");
        let db = crate::db::Db::open(&db_path).await.expect("db");
        db.insert_job(
            crate::api::types::JobRecord::new_running(
                "job-recovery-failure".to_string(),
                crate::api::types::JobType::Update,
                crate::api::types::JobScope::Service,
                None,
                None,
                "2026-08-28T00:00:00Z",
            )
            .to_db(),
        )
        .await
        .expect("insert job");
        db.finish_job(
            "job-recovery-failure",
            "failed",
            "2026-08-28T00:01:00Z",
            &serde_json::json!({"retained": "job summary"}),
        )
        .await
        .expect("finish job");

        let spool = spool_root(&db_path).join("job-recovery-failure");
        let candidate = spool.join("service-a/candidate-a");
        tokio::fs::create_dir_all(&candidate)
            .await
            .expect("candidate spool");
        tokio::fs::write(
            candidate.join("container.log"),
            b"partial raw candidate logs",
        )
        .await
        .expect("candidate log");
        tokio::fs::write(candidate.join("state.json"), b"{}")
            .await
            .expect("candidate state");
        tokio::fs::write(candidate.join("health.log"), b"[]")
            .await
            .expect("candidate health log");
        write_manifest(
            &spool,
            &[EvidenceMetadata {
                service_id: "service-a".to_string(),
                candidate_id: "candidate-a".to_string(),
                health_status: "unhealthy".to_string(),
                logs_bytes: 26,
                logs_truncated: true,
                ..Default::default()
            }],
        )
        .await
        .expect("manifest");
        let archive_path = spool.with_extension("tar.zst");
        tokio::fs::create_dir_all(&archive_path)
            .await
            .expect("block archive destination");

        recover_orphaned_evidence(&db, &db_path).await;

        let job = db
            .get_job("job-recovery-failure")
            .await
            .expect("load job")
            .expect("job exists");
        assert_eq!(job.status, "failed");
        assert_eq!(job.summary_json["retained"], "job summary");
        assert_eq!(job.summary_json["rollbackEvidence"]["status"], "incomplete");
        assert_eq!(
            job.summary_json["rollbackEvidence"]["services"][0]["logsTruncated"],
            true
        );
        assert!(
            !job.summary_json["rollbackEvidence"]["errors"][0]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tokio::fs::read(candidate.join("container.log"))
                .await
                .expect("preserved raw logs"),
            b"partial raw candidate logs"
        );
        assert!(spool.exists());
        assert!(archive_path.is_dir());
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
