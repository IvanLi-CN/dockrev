# Dockrev：独立 Web Demo 站点（build-time 版本 + mock Inspector）（#d3m0a）

## 状态

- Status: active
- Created: 2026-11-25
- Supersedes: `docs/specs/8m2dp-pages-public-demo/SPEC.md`（`/demo/` 子路径方案作废）

## 背景 / 问题陈述

- 已有 `/demo/` 是 GitHub Pages docs 站的子目录，站点边界、base path、404 恢复、PWA 开关和 smoke 全部围绕 `/demo/` 前缀编写；docs 与 demo 互相牵连。
- Demo 没有后端。它的版本号来自 mock 里的字面量 `0.5.0`，`Shell` 仍然走 `/api/version`，所以站点上显示的版本与构建产物无关。
- 演示状态靠 `demoScenario` 等分离参数并只能通过整页 reload 生效；控制面板是 overview 专属的桌面浮窗，非 overview 路由和移动端都没有入口。

## 目标 / 非目标

### Goals

- Demo 是**独立站点**：自有 build 变体、自有 deploy target、根 base `/`，不复用 docs 站点或 `/demo` 子路径。
- Demo 版本在 **vite 构建期确定**：`build-info.json` 与 `__DOCKREV_BUILD_INFO__` 同源，UI 与 mock `/api/version`、`/supervisor/version` 共用该值；demo 不查询任何后端版本接口。
- 发布只由 `Release` 驱动，消费同一 release identity；PR 与 main 不发布 demo。
- 场景、身份、网络与数据状态全部 URL-first（`demo=<scene>`、`d_*`），场景切换走 client-side navigation，不整页 reload。
- Inspector 是结构化控制面：scene / persona / network / data / share 五组，含模拟写入记录；桌面浮窗可收起为 bubble，移动端为 drawer。
- Bootstrap 失败必须有安全错误面，且强制在渲染前验证 mock 拦截已生效。

### Non-goals

- 不改 Rust API、supervisor 或 deploy runtime。
- 不引入真实登录、真实后端或生产同源运行时。
- 不恢复 demo 的 PWA install / service worker 合同。

## 范围（Scope）

### In scope

- `web/src/buildInfo.ts`、`web/vite.config.ts`、`web/src/main.tsx`
- `web/src/demo/**`（scenes、session、store、runtimeBootstrap、DemoInspector、inspectorLayout）
- `web/src/mock/**`（原 `web/src/stories/mocks/**` 下沉）
- `web/src/Shell.tsx`、`web/src/App.tsx`、`web/src/routes.ts`
- `web/404.html`、`web/scripts/demo-site-smoke.mjs`、`web/scripts/demo-e2e-server.mjs`
- `web/playwright.config.ts`、`web/e2e/demo-site.spec.ts`
- `.github/workflows/{ci-pr,release,docs-pages}.yml`、`.github/scripts/{assemble-pages-site.sh,deploy-demo-edgeone.sh}`、`.github/quality-gates.json`
- `docs-site/**`、`README.md`

### Out of scope

- 真实 EdgeOne 项目首配、域名绑定与证书（owner-side rollout）
- 历史 spec 文档重写

## 需求 / 行为合同

- Demo 站点根路径就是应用根；产物内不得出现 `/demo/` 前缀、不得嵌套 `demo/` 目录。
- `vite build --mode demo` 输出 `dist-demo/`，包含 `index.html`、`build-info.json`、`edgeone.json`；`edgeone.json` 的 rewrite 覆盖 `routeContract.ts` 中的全部静态与动态路由族。
- Demo 版本来源唯一：CI/release 传入 `DOCKREV_BUILD_VERSION` 与 `DOCKREV_BUILD_CHANNEL`；未传入时按 `main+<sha>` / `0.0.0-pr.<sha>` / `0.0.0-local` 降级，且不伪造 release 链接。
- `build-info.json` 的 `version` 与 `sourceSha` 必须与 release identity 一致，否则部署失败关闭。
- `demo=<scene>` 选择场景；`d_persona`、`d_net`、`d_case`、`d_events`、`d_cleanup` 记录可分享状态；`d_controls=hidden` 提供干净证据态。默认值不写入 URL。
- pathname 与 `demo` 参数冲突时 pathname 优先，Inspector 显示的场景不得与当前路由不一致。
- 场景切换使用 `pushState`，不重载文档；变更影响 mock 模型的字段会重建 mock 并 remount 应用子树。
- Inspector 挂载在 App shell 级，覆盖所有路由；`<961px` 使用 bubble → drawer。
- 写操作只改内存 mock 模型，并在 Inspector 的“模拟写入”区留痕。
- Bootstrap 在 `createRoot` 之前完成 mock 安装，且校验 `globalThis.fetch` 已被接管、`__DOCKREV_MOCK_DEBUG__` 存在；任一失败渲染安全错误面，绝不回退真实 API。
- Demo 发布只由 `Release` 触发；PR 只做 build + smoke。实际 EdgeOne 部署受仓库变量 `DOCKREV_DEMO_PUBLISH_ENABLED` 门控：未启用时 Release 仍产出可手工部署的 `dockrev-demo-web-<version>` artifact，且该 skip 必须可见。

