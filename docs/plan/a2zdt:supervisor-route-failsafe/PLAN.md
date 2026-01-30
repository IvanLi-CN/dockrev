# Dockrev: /supervisor 路由防呆（兜底页 + API 不可吞）（#a2zdt）

## 状态

- Status: 待实现
- Created: 2026-01-30
- Last: 2026-01-30

## 背景 / 问题陈述

- `/supervisor/` 目录用于 Dockrev 自我升级（由 `dockrev-supervisor` 提供 UI 与 API）。
- 现状：当部署时反代/路由未把 `/supervisor/` 映射到 supervisor（误指向 Dockrev 主服务）时：
  - 浏览器打开 `/supervisor/` 会显示 Dockrev 主 Web（误导用户“看起来正常”）。
  - Dockrev Web 的 supervisor 探测可能误判为在线（因为 `/supervisor/*` 被主 Web SPA fallback 吞掉，返回 200）。
- 期望：出现部署漏配/误配时，应明确提示“这是部署问题”，并给出可执行的排查/修复指引。

## 目标 / 非目标

### Goals

- Dockrev 主服务不得吞掉 supervisor 的 API 路径：当 `/supervisor/*` 实际落到 Dockrev 主服务时，必须返回明确的错误（避免误判“supervisor ok”）。
- Dockrev 主服务在 `/supervisor/`（及 base path 根路径）提供兜底页面，清晰告知：该路径应由 supervisor 提供，并提示如何修复部署映射。

### Non-goals

- 不在本计划内修改 supervisor 的功能/接口。
- 不在本计划内新增复杂的“自动探测反代是否正确”的机制（仅做防呆与可行动提示）。

## 范围（Scope）

### In scope

- Dockrev API（主服务）UI fallback：对 supervisor base path 做特殊处理：
  - supervisor API 路径：返回非 2xx（建议 502）+ 可读错误（JSON）。
  - supervisor base path 根路径：返回兜底 HTML 页面（可用 502 + HTML body）。
- 最小测试覆盖：防止回归（确保 `/supervisor/self-upgrade` 不再被 UI fallback 返回 200）。

### Out of scope

- 反代配置生成/自动修复（仅给指引，不代替运维配置）。

## 需求（Requirements）

### MUST

- 当请求命中 Dockrev 主服务且路径位于 `selfUpgradeUrl` 的同域 path（默认 `/supervisor/`）之下：
  - `GET <base>/self-upgrade` 必须返回非 2xx（建议 `502 Bad Gateway`），并给出可行动错误提示。
  - `GET <base>/health`、`GET <base>/version` 等 supervisor API 路径不得返回 SPA `index.html`。
- 当请求命中 Dockrev 主服务且路径等于 `<base>/`（supervisor base path 根路径）：
  - 必须展示兜底页面，说明“supervisor 路由未正确映射”，并给出排查步骤（含期望响应示例）。
- 文案必须明确区分：
  - Dockrev 主服务（当前正在提供响应）
  - Dockrev supervisor（应提供自我升级 UI/API 的服务）

### SHOULD

- 兜底页面提供：
  - 返回 Dockrev 主站入口（`/`）
  - 一段“如何验证”的命令示例（`curl`）
  - 关键环境变量提示（例如 `DOCKREV_SELF_UPGRADE_URL`、`DOCKREV_SUPERVISOR_BASE_PATH`）

## 验收标准（Acceptance Criteria）

- Given 反代正确把 `/supervisor/` 映射到 supervisor
  When 访问 `/supervisor/` 与 `/supervisor/self-upgrade`
  Then 由 supervisor 正常返回（该计划不影响此行为）
- Given 反代误把 `/supervisor/` 映射到 Dockrev 主服务（或 supervisor 未部署）
  When `GET /supervisor/self-upgrade`
  Then 返回 `502`（或其它非 2xx）且响应体包含可读错误信息（JSON），不会被误判为“supervisor ok”
- Given 同上误配置
  When 浏览器访问 `/supervisor/`
  Then 展示兜底页面，明确提示“部署映射错误”，并给出可行动的修复指引

## 里程碑（Milestones）

- [ ] Dockrev 主服务对 supervisor API 路径返回非 2xx（防止 SPA fallback 吞掉）
- [ ] Dockrev 主服务在 supervisor base path 根路径提供兜底 HTML 页面
- [ ] 补充最小测试，覆盖 `/supervisor/self-upgrade` 不再返回 200

## 风险与开放问题（Risks & Open Questions）

- base path 不一定是 `/supervisor/`（可能通过 `DOCKREV_SELF_UPGRADE_URL` 改为其它路径或绝对 URL）；实现需要从配置中提取同域 path 作为拦截前缀。

