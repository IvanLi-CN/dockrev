---
title: API 参考（全量）
description: Dockrev API 与 Supervisor API 的全量接口清单。
---

# API 参考（全量）

本页覆盖以下源码中暴露的全部 HTTP 路由：

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-supervisor/src/app.rs`

## 鉴权模型

- **公开接口**：无需认证头。
- **Forward Auth**：入口代理可透传来自身份提供方的可信用户/组头；Dockrev 仍自行决定受保护接口是返回数据还是 `401 auth_required`。
- **Dockrev 鉴权**：Dockrev 读取 `DOCKREV_AUTH_ALLOWED_USER` 与 `DOCKREV_AUTH_ALLOWED_GROUP`；两者各只接受一个值，同时配置时命中任意一个即可通过。
- **Webhook Secret**：`X-Dockrev-Webhook-Secret` 必须与服务端配置一致。
- **GitHub Signature**：`X-Hub-Signature-256` + `X-GitHub-Event` + `X-GitHub-Delivery`。

---

## Dockrev API（`/api/*`）

### 1) Core

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/health` | 公开 | 健康探针 | `200` |
| GET | `/api/version` | 公开 | 返回 APP 有效版本 | `200` |

### 2) Stacks / Services / Version inference

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/stacks` | Forward Auth | 查询 stack 列表（含 archived 过滤） | `200` `401` `400` |
| POST | `/api/stacks` | Forward Auth | 手动注册已禁用（保持接口占位） | `400/405` `401` |
| GET | `/api/stacks/{stack_id}` | Forward Auth | 查询 stack 详情 | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/archive` | Forward Auth | 归档 stack | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/restore` | Forward Auth | 取消归档 stack | `200` `404` `401` |
| POST | `/api/services/{service_id}/archive` | Forward Auth | 归档 service | `200` `404` `401` |
| POST | `/api/services/{service_id}/restore` | Forward Auth | 取消归档 service | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags` | Forward Auth | 查询 digest 对应 tags | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags-snapshot` | Forward Auth | 查询 digest tags 快照 | `200` `404` `401` |
| POST | `/api/services/{service_id}/version-inference/refresh` | Forward Auth | 触发单服务版本推断刷新 | `200` `404` `401` |
| GET | `/api/version-inference/overview` | Forward Auth | 版本推断总览 | `200` `401` |
| GET | `/api/version-inference/events` | Forward Auth | 版本推断事件流（SSE） | `200` `401` |

### 3) Discovery / Check / Runtime scan / Update

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| POST | `/api/discovery/scan` | Forward Auth | 触发 discovery 扫描任务 | `200` `401` |
| GET | `/api/discovery/projects` | Forward Auth | discovery 项目列表 | `200` `401` |
| POST | `/api/discovery/projects/{project}/archive` | Forward Auth | 归档 discovery project | `200` `404` `401` |
| POST | `/api/discovery/projects/{project}/restore` | Forward Auth | 恢复 discovery project | `200` `404` `401` |
| POST | `/api/checks` | Forward Auth | 创建 check 任务 | `200` `400` `401` `409` |
| POST | `/api/runtime-scans` | Forward Auth | 创建 runtime 扫描任务 | `200` `400` `401` `409` |
| POST | `/api/updates` | Forward Auth | 创建 update 任务 | `200` `400` `401` `409` |

- `POST /api/updates` 契约：
  - `scope=service`：必须显式携带 `serviceId + targetTag + targetDigest + pullTags`。
  - `scope=stack|all`：必须显式携带 `targets[]`，元素为 `{ serviceId, targetTag, targetDigest, pullTags }`。

### 4) Jobs / Events

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/jobs` | Forward Auth | 作业列表 | `200` `401` |
| GET | `/api/jobs/events` | Forward Auth | 作业事件流（SSE） | `200` `401` |
| GET | `/api/jobs/{job_id}` | Forward Auth | 单作业详情 | `200` `404` `401` |
| GET | `/api/jobs/{job_id}/events` | Forward Auth | 单作业事件流（SSE） | `200` `404` `401` |

### 5) Ignores / Service settings / Notifications / Settings

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/ignores` | Forward Auth | 忽略规则列表 | `200` `401` |
| POST | `/api/ignores` | Forward Auth | 新建忽略规则 | `200` `400` `401` |
| DELETE | `/api/ignores` | Forward Auth | 删除忽略规则 | `200` `400` `401` |
| GET | `/api/services/{service_id}/settings` | Forward Auth | 服务级设置查询 | `200` `404` `401` |
| PUT | `/api/services/{service_id}/settings` | Forward Auth | 服务级设置更新 | `200` `400` `404` `401` |
| GET | `/api/notifications` | Forward Auth | 通知配置读取（`botToken` 不回传，`botTokenConfigured` + `chatId` 明文） | `200` `401` |
| PUT | `/api/notifications` | Forward Auth | 通知配置更新 | `200` `400` `401` |
| POST | `/api/notifications/test` | Forward Auth | 发送测试通知 | `200` `400` `401` |
| GET | `/api/settings` | Forward Auth | 系统设置读取 | `200` `401` |
| PUT | `/api/settings` | Forward Auth | 系统设置更新 | `200` `400` `401` |

- `POST /api/notifications/test` 从 `2026-03-05` 起将测试通知下游 payload 切换为 **v2（breaking）**：
  - 不再发送旧字段 `type/ts/message`。
  - 基础结构：`schema/kind/sentAt/channel/human/debug`。
  - Web Push 会额外附带顶层 `title/body`，用于 Service Worker 直接展示通知。
  - 文本渲染按通道能力：Telegram/Email 带代码块，Web Push 仅自然语言（无代码块）。

### 6) GitHub Packages (GHCR) integration

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/github-packages/settings` | Forward Auth | 读取 GHCR webhook 配置（PAT 掩码） | `200` `401` |
| PUT | `/api/github-packages/settings` | Forward Auth | 更新 GHCR webhook 配置 | `200` `400` `401` |
| GET | `/api/github-packages/repos` | Forward Auth | 仓库分页列表与筛选 | `200` `401` |
| POST | `/api/github-packages/repos/selected` | Forward Auth | 设置单仓库 selected | `200` `400` `401` |
| POST | `/api/github-packages/repos/delete` | Forward Auth | 删除单仓库跟踪 | `200` `400` `401` |
| POST | `/api/github-packages/repos/bulk-selected` | Forward Auth | 批量设置 selected | `200` `400` `401` |
| POST | `/api/github-packages/targets/add` | Forward Auth | 添加 target（repo/owner 输入） | `200` `400` `401` |
| POST | `/api/github-packages/targets/remove` | Forward Auth | 删除 target | `200` `400` `401` |
| POST | `/api/github-packages/resolve` | Forward Auth | 解析 repo/owner 输入并返回仓库候选 | `200` `400` `401` `422` |
| GET | `/api/github-packages/webhook/overview` | Forward Auth | GHCR webhook 状态聚合（repo + job） | `200` `401` |
| GET | `/api/github-packages/webhook/deliveries/events` | Forward Auth | GHCR webhook 收件箱事件流（SSE） | `200` `401` |
| POST | `/api/github-packages/webhook/sync-all` | Forward Auth | 触发全量 webhook 状态同步（复用未完成任务） | `200` `400` `401` |
| POST | `/api/github-packages/webhook/sync-repo` | Forward Auth | 触发单仓库 webhook 状态同步（同仓库未完成任务复用） | `200` `400` `401` `404` `409` |
| POST | `/api/github-packages/sync` | Forward Auth | 与 GitHub webhook 状态同步 | `200` `400` `401` |

#### GHCR webhook deliveries SSE

- `GET /api/github-packages/webhook/deliveries/events` 返回 `text/event-stream`，事件名固定为 `github_packages_delivery_event` 与 `github_packages_delivery_events_error`。
- 支持 query `afterId` 与 `Last-Event-ID` 续传；服务端取两者最大值作为 cursor。
- 未提供 cursor 时，默认 tail-follow：从当前最新 delivery event id 开始，仅追随后续新事件。
- 建议前端在收到 delivery 事件或 error 事件后重新请求当前收件箱列表，以保持分页、筛选和 summary 一致。

### 7) Web Push / Webhooks / Deploy checks

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| POST | `/api/web-push/subscriptions` | Forward Auth | 创建/更新 Web Push 订阅 | `200` `400` `401` |
| DELETE | `/api/web-push/subscriptions` | Forward Auth | 删除 Web Push 订阅 | `200` `400` `401` |
| POST | `/api/webhooks/trigger` | Webhook Secret | 外部触发 check/update 任务（`action=update` 必须显式携带 `targets[]`） | `200` `400` `401` |
| POST | `/api/webhooks/github-packages` | GitHub Signature | 接收 GH package webhook 并触发 discovery | `200` `202` `400` `401` |
| GET | `/api/deploy-check/report` | Forward Auth | 返回部署预检报告；匿名或未命中 allowlist 时返回 Dockrev 生成的 `401 auth_required` | `200` `401` |
| GET | `/api/deploy-welcome` | Forward Auth | 查询 deploy welcome 状态 | `200` `401` |
| PUT | `/api/deploy-welcome` | Forward Auth | 更新 deploy welcome 状态 | `200` `400` `401` |

---

## Supervisor API（base path 默认 `/supervisor`）

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/supervisor/health` | Forward Auth | Supervisor 健康探针（受保护） | `200` `401` |
| GET | `/supervisor/version` | Forward Auth | Supervisor 元信息（`version` + `repository` + `developerName` + `developerUrl`） | `200` `401` |
| GET | `/supervisor/self-upgrade` | Forward Auth | 查询当前自升级状态 | `200` `401` |
| POST | `/supervisor/self-upgrade` | Forward Auth | 发起自升级（dry-run/apply） | `200` `400` `401` `409` |
| POST | `/supervisor/self-upgrade/rollback` | Forward Auth | 回滚当前操作 | `200` `400` `401` |
| GET | `/supervisor/favicon.png` | 公开 | UI favicon | `200` |
| GET | `/supervisor/` | Forward Auth | Supervisor UI 页面 | `200` `401` |

- `GET /supervisor/self-upgrade` 在 `state=running` 时会返回可选字段 `request`（`mode` + `rollbackOnFailure`），用于页面恢复当前运行按钮状态；空闲/历史状态下该字段可缺省。

---

## 示例请求

### 触发全量检查

```bash
curl -X POST \
  -H 'Content-Type: application/json' \
  -H 'X-Forwarded-User: ops' \
  -d '{"scope":"all"}' \
  http://127.0.0.1:50883/api/checks
```

### 外部 webhook 触发 update

```bash
curl -X POST \
  -H 'Content-Type: application/json' \
  -H 'X-Dockrev-Webhook-Secret: change-me' \
  -d '{"action":"update","scope":"stack","stackId":"stk_xxx","targets":[{"serviceId":"svc_web","targetTag":"latest","targetDigest":"sha256:abc123","pullTags":["v1.1.2"]}],"allowArchMismatch":false,"backupMode":"inherit"}' \
  http://127.0.0.1:50883/api/webhooks/trigger
```

> 说明：`POST /api/webhooks/trigger` 在 `action=update` 时会拒绝未携带 `targets[]` 的旧 payload，返回 `400 invalid_argument`。
> webhook update 同样支持 `scope=service`，但仍需显式提供 `serviceId`，并在 `targets[]` 中给出对应 `{ serviceId, targetTag, targetDigest, pullTags }`。

### 查询 supervisor 状态

```bash
curl -H 'X-Forwarded-User: ops' \
  http://127.0.0.1:50883/supervisor/self-upgrade
```
