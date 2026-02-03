# Dockrev API: 修复 check jobs 长时间卡在 running（#fay4j）

## 状态

- Status: 已完成
- Created: 2026-02-02
- Last: 2026-02-03

## 背景 / 问题陈述

- 线上「更新队列」出现多个 `check` 类型任务长期停留在 `running`，即使实际没有继续推进（无 `success/failed` 退出）。
- 日志可见部分任务只记录了早期的 registry 相关 warn（例如 `list tags failed ... 401 Unauthorized`），随后不再追加日志，也未写入 `finishedAt`。
- 预期行为：无论成功或失败，`check` job 都必须最终落到一个明确的终态（`success/failed`），并写入 `finishedAt` 与 summary。

## 目标 / 非目标

### Goals

- `/api/checks` 触发的 `check` job 不再依赖 HTTP 请求生命周期：即使客户端断开/网关超时，job 也能继续执行并最终 `finish_job`。
- `check` job 在异常路径也能可靠落盘终态（`failed` + error summary），避免“孤儿 running job”。
- 为容易卡住的外部调用增加超时保护（至少覆盖 registry 请求），避免单个请求无界阻塞导致 job 永不结束。

### Non-goals

- 不在本计划内改变候选 tag 选择策略、版本号解析/推测逻辑（如需调整另立计划）。
- 不引入全新的队列/worker 系统（仅修复现有 job 触发与收尾行为）。

## 范围（Scope）

### In scope

- 后端：
  - 将 `/api/checks` 的执行改为后台任务（spawn）并返回 `checkId`（job id）；
  - 统一 job 收尾：无论成功/失败都调用 `finish_job`，并在失败时记录 error log；
  - 为 registry 请求设置合理超时（避免无限等待）。
- 测试：补/改回归测试，覆盖“请求被提前取消也能最终完成 job”与“超时会失败并写入终态”。

### Out of scope

- UI 交互/视觉调整（队列页按现有接口展示即可）。

## 验收标准（Acceptance Criteria）

- Given 触发 `/api/checks`（UI reason=ui），When 客户端在 job 完成前断开连接，
  Then job 仍会在合理时间内从 `running` 变为 `success` 或 `failed`，且 `finishedAt` 非空。
- Given registry 请求出现 401/超时/网络错误，
  Then job 不会无限卡住：应在超时边界内结束，并写入 `failed`（或按既有降级策略继续，但必须结束）。
- Given 队列页存在旧的 running job，
  Then 新触发的 check job 应能正常完成（不会再次产生“running 不落地”）。

## 测试（Testing）

- `cargo test -p dockrev-api`

## 风险 / 开放问题（Risks & Open Questions）

- 超时与并发策略需要保守设置，避免误伤慢 registry；必要时再引入缓存/重试退避。

## 变更记录（Change log）

- 2026-02-02: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-03: 实现完成并通过测试（PR #48）。
