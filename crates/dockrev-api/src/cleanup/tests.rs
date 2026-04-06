use super::*;

fn sample_candidate(
    key: &str,
    kind: CleanupResourceKind,
    ownership: CleanupOwnership,
    category: CleanupCandidateCategory,
    estimated_reclaimable_bytes: Option<u64>,
) -> CleanupCandidate {
    CleanupCandidate {
        key: key.to_string(),
        resource_id: key.to_string(),
        kind,
        label: key.to_string(),
        estimated_reclaimable_bytes,
        estimate_unknown: estimated_reclaimable_bytes.is_none(),
        ownership,
        category,
    }
}

#[test]
fn preset_filter_respects_progressive_rules() {
    assert!(preset_includes_candidate(
        &CleanupPreset::Conservative,
        &CleanupCandidateCategory::DanglingImage
    ));
    assert!(!preset_includes_candidate(
        &CleanupPreset::Conservative,
        &CleanupCandidateCategory::ManagedUnusedImage
    ));
    assert!(preset_includes_candidate(
        &CleanupPreset::Balanced,
        &CleanupCandidateCategory::ManagedUnusedImage
    ));
    assert!(!preset_includes_candidate(
        &CleanupPreset::Balanced,
        &CleanupCandidateCategory::ManagedUnusedVolume
    ));
    assert!(preset_includes_candidate(
        &CleanupPreset::ProjectDeepClean,
        &CleanupCandidateCategory::ManagedUnusedVolume
    ));
    assert!(preset_includes_candidate(
        &CleanupPreset::Aggressive,
        &CleanupCandidateCategory::GlobalUnusedImage
    ));
}

#[test]
fn service_and_stack_scope_exclude_unowned_candidates() {
    let request = CleanupPlanRequest {
        preset: CleanupPreset::Aggressive,
        scope: CleanupScope::Stack,
        stack_id: Some("stack-1".to_string()),
        service_id: None,
    };
    let unowned = sample_candidate(
        "global",
        CleanupResourceKind::Image,
        CleanupOwnership::Unowned,
        CleanupCandidateCategory::GlobalUnusedImage,
        Some(1),
    );
    let service = sample_candidate(
        "svc",
        CleanupResourceKind::Image,
        CleanupOwnership::Service {
            stack_id: "stack-1".to_string(),
            stack_name: "alpha".to_string(),
            service_id: "svc-1".to_string(),
            service_name: "web".to_string(),
        },
        CleanupCandidateCategory::ManagedUnusedImage,
        Some(1),
    );
    assert!(!candidate_matches_request(&unowned, &request));
    assert!(candidate_matches_request(&service, &request));
}

#[test]
fn build_grouped_response_keeps_service_and_stack_orphans_separate() {
    let candidates = vec![
        sample_candidate(
            "svc-image",
            CleanupResourceKind::Image,
            CleanupOwnership::Service {
                stack_id: "stack-1".to_string(),
                stack_name: "alpha".to_string(),
                service_id: "svc-1".to_string(),
                service_name: "web".to_string(),
            },
            CleanupCandidateCategory::ManagedUnusedImage,
            Some(10),
        ),
        sample_candidate(
            "stack-net",
            CleanupResourceKind::Network,
            CleanupOwnership::StackOrphan {
                stack_id: "stack-1".to_string(),
                stack_name: "alpha".to_string(),
            },
            CleanupCandidateCategory::UnusedNetwork,
            Some(0),
        ),
        sample_candidate(
            "global-vol",
            CleanupResourceKind::Volume,
            CleanupOwnership::Unowned,
            CleanupCandidateCategory::GlobalUnusedVolume,
            None,
        ),
    ];

    let (stacks, unowned, bytes, has_unknown) = build_grouped_response(&candidates);
    assert_eq!(stacks.len(), 1);
    assert_eq!(stacks[0].stack_orphans.len(), 1);
    assert_eq!(stacks[0].services.len(), 1);
    assert_eq!(stacks[0].services[0].resources.len(), 1);
    assert_eq!(stacks[0].stack_orphans[0].reason, "网络没有活动容器连接");
    assert_eq!(
        stacks[0].services[0].resources[0].reason,
        "旧镜像未被任何容器使用"
    );
    assert_eq!(unowned.expect("unowned").resources.len(), 1);
    assert_eq!(bytes, 10);
    assert!(has_unknown);
}

