use std::{collections::BTreeMap, path::Path};

use super::*;
use crate::{
    api::types::{BackupRetention, ComposeConfig, StackBackupConfig},
    models::{JobRecord, ServiceSeed, StackRecord},
};

fn make_summary(
    service_id: &str,
    current_tag: &str,
    current_display_tag: Option<&str>,
    current_digest: Option<&str>,
    candidate_digest: &str,
    candidate_display_tag: Option<&str>,
) -> serde_json::Value {
    make_summary_with_candidate_tag(
        service_id,
        current_tag,
        current_display_tag,
        current_digest,
        "latest",
        candidate_digest,
        candidate_display_tag,
    )
}

fn make_summary_with_candidate_tag(
    service_id: &str,
    current_tag: &str,
    current_display_tag: Option<&str>,
    current_digest: Option<&str>,
    candidate_tag: &str,
    candidate_digest: &str,
    candidate_display_tag: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "newVersions": {
            "count": 1,
            "services": [{
                "stackId": "stack_1",
                "serviceId": service_id,
                "serviceName": "web",
                "imageRef": "ghcr.io/acme/web",
                "currentTag": current_tag,
                "currentDigest": current_digest,
                "currentDisplayTag": current_display_tag.unwrap_or(current_tag),
                "candidateTag": candidate_tag,
                "candidateDisplayTag": candidate_display_tag.unwrap_or(candidate_tag),
                "candidateDigest": candidate_digest,
            }],
        }
    })
}

async fn seed_service(
    db: &Db,
    service_id: &str,
    current_digest: Option<&str>,
    current_resolved_tag: Option<&str>,
    image_tag: &str,
    candidate_digest: Option<&str>,
) {
    let now = "2026-03-19T00:00:00Z";
    let stack = StackRecord {
        id: "stack_1".to_string(),
        name: "demo".to_string(),
        archived: false,
        compose: ComposeConfig {
            kind: "compose".to_string(),
            compose_files: vec!["/tmp/demo.yml".to_string()],
            env_file: None,
        },
        backup: StackBackupConfig {
            targets: Vec::new(),
            retention: BackupRetention::default(),
        },
        services: Vec::new(),
    };
    let seeds = vec![ServiceSeed {
        id: service_id.to_string(),
        name: "web".to_string(),
        image_ref: "ghcr.io/acme/web".to_string(),
        image_tag: image_tag.to_string(),
        homepage: None,
        auto_rollback: false,
        backup_bind_paths: BTreeMap::new(),
        backup_volume_names: BTreeMap::new(),
    }];
    db.insert_stack(&stack, &seeds, now).await.unwrap();
    db.update_service_check_result(
        service_id,
        current_digest.map(ToString::to_string),
        current_resolved_tag.map(ToString::to_string),
        current_resolved_tag.map(|value| format!("[\"{value}\"]")),
        candidate_digest.map(|_| image_tag.to_string()),
        candidate_digest.map(|_| "1.1.0".to_string()),
        candidate_digest.map(ToString::to_string),
        candidate_digest.map(|_| "match".to_string()),
        candidate_digest.map(|_| "[\"linux/amd64\"]".to_string()),
        None,
        None,
        now,
        now,
    )
    .await
    .unwrap();
}

async fn insert_successful_check_job(db: &Db, job_id: &str, summary: serde_json::Value) {
    let now = "2026-03-19T00:00:00Z";
    let mut job = JobRecord::new_running(
        job_id.to_string(),
        JobType::Check,
        JobScope::Service,
        Some("stack_1".to_string()),
        Some("svc_1".to_string()),
        now,
    )
    .to_db();
    job.created_by = "test".to_string();
    job.reason = "ui".to_string();
    db.insert_job(job).await.unwrap();
    db.finish_job(job_id, "success", now, &summary)
        .await
        .unwrap();
}

