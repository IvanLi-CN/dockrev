# File / Config Contracts（#0003）

本文件定义发布相关的“可持续约定”（tags、镜像命名、GitHub Actions 触发与权限），用于让 CI/CD 的行为稳定且可验证。

## Git tag & GitHub Release naming

- Tag format: `<semver>`
  - Example: `0.1.0`
- Release:
  - Release name 与 tag 对齐（例如 `0.1.0`）
  - Release notes：默认使用 GitHub 自动生成（`generate_release_notes: true`）

## GHCR image naming & tagging

本计划已确定：**仅允许单镜像**（禁止出现双镜像）。

### Single image (fixed)

- App image: `ghcr.io/ivanli-cn/dockrev`
- Tags (minimum):
  - `<semver>`（例如 `0.1.0`）
  - `latest`（仅默认分支 / `main`）

### Explicitly forbidden

- 禁止发布/推送任何双镜像命名（例如 `dockrev-api` / `dockrev-web`）。

## GitHub Actions workflow contract (release-related)

### Triggers

- `pull_request`（to `main`）：执行 lint/tests + 发布链路构建校验（不 push）
- `workflow_run`（`CI (main)` 成功后触发 `Release`）：自动打 tag/推送 GHCR 镜像（含 `latest`）+ 创建/更新 GitHub Release 并上传 Release assets

### Required permissions

- Default: `contents: read`
- Release steps (tag/release):
  - `contents: write`
- GHCR push:
  - `packages: write`

### Dockerfile paths (current repo reality)

- `deploy/Dockerfile.api`
- `deploy/Dockerfile.web`

### Target Dockerfile (for single image)

- `Dockerfile`（repo root）

### Binary inside Docker image

- Runtime base 已确定为 `alpine:3.20`（musl），因此镜像内的服务端二进制应采用 musl target：
  - `x86_64-unknown-linux-musl`（linux/amd64）
  - `aarch64-unknown-linux-musl`（linux/arm64）
- `*-unknown-linux-gnu` 仍会作为 GitHub Release assets 发布（用于 glibc 发行版），但不作为 alpine 镜像内的默认运行二进制。

## GitHub Release assets

Release 必须附带二进制产物（assets）。

### Asset set (minimum)

同一版本必须同时提供 `gnu` 与 `musl` 变体（覆盖更多发行版）。

- `dockrev_<semver>_linux_amd64_gnu.tar.gz`
- `dockrev_<semver>_linux_arm64_gnu.tar.gz`
- `dockrev_<semver>_linux_amd64_musl.tar.gz`
- `dockrev_<semver>_linux_arm64_musl.tar.gz`
  - Contents (each):
    - `dockrev`（server binary）
- Checksums:
  - `dockrev_<semver>_linux_amd64_gnu.tar.gz.sha256`
  - `dockrev_<semver>_linux_arm64_gnu.tar.gz.sha256`
  - `dockrev_<semver>_linux_amd64_musl.tar.gz.sha256`
  - `dockrev_<semver>_linux_arm64_musl.tar.gz.sha256`

### Idempotency

- 同一 tag 的 Release 上重复运行上传步骤时，workflow 必须具备幂等策略（固定为“替换式上传”）：
  - 使用 `ncipollo/release-action@v1`：`allowUpdates: true` + `replacesArtifacts: true`

### Build targets (frozen)

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`

## Version metadata (required)

### OCI labels (Docker image)

- `org.opencontainers.image.version=<semver>`
- `org.opencontainers.image.revision=<git-sha>`
- `org.opencontainers.image.source=https://github.com/<owner>/<repo>`

### Runtime env

- `APP_EFFECTIVE_VERSION=<semver>`

### HTTP API alignment

- `GET /api/version` 的 `version` 字段必须与 `APP_EFFECTIVE_VERSION` 一致（见 `contracts/http-apis.md`）。

## Embedded web assets (required)

- 静态资源必须嵌入二进制（build-time embed），运行时不依赖外部 `web/dist` 目录。
- `GET /` 必须返回嵌入的 `index.html`（由前端构建产出）。
- `/assets/*` 必须可访问（由前端构建产出；具体文件名由构建系统决定）。

## Docker image base & toolchain pinning (to be frozen)

实现阶段需要在 CI 与 Dockerfile 中明确锁定这些版本，避免“今天能构建、明天突然不行”的漂移：

- Rust toolchain: `1.91.0`（与 `.github/workflows/ci.yml` 对齐）
- Node.js: `20`（用于前端构建；与 `deploy/Dockerfile.web` 对齐）
- Docker image runtime base: `alpine:3.20`
- Docker CLI major (as of 2026-01-20): `29.x`
  - 现实校验（Alpine v3.20）：
    - `docker-cli`: `26.1.5-r0`
    - `docker-cli-compose`: `2.27.0-r3`
  - 冻结安装来源：从官方镜像 `docker:29-cli` 拷贝 Docker CLI 与插件到最终镜像（避免依赖 Alpine v3.20 的旧版本包）。
    - `docker` binary：`/usr/local/bin/docker`
    - `docker compose` plugin：`/usr/local/libexec/docker/cli-plugins/docker-compose`
    - `docker-compose` command：由同一插件提供（与 `docker:29-cli` 的做法一致）

## Docker Engine access (runtime)

Dockrev 需要访问宿主机 Docker Engine 才能执行检查与更新。本计划要求**同时支持**两种部署方式（但默认示例走直挂 socket）。

### Option A: direct socket mount (default)

- Required mount: `/var/run/docker.sock:/var/run/docker.sock`
- `DOCKER_HOST`：不设置（使用默认 unix socket）

### Option B: `docker-socket-proxy`

- Dockrev container:
  - 不挂载 `/var/run/docker.sock`
  - 设置：`DOCKER_HOST=tcp://docker-socket-proxy:2375`
- `docker-socket-proxy` container:
  - Required mount: `/var/run/docker.sock:/var/run/docker.sock`
  - 端口：对 Dockrev 所在网络暴露 `2375`
  - 权限变量（最小集合；满足 `docker` + `docker compose` 的读写操作）：
    - `POST=1`（允许写操作）
    - `CONTAINERS=1`
    - `IMAGES=1`
    - `NETWORKS=1`
    - `VOLUMES=1`
    - `INFO=1`

> 说明：`docker-socket-proxy` 默认会拒绝未显式允许的 API 分组；上面的白名单是为了覆盖 Dockrev 当前通过 `docker inspect`、`docker image tag`、`docker compose pull/up/ps` 的需求。
