use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Mutex;

use crate::{
    config::Config, db::Db, management_events::ManagementEventEntity, now_rfc3339, preflight,
    runner::CommandRunner,
};

pub const DEPLOY_CHECK_SNAPSHOT_KEY: &str = "global";
pub const DEPLOY_CHECK_PENDING_RETRY_AFTER_MS: u64 = 800;
pub const DEPLOY_CHECK_REPORT_STALE_AFTER_SECONDS: i64 = 30;

#[derive(Clone)]
pub struct DeployCheckRefreshWorker {
    db: Db,
    runner: Arc<dyn CommandRunner>,
    config: Config,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl DeployCheckRefreshWorker {
    pub fn new(db: Db, runner: Arc<dyn CommandRunner>, config: Config) -> Self {
        Self {
            db,
            runner,
            config,
            running: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
        }
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

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
            let mut last_error = self.last_error.lock().await;
            *last_error = result.err().map(|err| err.to_string());
            drop(last_error);

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
        let report =
            preflight::build_report_with_parts(&self.config, &self.db, self.runner.clone()).await?;
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
            "result": report.overall.result,
            "generatedAt": report.generated_at,
            "blockingCheckIds": report.overall.blocking_check_ids,
        });
        let entities = vec![ManagementEventEntity {
            entity_type: "report".to_string(),
            id: DEPLOY_CHECK_SNAPSHOT_KEY.to_string(),
        }];
        if report.overall.result == crate::api::types::DeployCheckResult::Fail {
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
}

pub fn deploy_check_report_is_fresh(checked_at: &str, now: time::OffsetDateTime) -> bool {
    let Ok(checked_at) =
        time::OffsetDateTime::parse(checked_at, &time::format_description::well_known::Rfc3339)
    else {
        return false;
    };
    (now - checked_at) <= time::Duration::seconds(DEPLOY_CHECK_REPORT_STALE_AFTER_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::{
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
}
