#[tokio::test]
async fn jobs_events_stream_honors_after_id_or_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "first" }).to_string(),
            },
        )
        .await
        .unwrap();
    let first_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "second" }).to_string(),
            },
        )
        .await
        .unwrap();
    let second_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();
    let second_id_s = second_id.to_string();

    let resp_query = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/jobs/events?afterId={first_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_query.status(), 200);
    let mut body_query = resp_query.into_body();
    let evt_query = wait_for_sse_event(&mut body_query, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt_query.id.as_deref(), Some(second_id_s.as_str()));
    let payload_query: serde_json::Value = serde_json::from_str(&evt_query.data).unwrap();
    assert_eq!(payload_query["step"].as_str().unwrap(), "second");

    let resp_header = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .header("Last-Event-ID", first_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_header.status(), 200);
    let mut body_header = resp_header.into_body();
    let evt_header =
        wait_for_sse_event(&mut body_header, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt_header.id.as_deref(), Some(second_id_s.as_str()));
    let payload_header: serde_json::Value = serde_json::from_str(&evt_header.data).unwrap();
    assert_eq!(payload_header["step"].as_str().unwrap(), "second");
}

#[tokio::test]
async fn jobs_events_stream_default_starts_from_tail_without_replay() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Check,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "old" }).to_string(),
            },
        )
        .await
        .unwrap();
    let old_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/jobs/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

    state
        .db
        .insert_job_log(
            &job_id,
            &crate::api::types::JobLogLine {
                ts: now.clone(),
                level: "event".to_string(),
                msg: serde_json::json!({ "type": "job_progress", "step": "new" }).to_string(),
            },
        )
        .await
        .unwrap();
    let new_id = state.db.get_job_logs_last_id(&job_id).await.unwrap();
    let new_id_s = new_id.to_string();
    let old_id_s = old_id.to_string();

    let evt = wait_for_sse_event(&mut body, "job_event", Duration::from_secs(3)).await;
    assert_eq!(evt.id.as_deref(), Some(new_id_s.as_str()));
    assert_ne!(evt.id.as_deref(), Some(old_id_s.as_str()));
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["step"].as_str().unwrap(), "new");
}

#[tokio::test]
async fn version_inference_overview_reports_rows_filters_and_pagination() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let compose_path = format!("/tmp/dockrev-test-{}.yml", ulid::Ulid::new());
    std::fs::write(
        &compose_path,
        r#"
services:
  web:
    image: ghcr.io/acme/web:latest
  worker:
    image: ghcr.io/acme/worker:latest
  stable:
    image: ghcr.io/acme/stable:1.2.3
"#,
    )
    .unwrap();
    let _stack_id = seed_stack_from_compose(&state, "demo", &compose_path).await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/web",
        "sha256:web",
        "linux/amd64",
        &now,
        vec!["1.2.3".to_string(), "latest".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 2,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/worker",
        "sha256:worker",
        "linux/amd64",
        &now,
        Vec::new(),
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 2,
            repo_tags_considered: 2,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 2,
        },
    )
    .await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?page=1&perPage=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = response_json(resp).await;

    assert_eq!(body["summary"]["snapshotsTotal"].as_u64(), Some(2));
    assert_eq!(body["summary"]["ready"].as_u64(), Some(1));
    assert_eq!(body["summary"]["allFailed"].as_u64(), Some(1));
    assert_eq!(body["page"].as_u64(), Some(1));
    assert_eq!(body["perPage"].as_u64(), Some(10));

    let rows = body["rows"].as_array().expect("rows must be an array");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/web")
            && row["status"].as_str() == Some("ready")
            && row["serviceCount"].as_u64() == Some(1)
    }));
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/worker")
            && row["status"].as_str() == Some("all_failed")
            && row["serviceCount"].as_u64() == Some(1)
    }));

    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?status=all_failed&page=1&perPage=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(filtered.status(), 200);
    let filtered_body = response_json(filtered).await;
    assert_eq!(filtered_body["total"].as_u64(), Some(1));
    assert_eq!(filtered_body["summary"]["snapshotsTotal"].as_u64(), Some(2));
    assert_eq!(filtered_body["summary"]["ready"].as_u64(), Some(1));
    assert_eq!(filtered_body["summary"]["allFailed"].as_u64(), Some(1));
    let filtered_rows = filtered_body["rows"].as_array().unwrap();
    assert_eq!(filtered_rows.len(), 1);
    assert_eq!(filtered_rows[0]["status"].as_str(), Some("all_failed"));

    let overflow_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?page=4294967295&perPage=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(overflow_page.status(), 200);
    let overflow_body = response_json(overflow_page).await;
    assert_eq!(overflow_body["page"].as_u64(), Some(4_294_967_295));
    assert_eq!(overflow_body["perPage"].as_u64(), Some(200));
    assert_eq!(overflow_body["total"].as_u64(), Some(2));
    assert_eq!(overflow_body["rows"].as_array().unwrap().len(), 0);

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/overview?status=missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
}

