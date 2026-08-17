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

指标库与 `DOCKREV_DB_PATH` 的主库文件分离，默认路径为主库同目录的 `metrics.sqlite3`。`DOCKREV_METRICS_DB_PATH` 不能和主库解析为同一文件，包括主库创建前的符号链接别名。

主库只保存 `metrics_store_migration` 状态。启动从旧主库指标表首次复制时，目标写入必须幂等；manifest 同时保存 legacy raw 与 legacy latest 的稳定排序行哈希、行数和 raw 最大 id，任何源指纹变化都必须重新校验，只有 raw 哈希/行数覆盖验证通过才标记 complete。导入的 legacy raw 必须保存稳定内容签名，GC 删除 legacy raw 或孤儿服务数据前必须保存其 legacy id 墓碑。重启验证完整源哈希、保留 raw 的签名以及“保留 raw + 墓碑”对旧表总行数的覆盖关系；验证后的协调只能重算现有 raw 所覆盖的 latest/rollup 桶。latest 行以 `legacy_source` 区分导入投影和运行时采样：恢复先重建导入投影，再逐服务验证其与当前 legacy latest 一致；更新鲜的运行时样本可以保留，但陈旧、缺失或时间回退的导入值必须被源投影修复。旧 latest 表可恢复主库当前 active service 的 latest，即使相应 raw 已过期；非 active service 的 legacy latest 不得回灌。该流程不能删除超过 raw 留存的 active latest 或长窗口桶。完整源改变时才清除墓碑重拷，目标修复不得复活被 GC 清理的旧行。校验失败时进程不得启动新采样路径，旧表不可删除或修改。

指标库拥有以下表：

- `service_resource_samples`：5 秒原始样本，保留 24 小时。
- `service_resource_latest_samples`：每服务最新读模型。
- `service_resource_rollups`：1 分钟桶保留 7 天，5 分钟桶保留 30 天；保存 CPU、内存、PIDs、容器数与速率的均值/峰值，以及累计计数首末值。
- `metrics_migration_pruned_legacy_ids`：已从指标库 GC 的 legacy raw id 墓碑，用于可恢复迁移时保留留存裁剪结果。

`OperationalReadModel` 的 compact jobs 查询使用 SQLite JSON 函数投影进度、错误、目标版本和必要的转移字段；请求路径不得选取或在 Rust 中反序列化完整 `jobs.summary_json`。

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
- `legacy_source INTEGER NOT NULL`：`1` 表示当前行由旧主库迁移投影写入；运行时采样写入 `0`；从无来源列的旧 metrics 文件升级时为 `2`（未知来源）。

Indexes:

- `idx_service_resource_latest_samples_sampled_at(sampled_at)`

Rules:

- The resource sampler must update this table in the same metrics-store transaction that appends to `service_resource_samples`.
- Schema migration must backfill this table from each service's latest historical samples so upgraded databases keep existing homepage/overview metrics before the next live sample arrives.
- `prev_*` columns capture the previously latest network counters so requests can compute RX/TX rates without reading the historical table.
- Migration reconciliation replaces only rows marked `legacy_source=1`; a newer `legacy_source=0` row and an unknown pre-provenance `legacy_source=2` row are retained. A raw or source projection may replace an unknown row only when it is at least as recent.
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
