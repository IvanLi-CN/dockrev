# Dockrev：全站任务展示优先人可读名称（#gh58m）

## 状态

- Status: 已完成
- Created: 2026-03-02
- Last: 2026-03-02

## 背景 / 问题陈述

- 任务相关页面当前直接展示 `job.type` 和 `job.scope` 的机器标识（例如 `runtime_scan.all`、`update.service`），可读性不足。
- 一线操作员需要先在脑内做映射才能理解任务语义，影响扫描与排障效率。
- 同时完全隐藏机器名会降低追溯性，不利于日志和后端字段对齐。

## 目标 / 非目标

### Goals

- 在前端统一提供任务展示映射：优先显示中文人可读任务名。
- 未知 `type/scope` 值保持兼容回退（展示原始值，空值展示 `-`）。
- 在关键页面保留机器名辅助排障。

### Non-goals

- 不修改 Rust API 响应结构与字段。
- 不改任务执行语义、队列筛选、SSE 订阅逻辑。

## 范围（Scope）

### In scope

- `web/src/jobDisplay.ts`（新增）
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/JobDetailPage.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/tests/jobDisplay.test.ts`（新增）
- `docs/specs/README.md`

### Out of scope

- `crates/**` 后端接口与数据库。
- 非任务命名相关的 UI 视觉重构。

## 接口契约（Interfaces & Contracts）

- Backend API: None（无接口变更）。
- Frontend display helpers（新增）：
  - `formatJobTypeLabel(type: string): string`
  - `formatJobScopeLabel(scope: string): string`
  - `formatJobReadableName(type: string, scope: string): string`
  - `formatJobMachineName(type: string, scope: string): string`

## 验收标准（Acceptance Criteria）

- Given 队列页存在 `update.service` / `runtime_scan.all` / `check.all` 等任务，When 渲染列表项，Then 标题显示人可读任务名，且页面仍可看到机器名。
- Given 打开任务详情页，When 查看任务基础信息，Then 人可读名优先展示，并包含机器名信息用于排障。
- Given 概览页有最近任务，When 渲染最近任务摘要，Then 展示人可读任务名而非单独裸 `scope`。
- Given 任务类型或范围出现未知值，When 渲染，Then 不报错，回退到原值；若为空则展示 `-`。
- `bun test`（包含 `web/tests/jobDisplay.test.ts`）通过。
- `bun run --cwd web lint` 与 `bun run --cwd web build` 通过。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增统一任务命名映射工具并补齐单测。
- [x] M2: Queue/JobDetail/Overview 三个页面切换为人可读优先展示，保留机器名信息。
- [x] M3: 完成 lint/build/test 验证并同步 specs 索引状态。

## 变更记录（Change log）

- 2026-03-02: 创建规格，冻结“前端映射 + 全站任务展示 + 机器名保留”的实现边界。
- 2026-03-02: 完成 `web/src/jobDisplay.ts` 映射实现；更新 Queue/JobDetail/Overview 展示；新增 `web/tests/jobDisplay.test.ts` 并通过 `bun test --cwd web`、`bun run --cwd web lint`、`bun run --cwd web build`。
