# Dockrev：进度条全局平滑（420ms）（#n2pw5）

## 状态

- Status: 已完成
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

- 当前任务队列中的进度条在数值跳变时立即更新，视觉观感偏生硬。
- Job 详情与版本推测页面使用同类双层进度条，若不统一平滑策略会出现页面间手感不一致。
- 需要在不改动后端进度语义的前提下，提升进度变化的连续性与可读性。

## 目标 / 非目标

### Goals

- 为 Queue / JobDetail / VersionInference 三处进度填充层统一加入 `width` 平滑过渡。
- 固定平滑参数为 `420ms` + `cubic-bezier(0.4, 0, 0.2, 1)`。
- indeterminate（跑马）状态保持现有效果，并显式关闭 transition 冲突。
- 在 `prefers-reduced-motion: reduce` 下关闭本次新增 transition。

### Non-goals

- 不修改 API、SSE、TypeScript 类型、后端进度计算逻辑。
- 不改动进度条颜色、层级、布局与 aria 文案。
- 不引入新的动画库或 JS 动画实现。

## 范围（Scope）

### In scope

- `web/src/App.css`
- `docs/specs/README.md`
- `docs/specs/n2pw5-progress-bar-smoothing/SPEC.md`

### Out of scope

- `web/src/pages/QueuePage.tsx`
- `web/src/pages/JobDetailPage.tsx`
- `web/src/pages/VersionInferencePage.tsx`
- 所有 Rust 后端代码

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UI progress fill transition | css-style | internal | Modify | None | web | queue/job-detail/version-inference pages | 仅样式行为调整 |

### 契约文档（按 Kind 拆分）

- None

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- determinate 进度更新时，fill 宽度按统一 easing 平滑过渡。
- 双层进度条中 planned/completed 两层均应用同一过渡时长。
- VersionInference 的 assignment/result 两层进度与 Queue/JobDetail 保持一致手感。

### Edge cases / errors

- indeterminate 状态下关闭 transition，避免与 `progressIndeterminate` 动画叠加导致抖动。
- 在 reduced motion 偏好下，禁用新增 transition，保持无过渡更新。

## 验收标准（Acceptance Criteria）

- Given Queue running 任务进度从 `40% -> 60% -> 80%`，When UI 收到新值，Then 进度宽度连续过渡而非瞬时跳变。
- Given JobDetail 双层进度持续更新，When 百分比变化，Then 两层 fill 都平滑变化且层级关系不变。
- Given VersionInference assignment/result 百分比变化，When 数据刷新，Then 两条 fill 使用相同平滑节奏。
- Given 任一进度条进入 indeterminate，When 动画运行，Then 保持现有跑马效果且无 transition 冲突。
- Given 系统偏好 `prefers-reduced-motion: reduce`，When 进度变化，Then 不应用本次新增 transition。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web test-storybook`
- Playwright 手动走查 Queue / JobDetail / VersionInference

### Quality checks

- 不新增 ESLint / TypeScript 警告或错误。
- Storybook 自动化检查通过。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建 spec 并冻结验收口径。
- [x] M2: App.css 三类进度 fill 统一 420ms 平滑过渡。
- [x] M3: indeterminate 与 reduced-motion 规则落地。
- [x] M4: lint/build/test-storybook/Playwright 验证通过。
- [x] M5: 快车道交付完成（push + PR + checks + review-loop 收敛）。

## 风险 / 假设

- 风险：过渡时长设置过大可能造成“延迟感”。
- 风险：不同页面数据更新频率不同，体感仍可能存在细微差异。
- 假设：当前仓库进度条实现点仅为 Queue / JobDetail / VersionInference。

## 变更记录（Change log）

- 2026-02-26: 创建规格并冻结范围、验收标准与质量门槛。
- 2026-02-26: 完成 `web/src/App.css` 进度条平滑改动（三类 fill 统一 220ms easing，indeterminate 显式禁用 transition，reduced-motion 下关闭新增 transition）。
- 2026-02-26: 本地验证通过（`bun run --cwd web lint`、`bun run --cwd web build`、`bun run --cwd web build-storybook`、`bun run --cwd web test-storybook`）并完成 Playwright 三页走查。
- 2026-02-26: PR #94 在 CI 首轮 `Frontend (lint + Storybook)` 卡住后取消并重跑，`CI (PR)` attempt 2 与 `PR Label Gate` 均通过；review-loop 第 1 轮无 P0/P1/P2 阻塞。
- 2026-02-26: 按反馈将平滑参数调优为 `420ms` + `cubic-bezier(0.4, 0, 0.2, 1)`，并新增 Storybook `ProgressSmoothing` 自动进度演示场景。
