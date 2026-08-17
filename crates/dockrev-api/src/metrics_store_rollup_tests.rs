use super::*;

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
async fn metrics_store_gc_counts_native_rows_from_the_deleted_batch() {
    let metrics = MetricsStore::open(&temp_path("metrics-gc-mixed-native-batch"))
        .await
        .unwrap();
    metrics
        .writer_call(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            begin_managed_metrics_write_tx(&tx)?;
            tx.execute(
                r#"INSERT INTO service_resource_samples (
                     legacy_id, service_id, sampled_at, cpu_percent, container_count
                   ) VALUES (1, 'svc-legacy', '2000-01-01T00:00:00Z', 1.0, 1)"#,
                [],
            )?;
            tx.execute_batch(
                r#"WITH digit(value) AS (
                     VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
                   )
                   INSERT INTO service_resource_samples (
                     service_id, sampled_at, cpu_percent, container_count
                   )
                   SELECT 'svc-native', '2000-01-01T00:00:01Z', 1.0, 1
                   FROM digit AS a CROSS JOIN digit AS b CROSS JOIN digit AS c CROSS JOIN digit AS d"#,
            )?;
            adjust_native_raw_count_tx(&tx, GC_BATCH_SIZE as i64)?;
            end_managed_metrics_write_tx(&tx)?;
            trust_metrics_target_tx(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .await
        .unwrap();

    let active_service_ids = BTreeSet::from(["svc-legacy".to_string(), "svc-native".to_string()]);
    metrics
        .writer_call(move |conn| {
            gc_batch_tx(
                conn,
                "2001-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                "1970-01-01T00:00:00Z",
                &active_service_ids,
            )
        })
        .await
        .unwrap();

    let (native_rows, tracked_native_rows) = metrics
        .writer_call(|conn| {
            Ok((
                conn.query_row(
                    "SELECT COUNT(*) FROM service_resource_samples WHERE legacy_id IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
                conn.query_row(
                    "SELECT raw_row_count FROM metrics_native_integrity WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(native_rows, 1);
    assert_eq!(tracked_native_rows, native_rows);
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
async fn metrics_store_rebuilds_successor_rollup_for_out_of_order_samples() {
    let metrics = MetricsStore::open(&temp_path("metrics-rollup-out-of-order"))
        .await
        .unwrap();
    metrics
        .insert_samples(&[
            sample("svc-a", "2026-08-16T13:00:50Z", 10.0, 900),
            sample("svc-a", "2026-08-16T13:01:05Z", 20.0, 1_100),
        ])
        .await
        .unwrap();
    metrics
        .insert_samples(&[sample("svc-a", "2026-08-16T13:00:55Z", 15.0, 1_000)])
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
    let successor = history
        .samples
        .iter()
        .find(|sample| sample.sampled_at == "2026-08-16T13:02:00Z")
        .unwrap();
    assert_eq!(successor.net_rx_rate_bps, Some(10.0));
    let successor_peak = history
        .peaks
        .iter()
        .find(|sample| sample.sampled_at == "2026-08-16T13:02:00Z")
        .unwrap();
    assert_eq!(successor_peak.net_rx_rate_bps, Some(10.0));
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
