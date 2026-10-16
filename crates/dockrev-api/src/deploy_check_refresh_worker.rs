use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use serde::Serialize;
use tokio::sync::Mutex;

use crate::{
    api::types::{
        DeployCheckGroup, DeployCheckOperationImpact, DeployCheckResult, DeployCheckStartupStatus,
        DeployCheckStatus,
    },
    config::Config,
    db::Db,
    management_events::ManagementEventEntity,
    now_rfc3339, preflight,
    runner::CommandRunner,
};

pub const DEPLOY_CHECK_SNAPSHOT_KEY: &str = "global";
pub const DEPLOY_CHECK_PENDING_RETRY_AFTER_MS: u64 = 800;
#[cfg(test)]
pub const DEPLOY_CHECK_REPORT_STALE_AFTER_SECONDS: i64 = 30;
pub const DEPLOY_CHECK_REVIEW_INTERVAL_SECONDS: u64 = 300;
pub const DEPLOY_CHECK_REVIEW_BUDGET_SECONDS: u64 = 30;
const DEPLOY_CHECK_CHANGE_DEBOUNCE_MS: u64 = 1_000;

pub(crate) fn input_fingerprint<T: Serialize>(namespace: &str, value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{namespace}:{hash:016x}")
}

#[derive(Clone)]
pub struct DeployCheckRefreshWorker {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    config: Config,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    scheduler_started: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    last_input_fingerprint: Arc<Mutex<Option<String>>>,
    change_generation: Arc<AtomicU64>,
    server_boot_id: Arc<String>,
    report_revision: Arc<AtomicU64>,
    startup_state: Arc<Mutex<StartupState>>,
}

#[derive(Default)]
struct StartupState {
    first_report_completed: bool,
    initial_failure_ids: BTreeSet<String>,
    initial_failure_key: Option<String>,
    initial_failure_active: bool,
    recovery_pending: bool,
}

