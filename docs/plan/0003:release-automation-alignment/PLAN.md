# CI/CD：自动发布（GHCR + Release Assets + 单镜像 + 嵌入式 Web）对标与补齐（#0003）

## 状态

- Status: 待实现
- Created: 2026-01-20
- Last: 2026-01-20

## 背景 / 问题陈述

当前仓库已存在 GitHub Actions 工作流（`.github/workflows/ci.yml`），其意图是在 `main` 上自动创建 tag 与 GitHub Release，并发布 Docker 镜像到 GHCR。

但现状存在“流程口径与仓库实际结构未对齐”的风险点（例如 Dockerfile 路径与镜像形态），导致自动发布可能不可用或不可重复。主人希望对标参考仓库 `IvanLi-CN/tavily-hikari` 的发布方式，明确本仓库的目标形态与验收标准。

## 已确认决策（主人已拍板）

- 发布渠道：GHCR + GitHub Release（Release 必须附带二进制产物）。
- Docker 镜像：仅允许单镜像（禁止出现双镜像），镜像名固定为 `dockrev`。
- 触发策略：`push` 到 `main` 自动发布，同时也支持 `release: published` 触发发布（用于手动发布/兜底）。
- 版本可观测：发布镜像内必须包含版本元数据（OCI labels / env / API 口径对齐）。
- Web UI 静态资源：必须嵌入到服务端程序（不依赖运行时挂载/复制静态文件目录）。
- Release assets 平台：至少包含 `linux/amd64` 与 `linux/arm64`。
- Release assets 二进制名：固定为 `dockrev`。
- Release assets libc：同时发布 `gnu` 与 `musl` 变体（同一版本提供两套 Linux 产物，降低发行版兼容风险）。
- Docker 镜像运行时 base：`alpine:3.20`。
- Docker Engine 访问方式：同时支持
  - 直挂 `/var/run/docker.sock`（默认示例）
  - `docker-socket-proxy`（通过 `DOCKER_HOST=tcp://docker-socket-proxy:2375`）
- Docker 工具链：镜像内需要包含 `docker` + `docker compose`（CLI/插件），以匹配当前服务端实现的调用方式。
- Docker CLI 版本策略：锁到“当前最新 major”（以 2026-01-20 为准为 `29.x`）。
- Docker CLI + Compose 插件安装来源（冻结）：从官方镜像 `docker:29-cli` 拷贝 CLI 与插件到最终镜像（从而在 `alpine:3.20` 上获得 major=29 的 `docker` 与可用的 `docker compose` / `docker-compose`）。

## 目标 / 非目标

### Goals

- 明确 Dockrev 的发布产物与触发策略，并形成可实现、可验证的 CI/CD 口径：
  - Git tag 与 GitHub Release 的创建策略
  - GHCR 镜像的命名与 tag 策略
  - GitHub Release 二进制产物（assets）的命名与内容约定
  - 在 PR 中对发布链路做“可预演的构建校验”（避免发布时才发现 Dockerfile 不可用）
- 保持版本号口径稳定（以 `Cargo.toml` 与 `compute-version` 逻辑为准），并保证发布流程可幂等运行。
- 对标 `tavily-hikari`：补齐本仓库在“release 事件触发 / web 构建时机 / Dockerfile 形态”等方面的差异点，给出明确取舍。

### Non-goals

- 不在本计划内改动 Dockrev 的业务功能与 API 语义。
- 不在本计划内引入新的发布平台/渠道（例如 crates.io / Homebrew / APT），除非主人明确要求。
- 不在本计划内扩展到更多平台（例如 macOS / Windows）或更多架构（除 `linux/amd64` + `linux/arm64` 以外），除非主人明确要求。

## 用户与场景

- 维护者：合并到 `main` 后，希望自动得到一个新的版本（tag + GitHub Release），并能在 GHCR 拉取到对应版本镜像。
- 部署者：希望以固定版本号部署（例如 `v0.1.3`），并能选择是否跟随 `latest`。
- CI：在 PR 阶段就能验证“发布相关构建”不会在合并后失败。