#[tokio::test]
async fn version_inference_overview_merges_cached_and_in_flight_without_missing_rows() {
    let registry = Arc::new(SlowRegistry {
        delay: Duration::from_millis(250),
    });
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/cached",
        "sha256:cached",
        "linux/amd64",
        &now,
        vec!["1.2.3".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 1,
            repo_tags_considered: 1,
            manifests_ok: 1,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/running",
            "sha256:running",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let mut observed: Option<serde_json::Value> = None;
    for _ in 0..80 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/version-inference/overview?page=1&perPage=20")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = response_json(resp).await;

        let tasks = body["tasks"].as_array().cloned().unwrap_or_default();
        let has_task = tasks.iter().any(|task| {
            task["key"].as_str() == Some("ghcr.io/acme/running@sha256:running@linux/amd64")
        });
        let has_progress = tasks
            .iter()
            .any(|task| task["status"].as_str() == Some("running") && task["progress"].is_object());
        let progress_advanced = tasks.iter().any(|task| {
            task["status"].as_str() == Some("running")
                && task["progress"]["assignedCurrent"].as_u64().unwrap_or(0) > 0
        });
        if has_task && has_progress && progress_advanced {
            observed = Some(body);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let body = observed.expect("expected in-flight task with progress in overview");
    assert_eq!(body["summary"]["snapshotsTotal"].as_u64(), Some(1));

    let rows = body["rows"].as_array().expect("rows should be array");
    assert!(
        rows.iter()
            .all(|row| row["status"].as_str() != Some("missing")),
        "overview rows should not include missing status"
    );
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/cached")
            && row["status"].as_str() == Some("ready")
    }));
    assert!(rows.iter().any(|row| {
        row["imageRepo"].as_str() == Some("ghcr.io/acme/running")
            && matches!(row["status"].as_str(), Some("running") | Some("queued"))
    }));

    let running_task = body["tasks"]
        .as_array()
        .and_then(|tasks| {
            tasks.iter().find(|task| {
                task["key"].as_str() == Some("ghcr.io/acme/running@sha256:running@linux/amd64")
            })
        })
        .expect("in-flight task should be present");
    assert!(
        matches!(
            running_task["status"].as_str(),
            Some("running") | Some("queued")
        ),
        "task status should be queued or running"
    );
    if running_task["status"].as_str() == Some("running") {
        let progress = running_task["progress"]
            .as_object()
            .expect("running task should include progress");
        assert!(progress.contains_key("phase"));
        assert!(progress.contains_key("current"));
        assert!(progress.contains_key("total"));
        assert!(progress.contains_key("percent"));
        assert!(progress.contains_key("assignedCurrent"));
        assert!(progress.contains_key("assignedTotal"));
        assert!(progress.contains_key("assignedPercent"));
        assert!(progress.contains_key("resultCurrent"));
        assert!(progress.contains_key("resultTotal"));
        assert!(progress.contains_key("resultPercent"));
        assert!(
            progress["assignedCurrent"].as_u64().unwrap_or(0) > 0,
            "running task should expose advancing in-task progress"
        );
    }
}

