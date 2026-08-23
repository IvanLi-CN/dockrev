use super::*;

fn make_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dockrev-discovery-test-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn persisted_compose_files_distinguish_stopped_missing_and_invalid() {
    let dir = make_temp_dir();
    let healthy = dir.join("compose.yml");
    let missing = dir.join("missing.yml");
    let invalid = dir.join("invalid.yml");
    let unreadable = dir.join("directory.yml");

    std::fs::write(&healthy, "services:\n  app:\n    image: alpine:3.20\n").unwrap();
    std::fs::write(&invalid, "services: [not-valid").unwrap();
    std::fs::create_dir(&unreadable).unwrap();

    let stopped = classify_persisted_compose_files(Some(vec![healthy.display().to_string()])).await;
    assert!(matches!(
        stopped,
        PersistedComposeFilesState::Stopped { .. }
    ));

    let all_missing =
        classify_persisted_compose_files(Some(vec![missing.display().to_string()])).await;
    assert!(matches!(
        all_missing,
        PersistedComposeFilesState::Missing { .. }
    ));

    let mixed = classify_persisted_compose_files(Some(vec![
        healthy.display().to_string(),
        missing.display().to_string(),
    ]))
    .await;
    assert!(matches!(
        mixed,
        PersistedComposeFilesState::Invalid { ref reason, .. }
            if reason == "compose_files_partially_missing"
    ));

    let malformed =
        classify_persisted_compose_files(Some(vec![invalid.display().to_string()])).await;
    assert!(matches!(
        malformed,
        PersistedComposeFilesState::Invalid { ref reason, .. }
            if reason.starts_with("compose_file_invalid:")
    ));

    let io_error =
        classify_persisted_compose_files(Some(vec![unreadable.display().to_string()])).await;
    assert!(matches!(
        io_error,
        PersistedComposeFilesState::Invalid { ref reason, .. }
            if reason.starts_with("compose_file_unreadable:")
    ));

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn parse_labels_json_line_null_is_empty() {
    let out = parse_labels_json_line("null").unwrap();
    assert!(out.is_empty());
}

#[test]
fn parse_labels_json_line_non_object_is_empty() {
    let out = parse_labels_json_line("[]").unwrap();
    assert!(out.is_empty());
}

#[test]
fn parse_labels_json_line_object_extracts_strings() {
    let out = parse_labels_json_line(r#"{"a":"b","n":123}"#).unwrap();
    assert_eq!(out.get("a").map(String::as_str), Some("b"));
    assert_eq!(out.get("n"), None);
}

#[test]
fn stack_services_match_specs_detects_changes() {
    let stack = crate::api::types::StackRecord {
        id: "stk_1".to_string(),
        name: "demo".to_string(),
        archived: false,
        compose: crate::api::types::ComposeConfig {
            kind: "path".to_string(),
            compose_files: vec!["/srv/compose.yml".to_string()],
            env_file: None,
        },
        backup: crate::api::types::StackBackupConfig::default(),
        services: vec![crate::api::types::Service {
            id: "svc_1".to_string(),
            name: "web".to_string(),
            image: crate::api::types::ComposeRef {
                reference: "ghcr.io/acme/web:1.0".to_string(),
                tag: "1.0".to_string(),
                digest: None,
                resolved_tag: None,
                resolved_tags: None,
            },
            homepage: Some(crate::api::types::ServiceHomepage {
                group: Some("Developer".to_string()),
                name: Some("Web".to_string()),
                icon: Some("si-github".to_string()),
                href: Some("https://example.com/web".to_string()),
                description: Some("Docs".to_string()),
            }),
            update_guard: None,
            candidate: None,
            ignore: None,
            version_inference: None,
            new_version_discovery_count: None,
            settings: crate::api::types::ServiceSettings {
                auto_rollback: true,
                backup_targets: crate::api::types::BackupTargetOverrides {
                    bind_paths: BTreeMap::new(),
                    volume_names: BTreeMap::new(),
                },
                repo_url: None,
            },
            archived: None,
        }],
    };

    let specs_ok = vec![ComposeServiceSpec {
        name: "web".to_string(),
        image_ref: "ghcr.io/acme/web:1.0".to_string(),
        image_tag: "1.0".to_string(),
        homepage: Some(crate::api::types::ServiceHomepage {
            group: Some("Developer".to_string()),
            name: Some("Web".to_string()),
            icon: Some("si-github".to_string()),
            href: Some("https://example.com/web".to_string()),
            description: Some("Docs".to_string()),
        }),
        update_guard: None,
        backup_bind_paths: Vec::new(),
        backup_volume_names: Vec::new(),
    }];
    assert!(stack_services_match_specs(&stack, &specs_ok));

    let specs_changed = vec![ComposeServiceSpec {
        name: "web".to_string(),
        image_ref: "ghcr.io/acme/web:1.1".to_string(),
        image_tag: "1.1".to_string(),
        homepage: Some(crate::api::types::ServiceHomepage {
            group: Some("Developer".to_string()),
            name: Some("Web".to_string()),
            icon: Some("si-github".to_string()),
            href: Some("https://example.com/web".to_string()),
            description: Some("Docs".to_string()),
        }),
        update_guard: None,
        backup_bind_paths: Vec::new(),
        backup_volume_names: Vec::new(),
    }];
    assert!(!stack_services_match_specs(&stack, &specs_changed));

    let specs_homepage_changed = vec![ComposeServiceSpec {
        name: "web".to_string(),
        image_ref: "ghcr.io/acme/web:1.0".to_string(),
        image_tag: "1.0".to_string(),
        homepage: Some(crate::api::types::ServiceHomepage {
            group: Some("Developer".to_string()),
            name: Some("Web".to_string()),
            icon: Some("si-gitea".to_string()),
            href: Some("https://example.com/web".to_string()),
            description: Some("Docs".to_string()),
        }),
        update_guard: None,
        backup_bind_paths: Vec::new(),
        backup_volume_names: Vec::new(),
    }];
    assert!(!stack_services_match_specs(&stack, &specs_homepage_changed));
}

#[test]
fn normalize_config_files_splits_dedupes_preserves_order() {
    let raw = " /a.yml,\n/b.yml\r\n/a.yml\n\n/c.yml ";
    let out = normalize_config_files(raw).unwrap();
    assert_eq!(out, vec!["/a.yml", "/b.yml", "/c.yml"]);
}

#[test]
fn normalize_config_files_rejects_relative() {
    let raw = "compose.yml,/abs.yml";
    assert!(matches!(
        normalize_config_files(raw),
        Err(NormalizeConfigFilesError::RelativePathRejected)
    ));
}

#[test]
fn is_subsequence_preserves_order_semantics() {
    assert!(is_subsequence(&[] as &[String], &[] as &[String]));
    assert!(is_subsequence(&["/a".to_string()], &["/a".to_string()]));
    assert!(is_subsequence(
        &["/a".to_string(), "/c".to_string()],
        &["/a".to_string(), "/b".to_string(), "/c".to_string()]
    ));
    assert!(!is_subsequence(
        &["/b".to_string(), "/a".to_string()],
        &["/a".to_string(), "/b".to_string()]
    ));
}

#[tokio::test]
async fn resolve_project_compose_files_superset_is_warning_and_selects_superset() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let override_yml = dir.join("self-upgrade.override.yml");

    // The override file must be readable and "image-only" for the superset to be accepted.
    std::fs::write(
        &override_yml,
        "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
    )
    .unwrap();

    let base_s = base.display().to_string();
    let override_s = override_yml.display().to_string();

    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(base_s.clone()),
        },
    ];

    let resolved = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s, override_s]);
    assert!(
        resolved
            .warning
            .as_deref()
            .is_some_and(|w| w.contains("warning:config_files_superset_selected"))
    );
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_accepts_matching_managed_override_as_canonical() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let managed_root = dir.join("managed-overrides");
    std::fs::create_dir_all(&managed_root).unwrap();
    std::fs::write(
        &base,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let stack_id = "stk_01KMANAGED";
    let managed = crate::managed_override::managed_override_path(&managed_root, stack_id);
    std::fs::write(
        &managed,
        format!(
            "services:\n  web:\n    image: ghcr.io/acme/web@sha256:{}\n",
            "a".repeat(64)
        ),
    )
    .unwrap();
    let base_s = base.display().to_string();
    let managed_s = managed.display().to_string();
    let observed = vec![ObservedComposeContainer {
        service: "web".to_string(),
        config_files_raw: Some(format!("{base_s},{managed_s}")),
    }];
    let resolved = resolve_project_compose_files_with_context(
        "managed",
        &observed,
        None,
        Some(&managed_root),
        Some(stack_id),
    )
    .await
    .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s, managed_s]);
    assert!(resolved.warning.is_none());
}

