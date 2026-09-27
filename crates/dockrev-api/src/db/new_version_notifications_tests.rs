use std::{collections::BTreeMap, path::Path};

use super::*;
use crate::{
    api::types::{BackupRetention, ComposeConfig, StackBackupConfig},
    db::ComposeServiceSpec,
    models::{ServiceSeed, StackRecord},
};

fn pending(id: &str, service_id: &str, digest: &str) -> NewVersionNotificationPending {
    NewVersionNotificationPending {
        id: id.to_string(),
        service_id: service_id.to_string(),
        job_id: "chk_1".to_string(),
        reason: "schedule".to_string(),
        image_ref: "ghcr.io/acme/web".to_string(),
        image_tag: "latest".to_string(),
        current_tag: "latest".to_string(),
        current_display_tag: "1.0.0".to_string(),
        candidate_tag: "latest".to_string(),
        candidate_display_tag: "1.1.0".to_string(),
        candidate_digest: digest.to_string(),
        created_at: "2026-03-09T00:00:00Z".to_string(),
    }
}

async fn seed_service(db: &Db, service_id: &str, candidate_digest: Option<&str>) {
    let now = "2026-03-09T00:00:00Z";
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
        image_tag: "latest".to_string(),
        homepage: None,
        update_guard: None,
        auto_rollback: false,
        backup_bind_paths: BTreeMap::new(),
        backup_volume_names: BTreeMap::new(),
    }];
    db.insert_stack(&stack, &seeds, now).await.unwrap();
    if let Some(candidate_digest) = candidate_digest {
        db.update_service_check_result(
            service_id,
            Some("sha256:old".to_string()),
            Some("1.0.0".to_string()),
            Some("[\"1.0.0\"]".to_string()),
            Some("latest".to_string()),
            Some("1.1.0".to_string()),
            Some(candidate_digest.to_string()),
            Some("match".to_string()),
            Some("[\"linux/amd64\"]".to_string()),
            None,
            None,
            now,
            now,
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn reserve_reuses_pending_active_digest() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:new")).await;
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let mut second = pending("nvn_2", "svc_1", "sha256:new");
    second.job_id = "chk_2".to_string();

    let first_result = db.reserve_new_version_notification(&first).await.unwrap();
    let second_result = db.reserve_new_version_notification(&second).await.unwrap();

    assert_eq!(
        first_result,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    assert_eq!(
        second_result,
        NewVersionNotificationReserveResult::AlreadyPending("nvn_1".to_string())
    );

    db.finalize_new_version_notification(
        "nvn_1",
        &["webhook".to_string()],
        None,
        "2026-03-09T00:01:00Z",
    )
    .await
    .unwrap();
    assert_eq!(
        db.reserve_new_version_notification(&second).await.unwrap(),
        NewVersionNotificationReserveResult::SkippedDuplicate
    );
}

#[tokio::test]
async fn reserve_canonicalizes_equivalent_active_digests() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "ABC");
    let second = pending("nvn_2", "svc_1", "sha256:abc");

    assert_eq!(
        db.reserve_new_version_notification(&first).await.unwrap(),
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    assert_eq!(
        db.reserve_new_version_notification(&second).await.unwrap(),
        NewVersionNotificationReserveResult::AlreadyPending("nvn_1".to_string())
    );

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].candidate_digest, "sha256:abc");
}

#[tokio::test]
async fn concurrent_reserve_only_allows_one_active_record() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let second = pending("nvn_2", "svc_1", "sha256:new");

    let db_a = db.clone();
    let db_b = db.clone();
    let (first_result, second_result) = tokio::join!(
        db_a.reserve_new_version_notification(&first),
        db_b.reserve_new_version_notification(&second),
    );

    let first_result = first_result.unwrap();
    let second_result = second_result.unwrap();
    let results = [first_result, second_result];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, NewVersionNotificationReserveResult::Reserved(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                NewVersionNotificationReserveResult::AlreadyPending(_)
            ))
            .count(),
        1
    );
}