#[tokio::test]
async fn version_inference_overview_progress_keeps_success_lower_than_assignment_on_errors() {
    let registry = Arc::new(PartialFailureRegistry {
        delay: Duration::from_millis(140),
    });
    let state = test_state_with(":memory:", registry, Arc::new(FakeRunner)).await;
    let app = api::router(state.clone());

    let enqueued = state
        .snapshot_worker
        .enqueue(
            "ghcr.io/acme/partial-failure",
            "sha256:partial-failure",
            "linux/amd64",
            "force",
        )
        .await;
    assert!(enqueued);

    let mut observed = false;
    for _ in 0..120 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/version-inference/overview?page=1&perPage=20")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = response_json(resp).await;
        let maybe_progress = body["tasks"].as_array().and_then(|tasks| {
            tasks.iter().find_map(|task| {
                if task["key"].as_str()
                    != Some("ghcr.io/acme/partial-failure@sha256:partial-failure@linux/amd64")
                {
                    return None;
                }
                if task["status"].as_str() != Some("running") {
                    return None;
                }
                task["progress"].as_object()
            })
        });

        if let Some(progress) = maybe_progress {
            let assigned = progress
                .get("assignedCurrent")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let result = progress
                .get("resultCurrent")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if assigned > result {
                observed = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    assert!(
        observed,
        "expected running progress to show assignedCurrent > resultCurrent when manifest errors occur"
    );
}

#[tokio::test]
async fn version_inference_events_stream_emits_task_enqueued() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );

    let mut body = resp.into_body();
    let enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/web", "sha256:web", "linux/amd64", "force")
        .await;
    assert!(enqueued);

    let evt =
        wait_for_sse_event(&mut body, "version_inference_event", Duration::from_secs(3)).await;
    assert!(evt.id.is_some(), "SSE event should include id");
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("task_enqueued"));
    assert_eq!(payload["imageRepo"].as_str(), Some("ghcr.io/acme/web"));
}

#[tokio::test]
async fn version_inference_events_stream_reconnects_after_last_event_id() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut first_body = resp.into_body();

    let first_enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/web", "sha256:first", "linux/amd64", "force")
        .await;
    assert!(first_enqueued);

    let first_evt = wait_for_sse_event(
        &mut first_body,
        "version_inference_event",
        Duration::from_secs(3),
    )
    .await;
    let first_id = first_evt.id.expect("first SSE event should include id");
    let first_id_num = first_id.parse::<u64>().unwrap();

    let reconnect = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/version-inference/events?afterId={first_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reconnect.status(), 200);
    let mut reconnect_body = reconnect.into_body();

    let second_enqueued = state
        .snapshot_worker
        .enqueue("ghcr.io/acme/api", "sha256:second", "linux/amd64", "force")
        .await;
    assert!(second_enqueued);

    let second_evt = wait_for_sse_event(
        &mut reconnect_body,
        "version_inference_event",
        Duration::from_secs(3),
    )
    .await;
    let second_id = second_evt
        .id
        .expect("reconnected SSE event should include id");
    let second_id_num = second_id.parse::<u64>().unwrap();
    assert!(
        second_id_num > first_id_num,
        "expected reconnected stream to resume after {first_id_num}, got {second_id_num}"
    );

    let payload: serde_json::Value = serde_json::from_str(&second_evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("task_enqueued"));
    assert_eq!(payload["imageRepo"].as_str(), Some("ghcr.io/acme/api"));
}

