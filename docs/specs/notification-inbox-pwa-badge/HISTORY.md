# Dockrev：通知收件箱与 PWA Badge 主题历史

> 这里记录主题局部生命周期、兼容性与必要背景；当前行为合同仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- 本主题由通知事件开关、新版本通知去重和 PWA Push 能力之上新增，负责统一的通知项、已读状态和 Badge 计数。
- 现有外部通知设置、通知测试接口和三类通知 payload 的既有字段保持兼容；收件箱不复用 Push 投递记录作为未读状态。

## Replacements / Background

- `p2n8k-notification-event-switches-and-new-alerts` 继续拥有事件开关和外部渠道触发语义，本主题只定义这些正式事件进入应用内收件箱后的持久化与确认行为。
- `4n5vr-new-version-notification-records` 继续拥有新版本候选的服务级 digest 去重身份，本主题将其作为收件箱 item identity 的输入。
- `r8kpa-web-pwa-offline-shell` 继续拥有 PWA 壳和 Service Worker 生命周期；本主题接受其“不保证关闭页面后台周期更新”的边界。

## Related Changes

- ADR 0014 records the server-authoritative count, REST plus BroadcastChannel synchronization, optional Push, and Service Worker click-handshake boundary.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
