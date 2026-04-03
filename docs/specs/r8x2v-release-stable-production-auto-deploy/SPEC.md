# Dockrev：Stable Release 自动消费生产部署（#r8x2v）

## 状态

- Status: 已完成
- Created: 2026-04-03
- Owner flow: fast-track

## 背景 / 问题陈述

- `Release` workflow 已能正确发布 stable tag、GHCR `latest` 与 GitHub Release，但生产环境 `dockrev.ivanli.cc` 仍依赖宿主机手工执行 `docker compose pull && up -d` 才会消费新镜像。
- 这会导致“release 成功”与“线上已升级”之间出现人工断层：本次 `0.38.1` 就是 GHCR 已更新、生产仍停留在 `0.38.0`，直到手工补部署才恢复一致。
- 目标是把“stable + latest release 发布完成后自动应用到生产”的消费链路直接接入仓库现有 `Release` workflow，同时保持无凭据仓库/测试 fork 可安全跳过。

## 目标 / 非目标

### Goals

- 对 stable 且 `publish_latest=true` 的 release，提供一个可选的生产部署 job，自动拉取并重建生产 `dockrev` / `dockrev-supervisor`。
- 自动部署必须是“配置齐全才执行，否则显式跳过”，不得因为缺少生产 secrets/vars 让 release 失败。
- 自动部署完成后必须校验公网 `GET /api/version` 已等于当前 `release_tag`。
- 将生产自动部署所需的 GitHub Actions vars/secrets 契约写入仓库文档，避免再次出现“release 成功但无人消费”的隐性缺口。

### Non-goals

- 不改变现有 release intent label、snapshot queue、publication ledger、GitHub Release 资产契约。
- 不把 RC 或 `publish_latest=false` 的历史 stable backfill 自动部署到生产。
- 不引入第二套部署编排平台；继续基于生产现有 `docker compose` 栈执行最小更新。

## 范围（Scope）

### In scope

- `.github/workflows/release.yml`
- `.github/scripts/release_production_deploy.sh`
- `.github/scripts/release-channel-contract-check.sh`
- `deploy/README.md`

### Out of scope

- `crates/**`、`web/**` 的业务功能。
- 生产 Traefik / compose 拓扑重构。
- GitHub 仓库 secrets/vars 的实际创建动作。

## 功能与行为规格

- 仅当 `publish` job 成功、当前 release `publish_latest=true`，且仓库配置了完整的生产 deploy vars/secrets 时，`Release` workflow 才执行生产部署 job。
- 生产部署 job 通过 SSH 连接目标主机，在目标 stack 目录执行：
  - `docker compose pull <services...>`
  - `docker compose up -d <services...>`
  - `docker compose ps <services...>`
- 远端部署后必须逐个验证目标服务容器的 `org.opencontainers.image.version` 标签等于当前 `release_tag`。
- 远端验证通过后，还必须验证公网 `GET /api/version` 返回当前 `release_tag`，否则 job 失败。
- 若缺少任一必需配置，job 不执行 live deploy，但必须在 workflow summary 中明确写出“skip because configuration is missing”。

## 接口契约

### GitHub Actions vars

- `PRODUCTION_DEPLOY_HOST`
- `PRODUCTION_DEPLOY_SSH_PORT`（可选，默认 `22`）
- `PRODUCTION_DEPLOY_USER`
- `PRODUCTION_DEPLOY_STACK_DIR`
- `PRODUCTION_DEPLOY_COMPOSE_FILE`
- `PRODUCTION_DEPLOY_SERVICES`（可选，默认 `dockrev supervisor`）
- `PRODUCTION_DEPLOY_VERSION_URL`

### GitHub Actions secrets

- `PRODUCTION_DEPLOY_SSH_KEY`
- `PRODUCTION_DEPLOY_SSH_KNOWN_HOSTS`

## 验收标准

- Given stable release 产物已成功发布且 `publish_latest=true`，When workflow 持有完整 deploy 配置，Then workflow 会自动应用生产部署并使公网 `/api/version` 等于当前 `release_tag`。
- Given RC release 或历史 stable backfill 且 `publish_latest=false`，When workflow 结束，Then 自动部署 job 不会触发生产更新。
- Given 缺少任一生产 deploy vars/secrets，When stable release workflow 运行，Then 发布继续成功，但 deploy job 明确以“配置缺失”跳过。
- Given deploy job 执行完成，When 检查远端容器与公网接口，Then `dockrev` / `dockrev-supervisor` 的 image version label 与 `/api/version` 都与 `release_tag` 一致。

## 质量门槛

- `bash ./.github/scripts/release-channel-contract-check.sh`
- `bash -n ./.github/scripts/release_production_deploy.sh`

## 文档更新

- `deploy/README.md`
- `docs/specs/README.md`

## 风险 / 假设

- 假设生产环境继续使用 moving tag `latest`，并允许通过 SSH 执行最小 `docker compose pull/up`。
- 风险：若公网入口存在额外缓存/代理延迟，`/api/version` 校验可能需要短暂重试；当前实现先按直接返回校验，后续若出现稳定性问题再补最小重试。