## 需求（Requirements）

### MUST

- CI/CD 必须能稳定地产出版本号（`APP_EFFECTIVE_VERSION`）并形成一致的 tag（`v<semver>`）。
- 在 `main` 分支上，自动发布流程必须满足：
  - 通过 lint/tests 后才允许进入发布步骤
  - tag/release 创建具备幂等性（重复运行不应失败；tag 已存在应安全跳过或明确失败原因）
  - 发布到 GHCR 的镜像 tag 中必须包含版本 tag（`v<semver>`）
- Docker 发布产物必须满足：
  - 仅允许单镜像：`ghcr.io/<owner>/dockrev`（禁止出现 `dockrev-api`/`dockrev-web` 等双镜像）
  - 镜像 tags 至少包含：`v<APP_EFFECTIVE_VERSION>`（以及默认分支的 `latest`）
- GitHub Release 必须附带二进制产物（assets），且命名与内容符合契约。
- 发布镜像必须写入版本元数据（见契约），并在 API 层提供版本口径（见契约）。
- Web UI 必须由服务端进程直接提供：
  - `GET /` 可访问（返回 HTML）
  - 相关静态资源路径可访问（例如 Vite 产出的 `/assets/*`）
  - 静态资源来自“嵌入在二进制内”的构建产物（不依赖运行时文件系统）
- CI 在构建完成后必须执行运行测试（smoke test）：
  - 能启动服务端进程
  - `GET /api/health` 返回 ok（作为存活探针）
  - `GET /` 返回 HTML 且可被浏览器加载（最小化用 curl/HTTP 校验即可）
- 在 PR 中必须执行“发布链路的构建校验”（不推送）：
  - 至少验证目标 Dockerfile(s) 可成功构建（与计划中的镜像形态一致）
- 版本策略需有明确规则：
  - 版本来源（`Cargo.toml`）与“已存在 tag 时自动 patch +1”的行为是否保留
  - 何时 bump minor/major（由开发者在 `Cargo.toml` 修改触发）

## 唯一流程（Canonical CI/CD Flow）

本节定义“唯一且可重复”的发布流程：实现阶段不得再新增分支流程/可选分支，避免出现“这次走 A、下次走 B”导致不可复现。

### 统一版本口径

- 统一版本变量：`APP_EFFECTIVE_VERSION=<semver>`（不带 `v` 前缀）。
- `push main`：
  - 运行 `.github/scripts/compute-version.sh` 得到 `APP_EFFECTIVE_VERSION`
  - 创建/推送 tag：`v${APP_EFFECTIVE_VERSION}`（已存在则跳过）
- `release: published`：
  - 直接从 release tag 推导版本：`APP_EFFECTIVE_VERSION = <tag_name 去掉 v 前缀>`（例如 `v0.1.0` → `0.1.0`）

### PR（pull_request → main）：只做“可预演校验”

1. `lint` + `unit-tests`
2. 构建 Web（`web/` 存在时：`npm ci` + `npm run build`）
3. 构建 `dockrev`（至少 `linux/amd64` 的 `*-unknown-linux-musl`，用于后续 smoke test）
4. 运行 smoke test（见契约：`contracts/cli.md`）
5. 构建 Docker 单镜像（不 push），并验证可构建 multi-arch：
   - `platforms: linux/amd64,linux/arm64`
   - `file: ./Dockerfile`（单镜像 Dockerfile）

> PR 阶段不创建 tag/release，不上传 Release assets，不推送 GHCR。

### 自动发布（push → main）：一次性产出全部产物

