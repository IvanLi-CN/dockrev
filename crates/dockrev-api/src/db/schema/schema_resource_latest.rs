pub(super) const CREATE_SERVICE_RESOURCE_LATEST_SAMPLES_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS service_resource_latest_samples (
  service_id TEXT PRIMARY KEY NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  sampled_at TEXT NOT NULL,
  cpu_percent REAL NOT NULL,
  mem_used_bytes INTEGER,
  mem_limit_bytes INTEGER,
  net_rx_bytes INTEGER,
  net_tx_bytes INTEGER,
  block_read_bytes INTEGER,
  block_write_bytes INTEGER,
  pids INTEGER,
  container_count INTEGER NOT NULL DEFAULT 1,
  prev_sampled_at TEXT,
  prev_net_rx_bytes INTEGER,
  prev_net_tx_bytes INTEGER
);
CREATE INDEX IF NOT EXISTS idx_service_resource_latest_samples_sampled_at
  ON service_resource_latest_samples(sampled_at);
"#;

pub(super) const BACKFILL_SERVICE_RESOURCE_LATEST_SAMPLES_SQL: &str = r#"
INSERT INTO service_resource_latest_samples (
  service_id,
  sampled_at,
  cpu_percent,
  mem_used_bytes,
  mem_limit_bytes,
  net_rx_bytes,
  net_tx_bytes,
  block_read_bytes,
  block_write_bytes,
  pids,
  container_count,
  prev_sampled_at,
  prev_net_rx_bytes,
  prev_net_tx_bytes
)
SELECT
  latest.service_id,
  latest.sampled_at,
  latest.cpu_percent,
  latest.mem_used_bytes,
  latest.mem_limit_bytes,
  latest.net_rx_bytes,
  latest.net_tx_bytes,
  latest.block_read_bytes,
  latest.block_write_bytes,
  latest.pids,
  latest.container_count,
  previous.sampled_at,
  previous.net_rx_bytes,
  previous.net_tx_bytes
FROM service_resource_samples latest
LEFT JOIN service_resource_samples previous
  ON previous.id = (
    SELECT prev.id
    FROM service_resource_samples prev
    WHERE prev.service_id = latest.service_id
      AND (
        prev.sampled_at < latest.sampled_at
        OR (prev.sampled_at = latest.sampled_at AND prev.id < latest.id)
      )
    ORDER BY prev.sampled_at DESC, prev.id DESC
    LIMIT 1
  )
WHERE latest.id = (
  SELECT current.id
  FROM service_resource_samples current
  WHERE current.service_id = latest.service_id
  ORDER BY current.sampled_at DESC, current.id DESC
  LIMIT 1
)
ON CONFLICT(service_id) DO NOTHING;
"#;
