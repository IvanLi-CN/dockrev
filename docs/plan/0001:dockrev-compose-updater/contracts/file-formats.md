# 文件格式（File formats）

本计划涉及的“文件接口”主要用于：

- 描述 stack 的注册形状（compose 文件路径、env 文件路径）
- 在后续实现中（可能）改写 `.env` / compose 文件中的 tag（如采用 CLI 驱动方案）

## Stack compose definition（compose path based）

- 范围（Scope）: internal
- 变更（Change）: New
- 编码（Encoding）: UTF-8

### Schema（结构）

在 DB 与 HTTP API 中统一为：

```json
{
  "type": "path",
  "composeFiles": ["/abs/path/docker-compose.yml"],
  "envFile": "/abs/path/.env",
  "backup": {
    "targets": [
      { "kind": "docker-volume", "name": "app_db_data" },
      { "kind": "bind-mount", "path": "/srv/data/app" }
    ],
    "retention": {
      "keepLast": 1,
      "deleteAfterStableSeconds": 3600
    }
  }
}
```

### Compatibility / migration

- 仅允许新增字段；不得改变现有字段语义。

## Backup artifacts（预更新备份产物）

- 范围（Scope）: internal
- 变更（Change）: New
- 编码（Encoding）: binary

### Path pattern（路径约定）

默认 base dir（可配置）：`/data/backups`

产物路径建议：

- `/data/backups/<stackId>/<YYYYMMDD-HHMMSSZ>.tar.zst`

### Notes

- 备份目标由 stack 的 `backup.targets` 指定（见 HTTP API 契约）。
- MVP 覆盖 Docker named volumes + bind mounts（host 路径）。
- `tar.zst` 只是建议；实现阶段可选择 gzip 或不压缩，但必须在 UI 显示实际格式。

## Runtime config（环境变量）

以环境变量作为运行时配置接口（internal）。

建议的关键配置项：

- `DOCKREV_HTTP_ADDR`
- `DOCKREV_DOCKER_HOST`（默认 unix socket）
- `DOCKREV_DOCKER_CONFIG`（默认 `~/.docker/config.json`）
- `DOCKREV_COMPOSE_BIN`（默认 `docker-compose`）
- `DOCKREV_AUTH_FORWARD_HEADER_NAME`（默认 `X-Forwarded-User`）
- `DOCKREV_AUTH_ALLOW_ANON_IN_DEV`（默认 `true`）
- `DOCKREV_BACKUP_BASE_DIR`（默认 `/data/backups`）
- `DOCKREV_BACKUP_SKIP_TARGETS_OVER_BYTES`（默认 `104857600`，即 `100MiB`）

## Docker config.json（registry credentials）

- 范围（Scope）: external
- 变更（Change）: Modify（读取既有格式，不改写）
- 编码（Encoding）: UTF-8

### Notes

- Dockrev 仅读取 `~/.docker/config.json`（路径可配置），并解析其中 `auths` / `credsStore` / `credHelpers`（具体支持范围在实现阶段确认）。
- Dockrev 不会写回该文件。
