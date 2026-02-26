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

### 可直接照填的最小可行配置（MVP）

> 适用于你当前这类界面。以下 6 项全部满足，GHCR webhook 才会真正生效。

| 配置项 | 建议值 |
| --- | --- |
| 启用 | 打开（ON） |
| GitHub PAT | 推荐 classic PAT：`repo` + `admin:repo_hook`（仅公开仓库可用 `public_repo` + `admin:repo_hook`） |
| Callback URL | `https://<your-domain>/api/webhooks/github-packages`（例如 `https://dockrev.ivanli.cc/api/webhooks/github-packages`） |
| 添加 Repo | 输入 `owner/repo`（例如 `ivanli-cn/dockrev`）后点“解析并添加” |
| selected | 至少勾选 1 个仓库（`repos_selected_total > 0`） |
| 同步 webhook | 结果为 `created` / `updated` / `noop`，不能出现 `error/conflict` |

> Fine-grained PAT 也可用：目标仓库需授予 `Webhooks` 仓库权限（write），并保证能读取仓库列表（至少 `Metadata` 读取权限）。

### PAT 权限（明确最小集）

Dockrev 在 GHCR webhook 流程中会调用这些 GitHub API：

- `GET /orgs/{owner}/repos`、`GET /users/{owner}/repos`（解析 owner/profile 到仓库列表）
- `GET/POST/PATCH/DELETE /repos/{owner}/{repo}/hooks`（读取/创建/更新/删除 webhook）

因此 token 权限请按下面二选一配置：

#### 方案 A：Classic PAT（推荐，最省心）

- 私有仓库：勾选 `repo` + `admin:repo_hook`
- 仅公开仓库：勾选 `public_repo` + `admin:repo_hook`
- 若仓库在启用了 SSO 的组织中：需要在 GitHub 对该 PAT 执行组织授权（SSO authorize）

#### 方案 B：Fine-grained PAT（可用，但要按项配置）

创建 token 时明确设置：

1. **Resource owner**：选择目标用户/组织
2. **Repository access**：
   - 如果你想在 Dockrev 里用 `profile/org URL` 自动解析仓库，选 **All repositories**（推荐）
   - 如果你只监控少数仓库，选 **Only select repositories**，并确保目标 repo 全被选中
3. **Repository permissions**（最小集）：
   - `Webhooks`: **Read and write**
   - `Metadata`: **Read-only**

> `Packages` 权限不是本功能的必需条件；GHCR webhook 同步本质上是“仓库 webhook 管理”。

### 验收清单（按界面逐项确认）

1. 点击“保存设置”后，重新进入设置页，`GitHub PAT` 显示掩码（`ghp_...`）。
2. “解析并添加”后，Repos 区域右上角总数大于 0（不再是 `0 个`）。
3. 勾选 selected 后，“已跟踪”数量大于 0。
4. “同步 webhook”后，每个选中仓库为 `created/updated/noop`。
5. GitHub 仓库 `Settings -> Webhooks` 中出现指向 Dockrev 的 webhook（事件包含 `package`）。
6. 发布 GHCR 新版本后，Dockrev Queue 出现 discovery 任务。

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
- 页面右上角显示 `0 个`：还没有成功添加仓库，或者添加后未勾选 selected。

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
