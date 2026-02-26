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
2. 校验反向代理是否可注入认证头。
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
- 在网关注入 `DOCKREV_AUTH_FORWARD_HEADER_NAME` 指定的转发头
- 使用持久化 DB 路径（例如 `/data/dockrev.sqlite3`）
- 对 compose 文件目录做“同绝对路径只读挂载”

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
- `GET /api/deploy-check/report` 可返回 deploy check 报告
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
