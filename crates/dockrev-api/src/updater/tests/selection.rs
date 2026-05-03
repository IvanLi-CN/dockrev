use super::*;

#[test]
fn aggregate_selection_excludes_dockrev_but_keeps_supervisor() {
    let stack = StackRecord {
        id: "stk_guard".to_string(),
        name: "dockrev-mod".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![
            selection_test_service("svc-dockrev", "dockrev", "ghcr.io/ivanli-cn/dockrev:0.29.3"),
            selection_test_service(
                "svc-supervisor",
                "dockrev-supervisor",
                "ghcr.io/ivanli-cn/dockrev-supervisor:0.29.3",
            ),
        ],
    };

    let selection = select_update_services(
        &stack,
        &JobScope::Stack,
        None,
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
    );
    let ids = selection
        .services
        .iter()
        .map(|svc| svc.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["svc-supervisor"]);
}

#[test]
fn service_scope_excludes_dockrev_update_selection() {
    let stack = StackRecord {
        id: "stk_guard".to_string(),
        name: "dockrev-mod".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/dockrev/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![selection_test_service(
            "svc-dockrev",
            "dockrev",
            "ghcr.io/ivanli-cn/dockrev:0.29.3",
        )],
    };

    let selection = select_update_services(
        &stack,
        &JobScope::Service,
        Some("svc-dockrev"),
        false,
        "ui",
        Some("ghcr.io/ivanli-cn/dockrev"),
    );

    assert!(selection.services.is_empty());
}

#[test]
fn detect_semver_downgrade_ignores_opaque_hash_like_prerelease_versions() {
    let mut service = selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }

    assert_eq!(detect_semver_downgrade(&service), None);
}

#[test]
fn detect_semver_downgrade_does_not_fall_back_to_raw_tag_after_opaque_resolved_tag() {
    let mut service = selection_test_service(
        "svc-hash-tagged",
        "hash-build",
        "ghcr.io/acme/web:2026.3.28",
    );
    service.image.tag = "2026.3.28".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.tag = "2026.3.27".to_string();
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }

    assert_eq!(detect_semver_downgrade(&service), None);
}

#[test]
fn select_update_services_keeps_hash_like_prerelease_candidates_for_non_ui_runs() {
    let mut service = selection_test_service("svc-hash", "hash-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }
    let stack = StackRecord {
        id: "stk_hash".to_string(),
        name: "hash-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert_eq!(selection.services.len(), 1);
    assert!(selection.skipped_version_anomaly.is_empty());
}

#[test]
fn select_update_services_keeps_opaque_resolved_tags_even_when_raw_tags_look_semver_like() {
    let mut service = selection_test_service(
        "svc-hash-tagged",
        "hash-build",
        "ghcr.io/acme/web:2026.3.28",
    );
    service.image.tag = "2026.3.28".to_string();
    service.image.resolved_tag = Some("2026.3.28-e58516daf".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.tag = "2026.3.27".to_string();
        candidate.resolved_tag = Some("2026.3.28-6b9856d64".to_string());
    }
    let stack = StackRecord {
        id: "stk_hash_tagged".to_string(),
        name: "hash-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/hash/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert_eq!(selection.services.len(), 1);
    assert!(selection.skipped_version_anomaly.is_empty());
}

#[test]
fn select_update_services_still_skips_ordered_prerelease_downgrades() {
    let mut service = selection_test_service("svc-rc", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc.2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc.1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[test]
fn select_update_services_still_skips_single_token_prerelease_downgrades() {
    let mut service = selection_test_service("svc-rc1", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc1".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[test]
fn select_update_services_still_skips_hyphenated_prerelease_downgrades() {
    let mut service =
        selection_test_service("svc-rc-hyphen", "rc-build", "ghcr.io/acme/web:latest");
    service.image.tag = "latest".to_string();
    service.image.resolved_tag = Some("v1.0.0-rc-2".to_string());
    if let Some(candidate) = service.candidate.as_mut() {
        candidate.resolved_tag = Some("v1.0.0-rc-1".to_string());
    }
    let stack = StackRecord {
        id: "stk_rc_hyphen".to_string(),
        name: "rc-build".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/rc/docker-compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![service],
    };

    let selection = select_update_services(&stack, &JobScope::Stack, None, false, "schedule", None);

    assert!(selection.services.is_empty());
    assert_eq!(selection.skipped_version_anomaly.len(), 1);
    assert_eq!(
        selection.skipped_version_anomaly[0]["reason"].as_str(),
        Some("semver_downgrade")
    );
}

#[tokio::test]
async fn aggregate_dockrev_only_update_becomes_noop() {
    let stack = single_service_stack(
        "ghcr.io/ivanli-cn/dockrev:0.29.3",
        Some(Candidate {
            tag: "latest".to_string(),
            resolved_tag: Some("0.29.5".to_string()),
            digest: "sha256:candidate".to_string(),
            arch_match: ArchMatch::Match,
            arch: vec!["linux/amd64".to_string()],
        }),
    );
    let runner = FakeRunner::default();

    let outcome = run_update_job(
        &runner,
        "docker-compose",
        None,
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
