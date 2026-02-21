# Config contract

本文件定义 Plan 0012 引入的新配置接口（env/flags）。实现必须保持向后兼容：新增配置项需有合理默认值；默认行为可用。

## Web config

### `DOCKREV_SELF_UPGRADE_URL`

- Scope: external
- Change: New
- Default: `/supervisor/`
- Semantics: Dockrev UI 中“升级 Dockrev”跳转目标（由 supervisor 提供页面）；允许是绝对 URL 或站内路径；空/空白值视为未设置并回退到默认值。

### `DOCKREV_IMAGE_REPO`

- Scope: external
- Change: New
- Default: `ghcr.io/ivanli-cn/dockrev`
- Semantics: Dockrev UI 用于识别“Dockrev 自身服务”的 image repo（用于显示“升级 Dockrev”入口）；应填写 repo（不包含 `:tag` 或 `@digest`）。

## Supervisor config

### HTTP listen

#### `DOCKREV_SUPERVISOR_HTTP_ADDR`

- Scope: external
- Change: New
- Default: `0.0.0.0:50884`
- Semantics: supervisor HTTP server listen address.

### Base path / routing

#### `DOCKREV_SUPERVISOR_BASE_PATH`

- Scope: external
- Change: New
- Default: `/supervisor`
- Semantics: supervisor 以该 path prefix 暴露页面与 API（便于反代挂载到同域）。

### Dockrev identity (auto + override)

#### `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO`

- Scope: external
- Change: New
- Default: `ghcr.io/ivanli-cn/dockrev`
- Semantics: 用于自动匹配 Dockrev 容器（repo 匹配：`repo` / `repo:tag` / `repo@digest`）。

#### `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID`

- Scope: external
- Change: New
- Default: empty
- Semantics: 若设置则优先使用该容器作为 Dockrev 自身（覆盖自动匹配）。

#### `DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT`

- Scope: external
- Change: New
- Default: empty
- Semantics: 显式指定 compose project（当无法从 container labels 获取时使用）。

#### `DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE`

- Scope: external
- Change: New
- Default: empty
- Semantics: 显式指定 compose service 名（当无法从 container labels 获取时使用；或当多容器匹配 `TARGET_IMAGE_REPO` 时用于消歧）。

#### `DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES`

- Scope: external
- Change: New
- Default: empty
- Semantics: 逗号分隔的 compose file 绝对路径列表（当无法从 container labels 获取时使用）。

### Docker/compose execution

#### `DOCKREV_SUPERVISOR_DOCKER_HOST`

- Scope: external
- Change: New
- Default: empty (use docker default)
- Semantics: Docker engine 连接（例如 `unix:///var/run/docker.sock` 或 `tcp://docker-socket-proxy:2375`）。

#### `DOCKREV_SUPERVISOR_DOCKER_BIN`

- Scope: external
- Change: New
- Default: `docker`
- Semantics: supervisor 调用 Docker CLI 时使用的可执行文件名或绝对路径（例如 `docker`、`/usr/bin/docker`）。

#### `DOCKREV_SUPERVISOR_COMPOSE_BIN`

- Scope: external
- Change: New
- Default: `docker-compose`
- Semantics: 使用 `docker compose` 还是 `docker-compose`（实现需与 Dockrev 现有约定保持一致；如需使用 plugin 形式则设为 `docker` 或绝对路径如 `/usr/bin/docker`）。

### Dockrev HTTP port (auto-detect)

Supervisor 轮询 Dockrev 的 `/api/health` 与 `/api/version` 以完成升级后检查：

- 优先从目标容器 env 中读取 `DOCKREV_HTTP_ADDR` 并解析端口；
- 若未设置则回退到默认端口 `50883`。

### Persistence

#### `DOCKREV_SUPERVISOR_STATE_PATH`

- Scope: external
- Change: New
- Default: `./data/supervisor/self-upgrade.json`
- Semantics: 自我升级状态文件位置（见 `contracts/file-formats.md`），必须位于持久化卷中。

## Resolution order (must-follow)

1. 若设置 `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID`：直接使用该容器。
2. 否则：自动匹配运行中的 Dockrev 容器（必须保守；不唯一则要求显式配置）。
   - **labels-first（推荐，dangling 也能稳定工作）**：当设置了 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT` 和/或 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE` 时，优先按 compose labels 过滤目标容器：
     - `com.docker.compose.project=<project>`
     - `com.docker.compose.service=<service>`
     - 该路径不依赖 `docker ps` 的 `IMAGE` 字符串，因此即使镜像 tag 被删除导致 `docker ps` 显示短 image id（dangling）也能识别。
     - 候选仍需用 `docker inspect` 的 `Config.Image` 反查，并与 `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` 做 repo match 以校验正确性。
   - **legacy（兼容）**：若未设置 compose selectors（或 labels 过滤未命中），则按 `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` 进行 repo match：
     - 先按 `docker ps` 输出的 `Image` 字段做匹配（`repo` / `repo:tag` / `repo@digest`）。
     - 若仍 0 匹配：对所有运行中容器做 `docker inspect`，使用 `Config.Image` 做 repo match（覆盖 dangling 场景）。
   - **多匹配消歧**：若匹配到多个候选，优先按 compose labels `com.docker.compose.service` 消歧（优先使用 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE`；未设置时默认尝试 `dockrev`，并在可行时排除 `supervisor`）；若仍不唯一：报错并要求显式配置（推荐先设置 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE`，必要时再设置 `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID`）。
3. 对 compose 参数：
   - 优先从目标容器 labels 读取：
     - `com.docker.compose.project`
     - `com.docker.compose.project.config_files`
   - 若 label 缺失或 `config_files` 路径不可读：要求使用 `DOCKREV_SUPERVISOR_TARGET_COMPOSE_*` 显式配置覆盖。

## Semver tag pull（best-effort，避免“悬空”）

为减少 dangling 导致的可观测性/自动识别问题，更新流程会在拉取目标镜像后，按以下规则 **best-effort** 再拉一次 semver tag（失败仅记录 warning，不阻断更新）：

- 从镜像 OCI label `org.opencontainers.image.version` 推断版本号（允许 `v0.7.7` 前缀；若含 build metadata `+...` 则跳过）。
- 若推断成功，则尝试 `docker pull <repo>:<semver>`（例如 `ghcr.io/ivanli-cn/dockrev:0.7.7`）。
