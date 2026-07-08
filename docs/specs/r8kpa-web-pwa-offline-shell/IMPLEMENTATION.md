# Dockrev：Web PWA 离线壳、更新提示与分级缓存实现状态（#r8kpa）

## Implementation

- `vite-plugin-pwa` 已接入 `injectManifest`，`web/src/sw.ts` 统一承载 precache、受限导航 fallback、Push 与通知点击回跳。
- app bootstrap 已全局注册 service worker；Settings 页的 Web Push 订阅路径改为复用全局 worker，不再自行注册临时 `public/sw.js`。
- 已新增 installability 所需的 `manifest.webmanifest`、`theme-color`、`apple-touch-icon`、`pwa-192.png` 与 `pwa-512.png`。
- 已实现全局 PWA 状态管理：页面激活/focus/visible 时更新检查、可见态每小时轮询、ready prompt 手动刷新、不静默热切换。
- 已落统一只读快照层 `readonlySnapshotCache.ts`，并把首页旧 `localStorage` 快照迁移桥接到 IndexedDB。
- 已接入持久快照与离线提示的页面：`/`、`/services`、`/queue`、`/queue/version-inference`、stack detail、service detail 的 `overview / monitoring / backup` 只读子页。
- service detail 已改为：离线时只回放本地摘要与监控样本，`logs / settings` 明确回退到联网门控，不再伪装成可离线使用。
- 只读缓存消费已进一步收紧：各页面现在只接受 `fresh` 快照，离开新鲜窗口的本地数据一律不再展示，也不再向用户暴露“数据过时”类文案。
- 已补壳层状态条、只读快照提示组件、service detail 离线门控与监控快照 Storybook 场景，以及对应截图脚本入口。

## Outstanding

- `/services` 虽已进入只读快照模式并禁用关键扫描/批量更新入口，但尚未做到页内所有写动作都逐一细化门控。
- `service detail` 当前仍只持久化监控样本与只读摘要，未对更多高时效子模块做更细颗粒的只读/在线分层。
- `test-storybook` 在本次运行中出现长时间无输出卡住，未形成可用通过证明。

## Validation

- `bun run build`
- `bun run lint`
- `bun run build-storybook`
- `bun run storybook:screenshots -- --only components-appshellstatusbanner--update-ready,components-readonlysnapshotnotice--offline-snapshot`
- `bun run storybook:screenshots -- --only components-serviceresourcepanel--offline-snapshot,pages-servicedetailpage--settings-offline-readonly`
- `bun run test-storybook`：本次运行在启动后进入静默长等待，约 15 分钟无新输出，按 stalled 处理，未记为通过。

## Current Coverage

- 壳层更新提示与只读快照提示已具备 Storybook 视觉证据。
- 首页、任务队列、版本推测、运维大盘、Stack 详情、Service 详情只读子页已具备本地快照优先加载路径。
- 统一快照层已具备 7 天过期门控与首页旧快照迁移兼容。