#[tokio::test]
async fn resolve_project_compose_files_dedupes_duplicate_paths_in_labels() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let override_yml = dir.join("self-upgrade.override.yml");

    std::fs::write(
        &base,
        "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
    )
    .unwrap();
    std::fs::write(
        &override_yml,
        "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
    )
    .unwrap();

    let base_s = base.display().to_string();
    let override_s = override_yml.display().to_string();

    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s},{override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s}")),
        },
    ];

    let resolved = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s, override_s]);
    assert!(resolved.warning.is_none());
}

#[tokio::test]
async fn resolve_project_compose_files_non_subset_conflict_is_invalid_with_details() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let a = dir.join("a.yml");
    let b = dir.join("b.yml");

    // Ensure extra files are readable, otherwise the resolver will fall back to common files.
    std::fs::write(&a, "# a\n").unwrap();
    std::fs::write(&b, "# b\n").unwrap();

    let base_s = base.display().to_string();
    let a_s = a.display().to_string();
    let b_s = b.display().to_string();

    let observed = vec![
        ObservedComposeContainer {
            service: "svc-a".to_string(),
            config_files_raw: Some(format!("{base_s},{a_s}")),
        },
        ObservedComposeContainer {
            service: "svc-b".to_string(),
            config_files_raw: Some(format!("{base_s},{b_s}")),
        },
    ];

    let err = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap_err();
    assert!(err.reason.contains("config_files_conflict"));
    assert!(err.details.is_some());
    assert_eq!(
        err.details
            .as_ref()
            .and_then(|d| d.get("variants"))
            .and_then(|v| v.as_array())
            .map(|v| v.len()),
        Some(2)
    );
}