#[tokio::test]
async fn finish_job_counts_unique_visible_versions_for_current_baseline() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("v1.1.0"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_2",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-b",
            Some("1.1.0"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_3",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_4",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-c",
            Some("1.2.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
}

#[tokio::test]
async fn finish_job_canonicalizes_semver_equivalent_visible_versions() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("5.2"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_2",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-b",
            Some("5.2.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn finish_job_collapses_repeated_floating_aliases_by_visible_label() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("latest"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_2",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-b",
            Some("latest"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn finish_job_collapses_repeated_unresolved_non_semver_aliases_by_visible_label() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary_with_candidate_tag(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "15-alpine",
            "sha256:candidate-a",
            Some("15-alpine"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_2",
        make_summary_with_candidate_tag(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "15-alpine",
            "sha256:candidate-b",
            Some("15-alpine"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn finish_job_falls_back_to_display_tag_when_historical_current_digest_is_missing() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            None,
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn finish_job_keeps_unresolved_alias_history_when_current_baseline_is_unresolved() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        None,
        None,
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("latest"),
            None,
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn finish_job_excludes_older_unresolved_alias_history_from_stable_baseline() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("latest"),
            None,
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, None);
}

#[tokio::test]
async fn finish_job_ignores_floating_candidate_alias_once_same_digest_resolves() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("latest"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_2",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn current_baseline_change_drops_old_discovery_counts() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_1",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    db.update_service_check_result(
        "svc_1",
        Some("sha256:current-v2".to_string()),
        Some("2.0.0".to_string()),
        Some("[\"2.0.0\"]".to_string()),
        Some("latest".to_string()),
        Some("2.1.0".to_string()),
        Some("sha256:live-candidate-v2".to_string()),
        Some("match".to_string()),
        Some("[\"linux/amd64\"]".to_string()),
        None,
        None,
        "2026-03-19T00:10:00Z",
        "2026-03-19T00:10:00Z",
    )
    .await
    .unwrap();

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, None);
}

#[tokio::test]
async fn backfill_rebuilds_discoveries_from_successful_check_history() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    let now = "2026-03-19T00:00:00Z";
    let mut job = JobRecord::new_running(
        "job_backfill".to_string(),
        JobType::Check,
        JobScope::Service,
        Some("stack_1".to_string()),
        Some("svc_1".to_string()),
        now,
    )
    .to_db();
    job.status = "success".to_string();
    job.created_by = "test".to_string();
    job.reason = "schedule".to_string();
    job.finished_at = Some(now.to_string());
    job.summary_json = make_summary(
        "svc_1",
        "latest",
        Some("1.0.0"),
        Some("sha256:current-v1"),
        "sha256:candidate-backfill",
        Some("1.1.0"),
    );
    db.insert_job(job).await.unwrap();
    insert_successful_check_job(
        &db,
        "job_backfill_same_visible",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-backfill-2",
            Some("1.1.0"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_backfill_new_visible",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-backfill-3",
            Some("1.2.0"),
        ),
    )
    .await;

    db.call(|conn| {
        conn.execute("DELETE FROM service_new_version_discoveries", [])?;
        Ok(())
    })
    .await
    .unwrap();

    let inserted = db
        .rebuild_new_version_discoveries_from_successful_checks()
        .await
        .unwrap();
    assert_eq!(inserted, 3);

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
}

#[tokio::test]
async fn backfill_rebuild_preserves_image_ref_provenance() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_backfill_image_ref",
        serde_json::json!({
            "newVersions": {
                "count": 1,
                "services": [{
                    "stackId": "stack_1",
                    "serviceId": "svc_1",
                    "serviceName": "web",
                    "imageRef": "ghcr.io/acme/legacy-web",
                    "currentTag": "latest",
                    "currentDigest": "sha256:current-v1",
                    "currentDisplayTag": "1.0.0",
                    "candidateTag": "latest",
                    "candidateDisplayTag": "latest",
                    "candidateDigest": "sha256:candidate-a"
                }],
            }
        }),
    )
    .await;

    db.call(|conn| {
        conn.execute("DELETE FROM service_new_version_discoveries", [])?;
        Ok(())
    })
    .await
    .unwrap();

    db.rebuild_new_version_discoveries_from_successful_checks()
        .await
        .unwrap();

    let rows = db
        .list_new_version_discoveries_for_services(&["svc_1".to_string()])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].image_ref, "ghcr.io/acme/legacy-web");
}

#[tokio::test]
async fn finish_job_records_discoveries_even_when_notifications_are_disabled() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.0.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    let mut settings = db.get_notification_settings().await.unwrap();
    settings.email_enabled = false;
    settings.webhook_enabled = false;
    settings.telegram_enabled = false;
    settings.webpush_enabled = false;
    settings.event_new_version_enabled = false;
    db.put_notification_settings(&settings, "2026-03-19T00:00:00Z")
        .await
        .unwrap();

    insert_successful_check_job(
        &db,
        "job_notifications_disabled",
        make_summary(
            "svc_1",
            "latest",
            Some("1.0.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("1.1.0"),
        ),
    )
    .await;

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(1));
}

#[tokio::test]
async fn get_stack_normalizes_unsettled_discovery_history_from_notifications() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(
        &db,
        "svc_1",
        Some("sha256:current-v1"),
        Some("1.16.0"),
        "latest",
        Some("sha256:live-candidate"),
    )
    .await;

    insert_successful_check_job(
        &db,
        "job_unsettled_a",
        make_summary(
            "svc_1",
            "latest",
            Some("1.16.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-a",
            Some("latest"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_unsettled_b",
        make_summary(
            "svc_1",
            "latest",
            Some("1.16.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-b",
            Some("latest"),
        ),
    )
    .await;
    insert_successful_check_job(
        &db,
        "job_unsettled_c",
        make_summary(
            "svc_1",
            "latest",
            Some("1.16.0"),
            Some("sha256:current-v1"),
            "sha256:candidate-c",
            Some("latest"),
        ),
    )
    .await;

    for (id, digest, display_tag) in [
        ("nvn_a", "sha256:candidate-a", "1.16.2"),
        ("nvn_b", "sha256:candidate-b", "1.16.2"),
        ("nvn_c", "sha256:candidate-c", "1.17.0"),
    ] {
        db.reserve_new_version_notification(&crate::db::NewVersionNotificationPending {
            id: id.to_string(),
            service_id: "svc_1".to_string(),
            job_id: "job_notifications".to_string(),
            reason: "schedule".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: "latest".to_string(),
            current_tag: "latest".to_string(),
            current_display_tag: "1.16.0".to_string(),
            candidate_tag: "latest".to_string(),
            candidate_display_tag: display_tag.to_string(),
            candidate_digest: digest.to_string(),
            created_at: "2026-03-19T00:03:00Z".to_string(),
        })
        .await
        .unwrap();
    }

    let stack = db.get_stack("stack_1").await.unwrap().unwrap();
    assert_eq!(stack.services[0].new_version_discovery_count, Some(2));
}
