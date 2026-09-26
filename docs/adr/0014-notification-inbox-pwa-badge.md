# ADR 0014：通知收件箱与 PWA Badge 的一致性边界

## Status

Accepted

## Context

Dockrev 已经有更新完成、新版本发现和 GHCR Webhook 异常三类正式通知事件，也支持可选 Web Push。收件箱和 PWA 图标 Badge 需要在多个标签页、Push 开关和浏览器能力差异下保持可解释的一致状态。

浏览器不保证普通 Service Worker 常驻运行，也不保证 Periodic Background Sync 的触发时机。因此，浏览器端不能成为通知发现或未读数的唯一来源。

## Decision

1. 在服务端 SQLite 中持久化 `notification_items`，以唯一 `identity_key` 做事件幂等，以 `read_at` 表示共享部署级已读状态。服务端未读数是唯一真相。
2. 正式事件先写入收件箱，再尝试 Email、Webhook、Telegram 和 Web Push。任务终态与对应 `job_finished` 收件箱项在同一 SQLite 事务中提交；候选通知记录与收件箱项关联也在同一 SQLite 事务中提交；外部渠道是否启用、投递失败或 Push 订阅取消都不删除收件箱项。Web Push 对临时失败订阅做一次即时重试，系统通知使用通知项 ID 作为稳定 tag。
3. 保留现有通知设置和测试 API。真实 Push payload 增加持久化 `notificationId` 与绝对 `unreadCount`；测试和旧 payload 不进行本地增量。
4. 前台使用 REST 加 `BroadcastChannel("dockrev:notifications")` 同步。广播只用于快速传播绝对值或触发刷新，客户端不根据本地事件做 `+1/-1` 推算。页面启动、`pageshow`、恢复可见、焦点和联网恢复时同步，可见期间每 60 秒轮询。
5. Service Worker 只负责显示 Push、在支持时写 Badging API，以及把系统通知点击通过 `MessageChannel` 交给受控页面。页面确认已读后才能完成导航；冷启动使用临时查询参数交接。没有 Push 且 PWA 关闭时不承诺实时 Badge，下一次启动或恢复时修复。
6. GHCR 审计使用独立状态账本记录 owner/repository 当前异常状态，并为 pending 异常保留批次身份。相同状态不重复，状态变更和恢复后的再次异常生成新的通知机会，恢复本身不减少未读数；同一 pending 批次追加异常时复用原收件箱项。

## Consequences

- Push 是低延迟加速通道，不是通知存在、未读计数或数据安全的前提。
- 多标签页共享一个部署级读状态，任一客户端读操作都会影响其他客户端的下一次 REST 收敛。
- Badging API 不存在时只缺少图标表现，收件箱和服务端计数仍可用。
- 已读记录由 API 访问触发有界 lazy cleanup，保留 90 天；不增加浏览器后台定时器，也不依赖 Periodic Background Sync。
- 收件箱事件内容在写入时确定，外部渠道模板仍由原有投递逻辑负责。

## Rejected Alternatives

- **仅依赖 Push 计数：**无法覆盖 Push 关闭、权限拒绝、订阅过期和投递失败。
- **浏览器本地 `+1/-1`：**多标签页、重复 Push、旧 payload 和服务端竞态会造成漂移。
- **新增通知 SSE：**在首版中增加连接、恢复和权限边界，而 REST 生命周期同步已满足正确性要求。
- **依赖 Periodic Background Sync：**浏览器覆盖和 User Agent 调度不稳定，不能提供正确性或时效性承诺。
