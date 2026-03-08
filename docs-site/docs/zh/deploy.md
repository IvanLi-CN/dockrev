---
title: 部署指南
description: Dockrev 生产部署与透明 Forward Auth 接入说明。
---

# 部署指南

## 最小拓扑

`deploy/docker-compose.yml` 默认包含三个服务：

- `gateway`（nginx）：外部入口
- `dockrev`：API + 内嵌 Web UI
- `supervisor`：自我升级执行器与控制台

## 推荐上线步骤

1. 准备目录与凭据文件（只读挂载 Docker config）。
2. 校验反向代理能在已登录时注入可信 Forward Auth 用户/组头，并在未登录时仍让请求透传到 Dockrev。
3. 启动 compose 并检查匿名健康接口。
4. 使用命中 allowlist 的身份完成首轮 discovery/check 与 deploy-check 回归。

## 生产必备项

- 设置 `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
- 由入口注入 `DOCKREV_AUTH_FORWARD_HEADER_NAME`，如使用组鉴权再注入 `DOCKREV_AUTH_GROUP_HEADER_NAME`
- 至少配置一个 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`
- 持久化 DB（`DOCKREV_DB_PATH`）与 supervisor state
- 以只读方式把 compose 文件挂载到容器内相同绝对路径

## 可直接复制的示例文件

仓库里额外提供了一套可直接抄的生产示例：

- `deploy/examples/traefik-authelia/docker-compose.yml`
- `deploy/examples/traefik-authelia/authelia/configuration.yml`
- `deploy/examples/traefik-authelia/authelia/users.yml`
- `deploy/examples/traefik-authelia/README.md`

这套示例的特点是：所有 Dockrev 请求都按 service/path 直接转发到 Dockrev 或 Supervisor；Traefik 不再通过 webhook 分流、用户/组 ACL、路径 ACL 表达权限边界。Authelia 只负责透明身份透传；Dockrev 自己决定哪些接口匿名、哪些模块返回 `401 auth_required`。

如果你直接使用仓库里的 Traefik + Authelia 示例，建议保持 `traefik:v3.6.1` 或更新版本，避免旧版 Docker provider 在新 Docker Engine 上无法发现容器。

## Forward Auth（Traefik + Authelia）

### 职责拆分

- **Traefik**：路由、TLS、转发 Forward Auth 子请求。
- **Authelia**：登录会话与可信身份来源；在 Dockrev 拓扑里不承担用户/组/路径 ACL。
- **Dockrev / Supervisor**：划分公共匿名面与受保护业务面，并根据 `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` 判定是否允许访问。

### 透明透传接法

Authelia 侧保持 Dockrev 域名为透明放行：

```yaml
access_control:
  default_policy: deny
  rules:
    - domain: dockrev.example.com
      policy: bypass
```

Traefik 侧把同一套 Forward Auth middleware 挂到 Dockrev 与 Supervisor 路由：

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
```

Dockrev / Supervisor 侧继续只信任透传头与单值 allowlist：

```env
DOCKREV_AUTH_FORWARD_HEADER_NAME=Remote-User
DOCKREV_AUTH_GROUP_HEADER_NAME=Remote-Groups
DOCKREV_AUTH_ALLOWED_USER=
DOCKREV_AUTH_ALLOWED_GROUP=ops
DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false
```

这套接法的运行语义是：

- 运维人员先访问 `https://auth.example.com` 建立 Authelia 会话；后续请求若带有会话，Traefik 会把可信 `Remote-*` 头传给 Dockrev。
- 请求即便没有会话，也不会在网关被 Dockrev 专用 ACL 拦下；Dockrev 会对受保护 API、业务 UI、`/supervisor/*` 返回自己的 `401 auth_required`。
- `DOCKREV_AUTH_ALLOWED_USER` 与 `DOCKREV_AUTH_ALLOWED_GROUP` 各只接受一个值；两者同时配置时，命中任意一个即可通过。

### 路由边界

- **公共匿名 API**：`GET /api/health`、`GET /api/version`、`/api/webhooks/*`
- **受保护 API**：除上述外的全部 `/api/**`，包括 `GET /api/deploy-check/report`
- **受保护 UI / Supervisor**：全部 SPA 业务路由、`/supervisor/*`
- **Webhook 校验**：仍由 Dockrev 自己处理，不依赖网关 ACL

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

- Dockrev API/UI：`/` 与 `/api/*`
- Supervisor：`/supervisor/*`
- 自升级跳转由 `DOCKREV_SELF_UPGRADE_URL` 控制，默认 `/supervisor/`

## 验收检查

- 匿名访问 `GET /api/health` 与 `GET /api/version` 返回 `200`
- 匿名访问 `GET /api/deploy-check/report`、`GET /supervisor/health`、`GET /supervisor/version` 返回 Dockrev 生成的 `401 auth_required`
- 命中 allowlist 的透传身份可访问 deploy-check、业务 UI 与 `/supervisor/*`
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
