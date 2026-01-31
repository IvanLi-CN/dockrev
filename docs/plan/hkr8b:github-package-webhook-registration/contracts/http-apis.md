# HTTP API

本计划新增的 API 均为内部接口（UI → Dockrev API），鉴权沿用现有 Forward Header 机制。

## Resolve target（POST /api/github-packages/resolve）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: forward header（+ 管理员）

### 请求（Request）

- Body:
  - `input`: string（repo URL / profile URL / username）

### 响应（Response）

- Success:
  - `kind`: `"repo"` \| `"owner"`
  - `owner`: string
  - `repos`: `{ fullName: string, selected: boolean }[]`（默认全选）
  - `warnings`: string[]
- Error:
  - `400`: 输入无法解析
  - `401`: 鉴权失败
  - `422`: PAT 缺失或权限不足导致无法列出 repos

### 示例（Examples）

- Request:
  - `{ "input": "https://github.com/acme/widgets" }`
- Response:
  - `{ "kind": "repo", "owner": "acme", "repos": [{"fullName":"acme/widgets","selected":true}], "warnings":[] }`

## Get settings（GET /api/github-packages/settings）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: forward header（+ 管理员）

### 响应（Response）

- Success:
  - `enabled`: boolean
  - `targets`: `{ input: string, kind: "repo"|"owner", owner: string, warnings: string[] }[]`
  - `callbackUrl`: string
  - `repos`: `{ fullName: string, selected: boolean, hookId?: number|null, lastSyncAt?: string|null, lastError?: string|null }[]`
  - `patMasked`: `"******"` \| null
  - `secretMasked`: `"******"` \| null
- Error:
  - `401`: 鉴权失败

## Put settings（PUT /api/github-packages/settings）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: forward header（+ 管理员）

### 请求（Request）

- Body:
  - `enabled`: boolean
  - `targets`: `{ input: string }[]`
  - `callbackUrl`: string
  - `repos`: `{ fullName: string, selected: boolean }[]`
  - `pat`: string?（若为 `"******"` 表示不变）

### 响应（Response）

- Success: `{ "ok": true }`
- Error:
  - `400`: callbackUrl 非法 / repos 为空等校验失败
  - `401`: 鉴权失败

## Sync webhooks（POST /api/github-packages/sync）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: forward header（+ 管理员）

### 请求（Request）

- Body:
  - `dryRun`: boolean?（default: false）
  - `resolveConflicts`: `{ repo: string, keepHookId: number, deleteHookIds: number[] }[]`?（可选；仅在用户确认后使用）

### 响应（Response）

- Success:
  - `ok`: boolean
  - `results`: `{ repo: string, action: "noop"|"created"|"updated"|"conflict"|"error", hookId?: number|null, conflictHooks?: { id: number, url: string, events: string[], active: boolean }[], message?: string }[]`

### 备注（Notes）

- 幂等匹配规则：以 GitHub 已存在 webhook 的 `config.url == callbackUrl` 且 `events` 包含 `package` 为主；若匹配到多个返回 `conflict`。

## Receiver（POST /api/webhooks/github-packages）

- 范围（Scope）: external
- 变更（Change）: New
- 鉴权（Auth）: GitHub webhook signature（`X-Hub-Signature-256`）

### 请求（Request）

- Headers:
  - `X-GitHub-Event`: must be `package`
  - `X-GitHub-Delivery`: uuid
  - `X-Hub-Signature-256`: `sha256=<hmac>`
- Body: GitHub `package` event payload（本计划只处理 `action=published`）

### 响应（Response）

- Success: `{ "ok": true }`
- Error:
  - `401`: signature invalid
  - `202`: accepted-but-ignored（事件不匹配/未选中 repo，可选）
