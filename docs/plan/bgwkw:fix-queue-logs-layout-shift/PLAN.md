# Dockrev Web: Queue 日志切换导致布局错位（#bgwkw）

## 状态

- Status: 待实现
- Created: 2026-02-06
- Last: 2026-02-06

## 背景 / 问题陈述

- `/queue` 页面采用双栏布局：左侧任务列表，右侧日志面板。
- 现状：当切换到包含很长 URL / digest 的日志时，右侧日志的最小内容宽度（min-content width）变大，导致整体双栏 grid 重新分配宽度：右栏变宽、左栏被挤窄，出现明显“错位/布局不正确”（chips 换行成竖排、卡片宽度异常）。

## 目标 / 非目标

### Goals

- 切换不同 job（短日志 ↔ 长日志）时，双栏布局稳定，不因日志内容长度压缩左侧任务列表。
- 长日志在右侧面板内自行处理：
  - 优先可读（允许断行）；
  - 必要时可横向滚动，但不影响页面外层 grid。
- 在 Storybook 中可复现（提供 mock scenario + story），便于回归验证。

### Non-goals

- 不调整后端日志内容、格式或接口。
- 不重做 Queue 页面整体 UI（仅修复布局稳定性问题）。
- 不引入复杂的日志高亮/解析/筛选功能。

## 范围（Scope）

### In scope

- 修复 Queue 日志区域的 CSS grid 约束，避免 min-content 宽度外溢影响外层 `.twoCol`：
  - `.logLine` 第三列由 `1fr` 改为 `minmax(0, 1fr)`；
  - `.logMsg` 增加安全断行策略（例如 `overflow-wrap: anywhere`）。
- Storybook：
  - 新增一个包含“长日志”的 mock scenario（至少 2 个 job，便于切换对比）。
  - 新增一个 Story（例如 `Pages/QueuePage/LongLogs`）并写明复现步骤。
-（可选）在 `test-storybook` 增加一个小型断言：切换到长日志 job 后，左右两栏宽度不应出现极端偏斜。

### Out of scope

- 表格虚拟化、日志分页/懒加载等性能优化。
- 响应式布局策略调整（移动端本来会单列）。

## 需求（Requirements）

### MUST

- 在宽屏（>=1200px）双栏模式下，切换到包含长 URL/digest 的日志时：
  - 左侧任务列表卡片不应被压缩到“明显不合理”的窄宽；
  - 页面不应出现整体横向溢出（若需要滚动，应限制在 `.logs` 容器内）。
- Storybook 中必须能稳定复现“长日志场景”，并可用于验证修复生效。

### SHOULD

- 长日志优先断行展示，避免必须横向滚动才能阅读。
- 修复尽量局部，不影响非 Queue 页面样式。

## 验收标准（Acceptance Criteria）

- Given 打开 Storybook 的 `Pages/QueuePage/LongLogs`
  When 先点选短日志 job，再点选长日志 job
  Then 双栏布局保持稳定（左侧任务列表不再被明显挤窄，chips 不会被迫竖排）
- Given 打开 `https://dockrev.ivanli.cc/queue`
  When 在任务列表中切换短日志 job 与长日志 job
  Then 页面双栏宽度不发生明显偏斜，日志区域内的长文本可断行或在日志框内滚动

## 里程碑（Milestones）

- [ ] Storybook：新增长日志 mock scenario + story
- [ ] CSS：修复 `.logLine` 第三列 shrink 约束 + `.logMsg` 断行策略
- [ ]（可选）test-storybook：增加回归断言

## 风险与开放问题（Risks & Open Questions）

- `overflow-wrap: anywhere` 会在长 token 中间断行，可能影响复制阅读；但相比布局错位更可接受，且仍可复制完整文本。
- 若未来其他页面复用 `.logLine/.logMsg` 样式，需要确认改动是否应保持 Queue 专用（可通过选择器范围限制）。

