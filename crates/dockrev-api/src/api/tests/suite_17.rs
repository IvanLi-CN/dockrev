#[tokio::test]
async fn repo_link_backfill_job_updates_and_summarizes_mixed_results() {
    let registry = Arc::new(MixedRepoLinkRegistry::default());
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  updated:
    image: ghcr.io/acme/updated:latest
  disabled:
    image: ghcr.io/acme/disabled:latest
  nomatch:
    image: harbor.local/ops/nomatch:1.0
  error:
    image: harbor.local/ops/error:1.0
  existing:
    image: ghcr.io/acme/existing:latest
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let service_ids = services
        .iter()
        .map(|svc| (svc.name.clone(), svc.id.clone()))
        .collect::<BTreeMap<_, _>>();

    state
        .db
        .put_github_packages_repos(
            &[
                ("acme".to_string(), "updated".to_string(), true),
                ("acme".to_string(), "disabled".to_string(), true),
                ("acme".to_string(), "existing".to_string(), true),
            ],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            service_ids.get("disabled").unwrap(),
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();
    state
        .db
        .put_service_settings(
            service_ids.get("existing").unwrap(),
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: Some("https://example.com/manual/existing".to_string()),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let job_id = crate::repo_link_backfill::enqueue_startup_backfill_if_needed(state.as_ref())
        .await
        .unwrap()
        .expect("startup backfill job should be queued");
    let job = state
        .db
        .claim_next_queued_job_by_type(
            crate::api::types::JobType::RepoLinkBackfill,
            &test_now_rfc3339(),
        )
        .await
        .unwrap()
        .expect("queued repo backfill job should be claimable");
    assert_eq!(job.id, job_id);

    crate::repo_link_backfill::run_claimed_job(state.clone(), job)
        .await
        .unwrap();

    let finished = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(finished.status, "success");
    assert_eq!(
        finished.summary_json["counters"],
        json!({
            "total": 4,
            "updated": 1,
            "skippedDisabled": 1,
            "noMatch": 1,
            "error": 1
        })
    );
    assert_eq!(
        finished.summary_json["progress"]["phase"].as_str(),
        Some("done")
    );

    let updated = state
        .db
        .get_stored_service_settings(service_ids.get("updated").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.settings.repo_url.as_deref(),
        Some("https://github.com/acme/updated")
    );
    assert!(!updated.repo_url_auto_disabled);

    let disabled = state
        .db
        .get_stored_service_settings(service_ids.get("disabled").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(disabled.settings.repo_url.is_none());
    assert!(disabled.repo_url_auto_disabled);

    let nomatch = state
        .db
        .get_stored_service_settings(service_ids.get("nomatch").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(nomatch.settings.repo_url.is_none());
    assert!(!nomatch.repo_url_auto_disabled);

    let error = state
        .db
        .get_stored_service_settings(service_ids.get("error").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(error.settings.repo_url.is_none());
    assert!(!error.repo_url_auto_disabled);

    let existing = state
        .db
        .get_stored_service_settings(service_ids.get("existing").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        existing.settings.repo_url.as_deref(),
        Some("https://example.com/manual/existing")
    );
    let mut observed = registry.observed_references();
    observed.sort();
    assert_eq!(
        observed,
        vec![
            "ghcr.io/acme/updated@latest".to_string(),
            "harbor.local/ops/error@1.0".to_string(),
            "harbor.local/ops/nomatch@1.0".to_string(),
        ]
    );
}

#[tokio::test]
async fn sync_stack_from_compose_preserves_repo_url_when_only_service_image_tag_changes() {
    let state = test_state(":memory:").await;

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:1.0
"#,
    )
    .unwrap();

    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();

    state
        .db
        .put_service_settings(
            &service_id,
            &crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: Some("https://github.com/acme/web".to_string()),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    state
        .db
        .sync_stack_from_compose(
            &stack_id,
            std::slice::from_ref(&compose_path),
            &[crate::db::ComposeServiceSpec {
                name: "web".to_string(),
                image_ref: "ghcr.io/acme/web:1.1".to_string(),
                image_tag: "1.1".to_string(),
                homepage: None,
                update_guard: None,
            }],
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let settings = state
        .db
        .get_service_settings(&service_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        settings.repo_url.as_deref(),
        Some("https://github.com/acme/web"),
        "unexpected settings: {settings:?}"
    );
}

#[tokio::test]
async fn infer_service_repo_link_prefers_oci_source_and_current_digest() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_accepts_valid_non_github_oci_source() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://gitlab.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://gitlab.com/Acme/Web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_collapses_gitlab_browse_path_to_repo_root() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://gitlab.com/Acme/Web/-/tree/main",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://gitlab.com/Acme/Web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_normalizes_clone_style_oci_source_url() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://gitlab.com/Acme/Web.git",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://gitlab.com/Acme/Web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_collapses_generic_browse_path_to_repo_root() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://codeberg.org/Acme/Web/src/branch/main",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://codeberg.org/Acme/Web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_accepts_generic_repo_root_when_repo_name_matches_browse_marker() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://codeberg.org/acme/src",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://codeberg.org/acme/src")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_keeps_subgroup_paths_on_self_hosted_git_services() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://git.example.com/team/platform/api/-/tree/main",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://git.example.com/team/platform/api")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_rejects_non_repository_oci_source_url() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/acme",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["repoUrl"], serde_json::Value::Null);
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(body["reason"].as_str().is_some_and(|reason| {
        reason.starts_with("oci source not recognized as a valid repository URL")
    }));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_rejects_reserved_github_path_oci_source_url() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/topics/rust",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["repoUrl"], serde_json::Value::Null);
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(body["reason"].as_str().is_some_and(|reason| {
        reason.starts_with("oci source not recognized as a valid repository URL")
    }));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_rejects_credential_bearing_oci_source_url() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://token@gitlab.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(body["repoUrl"], serde_json::Value::Null);
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(body["reason"].as_str().is_some_and(|reason| {
        reason.starts_with("oci source not recognized as a valid repository URL")
    }));
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn infer_service_repo_link_uses_parsed_digest_reference_before_first_runtime_scan() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let digest = format!("sha256:{}", "a".repeat(64));
    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        format!(
            r#"
services:
  web:
    image: ghcr.io/acme/web@{digest}
"#
        ),
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec![digest]);
}

#[tokio::test]
async fn infer_service_repo_link_uses_tag_plus_digest_reference_before_first_runtime_scan() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let digest = format!("sha256:{}", "b".repeat(64));
    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        format!(
            r#"
services:
  web:
    image: ghcr.io/acme/web:latest@{digest}
"#
        ),
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
    );
    assert_eq!(body["strategy"].as_str(), Some("oci_source"));
    assert_eq!(registry.observed_references(), vec![digest]);
}

#[tokio::test]
async fn infer_service_repo_link_falls_back_to_tracked_ghcr_repo() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(None));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();
    state
        .db
        .put_github_packages_repos(
            &[("Acme".to_string(), "Web".to_string(), true)],
            &super::now_rfc3339().unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert_eq!(
        body["repoUrl"].as_str(),
        Some("https://github.com/acme/web")
    );
    assert_eq!(body["strategy"].as_str(), Some("ghcr_exact"));
    assert_eq!(registry.observed_references(), vec!["latest"]);
}

#[tokio::test]
async fn resolve_service_github_repo_ref_falls_back_to_inferred_github_repo_when_repo_url_missing()
{
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;

    let repo = crate::api::services::resolve_service_github_repo_ref(&state, &service_id, None)
        .await
        .unwrap();

    let repo = repo.expect("expected inferred github repo");
    assert_eq!(repo.full_name, "acme/web");
    assert_eq!(repo.html_url, "https://github.com/acme/web");
    assert_eq!(registry.observed_references(), vec!["sha256:current"]);
}

#[tokio::test]
async fn resolve_service_github_repo_ref_keeps_explicit_non_github_repo_url_unsupported() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

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
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();
    state
        .db
        .put_service_settings(
            &service_id,
            &crate::api::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: Some("https://gitlab.com/acme/web".to_string()),
            },
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let repo = crate::api::services::resolve_service_github_repo_ref(
        &state,
        &service_id,
        Some("https://gitlab.com/acme/web"),
    )
    .await
    .unwrap();

    assert!(
        repo.is_none(),
        "explicit non-github repoUrl should stay unsupported"
    );
    assert_eq!(registry.observed_references(), Vec::<String>::new());
}