impl DeployCheckRefreshWorker {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>, config: Config) -> Self {
        Self {
            db,
            runner,
            config,
            running: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            scheduler_started: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
            last_input_fingerprint: Arc::new(Mutex::new(None)),
            change_generation: Arc::new(AtomicU64::new(0)),
            server_boot_id: Arc::new(ulid::Ulid::new().to_string()),
            report_revision: Arc::new(AtomicU64::new(0)),
            startup_state: Arc::new(Mutex::new(StartupState::default())),
        }
    }

    pub fn spawn_after_startup(self: &Arc<Self>) {
        if self
            .scheduler_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            let _ = worker.enqueue().await;
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(
                DEPLOY_CHECK_REVIEW_INTERVAL_SECONDS,
            ));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _ = worker.enqueue().await;
            }
        });
    }

    pub async fn enqueue(&self) -> bool {
        self.pending.store(true, Ordering::SeqCst);
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        let worker = self.clone();
        tokio::spawn(async move {
            worker.run_loop().await;
        });
        true
    }

    pub async fn enqueue_for_change(&self, fingerprint: impl Into<String>) -> bool {
        // The startup scan is followed by the scheduler's explicit first run;
        // changes observed before that boundary are covered by that run.
        if !self.scheduler_started.load(Ordering::SeqCst) {
            return false;
        }
        let fingerprint = fingerprint.into();
        let mut last = self.last_input_fingerprint.lock().await;
        if last.as_deref() == Some(fingerprint.as_str()) {
            return false;
        }
        *last = Some(fingerprint);
        drop(last);
        let generation = self.change_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let worker = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(
                DEPLOY_CHECK_CHANGE_DEBOUNCE_MS,
            ))
            .await;
            if worker.change_generation.load(Ordering::SeqCst) == generation {
                let _ = worker.enqueue().await;
            }
        });
        true
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn server_boot_id(&self) -> &str {
        self.server_boot_id.as_str()
    }

    pub async fn last_error(&self) -> Option<String> {
        self.last_error.lock().await.clone()
    }

    #[cfg(test)]
    pub async fn set_last_error_for_test(&self, value: Option<String>) {
        *self.last_error.lock().await = value;
    }

    async fn run_loop(self) {
        loop {
            self.pending.store(false, Ordering::SeqCst);
            let result = self.refresh_once().await;
            match result {
                Ok(()) => {
                    *self.last_error.lock().await = None;
                }
                Err(err) => {
                    let message = err.to_string();
                    *self.last_error.lock().await = Some(message.clone());
                    self.publish_error(&message).await;
                }
            }

            if self.pending.swap(false, Ordering::SeqCst) {
                continue;
            }

            self.running.store(false, Ordering::SeqCst);
            if self.pending.load(Ordering::SeqCst)
                && self
                    .running
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                continue;
            }
            break;
        }
    }

    async fn refresh_once(&self) -> anyhow::Result<()> {
        let mut report =
            preflight::build_report_with_parts(&self.config, &self.db, self.runner.clone()).await?;
        let (startup_status, startup_failure_key, recovered) =
            self.startup_projection(&report).await;
        report.server_boot_id = Some(self.server_boot_id.to_string());
        report.report_boot_id = Some(self.server_boot_id.to_string());
        report.report_revision = Some(self.report_revision.fetch_add(1, Ordering::SeqCst) + 1);
        report.startup_status = Some(startup_status.clone());
        report.startup_failure_key = startup_failure_key.clone();
        report.operation_impacts = operation_impacts(&report);
        let now = now_rfc3339()?;
        let checked_at = report.generated_at.clone();
        let report_json = serde_json::to_string(&report)?;
        self.db
            .upsert_deploy_check_report_snapshot(
                DEPLOY_CHECK_SNAPSHOT_KEY,
                &report_json,
                &checked_at,
                &now,
            )
            .await?;
        let summary = serde_json::json!({
            "status": "ready",
            "result": report.overall.result,
            "generatedAt": report.generated_at,
            "blockingCheckIds": report.overall.blocking_check_ids,
            "serverBootId": report.server_boot_id,
            "reportBootId": report.report_boot_id,
            "reportRevision": report.report_revision,
            "startupStatus": report.startup_status,
            "startupFailureKey": report.startup_failure_key,
            "operationImpacts": report.operation_impacts,
        });
        let entities = vec![ManagementEventEntity {
            entity_type: "report".to_string(),
            id: DEPLOY_CHECK_SNAPSHOT_KEY.to_string(),
        }];
        if report.overall.result == DeployCheckResult::Fail || recovered {
            self.db
                .management_events()
                .publish_immediate("deploy_check", entities, summary)
                .await;
        } else {
            self.db
                .management_events()
                .publish_change("deploy_check", "report", DEPLOY_CHECK_SNAPSHOT_KEY, summary)
                .await;
        }
        Ok(())
    }

    async fn startup_projection(
        &self,
        report: &crate::api::types::DeployCheckReportResponse,
    ) -> (DeployCheckStartupStatus, Option<String>, bool) {
        let failed_ids = report
            .checks
            .iter()
            .filter(|item| {
                item.group == DeployCheckGroup::Core && item.status == DeployCheckStatus::Fail
            })
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        let current_key = (!failed_ids.is_empty())
            .then(|| format!("{}:{}", self.server_boot_id, failed_ids.join(",")));
        let mut state = self.startup_state.lock().await;
        if !state.first_report_completed {
            state.first_report_completed = true;
            state.initial_failure_ids = failed_ids.iter().map(|id| (*id).to_string()).collect();
            state.initial_failure_key = current_key.clone();
            state.initial_failure_active = current_key.is_some();
            return (
                if current_key.is_some() {
                    DeployCheckStartupStatus::Fail
                } else {
                    DeployCheckStartupStatus::Pass
                },
                current_key,
                false,
            );
        }

        let was_active = state.initial_failure_active;
        let initial_failure_still_active = state
            .initial_failure_ids
            .iter()
            .any(|id| failed_ids.iter().any(|failed_id| failed_id == id));
        if state.initial_failure_active && !initial_failure_still_active {
            state.initial_failure_active = false;
            state.recovery_pending = true;
        }
        if state.initial_failure_active {
            return (
                DeployCheckStartupStatus::Fail,
                state.initial_failure_key.clone(),
                false,
            );
        }
        let recovered = was_active && !state.initial_failure_active && state.recovery_pending;
        state.recovery_pending = false;
        (DeployCheckStartupStatus::Settled, None, recovered)
    }

    async fn publish_error(&self, message: &str) {
        let entities = vec![ManagementEventEntity {
            entity_type: "report".to_string(),
            id: DEPLOY_CHECK_SNAPSHOT_KEY.to_string(),
        }];
        self.db
            .management_events()
            .publish_immediate(
                "deploy_check",
                entities,
                serde_json::json!({
                    "status": "error",
                    "serverBootId": self.server_boot_id.as_str(),
                    "error": message,
                }),
            )
            .await;
    }
}