#[tokio::test]
async fn concurrent_item_reservation_allows_only_one_delivery_claim() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let second = pending("nvn_2", "svc_1", "sha256:new");
    let draft = crate::db::NotificationItemDraft {
        id: "item_1".to_string(),
        kind: "new_version_discovered".to_string(),
        identity_key: "new_version_discovered:chk_1".to_string(),
        title: "发现新版本".to_string(),
        body: "body".to_string(),
        target_url: "/queue/chk_1".to_string(),
        source_job_id: Some("chk_1".to_string()),
        created_at: "2026-03-09T00:00:00Z".to_string(),
    };
    let db_a = db.clone();
    let db_b = db.clone();
    let first_items = vec![first];
    let second_items = vec![second];
    let (first_result, second_result) = tokio::join!(
        db_a.reserve_new_version_notifications_with_item(&first_items, &draft),
        db_b.reserve_new_version_notifications_with_item(&second_items, &draft),
    );

    let results = [first_result, second_result];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(Some(_))))
            .count(),
        1
    );
}

#[tokio::test]
async fn pending_reservation_with_item_is_retried_after_delivery_crash() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let second = pending("nvn_2", "svc_1", "sha256:new");
    let draft = crate::db::NotificationItemDraft {
        id: "item_1".to_string(),
        kind: "new_version_discovered".to_string(),
        identity_key: "new_version_discovered:chk_1".to_string(),
        title: "发现新版本".to_string(),
        body: "body".to_string(),
        target_url: "/queue/chk_1".to_string(),
        source_job_id: Some("chk_1".to_string()),
        created_at: "2026-03-09T00:00:00Z".to_string(),
    };

    let first_result = db
        .reserve_new_version_notifications_with_item(&[first], &draft)
        .await
        .unwrap()
        .unwrap();
    db.call(|conn| {
            conn.execute(
                "UPDATE new_version_notifications SET delivery_claim_expires_at = '2026-03-09T00:00:00Z' WHERE id = 'nvn_1'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let retry_result = db
        .reserve_new_version_notifications_with_item(&[second], &draft)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first_result.0.len(), 1);
    assert_eq!(retry_result.0.len(), 1);
    assert_eq!(retry_result.0[0].0, "nvn_1");
    assert_eq!(retry_result.0[0].1, "nvn_2");
    assert_eq!(first_result.1, retry_result.1);
}

#[tokio::test]
async fn pending_reservation_with_different_batch_does_not_duplicate_inbox_item() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let mut second = pending("nvn_2", "svc_1", "sha256:new");
    second.job_id = "chk_2".to_string();
    let first_draft = crate::db::NotificationItemDraft {
        id: "item_1".to_string(),
        kind: "new_version_discovered".to_string(),
        identity_key: "new_version_discovered:chk_1".to_string(),
        title: "发现新版本".to_string(),
        body: "body".to_string(),
        target_url: "/queue/chk_1".to_string(),
        source_job_id: Some("chk_1".to_string()),
        created_at: "2026-03-09T00:00:00Z".to_string(),
    };
    let mut second_draft = first_draft.clone();
    second_draft.id = "item_2".to_string();
    second_draft.identity_key = "new_version_discovered:chk_2".to_string();
    second_draft.target_url = "/queue/chk_2".to_string();
    second_draft.source_job_id = Some("chk_2".to_string());

    db.reserve_new_version_notifications_with_item(&[first], &first_draft)
        .await
        .unwrap()
        .unwrap();
    assert!(
        db.reserve_new_version_notifications_with_item(&[second], &second_draft)
            .await
            .unwrap()
            .is_none()
    );

    let inbox = db
        .list_notification_items(50, None, "2026-03-09T00:00:01Z")
        .await
        .unwrap();
    assert_eq!(inbox.items.len(), 1);
    assert_eq!(inbox.unread_count, 1);
}

