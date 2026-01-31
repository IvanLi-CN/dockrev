# 数据库（DB）

## GitHub Packages webhook settings & targets

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: `github_packages_settings`, `github_packages_targets`, `github_packages_repos`, `github_packages_deliveries`

### Schema delta（结构变更）

新增（示意；具体 DDL 以实现为准）：

- `github_packages_settings`
  - `id TEXT PRIMARY KEY`（固定为 `default`）
  - `enabled INTEGER NOT NULL`
  - `callback_url TEXT NOT NULL`
  - `pat TEXT`（secret；GET 不回传明文）
  - `webhook_secret TEXT NOT NULL`（用于签名校验；GET 不回传明文）
  - `updated_at TEXT`
- `github_packages_targets`
  - `id TEXT PRIMARY KEY`（uuid 或 nanoid）
  - `input TEXT NOT NULL`（原始输入：repo URL / profile URL / username）
  - `kind TEXT NOT NULL`（`repo` \| `owner`）
  - `owner TEXT NOT NULL`
  - `warnings_json TEXT NOT NULL`（JSON array）
  - `updated_at TEXT`
- `github_packages_repos`
  - `owner TEXT NOT NULL`
  - `repo TEXT NOT NULL`
  - `selected INTEGER NOT NULL`
  - `hook_id INTEGER`（GitHub webhook id；用于幂等复用）
  - `last_sync_at TEXT`
  - `last_error TEXT`
  - `updated_at TEXT`
  - `PRIMARY KEY (owner, repo)`
- `github_packages_deliveries`
  - `delivery_id TEXT PRIMARY KEY`（`X-GitHub-Delivery`）
  - `received_at TEXT NOT NULL`
  - `owner TEXT`
  - `repo TEXT`

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）:
  - 新表为增量添加，不影响现有功能。
- 发布/上线步骤（Rollout steps）:
  - 应用启动时确保表存在；设置页按“未配置”展示。
- 回滚策略（Rollback strategy）:
  - 仅新增表；回滚到旧版本后表残留不影响运行（不读取）。
- 回填/数据迁移（Backfill / data migration）:
  - None（首次保存时写入）。