## 验收标准（Acceptance Criteria）

- Given `bun run --cwd web build:demo`，When 检查产物，Then 存在 `build-info.json`（`variant=demo`）、`edgeone.json`，且无 `demo/` 目录、`index.html` 无 `/demo/` 前缀。
- Given 组装后的 demo 站点，When 打开 `/`、`/services`、`/queue`、`/settings`、`/settings/ghcr-webhooks`、`/cleanup`、`/deploy-check`，Then 均按 mock 渲染且无 `unhandled mock route`、无启动失败面。
- Given demo 站点，When 读取 `/api/version`，Then 返回值等于 `build-info.json` 的 `version`。
- Given 非 release 构建，When 查看侧栏版本，Then 不生成 GitHub release 链接。
- Given `/queue?demo=queue`，When 点击 Inspector 的 Cleanup 场景，Then URL 变为 `/cleanup?demo=cleanup` 且文档未重载。
- Given 修改 persona/network，When 刷新页面，Then 状态保留。
- Given `<961px` 视口，When 打开 demo 页面，Then 有 bubble 入口且可展开为 drawer。
- Given mock 安装失败，When 打开 demo，Then 出现“Web Demo 启动失败”错误面而不是白屏或真实请求。

## Tasks

- [x] build-time 版本（`buildInfo.ts`、vite define、`build-info.json`）
- [x] 独立站点构建（`--mode demo`、`dist-demo`、`edgeone.json`）
- [x] 移除 docs 站 demo 组装、404 桥与 `/demo` 链接
- [x] scene registry / share state / client-side 场景切换
- [x] Inspector（桌面浮窗 + bubble + 移动 drawer + 模拟写入）
- [x] bootstrap 失败面与拦截断言
- [x] mock fixture 下沉到 `web/src/mock/**`
- [x] Release 驱动的 EdgeOne 发布脚本与 CI 校验
- [x] `web/e2e/demo-site.spec.ts` 与 site smoke
- [x] 发布门控与 owner 侧前提文档化
- [ ] EdgeOne 项目、域名与证书首配（owner-side）
- [ ] 视觉证据重拍为 demo 站点真 surface

## 验证

```bash
bun --cwd web test
bun --cwd web lint
DOCKREV_BUILD_VERSION=0.0.0-pr.$(git rev-parse --short HEAD) DOCKREV_BUILD_CHANNEL=pr bun --cwd web build:demo
node ./web/scripts/demo-site-smoke.mjs web/dist-demo
DEMO_E2E_DIST=dist-demo bun --cwd web e2e
python3 .github/scripts/test_docs_pages_workflow_contract.py
python3 .github/scripts/test_release_workflow_contract.py
python3 .github/scripts/test_workflow_failure_notification_contract.py
```

## 视觉证据

- 采集方式：demo 站点 true surface（`ui_demo`），使用 `?demo=<scene>&d_controls=hidden` 取得干净页面态；Inspector 证据额外补一张展开态。
- 待拍状态：`/`、`/services`、`/services/stack-prod/svc-prod-api/history`、`/cleanup`、Inspector 桌面浮窗、Inspector 移动 drawer。

## 变更记录（Change log）

- 2026-11-25：创建规格，取代 `/demo/` 子路径方案；冻结独立站点、build-time 版本、URL-first 场景与 mock-only 发布边界。
