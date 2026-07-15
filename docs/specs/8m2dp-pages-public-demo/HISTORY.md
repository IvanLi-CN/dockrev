# Dockrev：GitHub Pages 公共 Web Demo（/demo/）历史（#8m2dp）

## History

- 2026-07-13：创建规格，定义 `/demo/` 单一路径、真实 pathname 路由、session-backed mock state、PWA-off demo runtime 与 Pages 404 深链恢复合同。
- 2026-07-13：完成 Pages demo build 变体、`BASE_URL` 感知 route/helper、session-backed public demo mock state、docs/workflow discoverability 与 assembled site `/demo/` smoke。
- 2026-07-13：修复 Storybook/iframe 下相对 `BASE_URL='./'` 被错误拼成 `#/./...` 的路由归一化问题，补齐相应单测。
- 2026-07-13：补齐 public demo cleanup mock scope，并将 `demo-pages-smoke` 升级为会拦截 `unhandled mock route` 的守门脚本。
- 2026-07-13：补回 overview 左侧 `工具面板`，让 public `/demo/` 的桌面侧栏同时提供搜索入口、资源摘要与当前时间，并刷新对应 owner-facing 视觉证据。
- 2026-07-13：将 overview `工具面板` 改为真正的桌面浮动工具窗，展开态支持任意拖拽，点击动作按钮时收成贴边气泡，再由气泡展开回浮窗，并保持移动端继续走抽屉入口。
- 2026-07-15：将 overview 浮层彻底收口为 `Demo 控制面板`，移除所有与 Demo/Mock 无关的搜索、资源摘要和时钟，只保留 runtime 状态、假写场景入口与 session 控制。
- 2026-07-15：同步修正 `Demo 控制面板` 的默认浮层高度估计，避免首屏仍按旧 `320px` 工具窗高度定位而把新增控制项裁出桌面视口；对应 owner-facing 视觉证据改由 Storybook canvas fallback 固化。
- 2026-07-15：删除 `Demo 控制面板` 顶部低价值 runtime 状态卡，将场景按钮改成单列纵向排版以消除文案溢出，并把底部说明改为简短结果描述。
