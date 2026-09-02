---
title: 快速开始
description: 在本地快速启动 Dockrev 并完成第一轮扫描。
---

# 快速开始

本页目标：在 10 分钟内跑起 Dockrev，并验证从“发现服务”到“执行检查”的主流程。

## 前置条件

- Docker Engine 可用
- Docker Compose 可用（`docker-compose` 或 `docker compose`）
- 本机可访问端口 `50883`
- 已准备 Docker registry 凭据（私有镜像场景）

## 一键启动（最小方式）

```bash
cd deploy
mkdir -p data
# Dockrev 会桥接该文件给 update job 的 Docker/Compose 鉴权使用，无需额外挂载 /root/.docker/config.json。
cp ~/.docker/config.json data/docker-config.json
# 如果你依赖 Docker contexts，请把 DOCKREV_DOCKER_CONFIG 指向真正名为 config.json 的路径，而不是重命名后的副本。

docker compose up --build
```

访问入口：

- UI: `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`
- Supervisor: `http://127.0.0.1:50883/supervisor/`

## 已安装应用的图标更新

Dockrev 会稳定保持 Web App Manifest 的 `id`、`scope` 和 `start_url`，不会因为图标版本变化而改变已安装应用的身份。Manifest 的 regular/maskable 图标与浏览器 favicon 使用内容哈希文件名发布，HTML、manifest 和 service worker 会重新验证。产品页只以 Manifest 作为安装图标元数据来源，不声明 `apple-touch-icon`；新构建可以因此交付新的图标字节，不需要把重新安装作为常规更新方式。

Android Chrome WebAPK 与 Chromium desktop 的 PWA 安装遵循 manifest 更新生命周期和平台控制的刷新节奏。不由 manifest 驱动的浏览器快捷方式可能继续使用创建时保存的图标。

iOS/iPadOS Safari 与已有 Web Clip 存在平台限制：已有 Web Clip 会继续使用平台保存的图标和元数据，网站不能强制迁移它。Dockrev 产品页不会重新加入 `apple-touch-icon`，也不会宣称可以更新已有 Web Clip。不支持原地 manifest 迁移的其他浏览器也相同。这是平台限制，不是 Dockrev 的常规更新步骤。

每次发布后，应使用 HTML 解析器和 Web App Manifest 检查真实产物：确认只有一个 manifest link、产品页没有 `apple-touch-icon`、`id`/`scope`/`start_url` 未变、哈希图标字节正确、metadata 使用重新验证响应头、哈希图标使用 immutable 响应头，并确认 `sw.js` 的 precache 不包含 manifest 或任何安装图标。

## 5 分钟验收（必须全部通过）

1. `GET /api/health` 返回 `ok`。
2. 首页能看到服务列表，不出现 401。
3. 执行一次“立即扫描”后，Queue 中出现 discovery 任务并结束为 `success`。
4. 对任意服务执行一次 check，任务日志中能看到 registry 请求与结果。

## 第一次验证清单

1. 打开 Overview 页面，确认可以加载服务列表。
2. 点击“立即扫描”，确认系统可触发 Discovery。
3. 对任意服务触发 Check，确认 Job 进入队列并有日志。
4. 打开 Queue 页面，确认 Job 状态变化完整（running -> success/failed）。

## 本地开发启动（非容器）

### 工作区依赖

```bash
bun run hooks:install
bun run bootstrap:worktree
```

`hooks:install` 会安装 shared Git `post-checkout` hook。新 linked worktree 切入后会自动运行项目内依赖 bootstrap：根目录、`web/`、`docs-site/` 的 Bun 依赖，以及 `cargo fetch --locked`。它不会安装 Bun、Rust、Playwright browsers 或系统包；如需临时跳过自动运行，设置 `DOCKREV_BOOTSTRAP_SKIP=1`。

### 后端

```bash
DOCKREV_HTTP_ADDR=127.0.0.1:50883 DOCKREV_DB_PATH=/tmp/dockrev.sqlite3 cargo run -p dockrev-api --bin dockrev
```

### Supervisor

```bash
DOCKREV_SUPERVISOR_HTTP_ADDR=127.0.0.1:50884 cargo run -p dockrev-supervisor --bin dockrev-supervisor
```

### 前端

```bash
cd web
bun install
bun run dev
```

## 下一步

- 进入 [部署指南](/zh/deploy) 完成生产部署。
- 进入 [配置参考](/zh/config) 完成认证、镜像识别和重试参数配置。

## 启动失败时先看这里

- 容器启动失败：`docker compose logs --tail=200`
- API 不通：确认 `50883` 未被占用（`lsof -iTCP:50883 -sTCP:LISTEN -n -P`）
- 页面 401：检查反向代理是否注入了 `X-Forwarded-User`（以及你配置的组头，如 `Remote-Groups`），并确认 Dockrev 允许的用户/组配置正确