#[tokio::test]
async fn resolve_project_compose_files_no_superset_all_extras_unreadable_falls_back_to_common() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let a = dir.join("missing-a.yml");
    let b = dir.join("missing-b.yml");

    let base_s = base.display().to_string();
    let a_s = a.display().to_string();
    let b_s = b.display().to_string();

    // No canonical superset: two distinct variants with different extra files.
    // Since all extra files are unreadable, fall back to common files (base only) with a warning.
    let observed = vec![
        ObservedComposeContainer {
            service: "svc-a".to_string(),
            config_files_raw: Some(format!("{base_s},{a_s}")),
        },
        ObservedComposeContainer {
            service: "svc-b".to_string(),
            config_files_raw: Some(format!("{base_s},{b_s}")),
        },
    ];

    let resolved = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(
        resolved
            .warning
            .as_deref()
            .is_some_and(|w| { w.contains("warning:config_files_conflict_fallback_common") })
    );
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_stale_temp_no_superset_is_reconcile_eligible() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
        &base,
        "services:\n  svc-a:\n    image: alpine:3.20\n  svc-b:\n    image: alpine:3.20\n",
    )
    .unwrap();
    let a = std::env::temp_dir().join(format!("dockrev-override-stale-{}.yml", ulid::Ulid::new()));
    let b = std::env::temp_dir().join(format!("dockrev-override-stale-{}.yml", ulid::Ulid::new()));
    let base_s = base.display().to_string();
    let observed = vec![
        ObservedComposeContainer {
            service: "svc-a".to_string(),
            config_files_raw: Some(format!("{base_s},{}", a.display())),
        },
        ObservedComposeContainer {
            service: "svc-b".to_string(),
            config_files_raw: Some(format!("{base_s},{}", b.display())),
        },
    ];
    let resolved = resolve_project_compose_files("stale", &observed)
        .await
        .unwrap();
    assert!(resolved.warning.as_deref().is_some_and(|warning| {
        warning.starts_with("warning:config_files_stale_dockrev_temp_override")
    }));
    assert_eq!(
        resolved.details.as_ref().unwrap()["selected"]["reconcileEligible"],
        true
    );
    assert_eq!(
        resolved.details.as_ref().unwrap()["selected"]["affectedServices"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reconcile_managed_override_requires_matching_repo_digest() {
    assert!(repo_digest_matches(
        "ghcr.io/acme/web@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "ghcr.io/acme/web"
    ));
    assert!(!repo_digest_matches(
        "ghcr.io/other/web@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "ghcr.io/acme/web"
    ));
    assert!(!repo_digest_matches(
        "ghcr.io/acme/web:latest",
        "ghcr.io/acme/web"
    ));
    assert!(!repo_digest_matches(
        "ghcr.io/other/web@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "web"
    ));
    assert_eq!(
        parse_affected_services(
            "warning:config_files_stale_dockrev_temp_override services=[web, worker]"
        ),
        Some(vec!["web".to_string(), "worker".to_string()])
    );
}

#[test]
fn reconcile_managed_override_preserves_unaffected_managed_images() {
    let allowed = BTreeSet::from(["web".to_string(), "worker".to_string()]);
    let existing = "services:\n  web:\n    image: ghcr.io/acme/web@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n  worker:\n    image: ghcr.io/acme/worker@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
    let rendered = merge_managed_override_images(
        Some(existing),
        &allowed,
        &[ (
            "web".to_string(),
            "ghcr.io/acme/web@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
        )],
    )
    .unwrap();
    assert!(
        rendered.contains(
            "web@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        )
    );
    assert!(rendered.contains(
        "worker@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    ));
}

#[tokio::test]
async fn resolve_project_compose_files_superset_unsafe_extra_falls_back_to_common() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let override_yml = dir.join("self-upgrade.override.yml");
    let tmp_override = dir.join("dockrev-override.yml");

    std::fs::write(
        &override_yml,
        "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n",
    )
    .unwrap();

    let base_s = base.display().to_string();
    let override_s = override_yml.display().to_string();
    let tmp_s = tmp_override.display().to_string();

    // Superset candidate is reported by "dozzle", but the extra self-upgrade override touches
    // a different service ("dockrev"). Treat as unsafe for the superset and fall back to common.
    let observed = vec![
        ObservedComposeContainer {
            service: "dozzle".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s},{tmp_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(base_s.clone()),
        },
    ];

    let resolved = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(
        resolved
            .warning
            .as_deref()
            .is_some_and(|w| { w.contains("warning:config_files_unsafe_extra_fallback_common") })
    );
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_unreadable_extra_falls_back_to_common() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let override_yml = dir.join("missing.override.yml");

    let base_s = base.display().to_string();
    let override_s = override_yml.display().to_string();

    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(base_s.clone()),
        },
    ];

    let resolved = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(
        resolved
            .warning
            .as_deref()
            .is_some_and(|w| w.contains("warning:config_files_extra_unreadable_fallback_common"))
    );
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_unsafe_override_is_invalid() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    let override_yml = dir.join("unsafe.override.yml");

    std::fs::write(
            &override_yml,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n    environment:\n      A: B\n",
        )
        .unwrap();

    let base_s = base.display().to_string();
    let override_s = override_yml.display().to_string();

    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(base_s.clone()),
        },
    ];

    let err = resolve_project_compose_files("dockrev", &observed)
        .await
        .unwrap_err();
    assert!(err.reason.contains("unsafe override"));
    assert!(err.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_unreadable_dockrev_temp_override_falls_back()
{
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
        &base,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let temp_override =
        std::env::temp_dir().join(format!("dockrev-override-demo-{}.yml", ulid::Ulid::new()));

    let base_s = base.display().to_string();
    let temp_override_s = temp_override.display().to_string();
    let observed = vec![ObservedComposeContainer {
        service: "web".to_string(),
        config_files_raw: Some(format!("{base_s},{temp_override_s}")),
    }];

    let resolved = resolve_project_compose_files("demo", &observed)
        .await
        .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(resolved.warning.as_deref().is_some_and(|warning| {
        warning.contains("warning:config_files_stale_dockrev_temp_override")
    }));
    let details = resolved.details.expect("stale temp details");
    assert_eq!(details["selected"]["reconcileEligible"], true);
    assert_eq!(details["selected"]["affectedServices"][0], "web");
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_unreadable_self_upgrade_override_falls_back()
{
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
            &base,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n  dockrev-supervisor:\n    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest\n",
        )
        .unwrap();
    let self_upgrade_override = dir.join("self-upgrade.override.yml");

    let base_s = base.display().to_string();
    let self_upgrade_override_s = self_upgrade_override.display().to_string();
    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{self_upgrade_override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(format!("{base_s},{self_upgrade_override_s}")),
        },
    ];

    let resolved = resolve_project_compose_files_with_expected_override(
        "dockrev",
        &observed,
        Some(self_upgrade_override.as_path()),
    )
    .await
    .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(resolved.warning.as_deref().is_some_and(|warning| {
        warning.contains("warning:config_files_single_variant_dockrev_generated_override_fallback")
    }));
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_unreadable_self_upgrade_override_allows_custom_project_and_service_names()
 {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
            &base,
            "services:\n  app:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n  updater:\n    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest\n",
        )
        .unwrap();
    let self_upgrade_override = dir.join("self-upgrade.override.yml");

    let base_s = base.display().to_string();
    let self_upgrade_override_s = self_upgrade_override.display().to_string();
    let observed = vec![
        ObservedComposeContainer {
            service: "app".to_string(),
            config_files_raw: Some(format!("{base_s},{self_upgrade_override_s}")),
        },
        ObservedComposeContainer {
            service: "updater".to_string(),
            config_files_raw: Some(format!("{base_s},{self_upgrade_override_s}")),
        },
    ];

    let resolved = resolve_project_compose_files_with_expected_override(
        "my-dockrev-stack",
        &observed,
        Some(self_upgrade_override.as_path()),
    )
    .await
    .unwrap();
    assert_eq!(resolved.compose_files, vec![base_s]);
    assert!(resolved.warning.as_deref().is_some_and(|warning| {
        warning.contains("warning:config_files_single_variant_dockrev_generated_override_fallback")
    }));
    assert!(resolved.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_missing_user_self_upgrade_override_stays_invalid()
 {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
        &base,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let self_upgrade_override = dir.join("self-upgrade.override.yml");

    let base_s = base.display().to_string();
    let self_upgrade_override_s = self_upgrade_override.display().to_string();
    let observed = vec![ObservedComposeContainer {
        service: "web".to_string(),
        config_files_raw: Some(format!("{base_s},{self_upgrade_override_s}")),
    }];

    let err = resolve_project_compose_files("demo", &observed)
        .await
        .unwrap_err();
    assert!(err.reason.contains("compose_file_unreadable"));
    assert!(err.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_unexpected_self_upgrade_override_stays_invalid()
 {
    let dir = make_temp_dir();
    let other_dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
            &base,
            "services:\n  dockrev:\n    image: ghcr.io/ivanli-cn/dockrev:latest\n  dockrev-supervisor:\n    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest\n",
        )
        .unwrap();
    let reported_override = dir.join("self-upgrade.override.yml");
    let expected_override = other_dir.join("self-upgrade.override.yml");

    let base_s = base.display().to_string();
    let reported_override_s = reported_override.display().to_string();
    let observed = vec![
        ObservedComposeContainer {
            service: "dockrev".to_string(),
            config_files_raw: Some(format!("{base_s},{reported_override_s}")),
        },
        ObservedComposeContainer {
            service: "dockrev-supervisor".to_string(),
            config_files_raw: Some(format!("{base_s},{reported_override_s}")),
        },
    ];

    let err = resolve_project_compose_files_with_expected_override(
        "dockrev",
        &observed,
        Some(expected_override.as_path()),
    )
    .await
    .unwrap_err();
    assert!(err.reason.contains("compose_file_unreadable"));
    assert!(err.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_missing_user_override_stays_invalid() {
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
        &base,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let missing_override = dir.join("missing.override.yml");

    let base_s = base.display().to_string();
    let missing_override_s = missing_override.display().to_string();
    let observed = vec![ObservedComposeContainer {
        service: "web".to_string(),
        config_files_raw: Some(format!("{base_s},{missing_override_s}")),
    }];

    let err = resolve_project_compose_files("demo", &observed)
        .await
        .unwrap_err();
    assert!(err.reason.contains("compose_file_unreadable"));
    assert!(err.details.is_some());
}

#[tokio::test]
async fn resolve_project_compose_files_single_variant_unreadable_user_temp_override_stays_invalid()
{
    let dir = make_temp_dir();
    let base = dir.join("docker-compose.yml");
    std::fs::write(
        &base,
        "services:\n  web:\n    image: ghcr.io/acme/web:latest\n",
    )
    .unwrap();
    let temp_override = std::env::temp_dir().join("dockrev-override-demo-manual.yml");

    let base_s = base.display().to_string();
    let temp_override_s = temp_override.display().to_string();
    let observed = vec![ObservedComposeContainer {
        service: "web".to_string(),
        config_files_raw: Some(format!("{base_s},{temp_override_s}")),
    }];

    let err = resolve_project_compose_files("demo", &observed)
        .await
        .unwrap_err();
    assert!(err.reason.contains("compose_file_unreadable"));
    assert!(err.details.is_some());
}
