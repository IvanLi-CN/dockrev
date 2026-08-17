use super::*;
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
async fn metrics_store_migration_rejects_corrupted_native_rollups_after_raw_expiry() {
    let main_path = temp_path("metrics-migration-corrupted-native-rollup-main");
    let metrics_path = temp_path("metrics-migration-corrupted-native-rollup-target");
    let db = Db::open(&main_path).await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .insert_samples(&[sample("svc-native", "2026-08-16T12:02:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let active_service_ids = BTreeSet::from(["svc-native".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2026-08-16T12:02:30Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )?;
            conn.execute(
                "UPDATE service_resource_rollups SET cpu_avg = 99.0 WHERE service_id = 'svc-native'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(metrics.migrate_from_legacy(&db).await.is_err());
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("copying")
    );
}

#[tokio::test]
async fn metrics_store_migration_rejects_native_raw_loss() {
    let main_path = temp_path("metrics-migration-native-raw-loss-main");
    let metrics_path = temp_path("metrics-migration-native-raw-loss-target");
    let db = Db::open(&main_path).await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .insert_samples(&[sample("svc-native", "2026-08-16T12:02:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    metrics
        .writer_call(|conn| {
            conn.execute(
                "DELETE FROM service_resource_samples WHERE service_id = 'svc-native'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(metrics.migrate_from_legacy(&db).await.is_err());
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("copying")
    );
}

#[tokio::test]
async fn metrics_store_migration_rejects_native_latest_loss_after_raw_expiry() {
    let main_path = temp_path("metrics-migration-native-latest-loss-main");
    let metrics_path = temp_path("metrics-migration-native-latest-loss-target");
    let db = Db::open(&main_path).await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .insert_samples(&[sample("svc-native", "2026-08-16T12:02:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let active_service_ids = BTreeSet::from(["svc-native".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2026-08-16T12:02:30Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )?;
            conn.execute(
                "DELETE FROM service_resource_latest_samples WHERE service_id = 'svc-native'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(metrics.migrate_from_legacy(&db).await.is_err());
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("copying")
    );
}

#[tokio::test]
async fn metrics_store_migration_rejects_missing_native_rollups_after_raw_expiry() {
    let main_path = temp_path("metrics-migration-native-rollup-loss-main");
    let metrics_path = temp_path("metrics-migration-native-rollup-loss-target");
    let db = Db::open(&main_path).await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .insert_samples(&[sample("svc-native", "2026-08-16T12:02:00Z", 10.0, 1_000)])
        .await
        .unwrap();
    let active_service_ids = BTreeSet::from(["svc-native".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2026-08-16T12:02:30Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )?;
            conn.execute(
                "DELETE FROM service_resource_rollups WHERE service_id = 'svc-native'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(metrics.migrate_from_legacy(&db).await.is_err());
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("copying")
    );
}

#[tokio::test]
async fn metrics_store_migration_rejects_partial_native_rollups_after_raw_expiry() {
    let main_path = temp_path("metrics-migration-native-partial-rollup-main");
    let metrics_path = temp_path("metrics-migration-native-partial-rollup-target");
    let db = Db::open(&main_path).await.unwrap();
    let metrics = MetricsStore::open(&metrics_path).await.unwrap();
    metrics.migrate_from_legacy(&db).await.unwrap();
    metrics
        .insert_samples(&[
            sample("svc-native", "2026-08-16T12:02:00Z", 10.0, 1_000),
            sample("svc-native", "2026-08-16T12:02:45Z", 20.0, 1_090),
        ])
        .await
        .unwrap();
    let active_service_ids = BTreeSet::from(["svc-native".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2026-08-16T12:02:30Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )?;
            conn.execute(
                "UPDATE service_resource_rollups SET cpu_avg = 99.0 WHERE service_id = 'svc-native'",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(metrics.migrate_from_legacy(&db).await.is_err());
    assert_eq!(
        db.metrics_migration_state()
            .await
            .unwrap()
            .as_ref()
            .map(|state| state.state.as_str()),
        Some("copying")
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
async fn metrics_store_active_migration_does_not_rehydrate_gc_pruned_raw_after_target_damage() {
    let main_path = temp_path("metrics-migration-active-pruned-target-damage-main");
    let metrics_path = temp_path("metrics-migration-active-pruned-target-damage-target");
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

    metrics
        .writer_call(|conn| {
            conn.execute(
                "UPDATE service_resource_samples SET cpu_percent = 99.0 WHERE legacy_id IS NOT NULL",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    metrics
        .migrate_from_legacy_with_active_services(&db, &BTreeSet::from(["svc-a".to_string()]))
        .await
        .unwrap();
    let history = metrics
        .history_since("svc-a", "1970-01-01T00:00:00Z", None)
        .await
        .unwrap();
    assert_eq!(history.samples.len(), 1);
    assert_eq!(history.samples[0].sampled_at, retained_at);
    assert_eq!(history.samples[0].cpu_percent, 10.0);
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
