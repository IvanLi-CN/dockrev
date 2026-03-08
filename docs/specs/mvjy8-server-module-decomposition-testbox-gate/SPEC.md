# Dockrev：服务端模块拆分 + GitHub-hosted 官方环境验收（#mvjy8）

- Status: implemented
- Owner: Codex
- Flow: fast-track
- Scope: `dockrev-api` / `dockrev-supervisor` 模块拆分，以及把验收路径收敛到 GitHub Actions 官方环境。
- Notes: fast-track（服务端热点文件拆分 + official GitHub-hosted deploy smoke + backend regression coverage）

## Summary

- 完成 `dockrev-api` / `dockrev-supervisor` 热点文件拆分，保持 HTTP/JSON/SSE/DB 对外契约不变。
- 移除对 `codex-testbox` 与任何 self-hosted runner 的交付依赖，避免未授权外部设备进入验收链路。
- 将“真实部署拓扑 smoke”切换到 GitHub-hosted runner 上执行：基于仓库 `Dockerfile` + `deploy/docker-compose.yml` 本地构建并启动容器，再校验 `/`、`/api/health`、`/supervisor/`。
- 将原先依赖 testbox 的三类回归点收敛为 GitHub-hosted 官方环境内可执行的自动化校验：
  - `POST /api/checks` 并发冲突 + stale job recovery
  - `/api/version-inference/events` 的 SSE enqueue / reconnect-afterId
  - semver fallback pull 在非 semver OCI version 下跳过

## Scope

### In
- `dockrev-api` handlers / DTO / DB 模块拆分。
- `dockrev-supervisor` app 入口拆分。
- GitHub Actions 官方 runner 上的 deploy smoke。
- 通过 Rust 自动化测试补齐与历史 testbox 场景等价的关键回归点。

### Out
- 任何外部 testbox / self-hosted runner / SSH 验收路径。
- 对外 HTTP 路由、JSON/SSE shape、数据库 schema、migration id 变更。
- 新增外部环境依赖或人工机房操作。

## Changed Artifacts

- `crates/dockrev-api/src/api/**`
- `crates/dockrev-api/src/api/types/**`
- `crates/dockrev-api/src/db/**`
- `crates/dockrev-api/src/models/**`
- `crates/dockrev-supervisor/src/app/**`
- `.github/scripts/deploy-smoke.sh`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `deploy/docker-compose.yml`

## Contracts

| Surface | Type | Change | Notes |
| --- | --- | --- | --- |
| API / DTO / DB module layout | internal | modify | 对外接口保持不变 |
| `CI (PR)` / `CI (main)` deploy smoke job | workflow | new | `ubuntu-latest` 官方环境运行 |
| `deploy/docker-compose.yml` | config | modify | 允许 `DOCKREV_GATEWAY_BIND` 覆盖 gateway 绑定端口，默认行为不变 |
| Backend regression tests | test | modify | 新增 SSE reconnect 与 non-semver skip 回归 |

## Acceptance

- `cargo test --workspace --locked --all-features` 在 GitHub-hosted runner 上通过。
- Deploy smoke 在 GitHub-hosted runner 上通过，并满足：
  - 使用仓库 `Dockerfile` 构建 `dockrev` / `dockrev-supervisor` 镜像；
  - 使用 `deploy/docker-compose.yml` 启动真实部署拓扑；
  - `GET /` 返回 built HTML，且不是 placeholder；
  - `GET /api/health` 返回 `200 ok`；
  - `GET /supervisor/` 返回 HTML。
- 与原 testbox 场景对应的关键行为均由官方环境内自动化测试覆盖：
  - 并发 check 冲突 / stale job recovery
  - version inference SSE enqueue + reconnect-afterId
  - non-semver OCI version 跳过 semver fallback pull

## Validation

- `cargo fmt --check`
- `cargo test -p dockrev-api`
- `cargo test -p dockrev-supervisor`
- `cargo test --workspace`
- GitHub Actions official runner:
  - `CI (PR)` / `CI (main)` -> `Deploy Smoke`
  - `Backend Tests`（含新增回归测试）

## Milestones

- [x] M1: `dockrev-api` 模块拆分完成。
- [x] M2: `dockrev-supervisor` 模块拆分完成。
- [x] M3: 清理 `codex-testbox` / self-hosted 验收依赖。
- [x] M4: GitHub-hosted deploy smoke 落地。
- [x] M5: 关键回归点迁移到官方环境自动化测试。

## Risks / Assumptions

- 风险：GitHub-hosted Docker build 比纯单元测试更慢，但换来无需外部设备的可审计验收。
- 假设：`ubuntu-latest` 持续提供可用的 Docker Engine / Compose 插件。
- 假设：仓库现有 Backend Tests 足以承载新增回归测试的运行时开销。

## Notes

- 2026-03-08: 初稿曾引入 `codex-testbox` / self-hosted 验收路径；后续按约束清理，并改为 GitHub-hosted 官方环境方案。
- 2026-03-08: 保留 `DOCKREV_GATEWAY_BIND` 以支持官方 runner 上的唯一端口 smoke，不改变默认部署端口。
