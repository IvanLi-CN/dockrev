# Dockrev API/Web: 修复 check jobs 并发与退出未收尾（#wtb4a）

## 状态

- Status: 已完成
- Created: 2026-02-14
- Last: 2026-02-15
- PR: #66

## 背景 / 问题陈述

- 线上「更新队列」出现多个 `check(all)` 同时处于 `running`，且部分是历史遗留的长期 `running`（日志仅 `check started`，无 `finishedAt`）。
- Dockrev 服务端以容器部署，进程退出/重启时若未完成收尾，DB 会残留非终态任务，导致队列页面长期显示“运行中”并干扰判断。

## 目标 / 非目标

### Goals

- `POST /api/checks` 增加并发护栏：当已有同 scope 的 `check` job 处于 `running` 且非 stale 时，返回 `409 Conflict`，并在 error details 中提供 `existingJobId`，便于 UI 引导跳转查看。
- 增加退出/重启兜底：即使服务端退出收尾没能完成，下次启动时也能将所有“非终态 job”落到明确终态（`failed`）并写入可解释原因（terminated: server_restart）。
- UI 在触发 scan/check 时对 `409` 做友好提示，并提供进入 `/queue/<jobId>` 的引导。

### Non-goals

- 不引入全新的队列/worker 系统（不新增 `queued` 状态与调度器）。
- 不在本计划内改变候选 tag 选择策略或版本推测逻辑。
- 不在本计划内重做 update job 的并发策略（仅必要时复用“退出兜底”的终止逻辑）。

## 范围（Scope）

### In scope

- 后端：
  - 新增“启动恢复”逻辑：服务启动前扫描 DB 中非终态 jobs（主要是 `running`）并标记为 `failed`，写入 `finishedAt`，并追加 job log 与 summary 中的 terminated 字段。
  - 新增“退出 best-effort 收尾”逻辑：收到 SIGTERM/SIGINT 时尝试终止非终态 jobs（失败也不阻塞退出），由下次启动兜底保证一致性。
  - `POST /api/checks` 增加并发护栏：
    - running 且非 stale：返回 `409 Conflict` + `{ existingJobId }`
    - running 但 stale：先标记旧 job 为终态再创建新 job
- 前端：
  - 概览页/服务页触发扫描时，处理 `409` 并展示引导（带 jobId）。
- 测试：
  - Rust：覆盖 `409` 行为、stale 自愈、启动恢复落终态。

### Out of scope

- 数据库 schema 迁移新增 `terminated` status（本计划用 `failed + terminated details` 表达）。

## 验收标准（Acceptance Criteria）

- Given 已存在一个同 scope 的 `check` job 在 `running` 且未过 stale 阈值，
  When 再次调用 `POST /api/checks`，
  Then 返回 `409`，且 error details 含 `existingJobId`。
- Given Dockrev 服务端异常退出或重启导致 DB 残留非终态 job，
  When 服务端下次启动，
  Then 这些 jobs 会被标记为 `failed`，`finishedAt` 非空，并写入一条解释日志（terminated: server_restart）。
- Given UI 点击“立即扫描/立即扫描更新”触发 `POST /api/checks` 返回 `409`，
  Then UI 提示“已有扫描任务运行中”，并提供进入该 job 详情页的入口（`/queue/<jobId>`）。

## 测试（Testing）

- `cargo test -p dockrev-api`
- `bun -C web run build`

## 风险 / 开放问题（Risks & Open Questions）

- stale 阈值需要保守设置（默认建议 2h），避免误杀慢 registry/网络波动导致的长耗时 check。

## 变更记录（Change log）

- 2026-02-14: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-15: 实现完成并通过 CI（PR #66）。