#[tokio::test]
async fn failed_record_does_not_hold_active_slot() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:new")).await;
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let mut second = pending("nvn_2", "svc_1", "sha256:new");
    second.job_id = "chk_2".to_string();

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &[],
            Some("webhook failed"),
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
    );
}

#[tokio::test]
async fn failed_record_reuses_its_batch_identity_for_retry() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:new")).await;
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let mut second = pending("nvn_2", "svc_1", "sha256:new");
    second.job_id = "chk_2".to_string();
    let first_draft = crate::db::NotificationItemDraft {
        id: "item_1".to_string(),
        kind: "new_version_discovered".to_string(),
        identity_key: "new_version_discovered:chk_1".to_string(),
        title: "发现新版本".to_string(),
        body: "body".to_string(),
        target_url: "/queue/chk_1".to_string(),
        source_job_id: Some("chk_1".to_string()),
        created_at: "2026-03-09T00:00:00Z".to_string(),
    };

    db.reserve_new_version_notifications_with_item(&[first], &first_draft)
        .await
        .unwrap()
        .unwrap();
    db.finalize_new_version_notification(
        "nvn_1",
        &[],
        Some("delivery failed"),
        "2026-03-09T00:01:00Z",
    )
    .await
    .unwrap();

    assert_eq!(
        db.find_new_version_notification_batch_job_id(&[(
            "svc_1".to_string(),
            "sha256:new".to_string()
        )])
        .await
        .unwrap()
        .as_deref(),
        Some("chk_1")
    );
    let retry = db
        .reserve_new_version_notifications_with_item(&[second], &first_draft)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.0.len(), 1);
    assert_eq!(retry.1, "item_1");
    assert_eq!(retry.2, 1);
    assert_eq!(
        db.list_notification_items(50, None, "2026-03-09T00:02:00Z")
            .await
            .unwrap()
            .items
            .len(),
        1
    );
}

#[tokio::test]
async fn partial_delivery_failure_does_not_hold_active_slot() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:new")).await;
    let first = pending("nvn_1", "svc_1", "sha256:new");
    let mut second = pending("nvn_2", "svc_1", "sha256:new");
    second.job_id = "chk_2".to_string();

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            Some("telegram failed"),
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
    );
    let sent_channels = db
        .list_new_version_notification_sent_channels(&["nvn_2".to_string()])
        .await
        .unwrap();
    assert_eq!(
        sent_channels.get("nvn_2"),
        Some(&vec!["webhook".to_string()])
    );
    let job_ids = db
        .list_new_version_notification_job_ids(&["nvn_2".to_string()])
        .await
        .unwrap();
    assert_eq!(job_ids.get("nvn_2"), Some(&"chk_1".to_string()));
}

#[tokio::test]
async fn reconcile_supersedes_pending_rows_and_finalize_preserves_audit() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let first = pending("nvn_1", "svc_1", "sha256:old");
    let second = pending("nvn_2", "svc_1", "sha256:old");

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );

    let changed = db
        .reconcile_service_new_version_notifications(
            "svc_1",
            "ghcr.io/acme/web",
            "latest",
            Some("sha256:new"),
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);

    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:03:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
    );

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows[0].status, STATUS_SUPERSEDED);
    assert_eq!(
        rows[0].superseded_at.as_deref(),
        Some("2026-03-09T00:02:00Z")
    );
    assert_eq!(rows[0].sent_at.as_deref(), Some("2026-03-09T00:03:00Z"));
    assert_eq!(rows[0].sent_channels, vec!["webhook".to_string()]);
    assert_eq!(rows[1].status, STATUS_PENDING);
}

#[tokio::test]
async fn reconcile_supersedes_old_active_rows() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:old")).await;
    let first = pending("nvn_1", "svc_1", "sha256:old");

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    let changed = db
        .reconcile_service_new_version_notifications(
            "svc_1",
            "ghcr.io/acme/web",
            "latest",
            Some("sha256:new"),
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows[0].status, STATUS_SUPERSEDED);
    assert_eq!(
        rows[0].superseded_at.as_deref(),
        Some("2026-03-09T00:02:00Z")
    );
}