1. `lint` + `unit-tests`
2. 计算 `APP_EFFECTIVE_VERSION` → 创建/推送 `v${APP_EFFECTIVE_VERSION}`
3. 构建 Web（`npm ci` + `npm run build`）
4. 构建 Release assets（4 targets：amd64/arm64 × gnu/musl），并生成 `sha256`
5. 运行 smoke test（至少对 linux/amd64 的实际运行产物做验证；web/health/version 三项都要通过）
6. 创建或更新 GitHub Release，并“替换式上传” assets（幂等）
   - 采用 `ncipollo/release-action@v1`：`allowUpdates: true` + `replacesArtifacts: true`
7. 构建并推送 GHCR 单镜像：
   - Image: `ghcr.io/<owner>/dockrev`
   - Platforms: `linux/amd64,linux/arm64`
   - Tags: `v${APP_EFFECTIVE_VERSION}` + `latest`

### 手动/兜底发布（release: published）：同一套发布逻辑

1. 从 release tag 解析 `APP_EFFECTIVE_VERSION`
2. 执行与 “push main” 相同的步骤 #3–#7，但 **不更新 `latest`**（只推 `v${APP_EFFECTIVE_VERSION}`），避免手动发布意外改变默认版本指向。

### Docker 镜像内 CLI/Compose（冻结口径）

- 最终镜像 base：`alpine:3.20`
- 在 Dockerfile 中通过多阶段构建从 `docker:29-cli` 拷贝：
  - `docker` CLI
  - `docker compose` 插件（并提供 `docker-compose` 命令），以匹配当前配置默认 `DOCKREV_COMPOSE_BIN=docker-compose`
- Docker Engine 连接优先级（避免双口径不确定性）：
  - 若设置 `DOCKER_HOST`：由 Docker CLI/Compose 使用该值（用于 `docker-socket-proxy`）
  - 否则：使用默认 unix socket（要求挂载 `/var/run/docker.sock`）

## 接口清单与契约（Interface Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GitHub Actions CI/CD workflow | Config | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | CI | `.github/workflows/ci.yml`（含 release 相关 jobs） |
| Effective version computation | CLI | internal | Modify | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | `.github/scripts/compute-version.sh` |
| CI smoke test script | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | `.github/scripts/smoke-test.sh` |
| Git tag format | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | CI/users | `v<semver>` |
| GHCR image naming & tagging | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | `ghcr.io/<owner>/<image>:<tag>` |
| GitHub Release assets naming | File format | external | New | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | Release 附带二进制产物（assets） |
| `GET /api/version` | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/users | 版本可观测口径 |
| `GET /api/health` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | CI/users | 存活探针 |
| Static UI routes | HTTP API | external | New | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | browsers | `GET /` 与 `/assets/*` |

### 契约文档（按 Kind 拆分）

- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)
- [contracts/http-apis.md](./contracts/http-apis.md)

## 验收标准（Acceptance Criteria）

- Given 合并一个提交到 `main`
  When CI 通过 lint/tests 并进入发布阶段
  Then 若目标 tag `v<APP_EFFECTIVE_VERSION>` 不存在，则自动创建并推送 tag，且创建对应 GitHub Release（包含自动生成的 release notes）

- Given 合并一个提交到 `main`
  When 发布阶段执行镜像构建与推送
  Then GHCR 上存在与本次发布版本一致的单镜像 `ghcr.io/<owner>/dockrev`，且 tag 至少包含 `v<APP_EFFECTIVE_VERSION>`

- Given 合并一个提交到 `main`
  When 发布阶段创建 GitHub Release
  Then 该 Release 附带二进制 assets（命名与内容符合契约，且同时包含 gnu+musl 的 linux/amd64+arm64）

- Given 有一个 PR 指向 `main`
  When CI 在 PR 上运行
  Then 会执行发布链路的构建校验（不 push），且构建成功

- Given 已发布的单镜像在运行中
  When 调用 `GET /api/version`
  Then 返回的 `version` 与发布版本一致（`APP_EFFECTIVE_VERSION`）

- Given 已发布的单镜像存在
  When 通过 `docker inspect` 查看 OCI labels
  Then `org.opencontainers.image.version` 与发布版本一致

