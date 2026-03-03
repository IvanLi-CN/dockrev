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
- **Forward Header**：需要反向代理注入 `DOCKREV_AUTH_FORWARD_HEADER_NAME`（默认 `X-Forwarded-User`）。
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
| GET | `/api/stacks` | Forward Header | 查询 stack 列表（含 archived 过滤） | `200` `401` `400` |
| POST | `/api/stacks` | Forward Header | 手动注册已禁用（保持接口占位） | `400/405` `401` |
| GET | `/api/stacks/{stack_id}` | Forward Header | 查询 stack 详情 | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/archive` | Forward Header | 归档 stack | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/restore` | Forward Header | 取消归档 stack | `200` `404` `401` |
| POST | `/api/services/{service_id}/archive` | Forward Header | 归档 service | `200` `404` `401` |
| POST | `/api/services/{service_id}/restore` | Forward Header | 取消归档 service | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags` | Forward Header | 查询 digest 对应 tags | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags-snapshot` | Forward Header | 查询 digest tags 快照 | `200` `404` `401` |
| POST | `/api/services/{service_id}/version-inference/refresh` | Forward Header | 触发单服务版本推断刷新 | `200` `404` `401` |
| GET | `/api/version-inference/overview` | Forward Header | 版本推断总览 | `200` `401` |
| GET | `/api/version-inference/events` | Forward Header | 版本推断事件流（SSE） | `200` `401` |

### 3) Discovery / Check / Runtime scan / Update

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| POST | `/api/discovery/scan` | Forward Header | 触发 discovery 扫描任务 | `200` `401` |
| GET | `/api/discovery/projects` | Forward Header | discovery 项目列表 | `200` `401` |
| POST | `/api/discovery/projects/{project}/archive` | Forward Header | 归档 discovery project | `200` `404` `401` |
| POST | `/api/discovery/projects/{project}/restore` | Forward Header | 恢复 discovery project | `200` `404` `401` |
| POST | `/api/checks` | Forward Header | 创建 check 任务 | `200` `400` `401` `409` |
| POST | `/api/runtime-scans` | Forward Header | 创建 runtime 扫描任务 | `200` `400` `401` `409` |
| POST | `/api/updates` | Forward Header | 创建 update 任务 | `200` `400` `401` `409` |

### 4) Jobs / Events

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/jobs` | Forward Header | 作业列表 | `200` `401` |
| GET | `/api/jobs/events` | Forward Header | 作业事件流（SSE） | `200` `401` |
| GET | `/api/jobs/{job_id}` | Forward Header | 单作业详情 | `200` `404` `401` |
| GET | `/api/jobs/{job_id}/events` | Forward Header | 单作业事件流（SSE） | `200` `404` `401` |

### 5) Ignores / Service settings / Notifications / Settings

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/ignores` | Forward Header | 忽略规则列表 | `200` `401` |
| POST | `/api/ignores` | Forward Header | 新建忽略规则 | `200` `400` `401` |
| DELETE | `/api/ignores` | Forward Header | 删除忽略规则 | `200` `400` `401` |
| GET | `/api/services/{service_id}/settings` | Forward Header | 服务级设置查询 | `200` `404` `401` |
| PUT | `/api/services/{service_id}/settings` | Forward Header | 服务级设置更新 | `200` `400` `404` `401` |
| GET | `/api/notifications` | Forward Header | 通知配置读取（`botToken` 不回传，`botTokenConfigured` + `chatId` 明文） | `200` `401` |
| PUT | `/api/notifications` | Forward Header | 通知配置更新 | `200` `400` `401` |
| POST | `/api/notifications/test` | Forward Header | 发送测试通知 | `200` `400` `401` |
| GET | `/api/settings` | Forward Header | 系统设置读取 | `200` `401` |
| PUT | `/api/settings` | Forward Header | 系统设置更新 | `200` `400` `401` |

