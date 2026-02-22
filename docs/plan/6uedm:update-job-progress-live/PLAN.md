# Dockrev: Update Job 进度从“0→100跳变”改为持续可观测（#6uedm）

## 状态

- Status: 已完成
- Created: 2026-02-22
- Last: 2026-02-22

## 背景 / 问题陈述

- 当前 update job 进度按 stack 维度累计；当 `total_stacks=1` 且该 stack 内有多个 service 时，运行期间长期显示 `0%`，完成瞬间跳到 `100%`。
- 用户无法判断任务是否在推进，体验接近“卡死”。
- 现有执行器默认一次性回收命令输出，不支持 pull 过程实时进度消费。

## 目标 / 非目标

### Goals

- 提供对多容器更新任务真实可感知的运行中进度，不再长时间固定 `0%`。
- 在不破坏现有 API 的前提下，增强 update job 的 progress 更新粒度。
- 保持失败/回滚路径可观测，且最终状态语义不变。

### Non-goals

- 不改造任务队列模型。
- 不要求首版实现严格字节级下载百分比精度。

## 范围（Scope）

### In scope

- 后端：update job 进度改为服务级阶段推进（pull/up/health/complete）。
- 后端：增加流式命令执行能力，用于可选解析 pull 过程进度（best-effort，失败降级）。
- 前端：Queue 与 JobDetail 对“running + percent=0 但持续刷新”使用 indeterminate 展示，避免误导。

### Out of scope

- 新增外部 API 字段。
- 全量重构 runner 或日志系统。

## 需求（Requirements）

### MUST

- 单 stack、多 service 的 update job 运行中 `percent` 需出现持续增长（而非仅 0/100）。
- `GET /api/jobs`、`GET /api/jobs/{id}`、`/api/jobs/{id}/events` 的数据结构保持兼容。
- `percent` 全程单调不减，终态 success 时为 100。
- 失败路径下 progress 与 message/phase 要能定位所处阶段。

### SHOULD

- pull 过程在支持的运行环境下尽量输出更细粒度进度。
- 不支持流式解析时自动回退到阶段进度，不影响任务执行。

## 验收标准（Acceptance Criteria）

- Given 单 stack、4 services 的 update job
  When 任务运行中
  Then 页面进度不应长时间固定 `0%`，应持续变化或明确为 indeterminate 执行中。

- Given update job 运行中
  When 通过 SSE/轮询查看进度
  Then `percent` 单调不减，`message/currentTarget` 能体现当前 service/阶段。

- Given update job 执行失败
  When 任务结束
  Then 状态为 failed，`percent < 100`，并保留失败阶段信息。

## 测试（Testing）

- `cargo test -p dockrev-api updater`
- `cargo test -p dockrev-api api::tests::check_job_exposes_progress_in_detail_and_list`
- `bun run --cwd web build`

## 风险 / 开放问题

- 风险：pull 输出在不同 compose 版本下格式差异较大，解析稳定性存在不确定性。
- 缓解：流式解析仅作为增强路径，失败自动降级到稳定的阶段进度。

## 里程碑（Milestones）

- [x] M1: update job 服务级进度模型落地（后端）
- [x] M2: runner 流式执行 + pull 进度增强（best-effort）
- [x] M3: Queue/JobDetail 展示策略对齐（前端）
- [x] M4: 自动化验证与 PR 交付

## 变更记录（Change log）

- 2026-02-22: 创建计划并冻结范围、验收与测试口径（Status=待实现）。
- 2026-02-22: 完成实现与验证，创建 PR #84（Status=已完成）。