fn operation_impacts(
    report: &crate::api::types::DeployCheckReportResponse,
) -> Vec<DeployCheckOperationImpact> {
    report
        .checks
        .iter()
        .filter(|item| {
            item.group == DeployCheckGroup::Core && item.status == DeployCheckStatus::Fail
        })
        .map(|item| {
            let (scope, operations) = match item.id.as_str() {
                "core.docker_engine" => (
                    "global",
                    vec!["update", "rollback", "start", "stop", "restart", "cleanup"],
                ),
                "core.update_executor_ready" => (
                    "global",
                    vec!["update", "rollback", "start", "stop", "restart"],
                ),
                "core.compose_access" => (
                    "affected_projects",
                    vec!["update", "rollback", "start", "stop", "restart"],
                ),
                "core.service_image_ref_valid" => ("affected_services", vec!["update"]),
                _ => ("unknown", vec![]),
            };
            DeployCheckOperationImpact {
                check_id: item.id.clone(),
                scope: scope.to_string(),
                object_ids: Vec::new(),
                operations: operations.into_iter().map(str::to_string).collect(),
                reason: item.impact.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::{
        api::types::{
            DeployCheckGroup, DeployCheckItem, DeployCheckOverall, DeployCheckReportResponse,
            DeployCheckResult, DeployCheckStatus,
        },
        config::Config,
        db::Db,
        runner::{CommandOutput, CommandRunner, CommandSpec},
    };

    #[derive(Clone, Default)]
    struct SlowSuccessRunner {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for SlowSuccessRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _timeout: std::time::Duration,
        ) -> anyhow::Result<CommandOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if spec.program == "docker" && spec.args.first().map(String::as_str) == Some("info") {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "27.0.0\n".to_string(),
                    stderr: String::new(),
                });
            }
            if spec.program == "docker-compose"
                && spec.args.first().map(String::as_str) == Some("version")
            {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "Docker Compose version v2.27.0\n".to_string(),
                    stderr: String::new(),
                });
            }
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn test_config(db_path: &str) -> Config {
        Config {
            app_effective_version: "0.1.0".to_string(),
            http_addr: "127.0.0.1:0".to_string(),
            db_path: PathBuf::from(db_path),
            managed_override_dir: PathBuf::from("/tmp/dockrev-managed-overrides"),
            supervisor_state_path: None,
            metrics_db_path: PathBuf::from(format!("{db_path}.metrics.sqlite3")),
            docker_config_path: None,
            compose_bin: "docker-compose".to_string(),
            auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
            auth_group_header_name: "Remote-Groups".parse().unwrap(),
            auth_allowed_user: None,
            auth_allowed_group: None,
            auth_allow_anonymous_in_dev: true,
            self_upgrade_url: "/supervisor/".to_string(),
            dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
            webhook_secret: Some("secret".to_string()),
            host_platform: Some("linux/amd64".to_string()),
            discovery_interval_seconds: 60,
            discovery_max_actions: 200,
            runtime_scan_interval_seconds: 600,
            deploy_check_local_command_timeout_seconds: 8,
            registry_per_host_concurrency: crate::config::FIXED_REGISTRY_PER_HOST_CONCURRENCY,
            registry_retry_max_attempts: 3,
            registry_retry_base_ms: 250,
            registry_retry_max_ms: 2000,
            registry_rate_limit_cooldown_seconds: 21600,
            update_idempotent_retry_max_attempts: 3,
            update_idempotent_retry_base_ms: 300,
            update_idempotent_retry_max_ms: 3000,
        }
    }

    #[tokio::test]
    async fn enqueue_during_shutdown_window_is_not_lost() {
        let db_path = format!(
            "/tmp/dockrev-deploy-check-worker-race-{}.sqlite3",
            ulid::Ulid::new()
        );
        let config = test_config(&db_path);
        let db = Db::open(&config.db_path).await.unwrap();
        let runner = Arc::new(SlowSuccessRunner::default());
        let worker = DeployCheckRefreshWorker::new(db.clone(), runner.clone(), config);

        assert!(worker.enqueue().await);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!worker.enqueue().await);

        for _ in 0..200 {
            if !worker.is_running() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(!worker.is_running(), "worker did not finish in time");
        assert!(
            runner.calls.load(Ordering::SeqCst) >= 4,
            "second refresh should not be lost"
        );

        let row = db
            .get_deploy_check_report_snapshot(DEPLOY_CHECK_SNAPSHOT_KEY)
            .await
            .unwrap()
            .expect("snapshot should be persisted");
        assert!(!row.checked_at.is_empty());
    }

    fn report_with_core_status(status: DeployCheckStatus, id: &str) -> DeployCheckReportResponse {
        DeployCheckReportResponse {
            overall: DeployCheckOverall {
                result: if status == DeployCheckStatus::Fail {
                    DeployCheckResult::Fail
                } else {
                    DeployCheckResult::Pass
                },
                blocking_check_ids: if status == DeployCheckStatus::Fail {
                    vec![id.to_string()]
                } else {
                    Vec::new()
                },
                summary: "test".to_string(),
            },
            generated_at: "2026-10-16T00:00:00Z".to_string(),
            checks: vec![DeployCheckItem {
                id: id.to_string(),
                title: "test".to_string(),
                group: DeployCheckGroup::Core,
                required: true,
                status,
                na_reason: None,
                summary: "test".to_string(),
                impact: "test".to_string(),
                evidence: "test".to_string(),
                recommendation: String::new(),
            }],
            server_boot_id: None,
            report_boot_id: None,
            report_revision: None,
            startup_status: None,
            startup_failure_key: None,
            operation_impacts: Vec::new(),
        }
    }

    #[tokio::test]
    async fn startup_failure_key_is_stable_and_recovery_is_one_shot() {
        let db_path = format!(
            "/tmp/dockrev-deploy-check-worker-startup-{}.sqlite3",
            ulid::Ulid::new()
        );
        let config = test_config(&db_path);
        let db = Db::open(&config.db_path).await.unwrap();
        let worker =
            DeployCheckRefreshWorker::new(db, Arc::new(SlowSuccessRunner::default()), config);

        let first = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Fail,
                "core.docker_engine",
            ))
            .await;
        assert_eq!(first.0, crate::api::types::DeployCheckStartupStatus::Fail);
        let key = first.1.clone().expect("initial failure key");
        assert!(!first.2);

        let continuing = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Fail,
                "core.docker_engine",
            ))
            .await;
        assert_eq!(
            continuing.0,
            crate::api::types::DeployCheckStartupStatus::Fail
        );
        assert_eq!(continuing.1.as_deref(), Some(key.as_str()));
        assert!(!continuing.2);

        let recovered = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Pass,
                "core.docker_engine",
            ))
            .await;
        assert_eq!(
            recovered.0,
            crate::api::types::DeployCheckStartupStatus::Settled
        );
        assert!(recovered.1.is_none());
        assert!(recovered.2);

        let later_failure = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Fail,
                "core.docker_engine",
            ))
            .await;
        assert!(later_failure.1.is_none());
        assert!(!later_failure.2);
    }

    #[tokio::test]
    async fn startup_failure_key_clears_when_initial_failure_is_replaced() {
        let db_path = format!(
            "/tmp/dockrev-deploy-check-worker-replacement-{}.sqlite3",
            ulid::Ulid::new()
        );
        let config = test_config(&db_path);
        let db = Db::open(&config.db_path).await.unwrap();
        let worker =
            DeployCheckRefreshWorker::new(db, Arc::new(SlowSuccessRunner::default()), config);

        let first = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Fail,
                "core.docker_engine",
            ))
            .await;
        assert!(first.1.is_some());

        let replacement = worker
            .startup_projection(&report_with_core_status(
                DeployCheckStatus::Fail,
                "core.compose_access",
            ))
            .await;
        assert_eq!(
            replacement.0,
            crate::api::types::DeployCheckStartupStatus::Settled
        );
        assert!(replacement.1.is_none());
        assert!(replacement.2);
    }
}
