use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use crate::{
    api, compose,
    config::Config,
    db::Db,
    ids,
    registry::{ImageRef, ManifestInfo, RegistryClient},
    runner::{CommandOutput, CommandRunner, CommandSpec},
    state::AppState,
};

async fn response_json(resp: axum::response::Response) -> serde_json::Value {
    let payload = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&payload).unwrap()
}

#[derive(Clone, Default)]
struct FakeRegistry;

#[async_trait::async_trait]
impl RegistryClient for FakeRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2".to_string(), "5.3".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let digest = match reference {
            "5.2" => "sha256:old",
            "5.3" => "sha256:new",
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone, Default)]
struct FakeRunner;

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(&self, _spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
struct StatefulRegistry {
    calls: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u32>>>,
}

impl Default for StatefulRegistry {
    fn default() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl RegistryClient for StatefulRegistry {
    async fn list_tags(&self, _image: &ImageRef) -> anyhow::Result<Vec<String>> {
        Ok(vec!["5.2.0".to_string(), "5.3.0".to_string()])
    }

    async fn get_manifest(
        &self,
        _image: &ImageRef,
        reference: &str,
        _host_platform: &str,
    ) -> anyhow::Result<ManifestInfo> {
        let mut calls = self.calls.lock().unwrap();
        let count = calls.entry(reference.to_string()).or_insert(0);
        *count += 1;

        // Simulate a transient failure for the would-be candidate tag on its first lookup.
        if reference == "5.3.0" && *count == 1 {
            return Err(anyhow::anyhow!("transient registry error"));
        }

        let digest = match reference {
            "5.2.0" => "sha256:other",
            "5.3.0" => "sha256:match",
            // For floating tags (e.g. latest), we don't rely on this value in the test.
            _ => "sha256:unknown",
        };
        Ok(ManifestInfo {
            digest: Some(digest.to_string()),
            arch: vec!["linux/amd64".to_string()],
        })
    }
}

#[derive(Clone)]
struct ScriptedRunner {
    calls: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl Default for ScriptedRunner {
    fn default() -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl CommandRunner for ScriptedRunner {
    async fn run(&self, spec: CommandSpec, _timeout: Duration) -> anyhow::Result<CommandOutput> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let args = spec.args;
        let (status, stdout) = if args.first().map(|s| s.as_str()) == Some("ps")
            && args.get(1).map(|s| s.as_str()) == Some("-q")
        {
            (0, "cid1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("inspect")
            && args.get(1).map(|s| s.as_str()) == Some("--format")
            && args.get(2).map(|s| s.as_str()) == Some("{{.Image}}")
        {
            (0, "img1\n".to_string())
        } else if args.first().map(|s| s.as_str()) == Some("image")
            && args.get(1).map(|s| s.as_str()) == Some("inspect")
            && args.get(3).map(|s| s.as_str()) == Some("--format")
            && args
                .get(4)
                .map(|s| s.as_str())
                .is_some_and(|s| s.contains("RepoDigests"))
        {
            (0, "[\"ghcr.io/acme/web@sha256:match\"]".to_string())
        } else {
            (0, String::new())
        };
        Ok(CommandOutput {
            status,
            stdout,
            stderr: String::new(),
        })
    }
}

async fn test_state_with(
    db_path: &str,
    registry: Arc<dyn RegistryClient>,
    runner: Arc<dyn CommandRunner>,
) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
    };

    let db = Db::open(&config.db_path).await.unwrap();
    AppState::new(config, db, registry, runner)
}

async fn test_state(db_path: &str) -> Arc<AppState> {
    let config = Config {
        app_effective_version: "0.1.0".to_string(),
        http_addr: "127.0.0.1:0".to_string(),
        db_path: PathBuf::from(db_path),
        docker_config_path: None,
        compose_bin: "docker-compose".to_string(),
        auth_forward_header_name: "X-Forwarded-User".parse().unwrap(),
        auth_allow_anonymous_in_dev: true,
        self_upgrade_url: "/supervisor/".to_string(),
        dockrev_image_repo: "ghcr.io/ivanli-cn/dockrev".to_string(),
        webhook_secret: Some("secret".to_string()),
        host_platform: Some("linux/amd64".to_string()),
        discovery_interval_seconds: 60,
        discovery_max_actions: 200,
    };

    let db = Db::open(&config.db_path).await.unwrap();

    let registry = Arc::new(FakeRegistry);
    let runner = Arc::new(FakeRunner);
    AppState::new(config, db, registry, runner)
}

async fn seed_stack_from_compose(state: &Arc<AppState>, name: &str, compose_file: &str) -> String {
    let contents = std::fs::read_to_string(compose_file).unwrap();
    let parsed = compose::parse_services(&contents).unwrap();
    let mut merged = BTreeMap::<String, compose::ServiceFromCompose>::new();
    merged = compose::merge_services(merged, parsed);

    let stack_id = ids::new_stack_id();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let stack = crate::api::types::StackRecord {
        id: stack_id.clone(),
        name: name.to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec![compose_file.to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: Vec::new(),
    };

    let mut seeds = Vec::new();
    for svc in merged.values() {
        seeds.push(crate::api::types::ServiceSeed {
            id: ids::new_service_id(),
            name: svc.name.clone(),
            image_ref: svc.image_ref.clone(),
            image_tag: svc.image_tag.clone(),
            auto_rollback: true,
            backup_bind_paths: BTreeMap::new(),
            backup_volume_names: BTreeMap::new(),
        });
    }

    state.db.insert_stack(&stack, &seeds, &now).await.unwrap();
    stack_id
}

#[tokio::test]
async fn health_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn version_ok() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["version"], "0.1.0");
}

#[tokio::test]
async fn unknown_api_path_is_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn supervisor_paths_are_not_swallowed_by_ui_fallback() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/self-upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["code"], "supervisor_misrouted");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/supervisor/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn register_stack_then_check_updates() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["id"].as_str().unwrap(), stack_id.as_str());

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn create_ignore_then_delete() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let _stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = response_json(resp).await;
    let stack_id = list["stacks"][0]["id"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let service_id = detail["stack"]["services"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let create = serde_json::json!({
        "enabled": true,
        "scope": { "type": "service", "serviceId": service_id },
        "match": { "kind": "prefix", "value": "5.3." },
        "note": "test"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ignores")
                .header("content-type", "application/json")
                .body(Body::from(create.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created = response_json(resp).await;
    let rule_id = created["ruleId"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/ignores")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert_eq!(list["rules"][0]["id"].as_str().unwrap(), rule_id);

    let del = serde_json::json!({ "ruleId": rule_id });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/ignores")
                .header("content-type", "application/json")
                .body(Body::from(del.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let deleted = response_json(resp).await;
    assert!(deleted["deleted"].as_bool().unwrap());
}

#[tokio::test]
async fn update_creates_job_and_logs() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated = response_json(resp).await;
    let job_id = updated["jobId"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert!(
        list["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|j| j["id"].as_str().unwrap() == job_id)
    );

    let job = {
        let mut out = None;
        for _ in 0..50 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/jobs/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            let job = response_json(resp).await;
            if job["job"]["status"].as_str().unwrap() != "running" {
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };
    assert_eq!(job["job"]["id"].as_str().unwrap(), job_id);
    assert!(!job["job"]["logs"].as_array().unwrap().is_empty());
    assert_eq!(
        job["job"]["summary"]["stacks"][0]["backup"]["status"]
            .as_str()
            .unwrap(),
        "skipped"
    );
}

#[test]
fn infer_resolved_tag_picks_highest_semver_and_exposes_all_matches() {
    let runtime_digest = "sha256:run";
    let current_tag = "latest";
    let tags: Vec<String> = ["latest", "v1.0.0-alpha.1", "1.0.0", "v1.0.0", "v0.9.0"]
        .into_iter()
        .map(str::to_string)
        .collect();

    let digest_for_tag = |tag: &str| -> Option<&'static str> {
        match tag {
            "v1.0.0" => Some("sha256:run"),
            "1.0.0" => Some("sha256:run"),
            "v1.0.0-alpha.1" => Some("sha256:run"),
            "v0.9.0" => Some("sha256:old"),
            _ => None,
        }
    };

    let mut semver_tags: Vec<(semver::Version, String)> = tags
        .iter()
        .filter_map(|t| crate::ignore::parse_version(t).map(|v| (v, t.clone())))
        .collect();
    semver_tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let mut resolved_tags: Vec<String> = Vec::new();
    for (_v, tag) in semver_tags {
        if let Some(d) = digest_for_tag(&tag)
            && d == runtime_digest
            && tag != current_tag
        {
            resolved_tags.push(tag);
        }
    }

    assert_eq!(resolved_tags, vec!["v1.0.0", "1.0.0", "v1.0.0-alpha.1"]);
    assert_eq!(resolved_tags.first().map(String::as_str), Some("v1.0.0"));
}

#[tokio::test]
async fn archived_stack_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .set_stack_archived(&stack_id, true, Some("user_archive"), &now)
        .await
        .unwrap();

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn archived_services_stack_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
  worker:
    image: ghcr.io/acme/worker:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn archived_services_all_update_skips_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let stack = state.db.get_stack(&stack_id).await.unwrap().unwrap();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for svc in &stack.services {
        state
            .db
            .set_service_archived(&svc.id, true, Some("user_archive"), &now)
            .await
            .unwrap();
    }

    let update = serde_json::json!({
        "scope": "all",
        "mode": "dry-run",
        "allowArchMismatch": false,
        "backupMode": "inherit",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                logs.iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped"))
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn empty_new_digests_does_not_skip_notify() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    // Use apply mode to produce updater summary with `newDigests: {}` (FakeRunner returns empty container id).
    // Skip backups to keep the test isolated.
    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "mode": "apply",
        "allowArchMismatch": false,
        "backupMode": "skip",
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/updates")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            let logs = job["job"]["logs"].as_array().unwrap();
            assert!(
                !logs
                    .iter()
                    .any(|l| l["msg"].as_str().unwrap().contains("notify skipped")),
                "notify should not be skipped just because newDigests is empty"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("job did not finish in time");
}

#[tokio::test]
async fn webhook_trigger_check_creates_job_and_updates_stack() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let trigger = serde_json::json!({
        "action": "check",
        "scope": "stack",
        "stackId": stack_id,
        "allowArchMismatch": false,
        "backupMode": "inherit"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stacks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list = response_json(resp).await;
    assert_eq!(list["stacks"][0]["updates"].as_u64().unwrap(), 1);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let jobs = response_json(resp).await;
    let job = jobs["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["id"].as_str().unwrap() == job_id)
        .unwrap();
    assert_eq!(job["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["type"].as_str().unwrap(), "check");
}

#[tokio::test]
async fn check_persists_registry_digest_when_runtime_digest_missing() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..50 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let digest = detail["stack"]["services"][0]["image"]["digest"]
        .as_str()
        .unwrap();
    assert_eq!(digest, "sha256:old");
}

#[tokio::test]
async fn resolved_tag_inference_does_not_skip_candidate_tag_when_candidate_digest_none() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();
    let compose_project = state.db.get_stack_compose_project(&stack_id).await.unwrap();
    assert_eq!(compose_project.as_deref(), Some("demo"));

    let img = crate::registry::ImageRef::parse("ghcr.io/acme/web:latest").unwrap();
    let runtime = super::docker_compose_service_runtime_digest(
        &state,
        "demo",
        "web",
        &super::repo_candidates(&img),
    )
    .await
    .unwrap();
    assert_eq!(runtime.as_deref(), Some("sha256:match"));

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/{check_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let job_detail = response_json(resp).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let image = &detail["stack"]["services"][0]["image"];
    let digest = image["digest"].as_str().unwrap_or("<none>");
    let resolved = image["resolvedTag"].as_str().unwrap_or("<none>");
    let runner_calls = runner.calls.lock().unwrap().clone();
    assert_eq!(
        digest, "sha256:match",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
    assert_eq!(
        resolved, "5.3.0",
        "unexpected stack detail: {detail}\njob detail: {job_detail}\nrunner calls: {runner_calls:?}"
    );
}

#[tokio::test]
async fn resolved_tag_inference_runs_for_major_minor_tags() {
    let runner: Arc<ScriptedRunner> = Arc::new(ScriptedRunner::default());
    let state = test_state_with(
        ":memory:",
        Arc::new(StatefulRegistry::default()),
        runner.clone(),
    )
    .await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .upsert_discovered_compose_project(crate::db::DiscoveredComposeProjectUpsert {
            project: "demo".to_string(),
            stack_id: Some(stack_id.clone()),
            status: "active".to_string(),
            last_seen_at: Some(now.clone()),
            last_scan_at: now,
            last_error: None,
            last_config_files: Some(vec![compose_path.clone()]),
            unarchive_if_active: true,
        })
        .await
        .unwrap();

    let check = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "reason": "ui"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/checks")
                .header("content-type", "application/json")
                .body(Body::from(check.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let check_id = triggered["checkId"].as_str().unwrap().to_string();

    let mut finished = false;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{check_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let job = response_json(resp).await;
        if job["job"]["status"].as_str().unwrap() != "running" {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "check job did not finish in time");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/stacks/{stack_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail = response_json(resp).await;
    let resolved = detail["stack"]["services"][0]["image"]["resolvedTag"]
        .as_str()
        .unwrap_or("<none>");
    assert_ne!(
        resolved, "<none>",
        "expected resolvedTag for 5.2 tag: {detail}"
    );
}

#[tokio::test]
async fn webhook_trigger_update_creates_job() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:5.2
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let trigger = serde_json::json!({
        "action": "update",
        "scope": "stack",
        "stackId": stack_id,
        "allowArchMismatch": false,
        "backupMode": "skip"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/trigger")
                .header("content-type", "application/json")
                .header("X-Dockrev-Webhook-Secret", "secret")
                .body(Body::from(trigger.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggered = response_json(resp).await;
    let job_id = triggered["jobId"].as_str().unwrap().to_string();

    let job = {
        let mut out = None;
        for _ in 0..50 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/jobs/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            let job = response_json(resp).await;
            if job["job"]["status"].as_str().unwrap() != "running" {
                out = Some(job);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        out.expect("job did not finish in time")
    };
    assert_eq!(job["job"]["id"].as_str().unwrap(), job_id);
    assert_eq!(job["job"]["createdBy"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["reason"].as_str().unwrap(), "webhook");
    assert_eq!(job["job"]["type"].as_str().unwrap(), "update");
    assert_eq!(job["job"]["summary"]["mode"].as_str().unwrap(), "apply");
    assert!(job["job"]["finishedAt"].as_str().unwrap().len() > 10);
}

#[tokio::test]
async fn settings_and_notifications_roundtrip() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert!(settings["backup"].is_object());
    assert!(settings["auth"].is_object());

    let put = serde_json::json!({
        "backup": {
            "enabled": true,
            "requireSuccess": true,
            "baseDir": "/tmp/dockrev-backups",
            "skipTargetsOverBytes": 123
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let settings = response_json(resp).await;
    assert_eq!(
        settings["backup"]["skipTargetsOverBytes"].as_u64().unwrap(),
        123
    );

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"].is_object());

    let put = serde_json::json!({
        "email": { "enabled": false },
        "webhook": { "enabled": true, "url": "https://example.com/hook" },
        "telegram": { "enabled": false },
        "webPush": { "enabled": false }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/notifications")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let conf = response_json(resp).await;
    assert!(conf["webhook"]["enabled"].as_bool().unwrap());
    assert_eq!(conf["webhook"]["url"].as_str().unwrap(), "******");
}

#[tokio::test]
async fn github_packages_settings_masks_pat() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let put = serde_json::json!({
      "enabled": true,
      "callbackUrl": "https://dockrev.example.com/api/webhooks/github-packages",
      "targets": [],
      "repos": [],
      "pat": "ghp_example"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/github-packages/settings")
                .header("content-type", "application/json")
                .body(Body::from(put.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/github-packages/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(
        body["callbackUrl"],
        "https://dockrev.example.com/api/webhooks/github-packages"
    );
    assert_eq!(body["patMasked"], "******");
}

#[tokio::test]
async fn github_packages_resolve_owner_requires_pat_saved() {
    let state = test_state(":memory:").await;
    let app = api::router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/github-packages/resolve")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"input":"acme"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn github_packages_webhook_validates_signature_and_dedupes_delivery() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    // Seed settings + selected repo.
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: true,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state.clone());

    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d1")
                .header("X-Hub-Signature-256", "sha256=deadbeef")
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ok"], true);
    assert!(
        body["jobId"]
            .as_str()
            .unwrap_or_default()
            .starts_with("dsc_")
    );

    // Same delivery id should be ignored.
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "d2")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "duplicate_delivery");
}

#[tokio::test]
async fn github_packages_webhook_respects_disabled_setting() {
    use ring::hmac;

    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .put_github_packages_settings(
            &crate::api::types::GitHubPackagesSettingsDb {
                enabled: false,
                callback_url: "https://dockrev.example.com/api/webhooks/github-packages"
                    .to_string(),
                pat: Some("ghp_example".to_string()),
                webhook_secret: Some("secret123".to_string()),
                updated_at: Some(now.clone()),
            },
            &now,
        )
        .await
        .unwrap();
    state
        .db
        .put_github_packages_repos(
            &[(String::from("acme"), String::from("widgets"), true)],
            &now,
        )
        .await
        .unwrap();

    let app = api::router(state);
    let payload = serde_json::json!({
      "action": "published",
      "repository": { "full_name": "acme/widgets", "owner": { "login": "acme" } }
    });
    let payload_bytes = payload.to_string().into_bytes();
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret123");
    let tag = hmac::sign(&key, &payload_bytes);
    let sig = format!("sha256={}", hex::encode(tag.as_ref()));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/github-packages")
                .header("X-GitHub-Event", "package")
                .header("X-GitHub-Delivery", "disabled-1")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(payload_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["ignored"], true);
    assert_eq!(body["reason"], "disabled");
}
