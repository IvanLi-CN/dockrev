# Dockrev Web: Queue 日志切换导致布局错位（#bgwkw）

## 状态

- Status: 已完成
- Created: 2026-02-06
- Last: 2026-02-06

## 背景 / 问题陈述

- `/queue` 页面用于查看任务列表与执行结果。
- 现状（现网）：当日志与任务列表放在同一页（宽屏双栏）时，日志里可能包含长 URL / digest 等 token，容易导致布局挤压/错位（右侧最小内容宽度变大，外层 grid 重新分配宽度，左侧任务列表被挤窄）。
- 反馈：仅通过“让双栏更稳”的 CSS 修补不合理；日志应作为“任务详情”的一部分，放在单独页面中展示。

## 目标 / 非目标

### Goals

- Queue 列表页（`/queue`）仅展示任务列表与状态信息；日志不在列表页渲染。
- 新增任务详情页（Job detail）展示 job 元信息 + 日志：
  - 支持通过 URL 直达（deep link）。
  - 长 URL/digest 等内容可断行或在容器内滚动，但不应造成页面整体横向溢出。
- Storybook 中可复现并验证：从列表进入详情页后，长日志展示正常，且列表页布局不再受日志长度影响。

### Non-goals

- 不调整后端日志内容、格式或接口。
- 不引入复杂的日志高亮/解析/筛选功能。
- 不做 Jobs 的分页/虚拟列表/增量加载等性能优化（除非后续另立计划）。

## 范围（Scope）

### In scope

- 路由：
  - 新增任务详情路由：`/queue/<jobId>`。
  - `Route` 增加 `job` 类型（或等价路由名），并确保 sidebar 仍高亮“更新队列”。
- 页面：
  - Queue 列表页：点击任务进入任务详情页。
  - 任务详情页：展示 job 元信息 + 日志列表（复用现有 `.logs/.logLine` 样式）。
- 样式：
  - 对日志行 CSS grid 做约束，确保长 token 不会撑破容器（例如 `.logLine` 使用 `minmax(0, 1fr)`；`.logMsg` 允许断行）。
- Storybook：
  - mock scenario：提供包含“长 URL/digest 日志”的 job。
  - 流程型 story：推荐用 `Pages/InteractiveApp`（从 `/queue` 点进详情页）以验证真实导航行为。
- 自动化回归：
  - 更新 `test-storybook`：验证从 `/queue` 点击进入详情页后，长日志可见，且无脚本错误。

### Out of scope

- 后端 jobs API 变更（字段/接口/排序等）。
- 复杂的日志聚合、筛选、下载等。
- 响应式布局策略调整（移动端本来会单列）。

## 需求（Requirements）

### MUST

- Queue 列表页不渲染日志；点击任务进入任务详情页。
- 任务详情页展示完整日志；长 URL/digest 等内容必须可读（允许断行或容器内滚动），且不应造成页面整体横向溢出。
- Storybook 必须能稳定复现并验证该流程。

### SHOULD

- 任务详情页提供“返回列表”入口。
- 任务详情页支持刷新（复用现有 `getJob` / `listJobs` 刷新逻辑）。

## 验收标准（Acceptance Criteria）

- Given 打开 Storybook 的流程型 story（例如 `Pages/InteractiveApp/QueueLongLogs`）
  When 点击任务列表中的 job-long（包含长 URL/digest 日志）
  Then 进入任务详情页并展示日志；长 token 不导致页面整体横向溢出
- Given 打开 `https://dockrev.ivanli.cc/queue`
  When 点击任意任务进入详情页
  Then 日志在任务详情页展示；列表页布局不再受日志长度影响

## 里程碑（Milestones）

- [x] 路由：新增任务详情路由 + App 渲染入口
- [x] 页面：Queue 列表页改为“点击进入详情页”
- [x] 页面：任务详情页展示 job 元信息 + 日志
- [x] Storybook：mock scenario + 流程型 story（从列表进入详情页）
- [x] test-storybook：覆盖“进入详情页后长日志可见”的回归

## 风险与开放问题（Risks & Open Questions）

- 日志断行策略（如 `overflow-wrap: anywhere`）会在长 token 中间断行，可能影响阅读的“视觉完整性”；但相比布局错位更可接受，且复制内容不受影响。
- 任务详情页 URL 结构（`/queue/<jobId>`）一旦发布应尽量稳定，避免外部链接失效。

## 变更记录（Change log）

- 2026-02-06：根据反馈调整方案：从“在 `/queue` 双栏内修补布局稳定性”改为“日志迁移到任务详情页展示”，列表页仅展示任务列表与状态。
- 2026-02-06：实现：新增 Job detail 页面承载日志展示；日志区域优化为等宽字体 + 水平滚动以保留长 token 可读/可复制（PR #58）。
