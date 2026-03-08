# Dockrev：服务端模块拆分 + codex-testbox 全面部署/E2E 验收门槛（#mvjy8）

## 状态

- Status: 部分完成（5/7）
- Created: 2026-03-08
- Last: 2026-03-08
- Notes: fast-track（服务端热点文件拆分 + shared testbox real deploy smoke + workflow E2E all）

## 背景 / 问题陈述

- `dockrev-api` 的 `api/mod.rs`、`api/types.rs`、`db.rs` 已经演变成高耦合热点文件，职责混杂，导致后续功能迭代、review 与回归验证成本持续升高。
- `dockrev-supervisor` 的 `app.rs` 同时承担路由、UI、鉴权、元信息、自升级编排与状态持久化，入口文件过重。
- 仓库已经具备 `codex-testbox` 手动 E2E workflow 与三条 testbox 场景脚本，但尚未覆盖“真实部署拓扑 smoke”，也没有把实机部署 + workflow 全量回归冻结为此次重构的硬门槛。

## 目标 / 非目标

### Goals

- 将 `dockrev-api` 的 API、DTO、DB 热点文件按稳定边界拆分，保持外部 HTTP/JSON/SSE/DB 契约不变。
- 将 `dockrev-supervisor` 的 `app.rs` 拆成入口壳层与按职责划分的子模块，保持自升级/回滚行为不变。
- 新增一个 `scripts/testbox/full-deploy-smoke.e2e.ts` 场景，使用 `deploy/docker-compose.yml` 的部署拓扑在 `codex-testbox` 上完成真实部署 smoke。
- 将 `full-deploy-smoke` 纳入 `.github/workflows/e2e-testbox.yml` 的 `scenario=all`，并把 `codex-testbox` 实机部署验证与 workflow 全量回归冻结为快车道阻塞门槛。

### Non-goals

- 不修改对外路由、请求/响应 JSON、SSE payload、数据库 schema/migration id、环境变量语义。
- 不在本次 PR 深拆 `notify.rs`、`updater.rs`、`api/tests.rs`；仅允许为模块边界调整做最小联动。
- 不把 Traefik / Authelia 代理栈扩展为本次 blocking gate 的必跑场景。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/**`
- `crates/dockrev-api/src/db/**`
- `crates/dockrev-supervisor/src/app/**`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-supervisor/src/main.rs`
- `scripts/testbox/**`
- `.github/scripts/testbox-e2e.sh`
- `.github/workflows/e2e-testbox.yml`
- `docs/specs/README.md`

### Out of scope

- `deploy/examples/traefik-authelia/**` 的代理拓扑扩展。
- release 流程与镜像命名策略调整。
- 默认 PR/main CI 是否自动接入 testbox workflow 的策略变更。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `dockrev-api` HTTP routes / JSON / SSE | API contract | external | None | None | api | Web UI / operators | 本次仅做结构拆分 |
| `dockrev-supervisor` `/supervisor` / `/supervisor/` 行为 | HTML/API contract | external | None | None | supervisor | operators | 保持兼容 |
| `scenario=all` for `E2E (codex-testbox)` | GitHub Actions workflow | external | Modify | None | infra | maintainers | 新增 `full-deploy-smoke` |
| `scripts/testbox/full-deploy-smoke.e2e.ts` | Test runner entry | internal | New | None | infra | maintainers / CI | 真实部署 smoke，禁止源码 docker build |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 本次服务端模块拆分完成，When 运行现有 API / Supervisor 自动化测试，Then 结果与拆分前一致，且无对外契约回归。
- Given `codex-testbox` 上执行 `full-deploy-smoke`，When 场景完成，Then 远端运行目录位于 `/srv/codex/workspaces/$USER/.../runs/<RUN_ID>`，Compose 项目名唯一，且不会在 testbox 上执行源码 `docker build`。
- Given `full-deploy-smoke` 启动 `deploy/docker-compose.yml`，When smoke 验证运行，Then `GET /`、`GET /api/health`、`GET /supervisor/` 都可访问且通过断言。
- Given 维护者手动触发 `E2E (codex-testbox)` 且 `scenario=all`，When workflow 完成，Then `full-deploy-smoke`、`check-job-recovery`、`check-version-inference-sse`、`check-service-update-no-semver-pull` 全部执行，并上传 `.artifacts/testbox-e2e/` artifact。
- Given 本次进入 fast-track PR 阶段，When 宣告收工，Then 必须同时满足：PR 已创建、常规 checks 明确通过、`codex-testbox` 实机部署通过、workflow `scenario=all` 通过、review-loop 无阻塞。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo fmt --check`
- `cargo test -p dockrev-api`
- `cargo test -p dockrev-supervisor`
- `cargo test`
- `bun scripts/testbox/full-deploy-smoke.e2e.ts`
- GitHub Actions: `E2E (codex-testbox)` with `scenario=all`, `repeat_count=2`, `keep_remote_artifacts=false`
- 浏览器/HTTP smoke：部署后的 gateway `/`、`/api/health`、`/supervisor/`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `dockrev-api` API 入口拆分为 router 壳层 + 领域 handler 模块；`api/types` 仅保留外部 DTO。
- [x] M2: `dockrev-api` DB 层拆分为 root/bootstrap + 领域实现模块；持久化模型/转换逻辑迁出 API types。
- [x] M3: `dockrev-supervisor` `app.rs` 拆分为入口壳层 + routes/ui/auth/meta/orchestration/state helpers/tests 模块。
- [x] M4: 新增 `full-deploy-smoke` testbox 场景，并纳入 `.github/scripts/testbox-e2e.sh` 与 workflow `scenario=all`。
- [x] M5: 本地验证通过，并在 `codex-testbox` 完成一次真实部署 smoke。
- [ ] M6: 创建/更新 PR，等待常规 checks + `E2E (codex-testbox)` 全量回归通过。
- [ ] M7: 通过 review-loop 收敛阻塞项，并同步 spec 状态/证据。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：热点文件拆分跨度大，若模块边界处理不稳，容易引入编译可见性与测试夹具回归。
- 风险：`codex-testbox` 共享环境存在网络/磁盘波动，workflow 可能出现外部不稳定性。
- 假设：共享测试机继续满足 `ssh codex-testbox`、Docker 可用、且 `/srv/codex/workspaces/$USER` 可写。
- 假设：testbox 的 LXC `CAP_SETFCAP` 限制依旧存在，因此真实部署 smoke 必须消费预构建产物或等价镜像。

## 变更记录（Change log）

- 2026-03-08: 创建规格，冻结服务端热点文件拆分范围与 `codex-testbox` 实机部署 + workflow 全量回归门槛。
- 2026-03-08: 完成 `dockrev-api` / `dockrev-supervisor` 热点文件拆分，补齐 `full-deploy-smoke` 与 `scenario=all` 四场景本地回归，并将 semver update 场景改为消费 HTTPS 预构建镜像路径。