#[test]
fn build_grouped_response_preserves_known_lower_bound_for_unknown_estimates() {
    let mut candidate = sample_candidate(
        "builder-cache",
        CleanupResourceKind::BuilderCache,
        CleanupOwnership::Unowned,
        CleanupCandidateCategory::BuilderCache,
        Some(256),
    );
    candidate.estimate_unknown = true;

    let (_stacks, unowned, bytes, has_unknown) = build_grouped_response(&[candidate]);
    let unowned = unowned.expect("unowned");
    assert_eq!(bytes, 256);
    assert!(has_unknown);
    assert_eq!(unowned.estimated_reclaimable_bytes, 256);
    assert!(unowned.has_unknown_size);
    assert_eq!(unowned.resources[0].estimated_reclaimable_bytes, Some(256));
    assert!(unowned.resources[0].estimate_unknown);
}

#[test]
fn fingerprint_changes_when_selected_resources_change() {
    let request = CleanupPlanRequest {
        preset: CleanupPreset::Balanced,
        scope: CleanupScope::All,
        stack_id: None,
        service_id: None,
    };
    let first = vec![sample_candidate(
        "a",
        CleanupResourceKind::Image,
        CleanupOwnership::Unowned,
        CleanupCandidateCategory::BuilderCache,
        Some(1),
    )];
    let second = vec![
        sample_candidate(
            "a",
            CleanupResourceKind::Image,
            CleanupOwnership::Unowned,
            CleanupCandidateCategory::BuilderCache,
            Some(1),
        ),
        sample_candidate(
            "b",
            CleanupResourceKind::Volume,
            CleanupOwnership::Unowned,
            CleanupCandidateCategory::GlobalUnusedVolume,
            Some(2),
        ),
    ];

    let a = compute_confirmation_fingerprint(&request, &first, "2026-03-29T00:00:00Z", 1, false)
        .unwrap();
    let b = compute_confirmation_fingerprint(&request, &second, "2026-03-29T00:00:00Z", 3, false)
        .unwrap();
    assert_ne!(a, b);
}

#[test]
fn fingerprint_changes_when_estimate_unknown_changes() {
    let request = CleanupPlanRequest {
        preset: CleanupPreset::Balanced,
        scope: CleanupScope::All,
        stack_id: None,
        service_id: None,
    };
    let exact = vec![sample_candidate(
        "builder-cache",
        CleanupResourceKind::BuilderCache,
        CleanupOwnership::Unowned,
        CleanupCandidateCategory::BuilderCache,
        Some(256),
    )];
    let mut lower_bound = exact.clone();
    lower_bound[0].estimate_unknown = true;

    let exact_fp =
        compute_confirmation_fingerprint(&request, &exact, "2026-03-29T00:00:00Z", 256, true)
            .unwrap();
    let lower_bound_fp =
        compute_confirmation_fingerprint(&request, &lower_bound, "2026-03-29T00:00:00Z", 256, true)
            .unwrap();
    assert_ne!(exact_fp, lower_bound_fp);
}

#[test]
fn command_spec_synthesizes_targeted_delete_commands() {
    let image = CleanupCommandAction {
        kind: CleanupResourceKind::Image,
        resource_id: "sha256:abc".to_string(),
        label: "demo".to_string(),
        ownership: CleanupOwnership::Unowned,
    };
    let volume = CleanupCommandAction {
        kind: CleanupResourceKind::Volume,
        resource_id: "data".to_string(),
        label: "data".to_string(),
        ownership: CleanupOwnership::Unowned,
    };
    let builder = CleanupCommandAction {
        kind: CleanupResourceKind::BuilderCache,
        resource_id: "global-builder-cache".to_string(),
        label: "builder".to_string(),
        ownership: CleanupOwnership::Unowned,
    };
    assert_eq!(
        command_spec_for_action(&image).args,
        vec!["image", "rm", "sha256:abc"]
    );
    assert_eq!(
        command_spec_for_action(&volume).args,
        vec!["volume", "rm", "data"]
    );
    assert_eq!(
        command_spec_for_action(&builder).args,
        vec!["builder", "prune", "-a", "-f"]
    );
}