#[tokio::test]
async fn resolve_service_github_repo_ref_respects_manual_repo_url_opt_out() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/Acme/Web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;

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
    let service_id =
        set_single_service_check_result(&state, &stack_id, Some("sha256:current"), None, None)
            .await;
    state
        .db
        .put_service_settings_with_repo_auto_disabled(
            &service_id,
            &crate::api::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            true,
            &test_now_rfc3339(),
        )
        .await
        .unwrap();

    let repo = crate::api::services::resolve_service_github_repo_ref(&state, &service_id, None)
        .await
        .unwrap();

    assert!(
        repo.is_none(),
        "manual repoUrl opt-out should stay unsupported"
    );
    assert_eq!(registry.observed_references(), Vec::<String>::new());
}

#[tokio::test]
async fn infer_service_repo_link_skips_deselected_ghcr_repo() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(None));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
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
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();
    state
        .db
        .put_github_packages_repos(
            &[("Acme".to_string(), "Web".to_string(), false)],
            &super::now_rfc3339().unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert!(body["repoUrl"].is_null(), "unexpected body: {body}");
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(
        body["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("ghcr exact fallback skipped because repo is not tracked"),
        "unexpected reason: {body}"
    );
}

#[tokio::test]
async fn infer_service_repo_link_returns_none_when_not_recognized() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(None));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: harbor.local/ops/web:1.0
"#,
    )
    .unwrap();
    let stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;
    let service_id = state
        .db
        .list_services_for_check(&stack_id)
        .await
        .unwrap()
        .first()
        .unwrap()
        .id
        .clone();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert!(
        body["repoUrl"].is_null(),
        "unexpected fallback body: {body}"
    );
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(
        body["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("ghcr exact fallback not applicable"),
        "unexpected reason: {body}"
    );
}