#[tokio::test]
async fn reconcile_without_candidate_supersedes_active_rows_and_allows_reuse() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:old")).await;
    let first = pending("nvn_1", "svc_1", "sha256:old");
    let second = pending("nvn_2", "svc_1", "sha256:old");

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    let changed = db
        .reconcile_service_new_version_notifications(
            "svc_1",
            "ghcr.io/acme/web",
            "latest",
            None,
            "2026-03-09T00:02:00Z",
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
    );

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows[0].status, STATUS_SUPERSEDED);
    assert_eq!(rows[1].status, STATUS_PENDING);
}

#[tokio::test]
async fn sync_stack_from_compose_keeps_active_rows_for_unchanged_services() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:old")).await;
    let first = pending("nvn_1", "svc_1", "sha256:old");
    let second = pending("nvn_2", "svc_1", "sha256:old");

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    db.sync_stack_from_compose(
        "stack_1",
        &["/tmp/demo.yml".to_string()],
        &[ComposeServiceSpec {
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web".to_string(),
            image_tag: "latest".to_string(),
            homepage: None,
            update_guard: None,
            backup_bind_paths: Vec::new(),
            backup_volume_names: Vec::new(),
        }],
        "2026-03-09T00:02:00Z",
    )
    .await
    .unwrap();

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::SkippedDuplicate
    );

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, STATUS_SENT);
    assert_eq!(rows[0].superseded_at, None);
}

#[tokio::test]
async fn sync_stack_from_compose_supersedes_active_rows_when_baseline_changes() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:old")).await;
    let first = pending("nvn_1", "svc_1", "sha256:old");
    let second = pending("nvn_2", "svc_1", "sha256:old");

    let reserved = db.reserve_new_version_notification(&first).await.unwrap();
    assert_eq!(
        reserved,
        NewVersionNotificationReserveResult::Reserved("nvn_1".to_string())
    );
    let finalized = db
        .finalize_new_version_notification(
            "nvn_1",
            &["webhook".to_string()],
            None,
            "2026-03-09T00:01:00Z",
        )
        .await
        .unwrap();
    assert!(finalized);

    db.sync_stack_from_compose(
        "stack_1",
        &["/tmp/demo.yml".to_string()],
        &[ComposeServiceSpec {
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/web-next".to_string(),
            image_tag: "stable".to_string(),
            homepage: None,
            update_guard: None,
            backup_bind_paths: Vec::new(),
            backup_volume_names: Vec::new(),
        }],
        "2026-03-09T00:02:00Z",
    )
    .await
    .unwrap();

    let retried = db.reserve_new_version_notification(&second).await.unwrap();
    assert_eq!(
        retried,
        NewVersionNotificationReserveResult::Reserved("nvn_2".to_string())
    );

    let rows = db
        .list_new_version_notifications_for_service("svc_1")
        .await
        .unwrap();
    assert_eq!(rows[0].status, STATUS_SUPERSEDED);
    assert_eq!(
        rows[0].superseded_at.as_deref(),
        Some("2026-03-09T00:02:00Z")
    );
    assert_eq!(rows[1].status, STATUS_PENDING);

    let target = db
        .list_current_new_version_notification_targets(&["svc_1".to_string()])
        .await
        .unwrap();
    assert_eq!(target.len(), 1);
    assert_eq!(target[0].image_ref, "ghcr.io/acme/web-next");
    assert_eq!(target[0].image_tag, "stable");
    assert_eq!(target[0].candidate_digest, None);
}

