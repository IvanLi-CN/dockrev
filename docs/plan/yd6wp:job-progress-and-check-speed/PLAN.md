# Dockrev: `check all` 提速 + 任务进度可观测性统一（#yd6wp）

## 状态

- Status: 已完成
- Created: 2026-02-22
- Last: 2026-02-22

## 背景 / 问题陈述

- 现状下 `check all` 体感明显偏慢，用户无法快速判断“是否在推进”还是“已经卡住”。
- 任务详情页缺少明确进度展示（百分比 / 已处理数量 / 当前目标），信息披露不足。
- 运行中日志对“当前正在处理什么”描述不足，常出现 `check started` 后长时间无有效上下文。

## 目标 / 非目标

### Goals

- 提升 `check all` 的执行速度，目标为同等基线下 2-4x 提速（优先稳态，不激进牺牲稳定性）。
- 为 `check / update / runtime_scan` 三类任务提供统一的进度模型，并在列表页与详情页展示。
- 在任务运行期间持续暴露“当前阶段 + 当前目标 + 已处理进度”，减少黑箱感。

### Non-goals

- 不改变现有业务语义（例如候选镜像选择策略、更新策略与回滚语义）。
- 不引入新的任务队列系统或独立 worker 进程架构。
- 不做日志高亮、搜索、下载等增强功能。

## 范围（Scope）

### In scope

- 后端：
  - 为 jobs 增加统一 `progress` 结构（API 读取一致）。
  - 在 SSE 中增加统一 `job_progress` 事件。
  - `check all` 改为有界并发 + 节流进度写入 + 节流进度日志。
  - `runtime_scan` / `update` 补齐进度更新。
- 前端：
  - `/queue` 列表页展示运行中任务简版进度。
  - `/queue/<jobId>` 详情页展示进度卡片（百分比 + 阶段文案 + 当前目标）。

### Out of scope

- 队列系统重构、数据库迁移（新增列）或跨服务拆分。
- 新增任务类型的 UI 页面。

## 需求（Requirements）

### MUST

- `GET /api/jobs` 与 `GET /api/jobs/{id}` 对 running 任务返回可读 progress。
- `GET /api/jobs/{id}/events` 在运行期间能持续推送 `job_progress`。
- `check all` 运行中至少按节流窗口持续更新进度，不可长时间静默。
- `/queue` 与任务详情页都可展示 running 任务进度，且兼容无 progress 的历史任务。
- `check all` 在基准场景达成 2-4x 提速目标区间（或给出明确降级原因）。

### SHOULD

- 任务结束后保留最终进度（100%）用于复盘。
- 进度日志中包含当前目标标识（stack/service）便于排障。

## 验收标准（Acceptance Criteria）

- Given 触发 `check all`
  When 任务处于 running
  Then `/api/jobs` 与 `/api/jobs/{id}` 的 `progress.current` 单调递增，`percent` 持续变化。

- Given 打开任务详情页
  When 任务持续执行
  Then 页面可见“百分比 + 阶段文案 + 当前目标 + current/total”，并随 SSE/轮询刷新。

- Given 打开任务队列页
  When 存在 running 任务
  Then 任务行显示简版进度（至少 `current/total` 与阶段信息）。

- Given registry 波动或个别服务失败
  When check 继续推进
  Then 不会因为单点失败导致进度停滞；任务最终进入终态并写入 summary/logs。

## 测试（Testing）

- 后端：`cargo test -p dockrev-api`
- 前端：`bun run --cwd web build`
- 前端：`bun run --cwd web lint`

## 风险 / 开放问题

- 风险：并发提升会增加 registry 限流概率（429）。
- 缓解：并发数固定上限 + 进度写入节流 + 保持失败降级路径可终止。

## 里程碑（Milestones）

- [x] M1: 后端统一 progress 模型（list/get + SSE `job_progress`）
- [x] M2: `check all` 并发化与进度/日志节流
- [x] M3: `runtime_scan` / `update` 进度对齐
- [x] M4: 前端 Queue/JobDetail 进度展示与兼容回退
- [x] M5: 自动化验证与 PR 交付

## 变更记录（Change log）

- 2026-02-22: 创建计划，冻结范围、验收与测试口径（Status=待实现）。
- 2026-02-22: 完成实现与验证，创建 PR #81（Status=已完成）。
