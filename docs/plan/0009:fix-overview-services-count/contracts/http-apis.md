# HTTP APIs（Dockrev Overview services count / #0009）

本文件冻结本计划涉及的 HTTP API 契约（本计划不修改服务端行为；仅补齐“前端消费契约”，避免字段误用导致回归）。

## Auth（统一约束）

- 继承现有 Dockrev forward-header 鉴权策略：未通过鉴权返回 `401`。

## List stacks（GET /api/stacks）

- Method: `GET`
- Path: `/api/stacks`
- Scope: external
- Change: None（本计划不改服务端；仅补齐契约）
- Auth: required（forward header）

### Response（200 OK）

Top-level:

- `stacks`: `StackListItem[]`

`StackListItem` (camelCase):

- `id`: string
- `name`: string
- `status`: `'healthy' | 'degraded' | 'unknown'`（或等价枚举）
- `services`: number
  - 含义：该 stack 下 **services 总数（包含 archived）**
- `archivedServices?`: number
  - 含义：该 stack 下已归档 services 数量（`archivedServices <= services`）
- `updates`: number
- `lastCheckAt`: string（RFC3339）
- `archived?`: boolean

### Notes（关键口径）

- `services` 是聚合计数（number），不是数组；前端禁止对其取 `.length`。

## Get stack（GET /api/stacks/{stack_id}）

- Method: `GET`
- Path: `/api/stacks/{stack_id}`
- Scope: external
- Change: None（本计划不改服务端；仅补齐契约）
- Auth: required（forward header）

### Response（200 OK）

Top-level:

- `stack`: `StackDetail`

`StackDetail` (camelCase):

- `id`: string
- `name`: string
- `compose`: object
- `services`: `Service[]`
  - 含义：该 stack 下的 services 明细数组（长度与 `GET /api/stacks[].services` 对应，但类型不同）
- `archived?`: boolean

### Notes（关键口径）

- `GET /api/stacks` 与 `GET /api/stacks/{id}` 中 `services` 同名但类型不同：
  - list: `services:number`（聚合计数）
  - detail: `services:Service[]`（明细数组）
  前端必须使用不同的类型（例如 `StackListItem` vs `StackDetail`）来显式区分。

