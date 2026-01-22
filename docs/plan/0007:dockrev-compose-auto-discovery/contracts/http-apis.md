# HTTP APIs（Dockrev Auto-Discovery / #0007）

本文件定义本计划涉及的 HTTP 接口契约（仅增量变更；不影响既有鉴权边界）。

## Auth（统一约束）

- 继承现有 Dockrev forward header 鉴权：未通过鉴权返回 `401`。
- 本计划不得引入匿名访问的“特殊例外”。

## `POST /api/discovery/scan`

触发一次“立即发现/重新同步”扫描（UI 按钮入口）。

- Method: `POST`
- Path: `/api/discovery/scan`
- Auth: required（forward header）
- Request:
  - Body: empty JSON `{}`（保留将来扩展空间）
- Response (`200 OK`):
  - `startedAt` (string, RFC3339)
  - `durationMs` (number)
  - `summary`:
    - `projectsSeen` (number)
    - `stacksCreated` (number)
    - `stacksUpdated` (number)
    - `stacksSkipped` (number)
    - `stacksFailed` (number)
    - `stacksMarkedMissing` (number)
  - `actions` (array, max N 条；按 config 限制)：
    - `project` (string)
    - `action` (`created` | `updated` | `skipped` | `failed` | `marked_missing`)
    - `stackId` (string, optional)
    - `reason` (string, optional; human-readable)
    - `details` (object, optional; machine-readable, best-effort)
- Errors:
  - `401 Unauthorized`: 未通过鉴权
  - `500 Internal Server Error`: 扫描过程发生不可恢复错误（应尽量降级为部分失败并在 `actions` 里体现）

Example response:

```json
{
  "startedAt": "2026-01-22T12:00:00Z",
  "durationMs": 148,
  "summary": {
    "projectsSeen": 3,
    "stacksCreated": 1,
    "stacksUpdated": 0,
    "stacksSkipped": 1,
    "stacksFailed": 1,
    "stacksMarkedMissing": 0
  },
  "actions": [
    { "project": "blog", "action": "created", "stackId": "st_01H..." },
    { "project": "db", "action": "skipped", "reason": "config_files_relative_path_rejected" },
    { "project": "srv", "action": "failed", "reason": "compose_file_unreadable" }
  ]
}
```

## `GET /api/discovery/projects`

获取当前“已发现的 compose projects（含未注册为 stack 的项目）”列表，用于 UI 将“无 Stack 的项目”聚合到一个组内展示。

- Method: `GET`
- Path: `/api/discovery/projects`
- Auth: required（forward header）
- Query (optional):
  - `archived`: `exclude|include|only` (default: `exclude`)
- Response (`200 OK`):
  - `projects` (array):
    - `project` (string) — `com.docker.compose.project`
    - `status` (`active` | `missing` | `invalid`)
    - `stackId` (string, optional) — 若已关联 stack 则返回
    - `configFiles` (string[], optional) — 规范化后的绝对路径列表（仅在可信/可展示时返回；不可读时可省略）
    - `lastSeenAt` (string, RFC3339, optional)
    - `lastScanAt` (string, RFC3339, optional)
    - `lastError` (string, optional)
    - `archived` (boolean) — 是否被归档（UI 默认不展示 archived 项）
- Errors:
  - `401 Unauthorized`: 未通过鉴权

Example response:

```json
{
  "projects": [
    {
      "project": "blog",
      "status": "active",
      "stackId": "st_01H...",
      "archived": false
    },
    {
      "project": "srv",
      "status": "invalid",
      "lastError": "compose_file_unreadable: /home/ivan/srv/srv/compose.yml (mount missing?)",
      "archived": false
    }
  ]
}
```

## `POST /api/discovery/projects/{project}/archive`

将某个 discovered project 归档（archive）。

- Method: `POST`
- Path: `/api/discovery/projects/{project}/archive`
- Auth: required（forward header）
- Response:
  - `204 No Content`
- Errors:
  - `401 Unauthorized`
  - `404 Not Found`：该 `project` 不存在（或已被清理）

Notes:

- `archived=true` 后该项目默认不在 UI 展示（除非用户开启“显示已归档”）。
- 自动恢复：当扫描观察到 `status=active` 时，服务端应将 `archived=false`（并清理/更新 `archived_reason`）。

## `POST /api/discovery/projects/{project}/restore`

手动恢复已归档的 discovered project。

- Method: `POST`
- Path: `/api/discovery/projects/{project}/restore`
- Auth: required（forward header）
- Response:
  - `204 No Content`
- Errors:
  - `401 Unauthorized`
  - `404 Not Found`

## `POST /api/stacks`（Delete: manual registration）

本计划实现后不再支持手动注册 stacks（以避免“手动口径/自动口径”并存）。

- Before: `POST /api/stacks` 创建 stack（手动提供 composeFiles）
- After: `POST /api/stacks` 返回 `405 Method Not Allowed`（或等价的不可用响应）

## `GET /api/stacks`（Modify: add archived filter; additive fields optional）

用于列出 stacks；默认不返回已归档 stacks，以保持主视图整洁。

- Method: `GET`
- Path: `/api/stacks`
- Auth: required（forward header）
- Query (optional):
  - `archived`: `exclude|include|only` (default: `exclude`)
- Response: existing `ListStacksResponse` (same top-level shape)
- Additive fields (optional):
  - `StackListItem.archived?: boolean`
  - `StackListItem.archivedServices?: number` — 该 stack 下已归档 services 数量（用于“归档箱”按 stack 成组展示）

Rules:

- `archived=exclude`：只返回未归档 stacks（主视图默认）。
- `archived=only`：只返回已归档 stacks（“已归档”视图数据源）。
- `archived=include`：返回全部（用于调试/管理视图）。

## `GET /api/stacks/{stack_id}`（Modify: service archived flag optional）

- Method: `GET`
- Path: `/api/stacks/{stack_id}`
- Auth: required（forward header）

Additive fields (optional):

- `StackResponse.archived?: boolean`
- `Service.archived?: boolean`

Notes:

- 对已归档的 stack：推荐仍允许读取详情（用于恢复页/排障），并通过 `archived` 字段表达状态。
- 通知策略（归档对象）：
  - 归档的 stack/service 仍参与 check/update 计算与计数展示
  - 但归档对象不得触发通知发送（email/webhook/telegram/webpush 等）

## `POST /api/stacks/{stack_id}/archive` / `POST /api/stacks/{stack_id}/restore`

归档/恢复 stack（不做硬删除）。

- Method: `POST`
- Path:
  - `/api/stacks/{stack_id}/archive`
  - `/api/stacks/{stack_id}/restore`
- Auth: required（forward header）
- Response: `204 No Content`
- Errors:
  - `401 Unauthorized`
  - `404 Not Found`

Rules:

- 归档的 stack 默认从主视图隐藏，且默认不参与 check/update 统计（具体以 `PLAN.md` 决策为准）。

## `POST /api/services/{service_id}/archive` / `POST /api/services/{service_id}/restore`

归档/恢复 service（不做硬删除）。

- Method: `POST`
- Path:
  - `/api/services/{service_id}/archive`
  - `/api/services/{service_id}/restore`
- Auth: required（forward header）
- Response: `204 No Content`
- Errors:
  - `401 Unauthorized`
  - `404 Not Found`
