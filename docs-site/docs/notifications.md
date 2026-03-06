---
title: Notifications
description: Dockrev 通知类型、字段格式、以及可点击 URL/跳转规则。
---

# Notifications（通知）

本页说明 Dockrev **有哪些通知**、各通知的 **payload 字段格式**、以及如何配置 **实例 Public Base URL** 以便在 Telegram / Email / Web Push / Webhook 中生成可点击的 Dockrev 实例链接。

## 配置：实例 Public Base URL（用于通知链接）

用途：把站内相对路径（如 `queue/{jobId}`）拼成可访问的绝对 URL（如 `https://dockrev.example.com/queue/job_...`）。

- Web UI：`设置 -> 系统设置 -> 实例 Public Base URL`
- API：
  - `GET /api/settings -> instance.publicBaseUrl`
  - `PUT /api/settings -> instance.publicBaseUrl`
- 校验与归一化规则：
  - 允许为 `null` 或空字符串（清空配置）
  - 非空时必须是 `http://` 或 `https://` 的**绝对 URL**
  - 保存时会 `trim`，并确保以 `/` 结尾（便于安全 join）
- 未配置时的降级行为：
  - `links.*Url` 仍会给出**站内路径**（以 `/` 开头）
  - Telegram / Email：站内路径会以代码样式显示（不可点击），并提示去配置 Public Base URL
  - Web Push：`url` 仍会是站内路径（浏览器可在当前 origin 下打开）

## 通知类型一览

| kind | schema | 触发条件（概述） |
| --- | --- | --- |
| `job_finished` | `dockrev.notification.job.v2` | 更新任务完成（成功/失败/回滚等）且未被过滤跳过 |
| `new_version_discovered` | `dockrev.notification.new_version_discovered.v2` | 定时检查更新（`check` schedule）发现新版本（按本次任务聚合） |
| `ghcr_webhook_anomaly` | `dockrev.notification.ghcr_webhook_anomaly.v2` | GHCR Webhook 定时巡检（`audit_all`）发现异常仓库（missing/conflict/error） |
| `notification_test` | `dockrev.notification.test.v2` | 调用 `POST /api/notifications/test` 发送测试通知 |

> Webhook 订阅方请始终以 `schema` 分流解析（`dockrev.notification.job.v2` 为 breaking 变更）。

## 通知开关（设置页）

路径：`设置 -> 通知`

事件级开关（独立控制）：

- `更新完成通知`：控制 `job_finished`
- `发现新版本通知（定时检查）`：控制 `new_version_discovered`
- `GitHub Webhook 异常通知（巡检）`：控制 `ghcr_webhook_anomaly`

说明：

- 事件开关关闭时，该事件不会发送到任何渠道。
- 渠道开关（Email/Webhook/Telegram/Web Push）与事件开关同时生效：两者都开启才会发送。

## Job finished（`dockrev.notification.job.v2`）

### 字段说明（v2）

顶层字段：

- `schema`：固定为 `dockrev.notification.job.v2`
- `kind`：固定为 `job_finished`
- `sentAt`：RFC3339 时间（字符串）
- `channel`：`telegram` / `email` / `webhook` / `webPush`
- `job`：任务基础信息
- `links`：Dockrev 实例内的可访问链接（核心）
- `human`：面向人的中文标题/摘要/详情（用于渲染）
- `debug`：调试信息（版本、来源）

`job` 字段（示例）：

```json
{
  "id": "job_...",
  "type": "update",
  "scope": "all",
  "status": "success",
  "reason": "manual",
  "createdBy": "web",
  "createdAt": "2026-03-05T13:40:00Z",
  "startedAt": "2026-03-05T13:41:00Z",
  "finishedAt": "2026-03-05T13:46:37Z",
  "stackId": "stk_...",
  "serviceId": "svc_..."
}
```

`links` 字段（示例）：

```json
{
  "primaryUrl": "https://dockrev.example.com/services/stk_.../svc_...",
  "jobUrl": "https://dockrev.example.com/queue/job_...",
  "serviceUrls": [
    {
      "stackId": "stk_...",
      "stackName": "blog",
      "serviceId": "svc_...",
      "serviceName": "api",
      "url": "https://dockrev.example.com/services/stk_.../svc_..."
    }
  ],
  "truncated": { "serviceUrlsOmitted": 0 }
}
```

### URL/跳转规则（核心）

Dockrev 会生成两类路径（总能生成）：

- 任务详情：`/queue/{jobId}`
- 服务详情：`/services/{stackId}/{serviceId}`

当配置了 `instance.publicBaseUrl` 时，会生成对应绝对 URL；否则降级为站内路径。

`primaryUrl` 选择规则：

1. 若本次更新可唯一定位到 **1 个服务**，则 `primaryUrl = serviceUrl`
2. 否则 `primaryUrl = jobUrl`

“可唯一定位到 1 个服务”的判定：