- Given CI 构建完成后运行 smoke test
  When 启动服务端进程并等待就绪
  Then `GET /api/health` 返回 `200` 且 body 为 `ok`

- Given CI 构建完成后运行 smoke test
  When 请求 `GET /`
  Then 返回 `200` 且 Content-Type 为 `text/html`（或等价），并包含可识别的 HTML 结构（例如 `<!doctype html>`）

- Given 主人手动发布一个 GitHub Release（`release: published`）
  When CI 在 release 事件上运行发布步骤
  Then 会推送对应版本的单镜像 `ghcr.io/<owner>/dockrev:v<semver>`，并为该 Release 上传二进制 assets（命名与内容符合契约）

- Given 发布流程被重复触发（例如重跑 workflow）
  When tag 已存在
  Then 流程行为明确且可预测：要么安全跳过 tag 创建并继续后续步骤，要么在不破坏仓库状态的前提下失败并给出明确原因

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing / Checks

- 保持现有门槛不退化：
  - `cargo fmt --check`
  - `cargo clippy -D warnings`
  - `cargo test --all-features`
- PR 阶段新增（或明确补齐）发布相关的构建校验：
  - Docker build（与计划中确定的 Dockerfile 形态一致）
  - 打印并留存发布产物体积（4 个 targets：amd64/arm64 × gnu/musl），用于评估体积与回归

### Security / Permissions

- 发布流程尽量只依赖 `secrets.GITHUB_TOKEN`（最小化额外 secrets）。
- Workflow permissions 必须最小化：
  - 仅发布步骤需要 `contents: write` / `packages: write`
  - 其余步骤保持 `contents: read`

## 约束与风险（Constraints & Risks）

- 当前 CI 存在明显未对齐点：
  - workflow 构建镜像使用 `file: ./Dockerfile`，但仓库当前只有 `deploy/Dockerfile.*`（实现阶段需补齐单镜像 `Dockerfile` 并对齐）。
  - workflow 当前 `PLATFORMS` 仅为 `linux/amd64`，与已确认的 `linux/arm64` 目标不一致（实现阶段需扩展）。
  - workflow 虽已声明触发 `release: published`，但发布相关 job 目前仅在 `push main` 条件下执行（实现阶段需补齐 release 事件的发布路径）。
- 单镜像要求会影响当前 `deploy/` 的形态：
  - 现有 `deploy/docker-compose.yml` 是 API+Nginx 双服务部署；实现阶段需给出“单镜像部署”的替代方式（或明确仅将 `deploy/` 作为本地演示，并同步文档）。
- Release assets 需要可重复上传：已冻结为“替换式上传”（使用 `ncipollo/release-action@v1` 的 `allowUpdates` + `replacesArtifacts`）。

## 需要更新的文档

- `README.md`：新增/补齐 “Releases / Images” 的说明（发布触发、版本规则、如何拉取镜像）。
- `deploy/README.md`：补齐“使用已发布镜像部署”的示例（如纳入本计划范围）。

## 里程碑（Milestones）

- [ ] 单镜像与嵌入式 Web：实现单一二进制 `dockrev`（内嵌 web assets），并补齐 `GET /api/version`（保持 `/api/health` 可用）
- [ ] CI 构建校验：PR 阶段可构建单镜像（不 push），且通过 smoke test（启动→`/api/health`→`/`）
- [ ] `main` 自动发布：tag + GitHub Release（幂等）+ Release assets（linux/amd64 + linux/arm64，gnu+musl）
- [ ] GHCR 自动发布：推送单镜像 `ghcr.io/<owner>/dockrev`（含 `v<semver>` 与 `latest`，并写入版本元数据）
- [ ] 文档同步：`README.md` / `deploy/README.md`

## 风险与开放问题（需主人决策）

- None

## 假设（Assumptions，待主人确认）

- None
