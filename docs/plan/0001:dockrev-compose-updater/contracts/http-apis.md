# HTTP API

约定：

- Base URL: `/api`
- Content-Type: `application/json; charset=utf-8`
- Auth: Forward Header（单用户）
  - 默认要求 `X-Forwarded-User`（名字可配置，见开放问题）
  - header 缺失时返回 `401`

错误返回统一形状：

```json
{
  "error": {
    "code": "string",
    "message": "string",
    "details": {}
  }
}
```

## Health（GET /api/health）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: none

### 响应（Response）

- Success: `text/plain`，内容为 `ok`

## List stacks（GET /api/stacks）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

### 响应（Response）

Success:

```json
{
  "stacks": [
    {
      "id": "stk_...",
      "name": "string",
      "status": "healthy|degraded|unknown",
      "services": 12,
      "updates": 3,
      "lastCheckAt": "2026-01-18T00:00:00Z"
    }
  ]
}
```

Errors:

- `401/auth_required`

## Get stack（GET /api/stacks/{stackId}）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

### 响应（Response）

Success（核心字段；可增量扩展）：

```json
{
  "stack": {
    "id": "stk_...",
    "name": "string",
    "compose": {
      "type": "path",
      "composeFiles": ["/srv/compose/app/docker-compose.yml"],
      "envFile": "/srv/compose/app/.env"
    },
    "services": [
      {
        "id": "svc_...",
        "name": "string",
        "image": {
          "ref": "ghcr.io/org/app:5.2",
          "tag": "5.2",
          "digest": "sha256:..."
        },
        "candidate": {
          "tag": "5.3",
          "digest": "sha256:...",
          "archMatch": "match|mismatch|unknown",
          "arch": ["linux/amd64", "linux/arm64"]
        },
        "ignore": {
          "matched": true,
          "ruleId": "ign_...",
          "reason": "string"
        },
        "settings": {
          "autoRollback": true,
          "backupTargets": {
            "mode": "allowlist|denylist",
            "bindPaths": ["/var/lib/postgresql/data"],
            "volumeNames": ["app_db_data"]
          }
        }
      }
    ]
  }
}
```

Errors:

- `401/auth_required`
- `404/not_found`

## Register stack（POST /api/stacks）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

### 请求（Request）

Body:

```json
{
  "name": "string",
  "compose": {
    "type": "path",
    "composeFiles": ["/srv/compose/app/docker-compose.yml"],
    "envFile": "/srv/compose/app/.env"
  },
  "backup": {
    "targets": [
      {
        "kind": "docker-volume",
        "name": "app_db_data"
      },
      {
        "kind": "bind-mount",
        "path": "/srv/data/app"
      }
    ],
    "retention": {
      "keepLast": 1,
      "deleteAfterStableSeconds": 3600
    }
  }
}
```

Validation:

- `composeFiles` 至少 1 个
- 路径必须是绝对路径（容器内路径）

### 响应（Response）

- `201` + `{"stackId":"stk_..."}`

Errors:

- `400/invalid_argument`
- `401/auth_required`

## Trigger check（POST /api/checks）

用途：刷新候选版本（不执行更新）。

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

Body:

```json
{
  "scope": "service|stack|all",
  "stackId": "stk_...",
  "serviceId": "svc_...",
  "reason": "ui|webhook|schedule"
}
```

Response:

```json
{ "checkId": "chk_..." }
```

Errors:

- `400/invalid_argument`
- `401/auth_required`

## Trigger update（POST /api/updates）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

Body:

```json
{
  "scope": "service|stack|all",
  "stackId": "stk_...",
  "serviceId": "svc_...",
  "mode": "apply|dry-run",
  "allowArchMismatch": false,
  "backupMode": "inherit|skip|force",
  "reason": "ui|webhook|schedule"
}
```

Response:

```json
{ "jobId": "job_..." }
```

Errors:

- `400/invalid_argument`
- `401/auth_required`
- `409/conflict`（stack 正在更新）

## Jobs（GET /api/jobs）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

Response（列表）：

```json
{
  "jobs": [
    {
      "id": "job_...",
      "type": "check|update|rollback",
      "scope": "service|stack|all",
      "status": "queued|running|success|failed|rolled_back",
      "createdAt": "2026-01-18T00:00:00Z",
      "startedAt": "2026-01-18T00:00:00Z",
      "finishedAt": "2026-01-18T00:00:00Z"
    }
  ]
}
```