#[tokio::test]
async fn version_inference_events_stream_emits_resync_required_when_after_id_is_too_old() {
    let state = test_state(":memory:").await;
    let app = api::router(state.clone());

    for i in 0..2105 {
        let image_repo = format!("ghcr.io/acme/resync-{i}");
        let enqueued = state
            .snapshot_worker
            .enqueue(&image_repo, "sha256:resync", "linux/amd64", "force")
            .await;
        assert!(enqueued);
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/version-inference/events?afterId=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let mut body = resp.into_body();

    let evt =
        wait_for_sse_event(&mut body, "version_inference_event", Duration::from_secs(3)).await;
    let payload: serde_json::Value = serde_json::from_str(&evt.data).unwrap();
    assert_eq!(payload["type"].as_str(), Some("resync_required"));
    assert_eq!(payload["requestedAfterId"].as_i64(), Some(1));
    assert!(
        payload["oldestAvailableId"].as_i64().unwrap_or_default() > 1,
        "expected ring buffer oldest event id to move forward"
    );
}

#[tokio::test]
async fn version_inference_gc_runs_on_start_and_deletes_expired_snapshots() {
    let state = test_state(":memory:").await;

    let old = (time::OffsetDateTime::now_utc() - time::Duration::days(40))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    upsert_image_digest_snapshot_for_test(
        &state,
        "ghcr.io/acme/old",
        "sha256:old",
        "linux/amd64",
        &old,
        vec!["1.0.0".to_string()],
        crate::api::types::ServiceDigestTagsScanSummary {
            repo_tags_total: 0,
            repo_tags_considered: 0,
            manifests_ok: 0,
            manifests_timeout: 0,
            manifests_error: 0,
        },
    )
    .await;

    assert!(
        state
            .db
            .get_image_digest_tags_snapshot("ghcr.io/acme/old", "sha256:old", "linux/amd64")
            .await
            .unwrap()
            .is_some()
    );

    state.snapshot_worker.spawn_gc_task();

    let mut deleted = false;
    for _ in 0..80 {
        if state
            .db
            .get_image_digest_tags_snapshot("ghcr.io/acme/old", "sha256:old", "linux/amd64")
            .await
            .unwrap()
            .is_none()
        {
            deleted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        deleted,
        "expired version inference snapshots should be deleted"
    );

    let gc = state.snapshot_worker.gc_status().await;
    assert!(
        gc.last_run_at.is_some(),
        "gc should record last run timestamp"
    );
    assert!(
        gc.last_deleted.unwrap_or(0) >= 1,
        "gc should report at least one deleted snapshot"
    );
}

#[tokio::test]
async fn recover_incomplete_jobs_marks_running_as_failed() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    let mut job = crate::api::types::JobRecord::new_running(
        job_id.clone(),
        crate::api::types::JobType::Update,
        crate::api::types::JobScope::All,
        None,
        None,
        &now,
    )
    .to_db();
    job.created_by = "ivan".to_string();
    job.reason = "ui".to_string();
    state.db.insert_job(job).await.unwrap();

    let recovered = state
        .db
        .recover_incomplete_jobs(&now, "server_restart")
        .await
        .unwrap();
    assert!(recovered.iter().any(|id| id == &job_id));

    let got = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(got.status, "failed");
    assert!(got.finished_at.is_some());
    assert_eq!(
        got.summary_json["terminated"]["reason"].as_str().unwrap(),
        "server_restart"
    );
}

#[tokio::test]
async fn recover_incomplete_jobs_keeps_queued_jobs_pending() {
    let state = test_state(":memory:").await;

    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let job_id = ids::new_job_id();
    state
        .db
        .insert_job(crate::api::types::JobListItem {
            id: job_id.clone(),
            r#type: crate::api::types::JobType::GitHubPackagesWebhook,
            scope: crate::api::types::JobScope::All,
            stack_id: None,
            service_id: None,
            status: "queued".to_string(),
            created_at: now.clone(),
            created_by: "ivan".to_string(),
            reason: "ui".to_string(),
            started_at: None,
            finished_at: None,
            allow_arch_mismatch: false,
            backup_mode: "inherit".to_string(),
            summary_json: serde_json::json!({ "op": "register", "repos": ["acme/widgets"] }),
        })
        .await
        .unwrap();

    let recovered = state
        .db
        .recover_incomplete_jobs(&now, "server_restart")
        .await
        .unwrap();
    assert!(
        !recovered.iter().any(|id| id == &job_id),
        "queued job should not be force-failed by startup recovery"
    );

    let got = state.db.get_job(&job_id).await.unwrap().unwrap();
    assert_eq!(got.status, "queued");
    assert!(got.started_at.is_none());
    assert!(got.finished_at.is_none());
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

    let services = state.db.list_services_for_check(&stack_id).await.unwrap();
    let svc = services.first().unwrap().clone();
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    state
        .db
        .update_service_check_result(
            &svc.id,
            Some("sha256:old".to_string()),
            Some("5.2".to_string()),
            Some(r#"["5.2"]"#.to_string()),
            Some(svc.image_tag.clone()),
            Some("5.3".to_string()),
            Some("sha256:new".to_string()),
            Some("match".to_string()),
            Some(r#"["linux/amd64"]"#.to_string()),
            None,
            None,
            &now,
            &now,
        )
        .await
        .unwrap();
    let update = serde_json::json!({
        "scope": "stack",
        "stackId": stack_id,
        "targets": [{
            "serviceId": svc.id,
            "targetTag": svc.image_tag,
            "targetDigest": "sha256:new",
            "pullTags": []
        }],
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
        "targets": [],
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

