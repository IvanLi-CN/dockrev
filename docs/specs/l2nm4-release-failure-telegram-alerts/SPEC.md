# l2nm4 Release failure Telegram alerts

## Status
已完成

## Objective
为 `dockrev` 增加仓库内的发布失败 Telegram 告警 wrapper，复用共享的 `IvanLi-CN/github-workflows` reusable workflow，并在失败通知中优先解析真实发布目标 SHA。

## Scope
- 新增 `.github/workflows/notify-release-failure.yml`
- 复用共享 reusable workflow 发送 Telegram 通知
- 显式透传 `SHOUTRRR_URL`、仓库上下文与失败 run 元数据
- 保留 `workflow_dispatch` smoke test 入口
- 为 `Release` workflow 增加只读日志标记，显式打印 requested/target SHA 供 notifier 解析

## Requirements
1. wrapper workflow 监听 `Release` 的 `workflow_run.completed`，且仅在失败时触发失败告警。
2. wrapper workflow 提供无输入 `workflow_dispatch` smoke test，不触发真实发布。
3. 失败告警优先从失败 `Release` run 的 job 日志中解析真实 `target_sha`；无法解析时再回退到 `workflow_run.head_sha`。
4. 只允许增加观测日志，不能改变现有发布队列、跳过逻辑或 side effects。

## Acceptance criteria
1. `Release` workflow 失败时，Telegram 能收到失败通知。
2. `workflow_dispatch` smoke test 成功发送测试通知。
3. 在 manual backfill / release queue 场景下，通知里的 `sha` 优先展示从 release 日志解析出的真实目标 SHA。
