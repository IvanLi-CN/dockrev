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

## 一键启动（最小方式）

```bash
cd deploy
mkdir -p data
cp ~/.docker/config.json data/docker-config.json

docker compose up --build
```

访问入口：

- UI: `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`
- Supervisor: `http://127.0.0.1:50883/supervisor/`

## 第一次验证清单

1. 打开 Overview 页面，确认可以加载服务列表。
2. 点击“立即扫描”，确认系统可触发 Discovery。
3. 对任意服务触发 Check，确认 Job 进入队列并有日志。
4. 打开 Queue 页面，确认 Job 状态变化完整（running -> success/failed）。

## 本地开发启动（非容器）

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

- 进入 [部署指南](/deploy) 完成生产部署。
- 进入 [配置参考](/config) 完成认证、镜像识别和重试参数配置。
