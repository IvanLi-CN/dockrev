use super::*;

fn temp_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dockrev-{label}-{}.sqlite3", ulid::Ulid::new()))
}

fn sample(
    service_id: &str,
    sampled_at: &str,
    cpu_percent: f64,
    net_rx_bytes: u64,
) -> ServiceResourceSampleInput {
    ServiceResourceSampleInput {
        service_id: service_id.to_string(),
        sampled_at: sampled_at.to_string(),
        cpu_percent,
        mem_used_bytes: Some(100),
        mem_limit_bytes: Some(200),
        net_rx_bytes: Some(net_rx_bytes),
        net_tx_bytes: Some(net_rx_bytes / 2),
        block_read_bytes: Some(net_rx_bytes / 4),
        block_write_bytes: Some(net_rx_bytes / 8),
        pids: Some(3),
        container_count: 1,
    }
}

#[test]
fn rollup_bucket_is_stable() {
    let epoch = parse_epoch("2026-08-16T13:12:08Z").unwrap();
    assert_eq!(
        epoch - epoch.rem_euclid(60),
        parse_epoch("2026-08-16T13:12:00Z").unwrap()
    );
}

#[tokio::test]
async fn metrics_store_migration_is_idempotent_and_keeps_legacy_rows() {
    let main_path = temp_path("metrics-migration-main");
    let metrics_path = temp_path("metrics-migration-target");
    let db = Db::open(&main_path).await.unwrap();
    let rows = vec![sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)];
    db.insert_legacy_metric_fixture(&rows).await.unwrap();
    let source_before = db.legacy_metrics_integrity().await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("complete")
    );
    assert_eq!(metrics.integrity().await.unwrap(), source_before);
    assert_eq!(db.legacy_metrics_integrity().await.unwrap(), source_before);

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert_eq!(metrics.integrity().await.unwrap(), source_before);
}

#[tokio::test]
async fn metrics_store_migration_preserves_legacy_latest_after_raw_expiry() {
    let main_path = temp_path("metrics-migration-stale-latest-main");
    let metrics_path = temp_path("metrics-migration-stale-latest-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-stale", "2026-08-01T12:00:00Z", 42.0, 4_000)])
        .await
        .unwrap();
    db.delete_legacy_metric_fixture_samples_only("svc-stale")
        .await
        .unwrap();
    let source = db.legacy_metrics_integrity().await.unwrap();
    assert_eq!(source.sample_count, 0);
    assert_eq!(source.latest_count, 1);

    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    assert_eq!(metrics.integrity().await.unwrap(), source);
    let latest = metrics.list_latest_samples().await.unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].service_id, "svc-stale");
    assert_eq!(latest[0].cpu_percent, Some(42.0));
    assert_eq!(latest[0].net_rx_bytes, Some(4_000));
}

#[tokio::test]
async fn metrics_store_migration_skips_inactive_latest_without_raw() {
    let main_path = temp_path("metrics-migration-inactive-latest-main");
    let metrics_path = temp_path("metrics-migration-inactive-latest-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample(
        "svc-inactive-stale",
        "2026-08-01T12:00:00Z",
        42.0,
        4_000,
    )])
    .await
    .unwrap();
    db.delete_legacy_metric_fixture_samples_only("svc-inactive-stale")
        .await
        .unwrap();

    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics
        .migrate_from_legacy_with_active_services(&db, &BTreeSet::new())
        .await
        .unwrap();

    assert!(metrics.list_latest_samples().await.unwrap().is_empty());
}

#[tokio::test]
async fn metrics_store_migration_preserves_active_latest_after_raw_retention() {
    let main_path = temp_path("metrics-migration-active-latest-main");
    let metrics_path = temp_path("metrics-migration-active-latest-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample(
        "svc-active-stale",
        "2026-08-01T12:00:00Z",
        42.0,
        4_000,
    )])
    .await
    .unwrap();

    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .gc(&BTreeSet::from(["svc-active-stale".to_string()]))
        .await
        .unwrap();
    assert_eq!(metrics.pruned_legacy_ids().await.unwrap().len(), 1);

    metrics
        .migrate_from_legacy_with_active_services(
            &db,
            &BTreeSet::from(["svc-active-stale".to_string()]),
        )
        .await
        .unwrap();

    let latest = metrics.list_latest_samples().await.unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].service_id, "svc-active-stale");
    assert_eq!(latest[0].cpu_percent, Some(42.0));
}

