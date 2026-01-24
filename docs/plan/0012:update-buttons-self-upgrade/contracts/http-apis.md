# Supervisor HTTP API

目标：提供独立于 Dockrev 生命周期的“自我升级”控制面与可轮询状态；在 Dockrev 重启窗口内仍可用。

说明：

- 本计划将该 API 视为 **supervisor/agent** 的外部接口。Dockrev API 不代理该 API（避免 Dockrev 重启期间不可用）。
- 部署建议：通过反代将同域的 `/supervisor/` 路由到 supervisor（base path 可配置，见 `contracts/config.md`）。

## Auth（鉴权）

- 复用 forward header 方案（与 Dockrev 相同的前置反代），除非 endpoint 显式标注 `Auth: none`，否则默认要求已登录用户。
- Header 名称：`X-Forwarded-User`（可配置，需与 Dockrev 一致）。
-
- `Auth: none` 的 endpoints 仅用于“可用性探测/版本展示”，仍应部署在与 Dockrev 同域/同反代之后（避免公网暴露）。

## Health（GET /supervisor/health）

- Scope: external
- Change: New
- Auth: none

Response:

```json
{ "ok": true }
```

Notes:

- Dockrev UI 使用此 endpoint 进行“supervisor 可用性探测”，以决定是否允许跳转到自我升级页面。
- 若出现 `401`（不应发生，因 `Auth: none`）：视为反代/路由异常，不应当作“offline”吞掉。

## Version（GET /supervisor/version）

- Scope: external
- Change: New
- Auth: none

Response:

```json
{ "version": "x.y.z" }
```

## Self-upgrade status（GET /supervisor/self-upgrade）

- Scope: external
- Change: New
- Auth: forward header

Response:

```json
{
  "state": "idle|running|succeeded|failed|rolled_back",
  "opId": "sup_...",
  "target": { "image": "ghcr.io/ivanli-cn/dockrev", "tag": "1.2.3", "digest": "sha256:..." },
  "previous": { "tag": "1.2.2", "digest": "sha256:..." },
  "startedAt": "2026-01-24T00:00:00Z",
  "updatedAt": "2026-01-24T00:00:00Z",
  "progress": {
    "step": "precheck|pull|apply|wait_healthy|postcheck|rollback|done",
    "message": "..."
  },
  "logs": [
    { "ts": "2026-01-24T00:00:00Z", "level": "INFO|WARN|ERROR", "msg": "..." }
  ]
}
```

Errors:

- `401/auth_required`

## Start self-upgrade（POST /supervisor/self-upgrade）

- Scope: external
- Change: New
- Auth: forward header

Notes:

- 必须幂等（为 retry/刷新而设计）：
  - 当已有 `state=running`，且本次请求参数与当前 running 的操作一致时：返回 `200` + 相同 `opId`（不重复启动）。
  - 当已有 `state=running`，但本次请求试图变更目标（target/mode/rollbackOnFailure）时：返回 `409/conflict`，并在错误信息中提示“已有运行中的 self-upgrade，请等待完成或先回滚/结束后再发起”。

Request body:

```json
{
  "target": {
    "tag": "latest|<semver>|<custom>",
    "digest": "sha256:... (optional)"
  },
  "mode": "apply|dry-run",
  "rollbackOnFailure": true
}
```

Response:

```json
{ "opId": "sup_..." }
```

Errors:

- `400/invalid_argument`
- `401/auth_required`
- `409/conflict`（已有 running 且请求参数不一致）

## Rollback self-upgrade（POST /supervisor/self-upgrade/rollback）

- Scope: external
- Change: New
- Auth: forward header

Request body:

```json
{ "opId": "sup_..." }
```

Response:

```json
{ "ok": true }
```

Errors:

- `400/invalid_argument`
- `401/auth_required`
- `409/conflict`（不可回滚的状态）

## Static UI（GET /supervisor/）

- Scope: external
- Change: New
- Auth: forward header (recommended)

Notes:

- 返回自我升级页面（HTML + 静态资源），页面内部通过本文件 API 轮询状态。
