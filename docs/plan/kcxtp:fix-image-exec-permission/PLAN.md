# CI/CD: 修复 GHCR 镜像 `dockrev` 主程序不可执行（#kcxtp）

## 状态

- Status: 已完成
- Created: 2026-01-28
- Last: 2026-01-31

## 背景 / 问题陈述

- 当前发布到 GHCR 的单镜像 `ghcr.io/ivanli-cn/dockrev:<tag>` 中，`/usr/local/bin/dockrev` 缺少可执行位（`+x`），导致容器按默认 `CMD ["/usr/local/bin/dockrev"]` 启动时报 `permission denied`。
- 生产环境已使用临时 workaround（在 compose 中 `cp && chmod +x && exec`）以维持可用，但该方案增加了部署复杂度与重建风险。

## 目标 / 非目标

### Goals

- 发布的镜像可直接运行：默认 `CMD` 启动不再出现 `permission denied`。
- 镜像内 `/usr/local/bin/dockrev`（以及同镜像内的 `/usr/local/bin/dockrev-supervisor`）具备可执行位（例如 `0755`）。
- 在发布/构建阶段增加阻断性校验，避免错误镜像被 push 到 GHCR。

### Non-goals

- 不重构 Dockrev 的业务逻辑、API、UI。
- 不在本计划内修改生产侧 compose（workaround 的移除属于后续运维变更）。
- 不引入“运行时修权限”的方案（镜像应自洽）。

## 范围（Scope）

### In scope

- 修复镜像打包逻辑：确保 `runtime-prebuilt` 目标生成的镜像中 `dockrev*` 可执行。
  - 相关入口（已定位）：`Dockerfile` 的 `runtime-prebuilt` stage（从 `dist/ci/docker/${TARGETARCH}/dockrev*` COPY 到 `/usr/local/bin/`）。
- 修复发布流程：确保 `Release` workflow 在 `actions/download-artifact` 后、docker build/push 前，能保证/校验 `dist/ci/docker/*/dockrev*` 的可执行位。
  - 相关入口（已定位）：`.github/workflows/release.yml` 的 `publish` job（下载 `binaries-*` artifacts 后执行 `docker/build-push-action`）。
- 增加 CI 阻断性检查（smoke/权限校验），在 push 前能验证镜像可运行。

### Out of scope

- 改变镜像的对外交付方式（仍保持单镜像 `ghcr.io/ivanli-cn/dockrev`）。
- 改变默认端口、健康检查路径、鉴权策略等运行约定。

## 需求（Requirements）

### MUST

- GHCR 镜像内 `/usr/local/bin/dockrev` 为可执行文件（具备 `+x`）。
- 默认 `CMD ["/usr/local/bin/dockrev"]` 启动容器成功（不再 `permission denied`）。
- Release 发布流水线在 push 前增加阻断性检查：若镜像内二进制不可执行则失败并禁止 push。

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Dockrev GHCR 单镜像：运行契约（`/usr/local/bin/dockrev*` + 默认 `CMD`） | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | 修复 exec bit，保持路径与命令兼容 |

## 验收标准（Acceptance Criteria）

- Given 拉取 `ghcr.io/ivanli-cn/dockrev:<tag>`
  When 查看镜像内文件权限
  Then `/usr/local/bin/dockrev` 与 `/usr/local/bin/dockrev-supervisor` 均具备可执行位（例如 `0755`）。
- Given 拉取同一镜像
  When 以默认 `CMD` 启动容器
  Then 不出现 `exec: \"/usr/local/bin/dockrev\": permission denied`。
- Given 触发一次 `Release` workflow 的发布路径（`should_release=true`）
  When 执行到 GHCR push 前的检查步骤
  Then 若镜像内二进制不可执行则 workflow 失败且不 push；可执行则继续 push。
- Given 生产侧仍保留 workaround
  When 替换为修复后的镜像版本
  Then 即便移除 workaround，也能直接启动（运维侧移除属于后续变更执行）。

## 实现前置条件（Definition of Ready / Preconditions）

