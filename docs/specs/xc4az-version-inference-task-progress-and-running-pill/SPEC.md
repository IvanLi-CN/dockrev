# Dockrev：版本推测任务内进度修复 + running/queued 标签语义区分（#xc4az）

## 状态

- Status: 已完成
- Created: 2026-02-28
- Last: 2026-02-28

## 背景 / 问题陈述

- `/queue/version-inference` 执行中的进度条存在“长期不更新”的体感问题：文案分母使用仓库总条目，且扫描阶段缺少持续进度回写。
- 运行态与排队态标签都使用同一 warn 色，操作员难以快速区分“正在执行”与“尚未执行”。

## 目标 / 非目标

### Goals

- 版本推测执行阶段展示“任务内进度（x/y）”，并保留“仓库总数”作为参考信息。
- 扫描期间持续推送 `task_progress`，保证执行中的任务可观察到递增进度。
- 全站 `Pill` 语义中区分 `running` 与 `queued`：
  - `running` 使用 info 色并带呼吸动效
  - `queued` 维持 warn 色
- 动效遵循 `prefers-reduced-motion`。

### Non-goals

- 不改动版本推测外部 API 路径与响应字段合同。
- 不调整 snapshot worker 并发、TTL、GC 等后端策略。
- 不新增缓存管理写操作入口。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/service_check.rs`
- `crates/dockrev-api/src/snapshot_worker.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/ui.tsx`
- `web/src/App.css`
- `web/src/pages/VersionInferencePage.tsx`
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/JobDetailPage.tsx`
- `web/src/stories/components/Pill.stories.tsx`

### Out of scope

- 非 `Pill` 组件状态体系重构。
- 版本推测页面以外的信息架构调整。

## 接口契约（Interfaces & Contracts）

- 保持以下接口不变：
  - `GET /api/version-inference/overview`
  - `GET /api/version-inference/events`
- 仅增强后端扫描过程中的进度回写频率与文案语义，不引入 breaking change。

## 验收标准（Acceptance Criteria）

- 运行中版本推测任务在执行阶段能够观察到 `assignedCurrent` 递增（不长期停留在 `0/N`）。
- 页面文案体现“任务内进度”，并显示仓库总条目作为参考。
- `running` 与 `queued` 标签颜色区分明确；`running` 提供呼吸效果。
- 在 `prefers-reduced-motion: reduce` 下，呼吸效果关闭。
- `cargo test -p dockrev-api`、`bun run --cwd web lint`、`bun run --cwd web build`、`bun run --cwd web test-storybook` 通过。

## 变更记录

- 2026-02-28: 创建规格并冻结任务内进度口径与 running/queued 标签语义。
- 2026-02-28: 完成后端扫描进度实时回写（任务内进度 + 仓库总数参考文案）、全站 running/queued Pill 语义分离（running=info+呼吸，queued=warn），并通过 `cargo test -p dockrev-api`、`bun run --cwd web lint`、`bun run --cwd web build`、`bun run --cwd web build-storybook`、`bun run --cwd web test-storybook` 验证。
