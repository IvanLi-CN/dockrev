---
title: 集成指南
description: GHCR webhook、通知与外部触发集成。
---

# 集成指南

## GitHub Packages (GHCR) Webhook

目标：当 GHCR 发生 `package.published` 事件时，自动触发 Dockrev discovery scan。

### 配置步骤

1. 打开 Settings -> GitHub Packages（GHCR）Webhook。
2. 配置 `PAT`（仅后端保存，读取时掩码显示）。
3. 配置 `callbackUrl`（需公网 HTTPS 可达）。
4. 添加目标 repo / owner（repo 默认可选）。
5. 点击同步 webhook，确保每个选中 repo 完成 `created/noop/updated`。

### PAT 权限建议

- 能列出目标 owner 的仓库
- 能管理目标仓库 webhooks

### 常见失败

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
