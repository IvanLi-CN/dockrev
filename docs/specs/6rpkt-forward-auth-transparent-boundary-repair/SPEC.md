# Dockrev：Forward Auth 透明透传边界修补（#6rpkt）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-03-08
- Last: 2026-03-08
- Notes: fast-track（follow-up to #qh1dx；修正 PR #146 的网关拦截口径）

## 背景 / 问题陈述

- PR #146 将 Dockrev 的 Forward Auth 部署口径收紧成“网关先拦 + Dockrev 再兜底”，把 Traefik + Authelia 示例、匿名 deploy-check 回退，以及前端全局 401 跳转绑在一起。
- 主人要求恢复为“网关只负责身份透传，Dockrev 自己负责功能模块授权”的模型：网关不得承担用户/组/路径 ACL，访问限制必须由项目内部决定。
- 当前仓库已经明确区分了外部应用接口（如 webhook）与业务面 API/UI，但 PR #146 额外引入的 auth-only deploy-check 与 `/unauthorized` 全局跳转，破坏了这条边界。

## 目标 / 非目标

### Goals

- 保留 `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` 单值 allowlist，但仅用于受保护业务面与 `/supervisor/*` 的项目内授权，不影响公共接口是否可达。
- 明确公共匿名面仅限 `/api/health`、`/api/version`、`/api/webhooks/*`；`/api/deploy-check/report`、业务 API、业务页面、`/supervisor/*` 都必须返回 Dockrev 自身生成的 `401 auth_required`。
- 移除 PR #146 引入的匿名 `deploy-check` report 回退与前端 `/deploy-check` / `/unauthorized` 全局跳转，让 401 在当前受保护模块原位显示统一诊断。
- 重写 Traefik + Authelia 示例与部署文档，改为“所有业务路由直通后端、网关只做身份透传、不用网关 ACL 或 webhook 分流表达权限边界”。
- 保持 `/supervisor` 与 `/supervisor/` 双入口兼容，同时将 `/supervisor/health` / `/supervisor/version` 纳入受保护边界。

### Non-goals

- 不新增多角色/RBAC/多值 allowlist 模型。
- 不修改 webhook 的 secret / signature 校验语义。
- 不改动 Authelia 产品本身；仓库只表达 Dockrev 所要求的透明透传接法与边界。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `crates/dockrev-supervisor/src/app.rs`
- `web/src/App.tsx`
- `web/src/api.ts`
- `web/src/routes.ts`
- `web/src/pages/UnauthorizedPage.tsx`
- `deploy/examples/traefik-authelia/**`
- `deploy/README.md`
- `README.md`
- `docs-site/docs/**/deploy.md`
- `docs-site/docs/**/api-reference.md`
- `docs-site/docs/**/config.md`
- `docs/specs/README.md`

### Out of scope

- 新增独立匿名页面。
- 扩展新的外部匿名 API。
- 修改 `docs/specs/qh1dx-forward-auth-project-authz-and-testbox-e2e/SPEC.md` 的已完成结论，只允许通过 follow-up spec 纠偏。

## 接口与边界契约

- 公共匿名 API：`GET /api/health`、`GET /api/version`、`POST /api/webhooks/trigger`、`POST /api/webhooks/github-packages`
- 受保护 API：除上述外的全部 `/api/**`，包括 `GET /api/deploy-check/report`
- 受保护 UI：全部 SPA 业务路由（概览、服务、队列、设置、部署检查等）；静态资源匿名可读，但业务数据必须经 Dockrev 授权
- 受保护 Supervisor：`/supervisor`、`/supervisor/`、`/supervisor/health`、`/supervisor/version`、`/supervisor/self-upgrade*`
- 网关责任：路由转发 + 可信身份头透传
- Dockrev 责任：用户/组命中判断、匿名公共面判定、业务模块授权结果与诊断响应

## 验收标准（Acceptance Criteria）

- Given 匿名请求 `GET /api/health` 或 `GET /api/version`，When 请求到达 Dockrev，Then 返回 `200`。
- Given 匿名请求 `GET /api/stacks`、`GET /api/settings` 或 `GET /api/deploy-check/report`，When 未携带可信身份头，Then 返回 `401 auth_required`，而不是匿名 `200` deploy-check 报告。
- Given 匿名请求 `GET /supervisor/health`、`GET /supervisor/version` 或 `GET /supervisor/self-upgrade`，When 请求未命中 Dockrev 内部授权，Then 返回 `401`。
- Given 已透传且命中 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP` 的身份，When 访问业务 API、业务 UI 与 `/supervisor/*`，Then Dockrev 允许访问。
- Given 已透传但未命中 allowlist 的身份，When 访问受保护页面/API，Then 当前页面原位显示统一 auth-required 诊断，不再跳转 `/deploy-check` 或 `/unauthorized`。
- Given 使用仓库提供的 Traefik + Authelia 示例，When 阅读部署配置与说明，Then 不再出现“网关负责访问限制”“webhook 通过路由分流公开”“one_factor 保护全部业务路由”等旧口径。

## 里程碑（Milestones / checklist）

- [x] M1: 回收 API 的 auth-only deploy-check 回退，恢复受保护边界。
- [x] M2: 收紧 supervisor `/health` / `/version` 到同一内部授权模型，并保持 `/supervisor` / `/supervisor/` 兼容。
- [x] M3: 删除前端全局 401 跳转，改为当前受保护模块原位显示统一鉴权诊断。
- [x] M4: 重写 Traefik + Authelia 示例、README 与 docs-site 文档，统一成透明身份透传口径。
- [ ] M5: 通过 Rust/Web 验证、shared-testbox 验证、浏览器验证，并完成快车道 PR 收敛。

## 风险 / 假设

- 假设：主人确认 Authelia 可支撑本仓库要求的透明身份透传接法，因此文档与示例以该能力为前提。
- 风险：若旧部署已依赖 PR #146 的网关 ACL / webhook 分流语义，本次需要同步调整部署配置。
- 风险：SPA 静态入口保持匿名可加载时，必须确保任何业务数据仍由受保护 API 控制，不能在首屏 HTML/JS 中泄露受保护信息。

## 变更记录（Change log）

- 2026-03-08: 完成 API / Supervisor 受保护边界回收、前端原位 401 诊断、Traefik + Authelia 透明透传示例重写，以及本地 + shared-testbox + 浏览器验证。
- 2026-03-08: 新建 follow-up spec，冻结 Forward Auth 透明透传修补边界，专门修正 PR #146 引入的网关访问控制口径。
