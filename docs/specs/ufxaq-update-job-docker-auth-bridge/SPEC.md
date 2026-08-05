# Dockrev：Update Job Docker 凭据透传桥接（#ufxaq）

## 状态

- Status: 已完成
- Created: 2026-03-10
- Last: 2026-03-10

## 背景 / 问题陈述

- `DOCKREV_DOCKER_CONFIG` 目前只被 Dockrev 自己读取，用于 registry 扫描与预检。
- update job 触发的 `docker-compose pull/up/ps` 与 semver fallback `docker pull` 没有继承这份凭据，导致私有仓库镜像在扫描阶段可见、更新阶段却报 `unauthorized`。
- 生产部署通常只会把宿主机的 `config.json` 挂到容器内某个自定义路径（例如 `/data/docker-config.json`），不会额外挂到 `/root/.docker/config.json`；项目需要对这种部署方式负责。

## 目标 / 非目标

### Goals

- 让 update job 使用与 registry 扫描一致的 Docker 凭据来源。
- 保持 `DOCKREV_DOCKER_CONFIG` 的现有对外语义：它仍是 Docker `config.json` 的文件路径，而不是目录路径。
- 不要求运维额外挂载 `/root/.docker/config.json`；Dockrev 需要自行桥接 Docker CLI 默认读取路径。
- 覆盖 `docker-compose pull/up/ps`、更新后的 `docker image tag`，以及 semver fallback `docker pull`。

### Non-goals

- 不修改 registry 扫描/预检逻辑的鉴权来源。
- 不新增公开 API、数据库字段或新的运行时配置项。
- 不扩展 `DOCKREV_DOCKER_CONFIG` 去支持目录路径、credential helper 注入策略调整或线上部署迁移。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/updater.rs`：为单次 update job 创建临时 Docker CLI 配置目录，把 `DOCKREV_DOCKER_CONFIG` 投影到 `<tmp-workspace>/.docker/config.json`；若源路径本身就是 `config.json`，则连同 `contexts/` 元数据一起复制，并统一生成 `DOCKER_CONFIG` env overlay。
- `crates/dockrev-api/src/compose_runner.rs` 与 `crates/dockrev-api/src/docker_runner.rs`：让 compose/docker 子命令继承统一 env overlay。
- `crates/dockrev-api/src/api/operations.rs`：把 `docker_config_path` 传入 updater 执行路径。
- 回归测试：覆盖 auth bridge staging、compose/docker env 注入、unset config 无回归、plugin 进度 env 共存。
- 文档：更新 deploy / README / docs-site 配置说明，明确项目已内建 Docker CLI auth bridge。

### Out of scope

- 调整 101 线上部署或任何生产 compose 文件。
- 修改 `dockrev-supervisor` 的鉴权/自升级路径。
- 为 update job 增加新的 UI 配置或设置项。

## 需求（Requirements）

### MUST

- 当 `DOCKREV_DOCKER_CONFIG` 已配置时，update job 在真正执行命令前必须生成单次 job 级别的临时 Docker CLI 配置目录，并将原文件复制到 `<tmp-workspace>/.docker/config.json`。
- 所有由 updater 发出的 compose/docker CLI 命令都必须继承同一组 `DOCKER_CONFIG` env，且不得覆写进程原有 `HOME`。
- `docker` plugin 模式下带流式进度的 pull 必须使用 `COMPOSE_PROGRESS=tty` 与 `COMPOSE_ANSI=always`，并与 auth env 共存；不得退回 `plain`，否则 layer 更新会失去终端原地刷新语义。
- `DOCKREV_DOCKER_CONFIG` 未配置时，update job 命令环境保持当前行为，不额外注入 auth env。
- 若 `DOCKREV_DOCKER_CONFIG` 指向真实文件名 `config.json`，临时 Docker CLI 配置目录还必须复制其 `contexts/` 元数据。
- 相关临时目录必须在 job 生命周期结束后清理，不引入新的持久状态。

### SHOULD

- 失败信息保持在现有更新步骤语义下暴露，不引入新的公开错误字段。
- 测试应直接覆盖“非默认文件名路径”的场景，证明无需 `/root/.docker/config.json` 也能完成透传。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- update job 启动后，在 dry-run 之外先根据 `DOCKREV_DOCKER_CONFIG` 决定是否创建 auth bridge。
- auth bridge 生成的 `DOCKER_CONFIG` env overlay 会同时挂到 compose runner 与 docker runner，因此 `ps/pull/up/tag/pull semver` 全部走同一份 Docker CLI 认证环境。
- 命令执行结束后，临时 auth workspace 自动清理，不影响容器内其他进程的默认 `HOME` 与 compose 变量插值。

### Edge cases / errors

- 若 `DOCKREV_DOCKER_CONFIG` 未设置，则不创建临时 auth workspace，更新路径行为与修复前一致。
- 若本次筛选后没有待更新服务，则直接返回 noop success，不提前 staging auth bridge。
- 若 `DOCKREV_DOCKER_CONFIG` 指向的文件不可复制，update job 可直接失败并保留上下文错误，而不是静默退回到“无凭据执行”。
- dry-run 不创建临时 auth workspace，也不触发任何命令执行。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `updater::run_update_job` 参数 | internal | backend | Modify | None | backend | backend | 新增可选 `docker_config_path` 输入 |
| `ComposeRunnerConfig` / `DockerRunnerConfig` | internal | backend | Modify | None | backend | backend | 统一承载命令级 env overlay |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given `DOCKREV_DOCKER_CONFIG=/data/docker-config.json` 这类非默认文件路径，When update job 生成 `docker-compose pull/up` 与 `docker pull`，Then 命令都带有可复用的 `DOCKER_CONFIG` env，而不是依赖 `/root/.docker/config.json`。
- Given `DOCKREV_COMPOSE_BIN=docker`，When 执行带流式进度的 pull，Then `COMPOSE_PROGRESS=tty`、`COMPOSE_ANSI=always` 与 auth env 同时存在。
- Given `DOCKREV_DOCKER_CONFIG` 未配置，When 执行同一条 update job，Then 命令 env 不新增 auth 相关变量。
- Given dry-run 模式，When 触发 update job，Then 不执行命令，也不创建 auth workspace。
- Given `DOCKREV_DOCKER_CONFIG` 指向真实文件名 `config.json`，When update job 创建 auth bridge，Then `contexts/` 元数据也会被复制到临时 workspace。
- Given semver fallback 需要额外 `docker pull <repo>:<semver>`，When 命令发出，Then 同样继承 auth env。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api updater -- --nocapture`
- `cargo test -p dockrev-api compose_runner -- --nocapture`
- `cargo fmt --all --check`

## 变更记录（Change log）

- 2026-03-10：创建规格，冻结 `DOCKREV_DOCKER_CONFIG` 的 update-job auth bridge 范围、兼容性承诺与测试口径。
- 2026-03-10：完成 updater auth bridge、compose/docker env 透传、回归测试与部署文档同步。
- 2026-03-10：根据 review 收敛，改为仅注入 `DOCKER_CONFIG` 以避免 compose `${HOME}` 插值副作用，并补充真实 `config.json` 的 `contexts/` 元数据复制约束。

## 参考（References）

- `crates/dockrev-api/src/updater.rs`
- `crates/dockrev-api/src/compose_runner.rs`
- `crates/dockrev-api/src/docker_runner.rs`
- `crates/dockrev-api/src/api/operations.rs`
- `deploy/README.md`
- `docs-site/docs/config.md`
