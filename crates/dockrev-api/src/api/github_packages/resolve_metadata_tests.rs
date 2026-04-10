use super::*;

#[test]
fn github_packages_resolve_normalize_github_source_repo_key_accepts_repo_url() {
    assert_eq!(
        normalize_github_source_repo_key("https://github.com/Acme/Widgets").as_deref(),
        Some("acme/widgets")
    );
}

#[test]
fn github_packages_resolve_preferred_ghcr_inspection_reference_prefers_latest() {
    let tags = vec![
        "1.0.0".to_string(),
        "latest".to_string(),
        "2.0.0".to_string(),
    ];
    assert_eq!(preferred_ghcr_inspection_reference(&tags), Some("latest"));
}

#[test]
fn github_packages_resolve_deployed_repo_keys_only_keeps_ghcr_targets() {
    let keys = ghcr_deployed_repo_keys(vec![
        crate::db::GithubWebhookServiceTarget {
            stack_id: "stack-1".to_string(),
            service_id: "svc-1".to_string(),
            image_ref: "ghcr.io/acme/api:latest".to_string(),
        },
        crate::db::GithubWebhookServiceTarget {
            stack_id: "stack-2".to_string(),
            service_id: "svc-2".to_string(),
            image_ref: "docker.io/library/nginx:latest".to_string(),
        },
    ]);
    assert!(keys.contains("acme/api"));
    assert_eq!(keys.len(), 1);
}

#[test]
fn github_packages_resolve_ghcr_linked_selection_value_preserves_unknown_state() {
    assert_eq!(ghcr_linked_selection_value(None, "acme/widgets"), None);

    let mut linked_repo_keys = std::collections::HashSet::new();
    linked_repo_keys.insert("acme/widgets".to_string());
    let complete_probe = GhcrLinkedRepoProbeResult {
        linked_repo_keys: linked_repo_keys.clone(),
        probe_complete: true,
    };
    let partial_probe = GhcrLinkedRepoProbeResult {
        linked_repo_keys,
        probe_complete: false,
    };

    assert_eq!(
        ghcr_linked_selection_value(Some(&complete_probe), "acme/widgets"),
        Some(true)
    );
    assert_eq!(
        ghcr_linked_selection_value(Some(&complete_probe), "acme/api"),
        Some(false)
    );
    assert_eq!(
        ghcr_linked_selection_value(Some(&partial_probe), "acme/widgets"),
        Some(true)
    );
    assert_eq!(
        ghcr_linked_selection_value(Some(&partial_probe), "acme/api"),
        None
    );
}
