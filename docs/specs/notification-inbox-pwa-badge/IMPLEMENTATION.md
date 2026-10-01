# Dockrev：通知收件箱与 PWA Badge 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；本文记录实现覆盖、验证和 rollout 事实。

## Current Status

- Implementation: 通知摘要整改与桌面/移动收件箱视觉验收已完成
- Lifecycle: active
- Catalog note: 任务、新版本和 GHCR 异常摘要已统一为可读文案；运行时代码、数据库迁移、API、Push 与 AppShell 保持既有边界。

## Implementation Coverage

- `REQ-NPB-001` to `REQ-NPB-006`: `notification_items`、GHCR 状态账本、四个收件箱 API 和事件入口已实现；旧通知设置/测试 API 保持不变。
- `REQ-NPB-007` and `REQ-NPB-009`: 真实 Push 使用 `notificationId` 和绝对 `unreadCount`；Service Worker 支持 Badge、受控页点击确认和冷启动交接，不依赖 Periodic Background Sync。
- `REQ-NPB-008` and `REQ-NPB-011`: AppShell 在认证分支接入启动、恢复、焦点、联网、可见轮询、BroadcastChannel 和 Badging API 能力检测。
- `REQ-NPB-010`: Topbar Bell、桌面/移动抽屉、显式单条已读、全部已读和点击后导航已实现。
- `REQ-NPB-012`: 任务终态与 `job_finished` 收件箱项在同一 SQLite 事务提交；检查任务完成事务同时写入可重放的通知 dispatch outbox，进程在候选预留前或外部投递前退出时，服务启动会继续处理未完成 dispatch。检查任务的新版本候选与收件箱项在候选预留事务中一起提交，其他收件箱项先于外部投递写入。候选通知按 `service + candidate digest` 保持活动去重，并保留 canonical check-job identity，使失败重试、进程崩溃后的 pending 重试和候选子集观察复用同一收件箱项；候选投递使用短租约 claim，避免并发发送者重复投递，租约过期后仍可恢复。GHCR pending 异常由可重放的 occurrence 队列保留稳定批次身份，同一次审计的状态变化聚合为一个 batch，后续审计使用新 batch，恢复不会丢弃待投递 occurrence；每个异常批次还记录已成功的外部渠道和 Push 订阅，部分失败重试不会重复投递成功目标。DB identity key、dispatch outbox、状态账本、occurrence 重放、失败重试渠道继承和绝对读响应测试已覆盖核心并发/重试边界。
- `REQ-NPB-013`: 共享 formatter 按任务类型/终态、新版本服务名/可读版本、GHCR 异常类型生成摘要。聚合新版本标题显示总服务数，正文列出最多两项；不从任务进度 JSON、内部 ID、digest 或异常长错误构造收件箱文案，版本标签仅接受可读 Docker 标签格式并回退 ULID、`service-internal` 和其他非标签载荷。版本行的 first-seen `service_id` 去重与显示映射由 `notify/summary.rs` 集中承载，`notify.rs` 委托该 helper 后保持既有摘要结果。
- `REQ-NPB-014`: SQLite migration `0042_backfill_notification_summaries` 仅处理仍未读且可从保留源记录可靠重建的项目，只更新 `title`/`body`；缺失或不完整的源记录保持原文。抽屉正文自然换行，并显示与原目标一致的任务、服务、清单或审计入口；任务、单服务、聚合清单及 GHCR 审计入口均覆盖点击后先确认已读再导航。通知截图采集预检在写入截图前断言视口、抽屉和列表无溢出，且摘要未被行数截断或裁切。

## Existing Foundations

- `docs/specs/p2n8k-notification-event-switches-and-new-alerts/SPEC.md` 已定义三类正式通知事件及事件开关。
- `docs/specs/4n5vr-new-version-notification-records/SPEC.md` 已定义新版本候选的 `service + candidate digest` 去重身份。
- `docs/specs/r8kpa-web-pwa-offline-shell/SPEC.md` 已定义 PWA 壳、Service Worker、Push 和关闭页面后台同步的边界。
- 当前 `GET/PUT /api/notifications` 与 `POST /api/notifications/test` 继续归属于通知设置，不被收件箱 API 替换。

## Verification Commands

- `python3 "$CODEX_HOME/bin/spec_contract_check.py" --path docs/specs/notification-inbox-pwa-badge/SPEC.md`
- `cargo test -p dockrev-api notification_summary`: formatter 与回填测试通过（2 项）。
- `cargo test -p dockrev-api`: 1053 项通过、1 项忽略。
- `cargo fmt --all -- --check` 与 `git diff --check`: 通过。
- `bun run build`: 264 项 Bun 测试通过，TypeScript、Vite 与 PWA asset contract 通过；Vite 报告现有大 chunk 提示。
- `bun run lint`: 0 errors；3 条既有 warning 位于 `GitHubReleaseDrawer.tsx`、`ServiceVersionsSection.tsx`、`ServiceDetailPage.tsx`。
- `bun run build-storybook`: 通过；输出 `radix-ui` package metadata 与大 chunk 提示。
- `bun run test:notification-center-summary-interactions`（先运行 `bun run build-storybook`）: 通知中心 10 个故事通过，包含任务/服务确认后导航与摘要动作可见性。
- `bun run storybook:screenshots --only notification-center-desktop.png,notification-center-mobile-393.png --outdir <temporary-directory>`: Storybook mock-only 抽屉在 1800x960 与 393x852 CSS px 视口完成采集；边距、视口、抽屉和列表溢出预检通过，基线差异经主人确认后写入 Spec 规范图片。
- `DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY=1 bun run test-storybook`: 未通过；在既有 Overview 移动导航交互中等待 `document.body.style.overflow === "hidden"` 超时，与通知故事无关。

## Rollout Facts

- 服务端未读项是唯一计数真相；Push 关闭时的前台 REST 同步是必需兜底。
- 受控页面收到 Push 后触发带 single-flight 的 REST 同步，并以显式 ACK 确认已应用绝对未读数；没有 ACK 的页面（包括不含通知 Provider 的路由）由 Service Worker 回退到服务端查询。Badge 查询和网络请求都有界等待，不能阻塞系统通知展示。
- 不引入 Service Worker 常驻定时器，不把 Periodic Background Sync 作为正确性依赖。
- Badge 能力缺失只影响图标表现，不影响收件箱和服务端未读状态。
- Web Push 多订阅发送对临时失败订阅做一次即时重试，持久失败返回失败结果；带 `notificationId` 的系统通知使用稳定 tag，重试不会在浏览器中重复堆叠通知。

## Remaining Gaps

- 外部 Push 服务的真实网络投递和不同浏览器原生图标 Badge 仍需部署环境验收；本地浏览器已验证页面 Badge/抽屉和 393x852 布局。
- 端到端双标签页和冷启动系统通知需要在启用真实 Push 的部署环境中继续验证；代码路径和协议测试已就绪。
- 当前通知中心桌面与移动截图已完成基线比较和主人确认；规范图片包含任务、新版本、GHCR 异常摘要及其详情入口。

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
