## `services`

Homepage metadata persistence remains:

- `homepage_json TEXT NULL`

Stored shape:

```json
{
  "group": "Developer",
  "name": "Gitea",
  "icon": "si-gitea",
  "href": "https://git.example.com",
  "description": "Git forge"
}
```

Rules:

- Only the five basic Homepage fields are stored.
- `NULL` means the service currently has no Homepage metadata.
- Discovery sync must upsert, update, or clear `homepage_json` so persisted data matches compose truth.

## `service_resource_latest_samples`

Latest-per-service read-model table for homepage and overview summary.

Columns:

- `service_id TEXT PRIMARY KEY NOT NULL REFERENCES services(id) ON DELETE CASCADE`
- `sampled_at TEXT NOT NULL`
- `cpu_percent REAL NOT NULL`
- `mem_used_bytes INTEGER NULL`
- `mem_limit_bytes INTEGER NULL`
- `net_rx_bytes INTEGER NULL`
- `net_tx_bytes INTEGER NULL`
- `block_read_bytes INTEGER NULL`
- `block_write_bytes INTEGER NULL`
- `pids INTEGER NULL`
- `container_count INTEGER NOT NULL DEFAULT 1`
- `prev_sampled_at TEXT NULL`
- `prev_net_rx_bytes INTEGER NULL`
- `prev_net_tx_bytes INTEGER NULL`

Indexes:

- `idx_service_resource_latest_samples_sampled_at(sampled_at)`

Rules:

- The resource sampler must update this table in the same write transaction that appends to `service_resource_samples`.
- Schema migration must backfill this table from each service's latest historical samples so upgraded databases keep existing homepage/overview metrics before the next live sample arrives.
- `prev_*` columns capture the previously latest network counters so requests can compute RX/TX rates without reading the historical table.
- Rows are one-per-service and are deleted automatically when the service row is deleted.
- Historical retention and pruning still apply only to `service_resource_samples`; this table is the small read model optimized for homepage/opening-path queries.

## SQLite runtime PRAGMAs

Application startup must execute:

- `PRAGMA foreign_keys = ON`
- `PRAGMA journal_mode = WAL`
- `PRAGMA busy_timeout = 5000`

Rules:

- These are runtime requirements, not optional production tuning notes.
- Homepage performance validation assumes these PRAGMAs are active before any request handling begins.