#[tokio::test]
async fn infer_service_repo_link_returns_none_for_invalid_service_image_ref() {
    let registry = Arc::new(RepoLinkRegistry::with_oci_source(Some(
        "https://github.com/acme/web",
    )));
    let state = test_state_with(":memory:", registry.clone(), Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let stack_id = ids::new_stack_id();
    let service_id = ids::new_service_id();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let stack = crate::api::types::StackRecord {
        id: stack_id.clone(),
        name: "demo".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/tmp/invalid-image-ref.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: Vec::new(),
    };
    let seeds = vec![crate::api::types::ServiceSeed {
        id: service_id.clone(),
        name: "web".to_string(),
        image_ref: "ghcr.io/acme/web".to_string(),
        image_tag: "latest".to_string(),
        homepage: None,
        update_guard: None,
        auto_rollback: true,
        backup_bind_paths: BTreeMap::new(),
        backup_volume_names: BTreeMap::new(),
    }];
    state.db.insert_stack(&stack, &seeds, &now).await.unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/services/{service_id}/repo-link/infer"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;
    assert!(body["repoUrl"].is_null(), "unexpected body: {body}");
    assert_eq!(body["strategy"].as_str(), Some("none"));
    assert!(
        body["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("invalid service image ref"),
        "unexpected reason: {body}"
    );
    assert!(
        registry.observed_references().is_empty(),
        "registry should not be queried for invalid refs"
    );
}
