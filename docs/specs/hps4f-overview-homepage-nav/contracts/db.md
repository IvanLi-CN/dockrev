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

## `metrics.sqlite3`

指标库与 `DOCKREV_DB_PATH` 的主库文件分离，默认路径为主库同目录的 `metrics.sqlite3`。`DOCKREV_METRICS_DB_PATH` 不能和主库解析为同一文件。

主库只保存 `metrics_store_migration` 状态。启动从旧主库指标表首次复制时，目标写入必须幂等；只有稳定排序的行哈希和行数都一致才标记 complete。导入的 legacy raw 必须保存稳定内容签名，GC 删除 legacy raw 或孤儿服务数据前必须保存其 legacy id 墓碑。重启验证完整源哈希、保留 raw 的签名以及“保留 raw + 墓碑”对旧表总行数的覆盖关系；验证后的协调只能重算现有 raw 所覆盖的 latest/rollup 桶，不能删除超过 raw 留存的 latest 或长窗口桶。完整源改变时才清除墓碑重拷，目标修复不得复活被 GC 清理的旧行。校验失败时进程不得启动新采样路径，旧表不可删除或修改。

指标库拥有以下表：

- `service_resource_samples`：5 秒原始样本，保留 24 小时。
- `service_resource_latest_samples`：每服务最新读模型。
- `service_resource_rollups`：1 分钟桶保留 7 天，5 分钟桶保留 30 天；保存 CPU、内存、PIDs、容器数与速率的均值/峰值，以及累计计数首末值。
- `metrics_migration_pruned_legacy_ids`：已从指标库 GC 的 legacy raw id 墓碑，用于可恢复迁移时保留留存裁剪结果。

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

- The resource sampler must update this table in the same metrics-store transaction that appends to `service_resource_samples`.
- Schema migration must backfill this table from each service's latest historical samples so upgraded databases keep existing homepage/overview metrics before the next live sample arrives.
- `prev_*` columns capture the previously latest network counters so requests can compute RX/TX rates without reading the historical table.
- Rows are one-per-service. 指标 GC 根据主库活动 service id 清理孤儿行。
- Historical retention and pruning still apply only to `service_resource_samples`; this table is the small read model optimized for homepage/opening-path queries.

## SQLite runtime PRAGMAs

Application startup must execute:

- `PRAGMA foreign_keys = ON`
- `PRAGMA journal_mode = WAL`
- `PRAGMA busy_timeout = 5000`

Rules:

- These are runtime requirements, not optional production tuning notes.
- Homepage performance validation assumes these PRAGMAs are active before any request handling begins.