#[tokio::test]
async fn metrics_store_migration_restart_preserves_rollup_across_raw_retention_cutoff() {
    let main_path = temp_path("metrics-migration-rollup-retention-main");
    let metrics_path = temp_path("metrics-migration-rollup-retention-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[
        sample("svc-a", "2026-08-16T12:02:00Z", 10.0, 1_000),
        sample("svc-a", "2026-08-16T12:04:00Z", 30.0, 2_000),
    ])
    .await
    .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    let active_service_ids = BTreeSet::from(["svc-a".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2026-08-16T12:02:30Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )
        })
        .await
        .unwrap();

    let before_restart = metrics
        .history_since(
            "svc-a",
            "2026-08-16T12:00:00Z",
            Some(FIVE_MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(before_restart.samples.len(), 1);
    assert!((before_restart.samples[0].cpu_percent - 20.0).abs() < f64::EPSILON);

    metrics.migrate_from_legacy(&db).await.unwrap();

    let after_restart = metrics
        .history_since(
            "svc-a",
            "2026-08-16T12:00:00Z",
            Some(FIVE_MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(after_restart.samples.len(), 1);
    assert!((after_restart.samples[0].cpu_percent - 20.0).abs() < f64::EPSILON);
    assert_eq!(after_restart.peaks[0].cpu_percent, 30.0);
}

#[tokio::test]
async fn metrics_store_migration_preserves_duplicate_legacy_timestamps() {
    let main_path = temp_path("metrics-migration-duplicates-main");
    let metrics_path = temp_path("metrics-migration-duplicates-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[
        sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
        sample("svc-a", "2026-08-16T13:10:00Z", 20.0, 2_000),
    ])
    .await
    .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    let history = metrics
        .history_since("svc-a", "2026-08-16T13:00:00Z", None)
        .await
        .unwrap();

    assert_eq!(history.samples.len(), 2);
    assert_eq!(history.samples[0].cpu_percent, 10.0);
    assert_eq!(history.samples[1].cpu_percent, 20.0);
}

#[tokio::test]
async fn metrics_store_migration_recovers_after_the_target_file_is_replaced() {
    let main_path = temp_path("metrics-migration-recovery-main");
    let metrics_path = temp_path("metrics-migration-recovery-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)])
        .await
        .unwrap();

    {
        let metrics = MetricsStore::open(&metrics_path).await.unwrap();
        metrics.migrate_from_legacy(&db).await.unwrap();
    }
    std::fs::remove_file(&metrics_path).unwrap();
    let _ = std::fs::remove_file(metrics_path.with_extension("sqlite3-wal"));
    let _ = std::fs::remove_file(metrics_path.with_extension("sqlite3-shm"));

    let recovered = MetricsStore::open(&metrics_path).await.unwrap();
    recovered.migrate_from_legacy(&db).await.unwrap();
    let history = recovered
        .history_since("svc-a", "2026-08-16T13:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
}

#[tokio::test]
async fn metrics_store_migration_keeps_retention_gc_pruned_on_restart() {
    let main_path = temp_path("metrics-migration-gc-main");
    let metrics_path = temp_path("metrics-migration-gc-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", "2000-01-01T00:00:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    metrics
        .gc(&BTreeSet::from(["svc-a".to_string()]))
        .await
        .unwrap();
    assert!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );
}

#[tokio::test]
async fn metrics_store_migration_reconciles_deleted_legacy_service() {
    let main_path = temp_path("metrics-migration-delete-main");
    let metrics_path = temp_path("metrics-migration-delete-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    db.delete_legacy_metric_fixture_service("svc-a")
        .await
        .unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    assert!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );
}

#[tokio::test]
async fn metrics_store_migration_restarts_after_a_partial_copy() {
    let main_path = temp_path("metrics-migration-partial-main");
    let metrics_path = temp_path("metrics-migration-partial-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[
        sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
        sample("svc-a", "2026-08-16T13:10:05Z", 20.0, 2_000),
    ])
    .await
    .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics
        .insert_legacy_samples(db.list_legacy_metric_samples_after(0, 1).await.unwrap())
        .await
        .unwrap();
    db.set_metrics_migration_state("copying", Some(&metrics.target_identity), None)
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert_eq!(
        metrics
            .history_since("svc-a", "2026-08-16T13:00:00Z", None)
            .await
            .unwrap()
            .samples
            .len(),
        2
    );
}

#[tokio::test]
async fn metrics_store_migration_rebuilds_latest_and_rollups_after_target_damage() {
    let main_path = temp_path("metrics-migration-damage-main");
    let metrics_path = temp_path("metrics-migration-damage-target");
    let db = Db::open(&main_path).await.unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .writer_call(|conn| {
            conn.execute("DELETE FROM service_resource_latest_samples", [])?;
            conn.execute("DELETE FROM service_resource_rollups", [])?;
            Ok(())
        })
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert_eq!(metrics.list_latest_samples().await.unwrap().len(), 1);
    assert_eq!(
        metrics
            .history_since(
                "svc-a",
                "2026-08-16T13:00:00Z",
                Some(MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap()
            .samples
            .len(),
        1
    );
}

#[tokio::test]
async fn metrics_store_migration_recovers_partial_service_and_rollup_loss() {
    let main_path = temp_path("metrics-migration-partial-damage-main");
    let metrics_path = temp_path("metrics-migration-partial-damage-target");
    let db = Db::open(&main_path).await.unwrap();
    let sampled_at = "2026-08-16T13:10:00Z".to_string();
    db.insert_legacy_metric_fixture(&[
        sample("svc-a", &sampled_at, 10.0, 1_000),
        sample("svc-b", &sampled_at, 20.0, 2_000),
    ])
    .await
    .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    metrics
        .writer_call(|conn| {
            conn.execute(
                "DELETE FROM service_resource_samples WHERE service_id = 'svc-a'",
                [],
            )?;
            conn.execute(
                "DELETE FROM service_resource_latest_samples WHERE service_id = 'svc-a'",
                [],
            )?;
            conn.execute(
                "DELETE FROM service_resource_rollups WHERE service_id = 'svc-b' AND resolution_seconds = ?1",
                params![MINUTE_RESOLUTION_SECONDS as i64],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert_eq!(metrics.list_latest_samples().await.unwrap().len(), 2);
    assert_eq!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .len(),
        1
    );
    assert_eq!(
        metrics
            .history_since(
                "svc-b",
                "1970-01-01T00:00:00Z",
                Some(MINUTE_RESOLUTION_SECONDS)
            )
            .await
            .unwrap()
            .samples
            .len(),
        1
    );
}

#[tokio::test]
async fn metrics_store_migration_recovers_same_cardinality_target_corruption() {
    let main_path = temp_path("metrics-migration-content-damage-main");
    let metrics_path = temp_path("metrics-migration-content-damage-target");
    let db = Db::open(&main_path).await.unwrap();
    let sampled_at = "2026-08-16T13:10:00Z".to_string();
    db.insert_legacy_metric_fixture(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    metrics
        .writer_call(|conn| {
            conn.execute(
                "UPDATE service_resource_samples SET cpu_percent = 99.0 WHERE service_id = 'svc-a'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 10.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_migration_reimports_same_cardinality_source_correction() {
    let main_path = temp_path("metrics-migration-source-content-correction-main");
    let metrics_path = temp_path("metrics-migration-source-content-correction-target");
    let db = Db::open(&main_path).await.unwrap();
    let sampled_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    db.update_legacy_metric_fixture_cpu("svc-a", 25.0)
        .await
        .unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 25.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_migration_recovers_retained_raw_corruption_after_gc() {
    let main_path = temp_path("metrics-migration-pruned-content-damage-main");
    let metrics_path = temp_path("metrics-migration-pruned-content-damage-target");
    let db = Db::open(&main_path).await.unwrap();
    let retained_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    db.insert_legacy_metric_fixture(&[
        sample("svc-a", "2000-01-01T00:00:00Z", 1.0, 100),
        sample("svc-a", &retained_at, 10.0, 1_000),
    ])
    .await
    .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .gc(&BTreeSet::from(["svc-a".to_string()]))
        .await
        .unwrap();
    assert_eq!(metrics.pruned_legacy_ids().await.unwrap().len(), 1);

    let corrupted_at = retained_at.clone();
    metrics
        .writer_call(move |conn| {
            conn.execute(
                "UPDATE service_resource_samples SET cpu_percent = 99.0 WHERE sampled_at = ?1",
                params![corrupted_at],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert_eq!(history.samples[0].sampled_at, retained_at);
    assert!((history.samples[0].cpu_percent - 10.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_migration_rebuilds_corrupted_derived_models() {
    let main_path = temp_path("metrics-migration-derived-damage-main");
    let metrics_path = temp_path("metrics-migration-derived-damage-target");
    let db = Db::open(&main_path).await.unwrap();
    let sampled_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    metrics
        .writer_call(|conn| {
            conn.execute(
                "UPDATE service_resource_latest_samples SET cpu_percent = 99.0 WHERE service_id = 'svc-a'",
                [],
            )?;
            conn.execute(
                "UPDATE service_resource_rollups SET cpu_avg = 99.0, cpu_peak = 99.0 WHERE service_id = 'svc-a'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    metrics.migrate_from_legacy(&db).await.unwrap();
    let latest = metrics.list_latest_samples().await.unwrap();
    assert_eq!(latest[0].cpu_percent, Some(10.0));
    let history = metrics
        .history_since(
            "svc-a",
            "1970-01-01T00:00:00Z",
            Some(MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 10.0).abs() < f64::EPSILON);
    assert!((history.peaks[0].cpu_percent - 10.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_migration_does_not_rehydrate_gc_pruned_orphans() {
    let main_path = temp_path("metrics-migration-orphan-gc-main");
    let metrics_path = temp_path("metrics-migration-orphan-gc-target");
    let db = Db::open(&main_path).await.unwrap();
    let sampled_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics.gc(&BTreeSet::new()).await.unwrap();
    assert_eq!(metrics.pruned_legacy_ids().await.unwrap().len(), 1);

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );
    assert!(metrics.list_latest_samples().await.unwrap().is_empty());
}

#[tokio::test]
async fn metrics_store_migration_preserves_long_rollups_and_stale_latest_after_gc() {
    let main_path = temp_path("metrics-migration-retained-read-models-main");
    let metrics_path = temp_path("metrics-migration-retained-read-models-target");
    let db = Db::open(&main_path).await.unwrap();
    let current_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    db.insert_legacy_metric_fixture(&[sample("svc-main", &current_at, 10.0, 1_000)])
        .await
        .unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();

    let old_at = format_time(time::OffsetDateTime::now_utc() - time::Duration::days(8)).unwrap();
    metrics
        .insert_samples(&[sample("svc-stale", &old_at, 20.0, 2_000)])
        .await
        .unwrap();
    metrics
        .gc(&BTreeSet::from([
            "svc-main".to_string(),
            "svc-stale".to_string(),
        ]))
        .await
        .unwrap();
    assert!(
        metrics
            .history_since("svc-stale", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );
    assert_eq!(
        metrics
            .history_since(
                "svc-stale",
                "1970-01-01T00:00:00Z",
                Some(FIVE_MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap()
            .samples
            .len(),
        1
    );

    metrics.migrate_from_legacy(&db).await.unwrap();
    assert!(
        metrics
            .list_latest_samples()
            .await
            .unwrap()
            .iter()
            .any(|row| row.service_id == "svc-stale")
    );
    assert_eq!(
        metrics
            .history_since(
                "svc-stale",
                "1970-01-01T00:00:00Z",
                Some(FIVE_MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap()
            .samples
            .len(),
        1
    );
}

#[tokio::test]
async fn metrics_store_open_preserves_pre_legacy_target_samples() {
    let metrics_path = temp_path("metrics-pre-legacy-schema");
    let conn = rusqlite::Connection::open(&metrics_path).unwrap();
    conn.execute_batch(
        r#"
CREATE TABLE service_resource_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT NOT NULL,
  sampled_at TEXT NOT NULL,
  cpu_percent REAL NOT NULL,
  mem_used_bytes INTEGER,
  mem_limit_bytes INTEGER,
  net_rx_bytes INTEGER,
  net_tx_bytes INTEGER,
  block_read_bytes INTEGER,
  block_write_bytes INTEGER,
  pids INTEGER,
  container_count INTEGER NOT NULL DEFAULT 1
);
INSERT INTO service_resource_samples (
  service_id, sampled_at, cpu_percent, mem_used_bytes, mem_limit_bytes,
  net_rx_bytes, net_tx_bytes, block_read_bytes, block_write_bytes, pids, container_count
) VALUES ('svc-a', '2026-08-16T13:10:00Z', 10.0, 100, 200, 1000, 500, 250, 125, 3, 1);
"#,
    )
    .unwrap();
    drop(conn);

    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 10.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_memory_reader_uses_the_writer_connection() {
    let metrics = MetricsStore::open(Path::new(":memory:")).await.unwrap();
    metrics
        .insert_samples(&[sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000)])
        .await
        .unwrap();

    let history = metrics
        .history_since("svc-a", "2026-08-16T13:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
}

#[tokio::test]
async fn metrics_store_gc_removes_orphans_when_no_active_services_remain() {
    let metrics = MetricsStore::open(&temp_path("metrics-gc-empty-active"))
        .await
        .unwrap();
    let sampled_at = format_time(time::OffsetDateTime::now_utc()).unwrap();
    metrics
        .insert_samples(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();

    metrics.gc(&BTreeSet::new()).await.unwrap();
    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert!(history.samples.is_empty());
}

#[tokio::test]
async fn metrics_store_rollup_preserves_average_peak_and_terminal_counters() {
    let metrics = MetricsStore::open(&temp_path("metrics-rollup"))
        .await
        .unwrap();
    metrics
        .insert_samples(&[
            sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
            sample("svc-a", "2026-08-16T13:10:05Z", 30.0, 1_500),
        ])
        .await
        .unwrap();
    let history = metrics
        .history_since(
            "svc-a",
            "2026-08-16T13:00:00Z",
            Some(MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(history.resolution_seconds, Some(MINUTE_RESOLUTION_SECONDS));
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 20.0).abs() < f64::EPSILON);
    assert_eq!(history.samples[0].net_rx_bytes, Some(1_500));
    assert_eq!(history.samples[0].net_rx_rate_bps, Some(100.0));
    assert_eq!(history.peaks[0].cpu_percent, 30.0);
    assert_eq!(history.peaks[0].net_rx_rate_bps, Some(100.0));
}

#[tokio::test]
async fn metrics_store_five_minute_rollup_preserves_average_peak_and_terminal_counters() {
    let metrics = MetricsStore::open(&temp_path("metrics-five-minute-rollup"))
        .await
        .unwrap();
    metrics
        .insert_samples(&[
            sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
            sample("svc-a", "2026-08-16T13:10:05Z", 30.0, 1_500),
        ])
        .await
        .unwrap();
    let history = metrics
        .history_since(
            "svc-a",
            "2026-08-16T13:00:00Z",
            Some(FIVE_MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(
        history.resolution_seconds,
        Some(FIVE_MINUTE_RESOLUTION_SECONDS)
    );
    assert_eq!(history.samples.len(), 1);
    assert!((history.samples[0].cpu_percent - 20.0).abs() < f64::EPSILON);
    assert_eq!(history.samples[0].net_rx_bytes, Some(1_500));
    assert_eq!(history.samples[0].net_rx_rate_bps, Some(100.0));
    assert_eq!(history.peaks[0].cpu_percent, 30.0);
    assert_eq!(history.peaks[0].net_rx_rate_bps, Some(100.0));
}

#[tokio::test]
async fn metrics_store_gc_keeps_five_minute_rollups_after_raw_and_minute_expiry() {
    let metrics = MetricsStore::open(&temp_path("metrics-five-minute-retention"))
        .await
        .unwrap();
    let sampled_at =
        format_time(time::OffsetDateTime::now_utc() - time::Duration::days(8)).unwrap();
    metrics
        .insert_samples(&[sample("svc-a", &sampled_at, 10.0, 1_000)])
        .await
        .unwrap();

    metrics
        .gc(&BTreeSet::from(["svc-a".to_string()]))
        .await
        .unwrap();
    assert!(
        metrics
            .history_since("svc-a", "1970-01-01T00:00:00Z", None)
            .await
            .unwrap()
            .samples
            .is_empty()
    );
    assert!(
        metrics
            .history_since(
                "svc-a",
                "1970-01-01T00:00:00Z",
                Some(MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap()
            .samples
            .is_empty()
    );
    assert_eq!(
        metrics
            .history_since(
                "svc-a",
                "1970-01-01T00:00:00Z",
                Some(FIVE_MINUTE_RESOLUTION_SECONDS),
            )
            .await
            .unwrap()
            .samples
            .len(),
        1
    );
}

#[tokio::test]
async fn metrics_store_rollup_orders_duplicate_timestamps_by_id() {
    let metrics = MetricsStore::open(&temp_path("metrics-rollup-duplicates"))
        .await
        .unwrap();
    metrics
        .insert_samples(&[
            sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
            sample("svc-a", "2026-08-16T13:10:00Z", 20.0, 2_000),
        ])
        .await
        .unwrap();
    let history = metrics
        .history_since(
            "svc-a",
            "2026-08-16T13:00:00Z",
            Some(MINUTE_RESOLUTION_SECONDS),
        )
        .await
        .unwrap();
    assert_eq!(history.samples[0].net_rx_bytes, Some(2_000));
    assert!((history.samples[0].cpu_percent - 15.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn metrics_store_long_window_counts_use_rollup_sample_counts() {
    let metrics = MetricsStore::open(&temp_path("metrics-rollup-counts"))
        .await
        .unwrap();
    metrics
        .insert_samples(&[
            sample("svc-a", "2026-08-16T13:10:00Z", 10.0, 1_000),
            sample("svc-a", "2026-08-16T13:10:05Z", 20.0, 2_000),
        ])
        .await
        .unwrap();
    let counts = metrics
        .list_recent_counts_since("1970-01-01T00:00:00Z", Some(MINUTE_RESOLUTION_SECONDS))
        .await
        .unwrap();
    assert_eq!(counts[0].sample_count, 2);
}
