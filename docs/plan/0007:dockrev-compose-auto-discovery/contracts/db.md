# DB（Dockrev Auto-Discovery / #0007）

本文件定义本计划涉及的数据库持久化形状与变更范围（实现阶段以可迁移/可回滚为约束）。

## Goal

持久化“自动发现元数据”（包括**尚未注册为 stack 的项目**），以支持：

- 判定 `project` 是否已注册/是否需要更新 composeFiles
- 标记 `missing/inactive`
- 表达“冲突/失败原因”，供 UI 展示与排障

## Proposed schema（Recommended: new table for discovered projects）

新增表 `discovered_compose_projects`（名字可在实现阶段按仓库风格调整，但需稳定）：

- `project TEXT PRIMARY KEY NOT NULL`
  - `com.docker.compose.project`
- `stack_id TEXT`（nullable）
  - 若已注册为 stack：关联 `stacks.id`
- `status TEXT NOT NULL`
  - `active|missing|invalid`
- `last_seen_at TEXT`（nullable, RFC3339）
- `last_scan_at TEXT`（nullable, RFC3339）
- `last_error TEXT`（nullable, human-readable, capped length）
- `last_config_files_json TEXT`（nullable）
  - 规范化后的 `config_files` 列表（用于变更判定、冲突判断与排障）
- `archived INTEGER NOT NULL DEFAULT 0`
  - 0/1；用于 UI 默认隐藏该项目（归档/收纳）
- `archived_at TEXT`（nullable, RFC3339）
- `archived_reason TEXT`（nullable）
  - `user_archive|auto_archive_on_restart`（或等价枚举）

Indexes（optional）:

- `INDEX(stack_id)`（便于从 stack 反查 discovery 状态）

Backfill:

- 初始为空；首次 discovery 扫描按观察结果写入/更新。

## Data rules（must）

- `config_files` 规范化后再写入 `*_config_files_json`（split/trim/dedupe/sort）。
- “missing” 仅表达“未观察到容器”，不得自动删除 stacks（只更新状态字段）。
- “进程重启后默认隐藏 missing”：
  - 在 app startup 的 migration/初始化阶段，将 `status='missing' AND archived=0` 的记录批量置为 `archived=1`（`archived_reason='auto_archive_on_restart'`）。
  - 若该 project 后续再次被扫描到（`status` 回到 `active`），应自动解除 `archived`（恢复展示）。

## Stack / Service archive

为支持“允许归档 stack 或 service”，本计划建议为既有表增加 additive 字段：

- `stacks`:
  - `archived INTEGER NOT NULL DEFAULT 0`
  - `archived_at TEXT`（nullable, RFC3339）
  - `archived_reason TEXT`（nullable, e.g. `user_archive`）
- `services`:
  - `archived INTEGER NOT NULL DEFAULT 0`
  - `archived_at TEXT`（nullable, RFC3339）
  - `archived_reason TEXT`（nullable）

## Destructive migration（manual stack removal）

本计划实现阶段将移除“手动注册 stack”。为避免旧数据与新机制混杂，需要在 DB migration 时清理历史相关数据。

注意：该清理属于破坏性操作，会导致历史 stack/services/ignore rules/jobs/backups 等数据丢失；实现前必须由主人确认清理范围，并在发布说明中明确告知。

清理范围（已确认原则）：

- 清理 `stacks` 及其“与 stack 绑定的数据”
- 未与 stack 绑定的数据可保留

建议的可执行口径（实现阶段按此落地迁移；若仓库 schema 变更需同步更新）：

- 清理（stack-bound）：
  - `stacks`（整表清空）
  - `services`（由 `stacks` 的 `ON DELETE CASCADE` 自动清理，或显式清空）
  - `ignore_rules`（整表清空；否则会留下对已删除 services 的悬挂引用）
  - `backups`（由 `stacks` 的 `ON DELETE CASCADE` 自动清理，或显式清空）
  - `jobs`/`job_logs`：删除所有 `stack_id IS NOT NULL OR service_id IS NOT NULL` 的 jobs（并由 `job_logs` 的 `ON DELETE CASCADE` 清理日志）
- 保留（not stack-bound）：
  - `settings`、`notification_settings`、`web_push_subscriptions`
