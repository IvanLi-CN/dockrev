use super::*;

#[tokio::test]
async fn update_job_injects_docker_auth_env_into_compose_and_docker_commands() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = EnvCaptureUpdateRunner::default();
    let (docker_config_path, _docker_config_cleanup) = write_test_docker_config();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        Some(docker_config_path.as_path()),
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");

    let specs = runner.specs.lock().unwrap();
    let compose_pull = specs
        .iter()
        .find(|spec| args_end_with(&spec.args, &["pull", "web"]))
        .expect("compose pull command should exist");
    let compose_up = specs
        .iter()
        .find(|spec| args_end_with(&spec.args, &["up", "-d", "web"]))
        .expect("compose up command should exist");
    let docker_tag = specs
        .iter()
        .find(|spec| spec.args == vec!["image", "tag", "sha256:new", "ghcr.io/org/web:1.0"])
        .expect("docker tag command should exist");

    assert_eq!(compose_pull.env.len(), 4);
    assert_eq!(compose_pull.env[0].0, "DOCKER_CONFIG");
    assert!(compose_pull.env[0].1.ends_with("/.docker"));
    assert_eq!(
        compose_pull.env[1..],
        [
            ("COMPOSE_PROGRESS".to_string(), "tty".to_string()),
            ("COMPOSE_ANSI".to_string(), "always".to_string()),
            (crate::runner::STREAM_PTY_ENV.to_string(), "1".to_string()),
        ]
    );

    for spec in [compose_up, docker_tag] {
        assert_eq!(spec.env.len(), 1);
        assert!(spec.env.iter().all(|(k, _)| k == "DOCKER_CONFIG"));
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "DOCKER_CONFIG" && v.ends_with("/.docker"))
        );
    }
}

#[tokio::test]
async fn noop_update_with_broken_docker_config_path_stays_noop() {
    let stack = single_service_stack("ghcr.io/ivanli-cn/dockrev:1.0", None);
    let runner = FakeRunner::default();
    let missing_path = std::env::temp_dir()
        .join(format!("dockrev-missing-config-{}", ulid::Ulid::new()))
        .join("config.json");

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        Some(missing_path.as_path()),
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Stack,
        None,
        "live",
        None,
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    assert_eq!(outcome.summary_json["changedServices"].as_u64(), Some(0));
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn update_job_without_docker_config_keeps_only_pull_terminal_env() {
    let stack = single_service_stack("ghcr.io/org/web:1.0", None);
    let runner = EnvCaptureUpdateRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
        IdempotentRetryPolicy::default(),
        &stack,
        &JobScope::Service,
        Some("svc_1"),
        "live",
        None,
        false,
        "ui",
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, "success");
    let specs = runner.specs.lock().unwrap();
    let compose_pull = specs
        .iter()
        .find(|spec| args_end_with(&spec.args, &["pull", "web"]))
        .expect("compose pull command should exist");
    assert_eq!(
        compose_pull.env,
        vec![
            ("COMPOSE_PROGRESS".to_string(), "tty".to_string()),
            ("COMPOSE_ANSI".to_string(), "always".to_string()),
            (crate::runner::STREAM_PTY_ENV.to_string(), "1".to_string()),
        ]
    );
    assert!(
        specs
            .iter()
            .filter(|spec| !args_end_with(&spec.args, &["pull", "web"]))
            .all(|spec| spec.env.is_empty())
    );
}
