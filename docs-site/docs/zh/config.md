---
title: 配置参考
description: Dockrev API 与 Supervisor 运行参数说明。
---

# 配置参考

## 生产必填（先配这 5 项）

1. `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
2. `DOCKREV_AUTH_FORWARD_HEADER_NAME`（与网关注入头保持一致）
3. `DOCKREV_DB_PATH`（指向持久卷）
4. `DOCKREV_SUPERVISOR_STATE_PATH`（指向持久卷）
5. `DOCKREV_IMAGE_REPO`（与你实际部署的 Dockrev 镜像仓库一致）

## API 核心配置（`dockrev-api`）

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `DOCKREV_HTTP_ADDR` | `0.0.0.0:50883` | API 监听地址 |
| `DOCKREV_DB_PATH` | `./data/dockrev.sqlite3` | SQLite 文件路径 |
| `DOCKREV_DOCKER_CONFIG` | 空 | Docker registry 凭据文件 |
| `DOCKREV_COMPOSE_BIN` | `docker-compose` | Compose 命令选择 |
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | 转发认证头名 |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | 开发态匿名开关 |
| `DOCKREV_SELF_UPGRADE_URL` | `/supervisor/` | UI 中“升级 Dockrev”跳转地址 |
| `DOCKREV_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | 用于识别 Dockrev 自身服务 |
| `DOCKREV_WEBHOOK_SECRET` | 空 | `/api/webhooks/trigger` 共享密钥 |
| `DOCKREV_HOST_PLATFORM` | 空 | 覆盖主机平台（如 `linux/amd64`） |
| `DOCKREV_DISCOVERY_INTERVAL_SECONDS` | `60` | 自动发现周期 |
| `DOCKREV_DISCOVERY_MAX_ACTIONS` | `200` | 扫描返回动作上限 |

说明：

- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` 仅建议本地开发使用；生产必须关闭。
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
| `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | 自升级目标镜像仓库 |
| `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID` | 空 | 覆盖自动匹配容器 |
| `DOCKREV_SUPERVISOR_DOCKER_HOST` | 空 | Docker endpoint |
| `DOCKREV_SUPERVISOR_COMPOSE_BIN` | `docker-compose` | Compose 命令选择 |
| `DOCKREV_SUPERVISOR_STATE_PATH` | `./data/supervisor/self-upgrade.json` | 状态文件路径 |

## 生产基线建议

- 关闭匿名模式
- 固定 forward header 并由网关注入
- 使用持久卷保存 DB 与 supervisor state
- 限制 Docker socket 暴露面（可改用 docker-socket-proxy）
- 修改配置后，执行一次 `GET /api/deploy-check/report` 做回归核验
