# Dockrev：概览卡片运行任务整行背景进度条（慢速流光）（#b5tcx）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 概览页“运行态与结果”卡片中的任务列表目前仅展示状态标签与元信息，运行中任务的进度感知弱。
- 运维场景希望在不打扰阅读的前提下，快速扫到“哪些任务正在推进、推进到哪里”。
- 现有队列页/详情页已具备进度条，不需要扩展范围；仅需在概览卡片提供轻量感知。

## 目标 / 非目标

### Goals

- 仅在概览页任务列表中，为 `running` 任务增加“横向整行背景进度”。
- 视觉保持低存在感：不抢占文本层级，不破坏现有单行扫描节奏。
- 动画采用慢速流光（悠哉风格），并支持 reduced motion 降级。
- 继续保持任务排序、补齐、点击跳转行为不变。

### Non-goals

- 不修改 `/queue` 与任务详情页。
- 不修改后端 API、SSE 协议、数据库结构。
- 不新增“伪进度”推算逻辑。

## 范围（Scope）

### In scope

- `web/src/pages/overviewJobsCard.ts`：扩展 `OverviewJobCardItem` 并封装进度视觉判定。
- `web/src/pages/OverviewPage.tsx`：给任务行挂载背景进度层与样式变量。
- `web/src/App.css`：新增概览行背景进度样式、慢速流光动画、`prefers-reduced-motion` 兜底。
- `web/tests/overviewJobsCard.test.ts`：补充进度模式映射单测。
- `web/src/stories/mocks/dockrevMockApi.ts`、`web/src/stories/pages/OverviewPage.stories.tsx`：新增回归场景。
- `docs/specs/README.md`：登记规格索引。

### Out of scope

- `crates/**` 后端实现。
- 队列页 `queue` 与详情页 `job` 的进度样式。

## 接口契约（Interfaces & Contracts）

### Backend

- None（继续使用 `GET /api/jobs`）。

### Frontend internal

- `OverviewJobCardItem` 新增字段：
  - `progressMode: 'none' | 'determinate' | 'indeterminate'`
  - `progressPercent: number | null`
- 新增 helper：`getOverviewJobProgressVisual(job)`（前端内部逻辑）。

## 进度判定规则（冻结）

- `status !== 'running'` → `progressMode='none'`。
- `status === 'running'` 且满足：`total > 0` 且 `percent` 可用，且非“`percent=0` 且 `current<total`” → `determinate`。
- 其余 running（无 progress / unknown total / 0% 未推进）→ `indeterminate`。
- `percent` 统一 clamp 到 `0..100`。

## 验收标准（Acceptance Criteria）

- Given `running + total>0 + percent=40`，When 渲染概览任务行，Then 背景条为 determinate 且宽度对应 40%。
- Given `running + 无 progress`，When 渲染概览任务行，Then 背景条为 indeterminate 弱流动底纹。
- Given `running + percent=0 + current<total`，When 渲染概览任务行，Then 背景条为 indeterminate（不显示假 0%）。
- Given `success/failed/queued`，When 渲染概览任务行，Then 不显示背景进度条。
- Given percent 越界，When 映射视觉数据，Then percent 被 clamp 到 `0..100`。
- Given 用户点击任务行，When 跳转，Then 仍进入 `/queue/<jobId>`。
- Given `prefers-reduced-motion: reduce`，When 页面渲染，Then 关闭流光动画，仅保留静态底色/填充。

## 非功能性要求

- 保持现有行高与单行省略，不引入布局抖动。
- hover / active 不可压过文本可读性。

## 验证命令

- `bun test --cwd web`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `DOCKREV_TEST_STORYBOOK_PORT=50999 bun run --cwd web test-storybook`

## 风险 / 假设

- 风险：背景层与 hover 层叠加可能导致局部对比不足；通过低透明度与文本层 z-index 规避。
- 假设：概览页只做“轻感知”，不需要展示双层 planned/completed 细分。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 类型与判定逻辑落地（`overviewJobsCard.ts`）。
- [x] M2: 概览任务行接入背景进度层（`OverviewPage.tsx`）。
- [x] M3: 慢速流光样式 + reduced-motion 兜底（`App.css`）。
- [x] M4: 单测 + Storybook 场景补齐。
- [x] M5: lint/test/build/storybook 验证通过。

## 变更记录（Change log）

- 2026-03-03: 创建规格，冻结“仅概览卡片 running 行背景进度 + 慢速流光 + 低存在感”的边界。
- 2026-03-03: 完成 `OverviewPage` 背景进度层接线与标题补充（含 `determinate/indeterminate` 数据驱动）。
- 2026-03-03: 完成 `App.css` 慢速流光视觉与 `prefers-reduced-motion` 兜底。
- 2026-03-03: 补充 `overviewJobsCard` 进度映射逻辑与单测；新增 Storybook 概览场景回归。
- 2026-03-03: 验证通过 `bun test --cwd web`、`bun run --cwd web lint`、`bun run --cwd web build`、`DOCKREV_TEST_STORYBOOK_PORT=50999 bun run --cwd web test-storybook`。
