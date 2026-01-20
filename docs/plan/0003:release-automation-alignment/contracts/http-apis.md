# HTTP APIs Contracts（#0003）

本文件定义发布与可观测相关的 HTTP 契约（health/version + 静态 UI 路由）。

## `GET /api/health`

- Scope: external
- Auth: none

### Response (200)

Content-Type: `text/plain`

Body:

```text
ok
```

## `GET /api/version`

- Scope: external
- Auth: none（与 `/api/health` 一致）

### Response (200)

Content-Type: `application/json`

Body:

```json
{
  "version": "0.1.0"
}
```

Fields:

- `version` (string, required): 运行中服务的版本号（与发布版本一致，形如 `x.y.z`）。

## Static UI routes

静态 UI 必须由同一服务端进程提供（嵌入二进制内）。

### `GET /`

- Scope: external
- Auth: none
- Response (200):
  - Content-Type: `text/html`（或等价）
  - Body: 前端构建产物 `index.html`

### `GET /assets/*`

- Scope: external
- Auth: none
- Response:
  - `200`: 返回静态资源（`application/javascript` / `text/css` / fonts 等）
  - `404`: 资源不存在