## Job detail（GET /api/jobs/{jobId}）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

Response（含日志片段）：

```json
{
  "job": {
    "id": "job_...",
    "status": "queued|running|success|failed|rolled_back",
    "summary": {
      "changedServices": 1,
      "oldDigests": {"svc_...": "sha256:..."},
      "newDigests": {"svc_...": "sha256:..."}
    },
    "logs": [
      {"ts": "2026-01-18T00:00:00Z", "level": "info", "msg": "string"}
    ]
  }
}
```

## Ignore rules（GET/POST/DELETE /api/ignores）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

Rule shape（统一）：

```json
{
  "id": "ign_...",
  "enabled": true,
  "scope": { "type": "service", "serviceId": "svc_..." },
  "match": {
    "kind": "exact|prefix|regex|semver",
    "value": "string"
  },
  "note": "string"
}
```

## Service settings（GET/PUT /api/services/{serviceId}/settings）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

GET response:

```json
{
  "autoRollback": true,
  "backupTargets": {
    "mode": "allowlist|denylist",
    "bindPaths": ["/var/lib/postgresql/data"],
    "volumeNames": ["app_db_data"]
  }
}
```

PUT request:

```json
{
  "autoRollback": false,
  "backupTargets": {
    "mode": "denylist",
    "bindPaths": ["/srv/media"],
    "volumeNames": ["big_cache"]
  }
}
```

Notes:

- `autoRollback=true` 仅在存在 healthcheck 时才生效；无 healthcheck 的服务将被强制视为不可自动回滚（UI 需提示原因）。
- `backupTargets` 对 bind mounts + docker named volumes 生效：
  - bind mounts（host path 前缀匹配）：
    - `mode=allowlist`: 仅允许 `bindPaths` 覆盖的 bind mount source path 参与备份
    - `mode=denylist`: 禁止 `bindPaths` 覆盖的 bind mount source path 参与备份
  - docker named volumes（volume name 精确匹配）：
    - `mode=allowlist`: 仅允许 `volumeNames` 覆盖的 volume name 参与备份
    - `mode=denylist`: 禁止 `volumeNames` 覆盖的 volume name 参与备份

## Notifications config（GET/PUT /api/notifications）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

```json
{
  "email": { "enabled": true, "smtpUrl": "..." },
  "webhook": { "enabled": true, "url": "..." },
  "telegram": { "enabled": true, "botToken": "...", "chatId": "..." },
  "webPush": { "enabled": true, "vapidPublicKey": "..." }
}
```

敏感字段（token/密码）：

- 写入时允许提交
- 读取时默认脱敏（例如返回 `******` 或不返回），避免 UI 泄漏

## Web Push subscriptions（POST/DELETE /api/web-push/subscriptions）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

POST body:

```json
{
  "endpoint": "https://...",
  "keys": { "p256dh": "...", "auth": "..." }
}
```

DELETE body:

```json
{ "endpoint": "https://..." }
```

## Webhook trigger（POST /api/webhooks/trigger）

用途：外部系统触发检查/更新（同一契约支持 service/stack/all）。

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: shared secret header（独立于 forward header）

Headers:

- `X-Dockrev-Webhook-Secret: <token>`

Body:

```json
{
  "action": "check|update",
  "scope": "service|stack|all",
  "stackId": "stk_...",
  "serviceId": "svc_...",
  "allowArchMismatch": false,
  "backupMode": "inherit|skip|force"
}
```

Response:

```json
{ "jobId": "job_..." }
```

Errors:

- `401/unauthorized`
- `409/conflict`

## Settings（GET/PUT /api/settings）

用途：系统级默认策略（不含敏感凭据明文）。

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: forward header

GET response:

```json
{
  "backup": {
    "enabled": true,
    "requireSuccess": true,
    "baseDir": "/data/backups",
    "skipTargetsOverBytes": 104857600
  },
  "auth": {
    "forwardHeaderName": "X-Forwarded-User",
    "allowAnonymousInDev": true
  }
}
```

PUT request:

```json
{
  "backup": {
    "enabled": true,
    "requireSuccess": true,
    "baseDir": "/data/backups",
    "skipTargetsOverBytes": 104857600
  }
}
```

Notes:

- `auth.*` 主要由环境变量控制；API 以“只读展示”为主（PUT 不允许修改 auth）。
