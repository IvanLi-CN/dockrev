---
title: 集成指南
description: GHCR webhook、通知与外部触发集成。
---

# 集成指南

## GitHub Packages (GHCR) Webhook

目标：当 GHCR 发生 `package.published` 事件时，自动触发 Dockrev discovery scan。

### 设置页字段说明（Settings -> GitHub Packages (GHCR) Webhook）

| 字段 | 说明 | 注意事项 |
| --- | --- | --- |
| 启用 | 开关 GHCR webhook 功能。 | 关闭后不会同步 webhook，也不会消费 GHCR 事件。 |
| GitHub PAT（留空=保持原值） | 用于解析 owner/repo 与同步 webhook。 | 留空不会清空已保存 PAT；要更新 PAT 需输入新值并保存。 |
| Callback URL | 供 GitHub 回调的地址。 | 必须是公网可达 HTTPS 地址，通常为 `https://<your-domain>/api/webhooks/github-packages`。 |
| Repos / 添加 Repo | 维护要跟踪的仓库集合。 | 支持 `owner/repo`、`org/repo`、`https://github.com/org/repo`、`https://github.com/<owner>`。 |
| 解析并添加 | 将输入解析为候选仓库并加入列表。 | 解析 profile/org 依赖 PAT 与网络可达性。 |
| 搜索 owner/repo | 在已添加仓库中筛选。 | 仅影响当前展示，不影响 webhook 配置。 |
| 选中状态（selected） | 标记哪些仓库参与 webhook 同步。 | 只有 selected 仓库会创建/更新 webhook。 |

### 完整配置流程（推荐）

1. 打开 Settings -> GitHub Packages（GHCR）Webhook。
2. 打开“启用”，填写 `GitHub PAT`，确认 `Callback URL` 正确，然后点击“保存设置”。
3. 在“添加 Repo”输入目标（单仓库或 owner URL），点击“解析并添加”。
4. 在列表中勾选需要跟踪的仓库（selected=true）。
5. 点击“同步 webhook”，确保每个选中 repo 返回 `created/noop/updated`。
6. 在 GitHub 仓库的 Webhooks 页面确认已经存在回调到 Dockrev 的 webhook。
7. 发布一次 GHCR 新包（触发 `package.published`），在 Dockrev Queue/日志中确认 discovery 被触发。

### PAT 权限建议

- 能列出目标 owner 的仓库
- 能管理目标仓库 webhooks

### 回调可达性检查

- `Callback URL` 必须能被 GitHub 公网访问（内网地址不可用）。
- 反向代理需要保留 `POST /api/webhooks/github-packages` 路径，不要重写到其他地址。
- 若直接 `curl` 该回调接口返回 `400/401`，在未带 GitHub 签名时属于预期现象。

### 常见失败

- “解析并添加”没有结果：PAT 无效、权限不足或 GitHub API 不可达。
- “同步 webhook”后 repo 仍是 0：未勾选 selected，或未先保存设置中的 PAT/开关。
- `401 invalid_signature`：secret 不匹配或签名错误
- `422`：PAT 缺失或权限不足
- `conflict`：仓库已有重复 webhook，需确认删除旧 hook

## 通知通道

支持以下通知能力：

- Webhook
- Telegram
- Email（`smtpUrl` + `to/from` 参数）
- Web Push（VAPID）

### Web Push 初始化

```bash
bunx web-push generate-vapid-keys --json
```

将生成的公私钥写入设置页后，可在浏览器执行订阅/退订并测试通知。

## 外部触发更新（Webhook trigger）

使用 `/api/webhooks/trigger` 可从外部系统触发 check/update。

- 头：`X-Dockrev-Webhook-Secret`
- 体：`action` + `scope` + （可选）`stackId/serviceId`

该通道适合与 CI/CD、镜像发布事件或运维平台联动。