- job scope 为 `service` 且任务记录有 `serviceId`
- 或从更新摘要中解析到变更服务数为 1

### 截断规则（避免爆长）

- `links.serviceUrls` 最多保留 **10 条**
- 超出部分计数写入：`links.truncated.serviceUrlsOmitted`
- 错误节选（若有）会做长度截断（避免 Telegram / Email 超长）

## New version discovered（`dockrev.notification.new_version_discovered.v2`）

触发条件：仅由**定时检查更新任务**触发；当某次检查发现新的可更新服务时，按任务聚合发送。

关键字段：

- `check.jobId`：对应检查任务 ID
- `check.servicesChecked`：本次检查覆盖的服务数
- `check.newVersions`：本次发现新版本的服务数
- `links.jobUrl`：任务详情页（`/queue/{jobId}`）
- `links.serviceUrls[]`：服务详情链接（`/services/{stackId}/{serviceId}`）
- `links.primaryUrl`：若仅 1 个服务则指向服务详情，否则指向任务详情

截断规则：

- `links.serviceUrls` 最多 10 条
- 超出计入 `links.truncated.serviceUrlsOmitted`

示例（摘要）：

- 标题：`Dockrev：发现新版本`
- 摘要：`发现 3 个服务有新版本。`
- 主链接：`https://dockrev.example.com/queue/job_...`（多服务）或服务详情（单服务）

## GHCR webhook anomaly（`dockrev.notification.ghcr_webhook_anomaly.v2`）

触发条件：仅由 GHCR Webhook 定时巡检（`audit_all`）触发；当巡检结果出现 `missing/conflict/error` 时聚合发送。

关键字段：

- `job.id`：对应巡检任务 ID
- `job.missing/conflict/error`：异常统计
- `links.settingsUrl`：设置页（用于修复配置）
- `links.jobUrl`：任务详情页（查看巡检日志）
- `links.repos[]`：异常仓库列表（含状态与错误节选）

截断规则：

- `links.repos` 最多 10 条
- 超出计入 `links.truncated.reposOmitted`

示例（摘要）：

- 标题：`Dockrev：GitHub Webhook 巡检异常`
- 摘要：`巡检发现 2 个异常仓库（missing=1，conflict=0，error=1）。`
- 主链接：`https://dockrev.example.com/settings`

## Notification test（`dockrev.notification.test.v2`）

### 字段说明（v2）

- `schema`：固定为 `dockrev.notification.test.v2`
- `kind`：固定为 `notification_test`
- `sentAt` / `channel`：同上
- `url`：用于演示“可点击链接”的示例 URL（默认指向设置页 `/settings`）
- `human`：标题/摘要/详情
- `debug`：包含 `requestedChannel`、`rawMessage` 等

### Web Push 专用字段

当通过 Web Push 渠道发送时，payload 顶层会额外包含：

- `title`：通知标题
- `body`：通知正文（纯文本）
- `url`：点击后打开的 URL（等同于上面的 `url` 或 `links.primaryUrl`）

Service Worker（`web/public/sw.js`）会优先读取 `data.url` 并打开。

## 渠道渲染示例

### Telegram（HTML）

- 标题：加粗中文标题
- 内容：摘要 + 任务链接 + 主跳转链接 + 服务清单（每条可点击）+ 错误节选（`<pre>`，可能截断）

示例（单服务变更）：

```
Dockrev：更新完成（成功）
变更 1 个服务（blog / api）。
任务：job_...（可点击）
打开服务详情：（可点击）

服务清单
- blog / api：服务详情（可点击）
```

### Email（multipart）

- Subject：`[dockrev] 更新完成（成功） job_...`
- Body：同 Telegram 的结构，提供 HTML + 纯文本两份

### Webhook（JSON）

直接 POST 对应 schema 的 JSON：

- `dockrev.notification.job.v2`
- `dockrev.notification.new_version_discovered.v2`
- `dockrev.notification.ghcr_webhook_anomaly.v2`
- `dockrev.notification.test.v2`

### Web Push（Notification）

- `title` / `body` / `url` 顶层字段齐全
- 点击通知后打开 `url`（根据事件可能指向服务详情 / 任务详情 / 设置页）

## 手工验收（推荐）

1. 在设置页填写 `实例 Public Base URL`（例如 `https://dockrev.example.com/`）
2. 在设置页打开三个事件开关（更新完成 / 发现新版本 / GHCR Webhook 异常）
3. 触发一次更新任务并等待结束
4. 等待一次定时检查更新命中新版本（或手工触发检查并模拟定时场景）
5. 触发一次 GHCR Webhook 巡检并制造 missing/conflict/error 异常
6. 验证：
   - Telegram/Email：三类通知均可读，且包含可点击链接
   - Web Push：点击后能按事件跳转到服务详情、任务详情或设置页
   - Webhook：收到对应 v2 schema，且 `url/links.primaryUrl` 合规
