---
title: 配置参考
description: Dockrev API 与 Supervisor 运行参数说明。
---

# 配置参考

## 生产必填（先配这 7 项）

1. `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
2. `DOCKREV_AUTH_FORWARD_HEADER_NAME`（与入口 Forward Auth 用户头保持一致）
3. `DOCKREV_AUTH_GROUP_HEADER_NAME`（如使用组鉴权，需与入口组头保持一致）
4. `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`（至少配置一个）
5. `DOCKREV_DB_PATH`（指向持久卷）
6. `DOCKREV_SUPERVISOR_STATE_PATH`（指向持久卷）
7. `DOCKREV_IMAGE_REPO`（与你实际部署的 Dockrev 镜像仓库一致）

## API 核心配置（`dockrev-api`）

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DOCKREV_HTTP_ADDR` | `0.0.0.0:50883` | API 监听地址 |
| `DOCKREV_DB_PATH` | `./data/dockrev.sqlite3` | SQLite 文件路径 |
| `DOCKREV_DOCKER_CONFIG` | 空 | Docker `config.json` 文件路径；Dockrev 会在 update job 中自动桥接为临时 Docker CLI 配置目录，若路径本身就是 `config.json` 还会一并复制同目录元数据 |
| `DOCKREV_COMPOSE_BIN` | `docker-compose` | Compose 命令选择 |
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | Forward Auth 用户头名 |
| `DOCKREV_AUTH_GROUP_HEADER_NAME` | `Remote-Groups` | Forward Auth 组头名 |
| `DOCKREV_AUTH_ALLOWED_USER` | 空 | Dockrev 允许的单个用户名 |
| `DOCKREV_AUTH_ALLOWED_GROUP` | 空 | Dockrev 允许的单个组名 |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | 开发态匿名开关；配置允许用户/组后会自动失效 |
| `DOCKREV_SELF_UPGRADE_URL` | `/supervisor/` | UI 中“升级 Dockrev”跳转地址 |
| `DOCKREV_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | 用于识别 Dockrev 自身服务 |
| `DOCKREV_WEBHOOK_SECRET` | 空 | `/api/webhooks/trigger` 共享密钥 |
| `DOCKREV_HOST_PLATFORM` | 空 | 覆盖主机平台（如 `linux/amd64`） |
| `DOCKREV_DISCOVERY_INTERVAL_SECONDS` | `60` | 自动发现周期 |
| `DOCKREV_DISCOVERY_MAX_ACTIONS` | `200` | 扫描返回动作上限 |

说明：

- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` 仅建议本地开发使用；一旦配置 `DOCKREV_AUTH_ALLOWED_USER` 或 `DOCKREV_AUTH_ALLOWED_GROUP`，匿名旁路会自动失效；生产仍必须显式关闭。
- `DOCKREV_AUTH_ALLOWED_USER` 与 `DOCKREV_AUTH_ALLOWED_GROUP` 各只接受一个值；两者同时配置时，Dockrev 采用“用户或组命中其一即可通过”。
- 生产匿名公共面固定为 `GET /api/health`、`GET /api/version`、`/api/webhooks/*`；其余 API/UI/`/supervisor/*` 即使走透明透传，也仍由 Dockrev 自己鉴权。
- `DOCKREV_IMAGE_REPO` 配错会导致“升级 Dockrev”入口识别异常。

## 检查与重试参数

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS` | `3` | 429 重试次数 |
| `DOCKREV_REGISTRY_RETRY_BASE_MS` | `250` | 退避基数 |
| `DOCKREV_REGISTRY_RETRY_MAX_MS` | `2000` | 退避上限 |
| `DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS` | `12` | 本地探测超时 |

固定策略（非环境变量）：

- Check 并发固定为 `7`
- Worker 启动错峰固定 `1s`
- Registry host 并发固定 `5`

调参建议：

- 经常遇到 `429`：先增大 `DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS`。
- 目标 registry 响应慢：提高 `DOCKREV_REGISTRY_RETRY_MAX_MS`（保持 >= `BASE_MS`）。

## Supervisor 配置（`dockrev-supervisor`）

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DOCKREV_SUPERVISOR_HTTP_ADDR` | `0.0.0.0:50884` | Supervisor API 地址 |
| `DOCKREV_SUPERVISOR_BASE_PATH` | `/supervisor` | 挂载路径 |
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | Supervisor 读取的 Forward Auth 用户头名 |
| `DOCKREV_AUTH_GROUP_HEADER_NAME` | `Remote-Groups` | Supervisor 读取的 Forward Auth 组头名 |
| `DOCKREV_AUTH_ALLOWED_USER` | 空 | Supervisor 允许的单个用户名 |
| `DOCKREV_AUTH_ALLOWED_GROUP` | 空 | Supervisor 允许的单个组名 |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | Supervisor 开发态匿名开关；配置允许用户/组后会自动失效 |
| `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | 自升级目标镜像仓库 |
| `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID` | 空 | 覆盖自动匹配容器 |
| `DOCKREV_SUPERVISOR_DOCKER_HOST` | 空 | Docker endpoint |
| `DOCKREV_SUPERVISOR_COMPOSE_BIN` | `docker-compose` | Compose 命令选择 |
| `DOCKREV_SUPERVISOR_STATE_PATH` | `./data/supervisor/self-upgrade.json` | 状态文件路径 |

## 生产基线建议

- 关闭匿名模式
- 固定 Forward Auth 头并由入口网关注入
- 使用持久卷保存 DB 与 supervisor state
- 限制 Docker socket 暴露面（可改用 docker-socket-proxy）
- 修改配置后，以命中 allowlist 的身份执行 `GET /api/deploy-check/report` 与 `GET /supervisor/health` 做回归核验
