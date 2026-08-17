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

#[path = "metrics_store_migration_tests.rs"]
mod migration;
#[path = "metrics_store_recovery_tests.rs"]
mod recovery;
#[path = "metrics_store_rollup_tests.rs"]
mod rollup;
