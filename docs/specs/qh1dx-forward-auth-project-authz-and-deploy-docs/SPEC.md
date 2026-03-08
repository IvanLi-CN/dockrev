# Dockrev：Forward Auth 项目鉴权 + 部署文档补齐（#qh1dx）

## 状态

- Status: 已完成
- Created: 2026-03-07
- Last: 2026-03-08
- Notes: fast-track（review-loop 已收敛；鉴权边界 + 部署文档同步）

## 背景 / 问题陈述

- Dockrev 需要明确区分“代理层认证”和“项目侧鉴权”：Traefik / Authelia 负责 Forward Auth 认证，Dockrev 自身负责基于用户或组做授权决策。
- 现有部署文档需要给出可直接复用的 Traefik + Authelia 示例，并明确 webhook 公开访问的推荐分流方式。
- review-loop 进一步暴露出几个阻塞点：allowlist 配置后仍可能被匿名开发旁路绕过、未授权的 deploy-check 仍会执行重型 preflight、settings/supervisor 的边界兼容性不足。

## 目标 / 非目标

### Goals

- Dockrev API 与 Supervisor 支持单值 `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP`，同时配置时按“任意命中即可通过”授权。
- 允许开发模式保留匿名访问，但一旦配置允许用户或组，匿名开发旁路必须自动失效。
- 未授权请求返回可用于前端跳转和诊断的鉴权细节；未授权访问 deploy-check 时只返回 auth-only report，不执行完整 preflight。
- 设置页、未授权页、自检页能展示与当前鉴权状态一致的信息；`/supervisor` 与 `/supervisor/` 都可访问。
- 文档统一使用 `Forward Auth` 名称，并补齐 Traefik + Authelia 部署示例与验证说明。

### Non-goals

- 不把 webhook 的公开例外下沉到 Authelia `bypass` 规则；仍以 Traefik 路由分流为主。
- 不新增多用户、多组、RBAC 或角色层级模型。
- 不改 Supervisor 的升级状态机与核心执行流程。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/authz.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `crates/dockrev-api/src/config.rs`
- `crates/dockrev-supervisor/src/app.rs`
- `crates/dockrev-supervisor/src/config.rs`
- `crates/dockrev-supervisor/Cargo.toml`
- `web/src/api.ts`
- `web/src/App.tsx`
- `web/src/pages/SettingsPage.tsx`
- `web/src/pages/UnauthorizedPage.tsx`
- `deploy/examples/traefik-authelia/**`
- `deploy/README.md`
- `README.md`
- `docs-site/docs/deploy.md`
- `docs-site/docs/zh/deploy.md`
- `docs-site/docs/config.md`
- `docs-site/docs/zh/config.md`
- `docs-site/docs/en/config.md`
- `docs/specs/README.md`

### Out of scope

- 默认 CI / release workflow 的触发策略调整。
- 新增多用户、多组、RBAC 或角色层级模型。
- 修改 Authelia / Traefik 上游产品本身的能力边界。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` | Env config | external | Modify | None | api + supervisor | 运维 / 反向代理接入方 | 单值 allowlist；同时配置时命中任意一个即可 |
| `GET /api/settings` auth payload | HTTP API | external | Modify (backward-compatible) | None | api | Web UI / 运维 | 返回真实 `authorizationMode`，稳定序列化 `currentGroups` |
| `GET /api/deploy-check/report` | HTTP API | external | Modify (behavioral) | None | api | Web UI / 运维 | 未授权时仅返回 auth-only report |
| `GET /supervisor` / `GET /supervisor/` | HTML UI | external | Modify (compatibility) | None | supervisor | 操作员 | 两种根路径都可访问 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 同时配置 `DOCKREV_AUTH_ALLOWED_USER=alice` 与 `DOCKREV_AUTH_ALLOWED_GROUP=ops`，When 当前请求用户命中任一条件，Then Dockrev 允许访问。
- Given 已配置 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`，When 请求未携带可信 Forward Auth 身份头，Then API 与 Supervisor 都拒绝请求，即使 `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=true`。
- Given 未授权访问 `GET /api/deploy-check/report`，When 服务生成响应，Then 返回只包含鉴权检查项的 report，且不会执行完整 preflight。
- Given 当前请求没有组头，When 设置页读取鉴权信息，Then `currentGroups` 仍稳定为 `[]`，前端不会因为缺字段崩溃。
- Given Supervisor 运行在 `/supervisor` base path，When 访问 `/supervisor` 或 `/supervisor/`，Then 都返回可用 UI。
- Given 使用仓库提供的 Traefik + Authelia 示例，When 部署 Dockrev，Then 文档统一使用 `Forward Auth` 名称，并清晰说明：Traefik / Authelia 负责认证，Dockrev 负责鉴权，webhook 通过 Traefik 路由分流公开。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api authz -- --nocapture`
- `cargo test -p dockrev-api deploy_check_report_skips_preflight_when_request_is_unauthorized -- --nocapture`
- `cargo test -p dockrev-api settings_auth_serializes_empty_current_groups -- --nocapture`
- `cargo test -p dockrev-supervisor auth -- --nocapture`
- `cargo test -p dockrev-supervisor non_empty_trims_whitespace -- --nocapture`
- `bun run docs:build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: API / Supervisor 接入用户或组 allowlist，并在请求失败时返回鉴权诊断信息。
- [x] M2: allowlist 配置后自动禁用匿名开发旁路，补齐相关回归测试。
- [x] M3: deploy-check 未授权路径改为 auth-only report，避免无谓 preflight。
- [x] M4: 修复 `/supervisor/` 根路径兼容性，并补齐设置页鉴权状态序列化边界。
- [x] M5: 补齐 Traefik + Authelia `Forward Auth` 部署示例、文档与验证说明。
- [x] M6: 通过 review-loop 收敛剩余阻塞项与相关验证。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：Traefik / Authelia 按文档注入受信任的用户/组头，且 webhook 公开入口由 Traefik 路由层分流。
- 风险：若代理未注入受信任身份头，allowlist 模式下所有请求都会表现为未授权，这是预期的 fail-closed 行为。

## 变更记录（Change log）

- 2026-03-07: 创建规格，冻结 Forward Auth 项目鉴权、部署文档与验收标准的范围。
- 2026-03-07: 完成 API / Supervisor allowlist、匿名旁路收口、deploy-check auth-only report、`/supervisor/` 兼容、配置/文档同步与 review-loop 收敛。
