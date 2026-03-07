---
title: 部署指南
description: Dockrev 生产部署与反向代理接入说明。
---

# 部署指南

## 部署前确认

- 目标主机已安装 Docker 与 Compose。
- 域名/反向代理已就绪，且可转发 `/`、`/api/*`、`/supervisor/*`。
- 持久化目录已规划（至少 DB 与 supervisor state）。

## 部署拓扑（最小方案）

`deploy/docker-compose.yml` 默认包含 3 个服务：

- `gateway` (nginx): 对外统一入口
- `dockrev`: API + 内嵌 UI
- `supervisor`: 自升级执行与控制台

## 推荐部署步骤

1. 准备目录与凭据文件（只读挂载 Docker config）。
2. 校验反向代理是否可注入 Forward Auth 用户/组头。
3. 启动 compose 并检查健康接口。
4. 从 UI 完成首轮发现与检查。

最小启动命令：

```bash
cd deploy
mkdir -p data
cp ~/.docker/config.json data/docker-config.json
docker compose up -d --build
```

## 生产必要项

- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
- 在网关注入 `DOCKREV_AUTH_FORWARD_HEADER_NAME` 与 `DOCKREV_AUTH_GROUP_HEADER_NAME`（如使用组鉴权）
- 至少配置 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP` 之一
- 使用持久化 DB 路径（例如 `/data/dockrev.sqlite3`）
- 对 compose 文件目录做“同绝对路径只读挂载”

## Forward Auth（Traefik + Authelia）

### 职责拆分

- **Traefik / Authelia**：负责认证（是否已登录、是谁）。
- **Dockrev**：负责鉴权（这个用户或组是否允许进入 Dockrev）。
- `DOCKREV_AUTH_ALLOWED_USER` 与 `DOCKREV_AUTH_ALLOWED_GROUP` 各只接受一个值；两者同时配置时，命中任意一个即可通过。

### 推荐接法

推荐把 Dockrev UI、受保护 API、`/supervisor/*` 都放在 Traefik `forwardAuth` 后面，由 Authelia 做 `one_factor`，再把可信的用户/组头转给 Dockrev。

```yaml
http:
  middlewares:
    dockrev-forward-auth:
      forwardAuth:
        address: http://authelia:9091/api/authz/forward-auth
        trustForwardHeader: true
        authResponseHeaders:
          - Remote-User
          - Remote-Groups

  routers:
    dockrev:
      rule: Host(`dockrev.example.com`)
      service: dockrev
      middlewares:
        - dockrev-forward-auth
```

```env
DOCKREV_AUTH_FORWARD_HEADER_NAME=Remote-User
DOCKREV_AUTH_GROUP_HEADER_NAME=Remote-Groups
DOCKREV_AUTH_ALLOWED_USER=
DOCKREV_AUTH_ALLOWED_GROUP=ops
DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false
```

### 为什么不推荐“全站匿名放行 + 仅在已登录时带头”

Dockrev 需要稳定、可信的认证身份来执行项目侧鉴权。对于需要保护的页面/API，推荐始终经过 Forward Auth，并由 Dockrev决定是否允许该用户/组访问，而不是依赖匿名放行策略来“顺带”提供身份头。

### Webhook 说明

- `/api/webhooks/trigger`：使用 `DOCKREV_WEBHOOK_SECRET`。
- `/api/webhooks/github-packages`：使用 GitHub `X-Hub-Signature-256`。
- 这两个端点不依赖 Forward Auth；如果它们和 UI 复用同一域名入口，需要在入口层单独保证 webhook 请求能到达 Dockrev。

## 使用已发布镜像

将 compose 中 `build` 替换为镜像：

```yaml
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:<semver>
  supervisor:
    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest
```

注意：

- `latest` 仅在稳定 Release 更新。
- 建议使用 `0.3.5+`，避免历史 exec bit 问题。

## 反向代理与路径

- Dockrev API/UI: `/` 与 `/api/*`
- Supervisor: `/supervisor/*`
- 自升级跳转由 `DOCKREV_SELF_UPGRADE_URL` 控制，默认 `/supervisor/`

## 验收检查

- `GET /api/health` 返回 `ok`
- `GET /api/deploy-check/report` 可返回 deploy check 报告；未通过鉴权时也会返回仅含鉴权项的自检结果
- `GET /supervisor/health` 通过网关可访问
- 设置页中保存配置后，重新打开仍能看到 PAT/secret 掩码（说明已落库）

## 回滚建议

- 镜像回滚：将 compose 镜像 tag 切回上一个稳定版本
- 数据回滚：恢复 SQLite 备份
- 自升级中断：使用 supervisor rollback API

常用命令：

```bash
# 查看最近日志
docker compose logs --tail=200

# 回滚到上一镜像后重启
docker compose pull
docker compose up -d
```
