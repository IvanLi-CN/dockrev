# CI/CD: Release workflow 成功后自动清理 Actions Artifacts（#0014）

## 状态

- Status: 部分完成（2/3）
- Created: 2026-01-26
- Last: 2026-01-27

## 背景 / 问题陈述

- Release workflow 当前会产生多份 Actions Artifacts（例如 `web-dist-*`、`binaries-*-*`），用于跨 job 传递构建产物；workflow 结束后这些 artifacts 会按默认保留期留存，持续占用 Actions 存储。
- 除了显式上传的 artifacts 外，`docker/build-push-action` 在生成 build summary 时也会导出并上传一个 `.dockerbuild` build record artifact（见 run `21346306876` 的 `IvanLi-CN~dockrev~IZ5OVR.dockerbuild`）。

## 目标 / 非目标

### Goals

- 在 Release workflow 成功完成后，自动清理本次 workflow run 产生的 Actions Artifacts，减少长期存储占用。
- 保持对外交付物不受影响：GitHub Release Assets 与 GHCR 镜像不因 artifact 清理而变化或丢失。
- 在失败场景保留调试能力（是否保留 artifacts、保留多久）口径清晰且可配置。

### Non-goals

- 不清理/压缩 GitHub Actions cache（`actions/cache` 或 buildx `type=gha` cache）。
- 不改变 build/发布的主要流程（仍以 artifacts 作为 job 之间传递机制）。

## 范围（Scope）

### In scope

- 调整 `.github/workflows/release.yml`：
  - 在所有 `actions/upload-artifact@v4` 步骤设置明确的 `retention-days`（作为兜底）。
  - 新增一个“cleanup artifacts”阶段：在发布成功后调用 GitHub API 删除本次 workflow run 的 artifacts（包括 `.dockerbuild` build record）。

### Out of scope

- 修改 `.github/workflows/ci-main.yml` 或 PR CI 的 artifact 策略。
- 改变 `docker/build-push-action` 的 build summary 生成逻辑（仅在清理阶段处理其产物）。

## 需求（Requirements）

### MUST

- 当 Release workflow 整体 `conclusion=success`：
  - 清理该 run 下的所有 Actions Artifacts（包括 `web-dist-*`、`binaries-*`、以及 docker build record `*.dockerbuild`）。
  - 不影响本次发布已生成的 GitHub Release Assets 与已推送的 GHCR 镜像。
- 当 Release workflow `conclusion!=success`：
  - 不做“立即删除”，保留用于排障的关键 artifacts，但将默认保留期设置到最短：`retention-days: 1`。
  - “关键 artifacts”清单（本 workflow 自产、可配置保留期）：
    - `web-dist-${sha}`（若存在）
    - `binaries-amd64-${sha}`
    - `binaries-arm64-${sha}`
  - 对无法设置保留期或不需要保留的 artifacts（例如 `docker/build-push-action` 生成的 `*.dockerbuild` build record），在失败路径下也应主动删除，避免长尾存储占用。
- 清理逻辑幂等、可重试：
  - 重跑 cleanup 不应导致 workflow 失败（例如 artifacts 已不存在时应视为成功或可忽略）。
- 权限最小化：仅使用 workflow 运行时已授予的 GitHub token 权限完成清理。
- 权限要求明确：清理需要对 Actions artifacts 的删除权限（实现阶段需在 workflow `permissions` 中显式授予）。

## 接口契约（Interfaces & Contracts）

None

## 验收标准（Acceptance Criteria）

- Given 触发一次有效发版且 Release workflow 成功
  When 打开该 workflow run 的 artifacts 列表
  Then artifacts 列表为空（0 个 artifacts）。
- Given 同一次发版
  When 打开对应 tag 的 GitHub Release 页面
  Then Release assets 完整可下载，且不因 artifacts 清理而缺失。
- Given Release workflow 失败（任一 job 失败）
  When 打开该 workflow run 的 artifacts 列表
  Then 仍存在用于排障的关键 artifacts（`web-dist-*` 若存在、`binaries-amd64-*`、`binaries-arm64-*`），且保留期为最短（`retention-days: 1`）；同时不包含 `*.dockerbuild` build record。

## 实现前置条件（Definition of Ready / Preconditions）

- 策略已冻结：仅在成功时立即清理；失败时保留以便排障。
- `retention-days` 的默认值已确认为最短：`1`（作为兜底：成功时仍会立即清理）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 以一次真实 Release run 验证：
  - 成功路径：发布成功 + artifacts 被清理
  - 失败路径（可用受控失败注入或临时分支验证）：失败时 artifacts 行为符合策略

### Quality checks

- 清理 job 必须确保在发布步骤之后执行（避免过早删除导致后续步骤找不到 artifacts）。
- 清理 job 需对 GitHub API 的临时失败具备合理的重试/容错策略（例如短暂重试或可重跑）。

## 文档更新（Docs to Update）

- `docs/plan/README.md`: 状态推进与 Notes（实现完成后补 PR 号）。

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones）

- [x] M1: 为 `upload-artifact` 显式设置 `retention-days`（兜底）
- [x] M2: 新增 cleanup job（成功后删除本 run 的所有 artifacts，含 `.dockerbuild`）
- [ ] M3: 验证成功/失败两条路径，并冻结失败路径策略与文档口径

## 方案概述（Approach, high-level）

- 在 `publish` 完成且 workflow 成功后执行 cleanup：通过 GitHub API 枚举 `github.run_id` 下的 artifacts 并逐个删除。
- 额外设置较短 `retention-days` 作为兜底：即便 cleanup 因 API 失败未执行，也能控制长期存储占用上限。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：失败路径保留期最短（`retention-days: 1`）可能不足以覆盖跨时区排障窗口；如需更长窗口需重新冻结策略。
- 已确认：Release 成功后 artifacts 对外不再有价值（以 GHCR + GitHub Release Assets 为准）。

## 变更记录（Change log）

- 2026-01-26: 创建计划。
- 2026-01-26: 实现 M1+M2：`upload-artifact` 兜底 `retention-days: 1`；新增 `cleanup-artifacts` job（成功清空 artifacts；失败保留关键 artifacts、删除 `*.dockerbuild`）。
- 2026-01-27: 补充文档说明（仓库 README）；确认 Release run `21369236545` 为 `should_release=false`（build/publish/cleanup 均 skipped），因此仍需一次真实的 `should_release=true` Release run 完成成功/失败两条路径验证。

## 参考（References）

- `.github/workflows/release.yml`
- Release run `21346306876`（示例 artifacts：`web-dist-*`、`binaries-*`、`*.dockerbuild`）