#[tokio::test]
async fn list_stable_candidate_display_tags_batches_large_target_sets() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:seed")).await;

    let mut targets = Vec::new();
    for idx in 0..450 {
        let digest = format!("sha256:{idx:064x}");
        let mut row = pending(&format!("nvn_{idx}"), "svc_1", &digest);
        row.candidate_display_tag = format!("1.{}.0", idx + 1);
        db.reserve_new_version_notification(&row).await.unwrap();
        targets.push((
            "svc_1".to_string(),
            "ghcr.io/acme/web".to_string(),
            "latest".to_string(),
            digest,
        ));
    }

    let resolved = db
        .list_stable_candidate_display_tags_for_notification_targets(&targets)
        .await
        .unwrap();

    assert_eq!(resolved.len(), 450);
    assert_eq!(
        resolved.get(&(
            "svc_1".to_string(),
            "ghcr.io/acme/web".to_string(),
            "latest".to_string(),
            format!("sha256:{:064x}", 0)
        )),
        Some(&std::collections::BTreeSet::from(["1.1.0".to_string()]))
    );
    assert_eq!(
        resolved.get(&(
            "svc_1".to_string(),
            "ghcr.io/acme/web".to_string(),
            "latest".to_string(),
            format!("sha256:{:064x}", 449)
        )),
        Some(&std::collections::BTreeSet::from(["1.450.0".to_string()]))
    );
}

#[tokio::test]
async fn list_stable_candidate_display_tags_keeps_repo_track_provenance_separate() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    seed_service(&db, "svc_1", Some("sha256:seed")).await;

    let digest = "sha256:shared".to_string();
    let mut web = pending("nvn_web", "svc_1", &digest);
    web.image_ref = "ghcr.io/acme/web".to_string();
    web.image_tag = "latest".to_string();
    web.candidate_display_tag = "1.16.2".to_string();
    db.reserve_new_version_notification(&web).await.unwrap();

    db.sync_stack_from_compose(
        "stack_1",
        &["/tmp/demo.yml".to_string()],
        &[ComposeServiceSpec {
            name: "web".to_string(),
            image_ref: "ghcr.io/acme/worker".to_string(),
            image_tag: "stable".to_string(),
            homepage: None,
            update_guard: None,
            backup_bind_paths: Vec::new(),
            backup_volume_names: Vec::new(),
        }],
        "2026-03-09T00:02:00Z",
    )
    .await
    .unwrap();

    let mut worker = pending("nvn_worker", "svc_1", &digest);
    worker.image_ref = "ghcr.io/acme/worker".to_string();
    worker.image_tag = "stable".to_string();
    worker.candidate_display_tag = "2.0.0".to_string();
    db.reserve_new_version_notification(&worker).await.unwrap();

    let mut unrequested = pending("nvn_unrequested", "svc_1", "sha256:unrequested");
    unrequested.image_ref = "ghcr.io/acme/unrequested".to_string();
    unrequested.image_tag = "edge".to_string();
    unrequested.candidate_display_tag = "3.0.0".to_string();
    db.reserve_new_version_notification(&unrequested)
        .await
        .unwrap();

    let resolved = db
        .list_stable_candidate_display_tags_for_notification_targets(&[
            (
                "svc_1".to_string(),
                "ghcr.io/acme/web".to_string(),
                "latest".to_string(),
                digest.clone(),
            ),
            (
                "svc_1".to_string(),
                "ghcr.io/acme/worker".to_string(),
                "stable".to_string(),
                digest.clone(),
            ),
        ])
        .await
        .unwrap();

    assert_eq!(resolved.len(), 2);
    assert_eq!(
        resolved.get(&(
            "svc_1".to_string(),
            "ghcr.io/acme/web".to_string(),
            "latest".to_string(),
            digest.clone()
        )),
        Some(&std::collections::BTreeSet::from(["1.16.2".to_string()]))
    );
    assert_eq!(
        resolved.get(&(
            "svc_1".to_string(),
            "ghcr.io/acme/worker".to_string(),
            "stable".to_string(),
            digest
        )),
        Some(&std::collections::BTreeSet::from(["2.0.0".to_string()]))
    );
}
