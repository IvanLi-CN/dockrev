## New table: `image_version_inference_snapshots`

主键：`(image_repo, host_platform)`

字段：

- `image_repo TEXT NOT NULL`
- `host_platform TEXT NOT NULL`
- `snapshot_json TEXT NOT NULL`
- `all_failed INTEGER NOT NULL DEFAULT 0`
- `checked_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

约束：

- `PRIMARY KEY (image_repo, host_platform)`

用途：

- 镜像级缓存 `digest -> inferred semver tags` 及采集摘要。
- `all_failed=1` 表示上一轮所有推测方案均失败；读取时允许再次触发采集。