#[test]
fn in_use_error_detection_matches_expected_resources() {
    assert!(is_in_use_error(
        CleanupResourceKind::Volume,
        "Error response from daemon: remove data: volume is in use"
    ));
    assert!(is_in_use_error(
        CleanupResourceKind::Network,
        "network foo has active endpoints"
    ));
    assert!(!is_in_use_error(
        CleanupResourceKind::BuilderCache,
        "builder cache busy"
    ));
}

#[test]
fn parse_human_size_accepts_common_units() {
    assert_eq!(parse_human_size("2.0GB"), Some(2_000_000_000));
    assert_eq!(parse_human_size("512B"), Some(512));
    assert_eq!(parse_human_size("20.7kB"), Some(20_700));
    assert_eq!(parse_human_size("2.0GiB"), Some(2_147_483_648));
}

#[test]
fn parse_buildx_du_json_lines_sums_reclaimable_rows() {
    let input = r#"{"Reclaimable":true,"Shared":false,"Size":"829889526"}
{"Reclaimable":true,"Shared":false,"Size":"829898832"}
{"Reclaimable":false,"Shared":false,"Size":"12"}"#;
    assert_eq!(
        parse_buildx_du_json_lines(input),
        Some(BuilderCacheEstimate {
            reclaimable_bytes: Some(829_889_526 + 829_898_832),
            estimate_unknown: false,
        })
    );
}

#[test]
fn parse_buildx_du_json_lines_accepts_decimal_human_sizes() {
    let input = r#"{"Reclaimable":true,"Shared":false,"Size":"256MB"}
{"Reclaimable":true,"Shared":false,"Size":"1.5GB"}"#;
    assert_eq!(
        parse_buildx_du_json_lines(input),
        Some(BuilderCacheEstimate {
            reclaimable_bytes: Some(256_000_000 + 1_500_000_000),
            estimate_unknown: false,
        })
    );
}

#[test]
fn parse_buildx_du_json_lines_marks_lower_bound_unknown_when_shared_rows_are_present() {
    let input = r#"{"Reclaimable":true,"Shared":false,"Size":"829889526"}
{"Reclaimable":true,"Shared":true,"Size":"829898832"}"#;
    assert_eq!(
        parse_buildx_du_json_lines(input),
        Some(BuilderCacheEstimate {
            reclaimable_bytes: Some(829_889_526),
            estimate_unknown: true,
        })
    );
}

#[test]
fn parse_buildx_du_text_summary_reads_reclaimable_total() {
    let input = "ID: example
Reclaimable:  26.6GB
Total:  30.1GB
";
    assert_eq!(parse_buildx_du_text_summary(input), Some(26_600_000_000));
}

#[test]
fn parse_du_kilobytes_output_reads_first_column() {
    let input = "1536	/var/lib/docker/volumes/demo_named/_data
";
    assert_eq!(parse_du_kilobytes_output(input), Some(1_572_864));
}

#[test]
fn parse_volume_sizes_from_system_df_verbose_reads_local_volume_section() {
    let input = r#"Images space usage:
REPOSITORY          TAG                 IMAGE ID            CREATED             SIZE                SHARED SIZE         UNIQUE SIZE         CONTAINERS
alpine              latest              4e38e38c8ce0        9 weeks ago         4.799 MB            0 B                 4.799 MB            1
Containers space usage:
CONTAINER ID        IMAGE               COMMAND             LOCAL VOLUMES       SIZE                CREATED             STATUS                      NAMES
4a7f7eebae0f        alpine:latest       "sh"                1                   0 B                 16 minutes ago      Exited (0) 5 minutes ago    hopeful_yalow
Local Volumes space usage:

NAME                                                               LINKS               SIZE
07c7bdf3e34ab76d921894c2b834f073721fccfbbcba792aa7648e3a7a664c2e   2                   36 B
my-named-vol                                                       0                   1.5 GB
"#;
    let parsed = parse_volume_sizes_from_system_df_verbose(input);
    assert_eq!(
        parsed.get("07c7bdf3e34ab76d921894c2b834f073721fccfbbcba792aa7648e3a7a664c2e"),
        Some(&36)
    );
    assert_eq!(parsed.get("my-named-vol"), Some(&(1_500_000_000_u64)));
}