- 已确认保持镜像对外契约不变：保留 `CMD ["/usr/local/bin/dockrev"]`，不强制改为 `ENTRYPOINT`。
- 已确认 CI 校验方式采用“双保险”：
  - 快速卫兵：对下载后的 `dist/ci/docker/*/dockrev*` 执行 `chmod +x` 并 `test -x`（阻断）。
  - 真实门槛：在 GHCR push 前增加一次“构建并运行镜像”的阻断性 smoke test（至少 `linux/amd64`）。
- 已确认本次变更需要走一次 `type:patch` 发版（`Release` gating 基于 PR 意图标签）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 发布前 smoke test（阻断）至少覆盖：
  - `docker run --rm <image> /usr/local/bin/dockrev --version`（或等价的最小可执行验证）
  - `docker run --rm <image> /usr/local/bin/dockrev-supervisor --version`（若该二进制提供版本输出；否则只做 `test -x` 校验）

### Quality checks

- 保持多架构发布（`linux/amd64,linux/arm64`）不回退。
- 不依赖宿主机/运行时额外 chmod；镜像本身应具备正确权限。

## 文档更新（Docs to Update）

- `docs/plan/README.md`: 推进状态与 Notes（实现完成后补 PR/Release tag）。
- `README.md` / `deploy/README.md`: 若存在“使用 released image / 运行方式”相关说明，补充镜像可直接运行的约定（不再需要外部 workaround）。

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [x] M1: 修复 Dockerfile：确保 `runtime-prebuilt` 复制的 `dockrev*` 在镜像内为可执行（推荐 `COPY --chmod=0755` 或等价手段）。
- [x] M2: 修复 Release workflow：在 `publish` job 下载 artifacts 后、build/push 前补齐 `dockrev*` 可执行位，并增加阻断性校验。
- [x] M3: 增加发布前 smoke test：构建并运行镜像验证 `dockrev` 可执行（至少 `linux/amd64`），并将其作为 push 前门槛。
- [x] M4: 文档更新：说明镜像运行约定；记录已修复版本范围；提示生产侧可在后续变更移除 workaround。

## 方案概述（Approach, high-level）

- 根因方向（已定位）：
  - `Release` workflow 的 `publish` job 会通过 `actions/download-artifact` 获取 `dist/ci/docker/*/dockrev*`，该过程可能丢失可执行位；
  - `Dockerfile` 的 `runtime-prebuilt` 直接 `COPY dist/ci/docker/${TARGETARCH}/dockrev* /usr/local/bin/`，会把缺失的 mode 原样带入镜像。
- 修复策略（推荐，双保险）：
  1) 在 `Dockerfile` 的 `runtime-prebuilt` 使用 `COPY --chmod=0755`（或等价）确保镜像内权限正确；
  2) 在 `Release` 的 `publish` job 中对 `dist/ci/docker/*/dockrev*` 显式 `chmod +x` 并 `test -x` 校验；
  3) 在 push 前做一次“构建并运行镜像”的阻断性 smoke test（至少 `linux/amd64`；复用 `./.github/scripts/smoke-test.sh` 的 HTTP/UI/版本校验逻辑）。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：仅修复 workflow 但不修 Dockerfile，未来若有其他发布路径/本地构建复用 `runtime-prebuilt` 仍可能复发。
- 风险：仅修复 Dockerfile 但不加 smoke test，未来流程改动可能再次引入“镜像不可启动”而未被及时发现。
- 假设：保持 `CMD [\"/usr/local/bin/dockrev\"]` 不变（不强制改为 `ENTRYPOINT`），以最小化行为变化；如需改动需额外冻结口径。

## 变更记录（Change log）

- 2026-01-28: 创建计划；完成最小 repo 侦察（Dockerfile + Release workflow 路径已定位）。
- 2026-01-28: 完成 M1+M2+M3：Dockerfile 打包阶段强制 `dockrev*` 可执行；Release push 前校验/补齐 exec bit；新增 amd64 镜像阻断性 smoke test。
- 2026-01-31: 完成 M4：补齐“镜像可直接运行”的文档口径与版本范围（`0.3.5+`）；提示后续可移除生产侧 workaround。
