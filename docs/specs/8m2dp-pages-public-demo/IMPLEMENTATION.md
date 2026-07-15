# Dockrev：GitHub Pages 公共 Web Demo（/demo/）实现状态（#8m2dp）

## Implementation

- 已将原本仅供本地使用的 `demo:app` 提升为 GitHub Pages docs 组装站内的正式 `/demo/` public surface，并保持 Demo / Storybook / Docs 三者职责分离。
- `web` 新增 Pages demo build 变体，所有 route helper、pathname 解析、`href()`、public base URL 建议值与 demo deep-link restore 都统一尊重 `BASE_URL`；同时修复相对 `BASE_URL='./'` 在 Storybook/iframe 环境下被错误归一成 `/./` 的问题。
- Pages demo 明确关闭 PWA/service worker/install contract：demo build 不注册 SW、不暴露 root-scope install 行为，正式生产 app 的 PWA 行为保持不变。
- public demo runtime 改为复用共享 `dockrevMockApi`，以 seeded fixture + `sessionStorage` 持久化支撑可交互假写；同一浏览器会话内刷新和深链恢复保持状态，新会话回到 seed 数据。
- public demo mock scope 额外打开 cleanup 路由族，确保 `/demo/cleanup` 与 GHCR/settings 等 nav-visible 主路由在 Pages 组装产物中稳定可用，不再出现 `unhandled mock route`。
- overview 桌面端现在使用真正的浮动 `Demo 控制面板`：已去掉低价值 runtime 状态卡，仅保留 GHCR/cleanup/queue/deploy Demo 场景快捷入口与 session-backed 重置、restore 清理控制；按钮改为单列纵向排版，避免中文+英文说明在窄面板里横向溢出；底部说明改成简短结果描述，不再出现“只允许/不会触达 backend”这类需求式或实现细节文案。展开态可自由拖拽，只有点击动作按钮时才会收成贴边气泡，再由气泡展开回浮窗。默认浮层尺寸估计也同步回落到贴近真实内容高度，避免首屏定位过高或沿用旧 `320px` 高度而把控制面板裁出视口。
- root `404.html` 在组装阶段注入 inline restore bridge：当 GitHub Pages 命中 `/demo/...` 深链 404 时，先把原始路径写入 session storage，再回跳 `/demo/` 入口恢复真实 pathname 路由，而不是退回 hash routing。
- docs discoverability 已同步到 README、docs 首页、导航与 FAQ；`docs-pages` workflow 与组装脚本现在一起发布 docs root、`/storybook/` 和 `/demo/`。

## Validation

- `bun test ./web/tests/appBase.test.ts ./web/tests/pagesDemoRestore.test.ts`
- `bun test ./web/tests/overviewToolPanelState.test.ts`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `DOCKREV_WEB_BASE=/dockrev/demo/ bun run --cwd web build:demo:pages`
- `DOCS_BASE=/dockrev/ bun run docs:build`
- `bun run --cwd web build-storybook -- --quiet`
- `bun run --cwd web test-storybook`
- `bash ./.github/scripts/assemble-pages-site.sh docs-site/doc_build web/storybook-static web/dist .tmp/pages-site-local`
- `node ./web/scripts/demo-pages-smoke.mjs .tmp/pages-site-local /dockrev/`

烟测覆盖了：

- `/demo/`、`/demo/services`、`/demo/services/stack-prod/svc-prod-api/history`、`/demo/queue`、`/demo/settings`、`/demo/settings/ghcr-webhooks`、`/demo/cleanup`、`/demo/deploy-check` 等深链在组装站内可恢复。
- 至少一条 update 假写流程和一条 GHCR/settings 假写流程在 session-backed mock state 下可刷新保持。
- demo 页面不再容忍 `unhandled mock route` 这类 mock 覆盖缺口。

## Notes

- owner-facing 视觉证据见 [SPEC.md](./SPEC.md) 的 `## Visual Evidence`：主路由与流程仍以 assembled Pages `/demo/` true surface 为准，overview `Demo 控制面板` 使用 mock-only demo-runtime page harness 的 Storybook canvas fallback 固化证据。