### 6) GitHub Packages (GHCR) integration

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/api/github-packages/settings` | Forward Header | 读取 GHCR webhook 配置（PAT 掩码） | `200` `401` |
| PUT | `/api/github-packages/settings` | Forward Header | 更新 GHCR webhook 配置 | `200` `400` `401` |
| GET | `/api/github-packages/repos` | Forward Header | 仓库分页列表与筛选 | `200` `401` |
| POST | `/api/github-packages/repos/selected` | Forward Header | 设置单仓库 selected | `200` `400` `401` |
| POST | `/api/github-packages/repos/delete` | Forward Header | 删除单仓库跟踪 | `200` `400` `401` |
| POST | `/api/github-packages/repos/bulk-selected` | Forward Header | 批量设置 selected | `200` `400` `401` |
| POST | `/api/github-packages/targets/add` | Forward Header | 添加 target（repo/owner 输入） | `200` `400` `401` |
| POST | `/api/github-packages/targets/remove` | Forward Header | 删除 target | `200` `400` `401` |
| POST | `/api/github-packages/resolve` | Forward Header | 解析 repo/owner 输入并返回仓库候选 | `200` `400` `401` `422` |
| POST | `/api/github-packages/sync` | Forward Header | 与 GitHub webhook 状态同步 | `200` `400` `401` |

### 7) Web Push / Webhooks / Deploy checks

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| POST | `/api/web-push/subscriptions` | Forward Header | 创建/更新 Web Push 订阅 | `200` `400` `401` |
| DELETE | `/api/web-push/subscriptions` | Forward Header | 删除 Web Push 订阅 | `200` `400` `401` |
| POST | `/api/webhooks/trigger` | Webhook Secret | 外部触发 check/update 任务（`action=update` 仅支持 `all`/`stack`） | `200` `400` `401` |
| POST | `/api/webhooks/github-packages` | GitHub Signature | 接收 GH package webhook 并触发 discovery | `200` `202` `400` `401` |
| GET | `/api/deploy-check/report` | Forward Header | 返回部署预检报告 | `200` `401` |
| GET | `/api/deploy-welcome` | Forward Header | 查询 deploy welcome 状态 | `200` `401` |
| PUT | `/api/deploy-welcome` | Forward Header | 更新 deploy welcome 状态 | `200` `400` `401` |

---

## Supervisor API（base path 默认 `/supervisor`）

| 方法 | 路径 | 鉴权 | 用途 | 关键状态码 |
| --- | --- | --- | --- | --- |
| GET | `/supervisor/health` | 公开 | Supervisor 健康探针 | `200` |
| GET | `/supervisor/version` | 公开 | Supervisor 元信息（`version` + `repository` + `developerName` + `developerUrl`） | `200` |
| GET | `/supervisor/self-upgrade` | Forward Header | 查询当前自升级状态 | `200` `401` |
| POST | `/supervisor/self-upgrade` | Forward Header | 发起自升级（dry-run/apply） | `200` `400` `401` `409` |
| POST | `/supervisor/self-upgrade/rollback` | Forward Header | 回滚当前操作 | `200` `400` `401` |
| GET | `/supervisor/favicon.png` | 公开 | UI favicon | `200` |
| GET | `/supervisor/` | 公开 | Supervisor UI 页面 | `200` |

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
  -d '{"action":"update","scope":"stack","stackId":"stk_xxx","allowArchMismatch":false,"backupMode":"inherit"}' \
  http://127.0.0.1:50883/api/webhooks/trigger
```

> 说明：`POST /api/webhooks/trigger` 会拒绝 `{"action":"update","scope":"service"}`，返回 `400 invalid_argument`。  
> 若要更新单个 service，请改用 `POST /api/updates` 并显式携带 `targetTag` + `targetDigest`。

### 查询 supervisor 状态

```bash
curl -H 'X-Forwarded-User: ops' \
  http://127.0.0.1:50883/supervisor/self-upgrade
```
