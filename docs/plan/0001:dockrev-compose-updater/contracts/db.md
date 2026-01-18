# 数据库（DB）

目标：能支撑“注册 stack / 发现服务 / 计算候选版本 / 记录任务与审计 / 保存通知与订阅”。

SQLite 作为单机默认存储；后续如需迁移到 Postgres，应通过清晰的 schema 与迁移策略实现。

## Initial schema（MVP）

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: `stacks`, `services`, `images`, `candidates`, `ignore_rules`, `jobs`, `job_logs`, `notification_settings`, `web_push_subscriptions`
  - 追加：`backups`, `settings`

### Schema delta（结构变更）

（DDL 仅为契约形状，具体实现可用迁移工具生成；字段可增量增加但需保持兼容。）

```sql
CREATE TABLE stacks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  compose_type TEXT NOT NULL, -- 'path'
  compose_files_json TEXT NOT NULL,
  env_file TEXT NULL,
  backup_json TEXT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE services (
  id TEXT PRIMARY KEY,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  image_ref TEXT NOT NULL,    -- e.g. ghcr.io/org/app:5.2
  image_tag TEXT NOT NULL,
  image_digest TEXT NULL,     -- sha256:...
  settings_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- services.settings_json shape (contract)
-- {
--   "autoRollback": true,
--   "backupTargets": {
--     "mode": "allowlist|denylist",
--     "bindPaths": ["/abs/host/path"],
--     "volumeNames": ["volume_name"]
--   }
-- }

CREATE TABLE candidates (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  digest TEXT NOT NULL,
  arch_match TEXT NOT NULL,   -- match|mismatch|unknown
  arch_json TEXT NOT NULL,
  ignored_rule_id TEXT NULL,
  discovered_at TEXT NOT NULL
);

CREATE TABLE ignore_rules (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  service_id TEXT NOT NULL REFERENCES services(id) ON DELETE CASCADE,
  match_kind TEXT NOT NULL,   -- exact|prefix|regex|semver
  match_value TEXT NOT NULL,
  note TEXT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,         -- check|update|rollback
  scope TEXT NOT NULL,        -- service|stack|all
  stack_id TEXT NULL,
  service_id TEXT NULL,
  status TEXT NOT NULL,       -- queued|running|success|failed|rolled_back
  actor TEXT NOT NULL,        -- forward-user|webhook|system
  reason TEXT NOT NULL,       -- ui|webhook|schedule
  created_at TEXT NOT NULL,
  started_at TEXT NULL,
  finished_at TEXT NULL,
  summary_json TEXT NULL
);

CREATE TABLE job_logs (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  ts TEXT NOT NULL,
  level TEXT NOT NULL,
  msg TEXT NOT NULL
);

CREATE TABLE notification_settings (
  id TEXT PRIMARY KEY,
  updated_at TEXT NOT NULL,
  email_json TEXT NOT NULL,
  webhook_json TEXT NOT NULL,
  telegram_json TEXT NOT NULL,
  web_push_json TEXT NOT NULL
);

CREATE TABLE settings (
  id TEXT PRIMARY KEY,
  updated_at TEXT NOT NULL,
  backup_json TEXT NOT NULL
);

CREATE TABLE web_push_subscriptions (
  id TEXT PRIMARY KEY,
  endpoint TEXT NOT NULL UNIQUE,
  keys_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE backups (
  id TEXT PRIMARY KEY,
  stack_id TEXT NOT NULL REFERENCES stacks(id) ON DELETE CASCADE,
  status TEXT NOT NULL, -- queued|running|success|failed
  created_at TEXT NOT NULL,
  started_at TEXT NULL,
  finished_at TEXT NULL,
  artifact_path TEXT NULL,
  size_bytes INTEGER NULL,
  error TEXT NULL
);

CREATE INDEX idx_services_stack_id ON services(stack_id);
CREATE INDEX idx_candidates_service_id ON candidates(service_id);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_backups_stack_id ON backups(stack_id);
```

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）: MVP 阶段只允许新增字段与新增表；不得删除/重命名列。
- 发布/上线步骤（Rollout steps）:
  - 启动时自动执行 schema 创建/升级（单机）
- 回滚策略（Rollback strategy）:
  - 仅在同一版本间回滚；跨版本回滚需保证 migrations 可逆（后续实现时评估）
- 回填/数据迁移（Backfill / data migration, 如适用）:
  - 暂无（MVP 从 0 初始化）
