# Config（Dockrev Auto-Discovery / #0007）

本文件定义本计划新增/变更的运行时配置项（主要为环境变量）。

## Environment variables

说明：自动发现为基础能力，**无 enable 开关**（始终启用）；仅支持调整 interval 与响应裁剪等参数。

### `DOCKREV_DISCOVERY_INTERVAL_SECONDS`

- Type: integer (seconds)
- Default: `60`
- Constraints:
  - `>= 10`（建议；避免过度扫描 Docker API）
- Meaning: discovery 周期扫描间隔。

### `DOCKREV_DISCOVERY_MAX_ACTIONS`

- Type: integer
- Default: `200`
- Meaning: `POST /api/discovery/scan` 返回的 `actions[]` 最大条数（超出则截断，并在 summary/notes 中体现）。

## Related existing config (non-exhaustive)

- Docker API access: `DOCKER_HOST=tcp://docker-socket-proxy:2375`（或等价）
- Auth:
  - `DOCKREV_AUTH_FORWARD_HEADER_NAME`（forward header 名）
  - `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`（验收环境要求）
