use std::{
    collections::BTreeMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::*;

fn job(id: &str, job_type: JobType, status: &str, created_at: &str) -> JobListItem {
    JobListItem {
        id: id.to_string(),
        r#type: job_type,
        scope: JobScope::All,
        stack_id: None,
        service_id: None,
        status: status.to_string(),
        created_at: created_at.to_string(),
        created_by: "test".to_string(),
        reason: "test".to_string(),
        started_at: None,
        finished_at: None,
        allow_arch_mismatch: false,
        backup_mode: "inherit".to_string(),
        summary_json: serde_json::json!({}),
    }
}

#[tokio::test]
async fn claim_next_queued_job_query_uses_the_composite_index() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let plan = db
        .call(|conn| {
            let sql = format!("EXPLAIN QUERY PLAN {CLAIM_NEXT_QUEUED_JOB_SQL}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                params![JobType::GitHubPackagesWebhookSyncRepo.as_str()],
                |row| row.get::<_, String>(3),
            )?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
        .unwrap()
        .join("\n");

    assert!(
        plan.contains("idx_jobs_type_status_created_at_id"),
        "{plan}"
    );
    assert!(!plan.contains("SCAN jobs"), "{plan}");
    assert!(!plan.contains("USE TEMP B-TREE"), "{plan}");
}

#[tokio::test]
async fn claim_next_queued_job_filters_and_claims_in_fifo_order() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let job_type = JobType::GitHubPackagesWebhookSyncRepo;

    db.insert_job(job(
        "other-type-first",
        JobType::RepoLinkBackfill,
        "queued",
        "2026-01-01T00:00:00Z",
    ))
    .await
    .unwrap();
    db.insert_job(job(
        "already-running",
        JobType::GitHubPackagesWebhookSyncRepo,
        "running",
        "2026-01-01T00:00:00Z",
    ))
    .await
    .unwrap();
    db.insert_job(job(
        "queue-b",
        JobType::GitHubPackagesWebhookSyncRepo,
        "queued",
        "2026-01-01T00:01:00Z",
    ))
    .await
    .unwrap();
    db.insert_job(job(
        "queue-a",
        JobType::GitHubPackagesWebhookSyncRepo,
        "queued",
        "2026-01-01T00:01:00Z",
    ))
    .await
    .unwrap();

    let claimed = db
        .claim_next_queued_job_by_type(job_type, "2026-01-01T00:02:00Z")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.id, "queue-a");
    assert_eq!(claimed.status, "running");
    assert_eq!(claimed.started_at.as_deref(), Some("2026-01-01T00:02:00Z"));
    assert_eq!(
        db.get_job("queue-a").await.unwrap().unwrap().status,
        "running"
    );
    assert_eq!(
        db.get_job("queue-b").await.unwrap().unwrap().status,
        "queued"
    );
    assert_eq!(
        db.get_job("other-type-first")
            .await
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );
}

#[test]
fn slow_job_claim_warning_is_thresholded_and_rate_limited_by_type() {
    let warned_at_by_type = Mutex::new(BTreeMap::new());
    let now = Instant::now();

    assert!(!should_emit_slow_job_claim_warning(
        &warned_at_by_type,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD.saturating_sub(Duration::from_millis(1)),
        now,
    ));
    assert!(should_emit_slow_job_claim_warning(
        &warned_at_by_type,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now,
    ));
    assert!(!should_emit_slow_job_claim_warning(
        &warned_at_by_type,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now + Duration::from_secs(59),
    ));
    assert!(should_emit_slow_job_claim_warning(
        &warned_at_by_type,
        "repo_link_backfill",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now + Duration::from_secs(59),
    ));
    assert!(should_emit_slow_job_claim_warning(
        &warned_at_by_type,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now + SLOW_JOB_CLAIM_WARN_INTERVAL,
    ));
}

#[tokio::test]
async fn slow_job_claim_warning_limiter_is_shared_by_db_clones() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();
    let clone = db.clone();
    let now = Instant::now();

    assert!(should_emit_slow_job_claim_warning(
        &db.slow_job_claim_warnings,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now,
    ));
    assert!(!should_emit_slow_job_claim_warning(
        &clone.slow_job_claim_warnings,
        "github_packages_webhook_sync_repo",
        SLOW_JOB_CLAIM_WARN_THRESHOLD,
        now + Duration::from_secs(1),
    ));
}

#[tokio::test]
async fn list_jobs_returns_the_latest_two_thousand_jobs() {
    let db = Db::open(Path::new(":memory:")).await.unwrap();

    for index in 0..=2_000 {
        db.insert_job(JobListItem {
            id: format!("job-{index:04}"),
            r#type: JobType::Update,
            scope: JobScope::Service,
            stack_id: Some("stack-test".to_string()),
            service_id: Some("service-test".to_string()),
            status: "success".to_string(),
            created_at: format!("2026-01-01T00:{:02}:{:02}Z", index / 60, index % 60),
            created_by: "test".to_string(),
            reason: "ui".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({}),
        })
        .await
        .unwrap();
    }

    let jobs = db.list_jobs().await.unwrap();

    assert_eq!(jobs.len(), 2_000);
    assert_eq!(jobs.first().map(|job| job.id.as_str()), Some("job-2000"));
    assert_eq!(jobs.last().map(|job| job.id.as_str()), Some("job-0001"));
    assert!(!jobs.iter().any(|job| job.id == "job-0000"));
}
