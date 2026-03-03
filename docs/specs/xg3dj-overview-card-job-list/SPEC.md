# Dockrev：概览卡片任务列表增强（运行中/排队展示 + 补齐 + 直达详情）（#xg3dj）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 概览页“运行态与结果”卡片当前只展示统计与最近一条任务，无法快速查看当前排队与进行中的任务清单。
- 一线运维需要在概览页直接判断当前执行压力，并快速跳转到具体任务详情定位问题。
- 现有 `/queue` 页能力完整，但在概览路径下多一步跳转，影响扫描效率。

## 目标 / 非目标

### Goals

- 在概览页“运行态与结果”卡片新增可点击任务列表。
- 列表项采用单行紧凑列表（非卡片样式），避免占用过高垂直空间。
- 列表规则固定为：优先 `queued/running`，不足时按创建时间倒序由其他状态补齐。
- 总展示数量上限固定 10 条。
- 点击任意任务条目可直接跳转到任务详情页（`/queue/<jobId>`）。

### Non-goals

- 不修改后端 API、数据库查询和任务执行语义。
- 不调整队列页 `/queue` 的筛选交互与页面布局。
- 不修改任务详情页字段结构。

## 范围（Scope）

### In scope

- `web/src/pages/overviewJobsCard.ts`（新增：概览卡片任务选择器）
- `web/src/pages/OverviewPage.tsx`
- `web/src/App.css`
- `web/tests/overviewJobsCard.test.ts`（新增）
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/pages/OverviewPage.stories.tsx`
- `docs/specs/README.md`

### Out of scope

- `crates/**` 后端代码。
- 任务状态定义、SSE 推送协议、任务详情字段。

## 接口契约（Interfaces & Contracts）

- Backend API: None（继续使用 `GET /api/jobs`）。
- Route: None（继续使用 `navigate({ name: 'job', jobId })`）。
- Frontend helper（新增）：
  - `selectOverviewJobsForCard(jobs, { maxItems })`
  - `toOverviewJobCardItem(job)`

## 验收标准（Acceptance Criteria）

- Given jobs 中 `queued/running` 数量 >= 10，When 打开概览页，Then 卡片仅显示最新 10 条 `queued/running` 任务。
- Given jobs 中 `queued/running` 数量 < 10 且存在其他状态，When 打开概览页，Then 先显示全部 `queued/running`，再由其他状态补齐到 10 条。
- Given jobs 总数 < 10，When 打开概览页，Then 显示全部任务且不卡死。
- Given 同 `createdAt` 的任务，When 渲染卡片列表，Then 使用 `id` 倒序作为稳定次级排序。
- Given 点击卡片中的任务项，When 触发跳转，Then 进入 `/queue/<jobId>` 并可查看任务详情。
- Given 任务列表渲染，When 查看任一条目，Then 条目以单行显示并在内容过长时省略。
- 空态显示“暂无任务”。
- `bun test --cwd web`、`bun run --cwd web lint`、`bun run --cwd web build`、`DOCKREV_TEST_STORYBOOK_PORT=50999 bun run --cwd web test-storybook` 通过。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 提取概览卡片任务选择器（优先级 + 补齐 + 稳定排序）。
- [x] M2: Overview 卡片渲染可点击任务列表并接入跳转。
- [x] M3: 补齐单测与 Storybook 场景并完成验证。

## 变更记录（Change log）

- 2026-03-03: 创建规格，冻结“固定 10 条优先 + in-flight 优先 + 详情直达”的实现边界。
- 2026-03-03: 完成 `web/src/pages/overviewJobsCard.ts` 选择器与 `OverviewPage` 接入；新增概览卡片任务列表样式与状态标记，点击任务可跳转详情页。
- 2026-03-03: 新增 `web/tests/overviewJobsCard.test.ts`，覆盖 in-flight 优先、补齐、上限、空数据、同时间戳稳定排序；新增 Storybook 概览卡片场景（in-flight 堆积 / mixed fallback / empty）。
- 2026-03-03: 验证通过 `bun test --cwd web`、`bun run --cwd web lint`、`bun run --cwd web build`、`DOCKREV_TEST_STORYBOOK_PORT=50999 bun run --cwd web test-storybook`；review-loop 无阻断项。
- 2026-03-03: 根据评审反馈将任务行改为单行列表样式（移除条目卡片化），保留状态色标、点击跳转与排序/补齐逻辑。
- 2026-03-03: 优化单行列表排版：减少胶囊数量，改为“状态 + 任务标题 + 紧凑元信息”三段式，并对长文本做单行截断，提升扫描效率与整洁度。
- 2026-03-03: 补充视觉证据截图（Storybook iframe）：`docs/specs/xg3dj-overview-card-job-list/assets/overview-jobs-list-compact.png`。
