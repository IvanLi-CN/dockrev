# Dockrev：通知收件箱与 PWA Badge 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；本文记录实现覆盖、验证和 rollout 事实。

## Current Status

- Implementation: 已完成首版实现
- Lifecycle: active
- Catalog note: 需求与后台能力边界已对齐；运行时代码、数据库迁移、API、Push 与 AppShell 已落地。

## Implementation Coverage

- `REQ-NPB-001` to `REQ-NPB-006`: `notification_items`、GHCR 状态账本、四个收件箱 API 和事件入口已实现；旧通知设置/测试 API 保持不变。
- `REQ-NPB-007` and `REQ-NPB-009`: 真实 Push 使用 `notificationId` 和绝对 `unreadCount`；Service Worker 支持 Badge、受控页点击确认和冷启动交接，不依赖 Periodic Background Sync。
- `REQ-NPB-008` and `REQ-NPB-011`: AppShell 在认证分支接入启动、恢复、焦点、联网、可见轮询、BroadcastChannel 和 Badging API 能力检测。
- `REQ-NPB-010`: Topbar Bell、桌面/移动抽屉、显式单条已读、全部已读和点击后导航已实现。
- `REQ-NPB-012`: 任务终态与 `job_finished` 收件箱项在同一 SQLite 事务提交；检查任务的新版本收件箱项也在终态事务中预写，候选记录与对应收件箱项随后在同一预留事务中关联，其他收件箱项先于外部投递写入。候选通知按 `service + candidate digest` 保持活动去重，并保留 canonical check-job identity，使失败重试、进程崩溃后的 pending 重试和候选子集观察复用同一收件箱项；GHCR pending 异常保留稳定批次身份，恢复会结束旧批次，追加异常和恢复后的再次异常分别生成正确的通知机会。DB identity key、状态账本、失败重试渠道继承和绝对读响应测试已覆盖核心并发/重试边界。

## Existing Foundations

- `docs/specs/p2n8k-notification-event-switches-and-new-alerts/SPEC.md` 已定义三类正式通知事件及事件开关。
- `docs/specs/4n5vr-new-version-notification-records/SPEC.md` 已定义新版本候选的 `service + candidate digest` 去重身份。
- `docs/specs/r8kpa-web-pwa-offline-shell/SPEC.md` 已定义 PWA 壳、Service Worker、Push 和关闭页面后台同步的边界。
- 当前 `GET/PUT /api/notifications` 与 `POST /api/notifications/test` 继续归属于通知设置，不被收件箱 API 替换。

## Verification Commands

- `python3 bin/spec_contract_check.py --path docs/specs/notification-inbox-pwa-badge/SPEC.md`（若当前 checkout 提供该检查器）
- 后端：通知项迁移、事件幂等、API 读状态、Push payload 与 retention 测试。
- 前端：AppShell 收件箱交互、Service Worker payload、Badge 能力检测、前台轮询和跨标签页同步测试。
- 已执行：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test -p dockrev-api`、`bun test`、`bun run lint`、`bun run build`、`bun run build-storybook`、`bun run test-storybook`、PWA asset contract。

## Rollout Facts

- 服务端未读项是唯一计数真相；Push 关闭时的前台 REST 同步是必需兜底。
- 不引入 Service Worker 常驻定时器，不把 Periodic Background Sync 作为正确性依赖。
- Badge 能力缺失只影响图标表现，不影响收件箱和服务端未读状态。
- Web Push 多订阅发送对临时失败订阅做一次即时重试，持久失败返回失败结果；带 `notificationId` 的系统通知使用稳定 tag，重试不会在浏览器中重复堆叠通知。

## Remaining Gaps

- 外部 Push 服务的真实网络投递和不同浏览器原生图标 Badge 仍需部署环境验收；本地浏览器已验证页面 Badge/抽屉和 393x852 布局。
- 端到端双标签页和冷启动系统通知需要在启用真实 Push 的部署环境中继续验证；代码路径和协议测试已就绪。

## Related Changes

- `docs/adr/0014-notification-inbox-pwa-badge.md`
- `docs/specs/notification-inbox-pwa-badge/assets/notification-center-desktop.png`
- `docs/specs/notification-inbox-pwa-badge/assets/notification-center-mobile-393.png`
- `docs/specs/notification-inbox-pwa-badge/assets/notification-center-appshell-desktop.png`
- `docs/specs/notification-inbox-pwa-badge/assets/notification-center-appshell-mobile-393.png`

## References

- `./SPEC.md`
- `./HISTORY.md`
- `./contracts/http-api.md`
- `./contracts/db.md`
- `./contracts/push-payload.md`
