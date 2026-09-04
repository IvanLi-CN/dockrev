# Dockrev：Web PWA 离线壳、更新提示与分级缓存实现状态（#r8kpa）

## Implementation

- `vite-plugin-pwa` 已接入 `injectManifest`，`web/src/sw.ts` 统一承载低优先级 precache、路由合同 allowlist 的 app-shell fallback、Push 与通知点击回跳。
- 已新增 `web/src/routeContract.ts` 作为前端/Rust/Service Worker 的唯一页面路由合同；Vite 输出内部 `.dockrev-route-contract.json`，Rust `build.rs` 校验后嵌入且不对外提供。
- Vite 使用 `appType: 'mpa'` 生成主 `index.html` 与独立 `404.html`；两者共享 `NotFoundView` 样式，404 入口不加载运行时配置、主路由或 Service Worker 注册。
- Rust UI 路由已移除宽泛主文档回退：固定/动态合同页返回主文档，页面尾斜杠返回 308，未知文档返回 404 文档，未知带扩展名资源、API、Supervisor 和内部合同文件不会返回应用 HTML。
- install metadata 已改为逐资源 SHA-256 内容哈希文件名：构建后的 HTML 和 manifest 只引用当前哈希 favicon/regular/maskable 文件；产品 HTML 不声明 `apple-touch-icon`，manifest link 由 VitePWA 单独注入，避免模板与插件重复声明。
- `crates/dockrev-api/src/ui.rs` 已为哈希 install icon 返回 `public, max-age=31536000, immutable`，并为 `index.html`、`manifest.webmanifest`、`sw.js` 与旧固定名 favicon、regular/maskable 图标返回 `no-cache`，使入口元数据可重新验证而旧兼容路径不被错误长期缓存。
- app bootstrap 已全局注册 service worker；Settings 页的 Web Push 订阅路径改为复用全局 worker，不再自行注册临时 `public/sw.js`。
- 已新增 installability 所需的 `manifest.webmanifest`、`theme-color`、regular `pwa-192.png` / `pwa-512.png`，以及独立的 maskable 派生物；Vite 为产品 Manifest 图标和 favicon 计算独立的内容哈希文件名，稳定保留 manifest `id`、`scope`、`start_url`。产品不生成或发布根路径 Apple touch 图标，文档站点继续生成自己的独立 Apple touch 图标。Manifest、regular/maskable 图标、favicon 和 Apple 图标均排除在 Worker precache/cache-first 路径之外。
- 已实现全局 PWA 更新状态机：页面激活/focus/visible 时更新检查、可见态每小时轮询、`updatefound -> downloading`、仅以 Workbox `waiting -> ready` 作为完整缓存门禁，以及失败重试。
- 已实现 single-flight 更新激活：手动“立即更新”和下一次 pathname 导航复用同一 `SKIP_WAITING` 请求，先提交目标 URL 再由 controllerchange 重载；查询参数和内部抽屉状态不会触发。
- 已落统一只读快照层 `readonlySnapshotCache.ts`，并把首页旧 `localStorage` 快照迁移桥接到 IndexedDB。
- 已接入持久快照与离线提示的页面：`/`、`/services`、`/queue`、`/queue/version-inference`、stack detail、service detail 的 `overview / history / monitoring / backup` 只读子页。
- service detail 已改为：离线时只回放本地摘要、更新记录、监控样本与备份摘要；history 只使用现有 60 秒 fresh jobs snapshot，不建立 SSE，`logs / settings` 明确回退到联网门控，不再伪装成可离线使用。
- 只读缓存消费已进一步收紧：各页面现在只接受 `fresh` 快照，离开新鲜窗口的本地数据一律不再展示，也不再向用户暴露“数据过时”类文案。
- 已将更新提示从壳层状态条拆为固定右下气泡，支持下载禁用、ready 离线激活、失败重试、离线 hover/focus 隐藏和移动底部导航避让；离线壳与只读快照状态条保留在内容区。
- 已补更新气泡与 AppShell 的 Storybook 状态、交互覆盖和桌面/移动截图脚本入口。
- 已记录 Android Chrome/WebAPK 与 Chromium desktop 的 manifest 更新边界，以及 iOS/iPadOS Web Clips、浏览器快捷方式等不能由网站强制迁移既有图标的限制。

## Outstanding

- `/services` 虽已进入只读快照模式并禁用关键扫描/批量更新入口，但尚未做到页内所有写动作都逐一细化门控。
- `service detail` 当前仍只持久化监控样本与只读摘要，未对更多高时效子模块做更细颗粒的只读/在线分层。

## Validation

- `bun run build`
- `bun run test:pwa-assets`
- `bun run test:pwa-update`
- `cargo test -p dockrev-api ui::tests`
- `bun run lint`
- `bun run build-storybook`
- `bun run storybook:screenshots -- --only layouts-appshell--update-ready-bubble,layouts-appshell--update-ready-bubble-mobile`
- `bun run storybook:screenshots -- --only components-serviceresourcepanel--offline-snapshot,pages-servicedetailpage--settings-offline-readonly`
- `bun run test-storybook`

## Current Coverage

- 壳层更新气泡与只读快照提示已具备 Storybook 视觉证据。
- 首页、任务队列、版本推测、运维大盘、Stack 详情、Service 详情只读子页已具备本地快照优先加载路径。
- 统一快照层已具备 7 天过期门控与首页旧快照迁移兼容。
