# CI/CD: 自动发布时“同步发布镜像”口径冻结与验收（#0004）

## 状态

- Status: 待实现
- Created: 2026-01-21
- Last: 2026-01-21

## 背景 / 问题陈述

- 现状：仓库存在 `Release` workflow（`.github/workflows/release.yml`），其会在 `main` 的 `CI (main)` 成功后触发（`workflow_run`），并构建+推送 Docker 镜像到 GHCR。
- 问题：从维护与可观测角度，“自动发布”与“镜像发布”的同步语义需要冻结：当发布链路发生失败时，是否允许出现 GitHub Release 已创建/更新，但 GHCR 镜像未推送（或相反）的不一致状态。
- 目标：把“同步发布镜像”的口径明确为可实现、可测试的验收标准，并约束后续实现的失败策略与标签策略。

## 已确认决策（主人已拍板）

- 同步语义：不允许“半发布”（不允许出现 Release 已创建/更新但镜像未发布成功的状态）。
- 顺序：先确保镜像构建+推送成功，再创建/更新 GitHub Release（及其 assets）。
- 发布验证：不额外增加 `docker pull` / `docker inspect` 之类的二次验证；以镜像 push 成功为准。
- 触发策略：不得存在 `release: published` 触发路径；仅保留 `workflow_run`（`CI (main)` 成功后自动发布）。
- 失败残留处理：允许出现“镜像已推送成功，但 GitHub Release 创建/更新失败”的状态；不做镜像清理/回滚，workflow 直接失败并输出清晰错误说明。

## 目标 / 非目标

### Goals

- 冻结“自动发布（workflow_run / release published）”与“GHCR 镜像发布”的同步语义与失败策略（fail-fast/原子性边界）。
- 冻结 GHCR 镜像命名、tag 规则（含 `latest` 口径）与多架构平台范围，并形成可引用的契约文档。
- 让 CI/CD 行为可验证：在 PR 阶段可预演“会发布的镜像构建”（不 push），在 main/release 触发时保证发布链路可重复运行且行为可预测。

### Non-goals

- 不改 Dockrev 业务功能、API 语义、数据模型。
- 不新增发布渠道（例如 Docker Hub、Homebrew、系统包仓库等）。
- 不扩展更多平台/架构（除非主人明确要求）。

## 用户与场景

- 维护者：合并到 `main` 后，希望“发布链路”一次成功地产出 GitHub Release + GHCR 镜像；若失败，应明确失败点与是否存在“半发布”的残留。
- 部署者：希望以固定 tag（`<semver>`）部署，并能理解 `latest` 是否会变化及其触发条件。

## 需求（Requirements）

### MUST

- 自动发布路径（`workflow_run`）必须发布 GHCR 镜像，且镜像 tags 至少包含发布版本 tag（`<APP_EFFECTIVE_VERSION>`）。
- 不得存在 `release: published` 触发路径（避免出现“手动发布/兜底发布”导致口径分叉）。
- 同步语义必须明确且可测试：
  - 不允许出现“GitHub Release 与 GHCR 镜像不一致”的状态。
  - 原子性边界：镜像 push 成功是创建/更新 Release 的前置条件（镜像失败则不触发 Release 侧变更）。
- 发布行为必须可追溯：
  - 镜像必须写入版本元数据（OCI labels），并将 git revision/source 链接到仓库。
- PR 校验必须包含镜像构建（不 push），用于提前发现 Dockerfile/构建问题。

## 接口契约（Interfaces & Contracts）

本计划涉及“对外发布产物”的契约（GHCR 镜像命名与 tag 规则），以及 CI/CD 配置作为内部接口的稳定口径。

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GHCR image naming & tagging | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | `ghcr.io/<owner>/dockrev:<tag>` |
| Release workflow trigger & publish semantics | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | CI | `.github/workflows/release.yml` |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 验收标准（Acceptance Criteria）

- Given 合并一个提交到 `main`
  When `CI (main)` 成功且触发 `Release` workflow
  Then GHCR 上存在镜像 `ghcr.io/<owner>/dockrev:<APP_EFFECTIVE_VERSION>`（多架构在契约中约定），并包含 OCI labels 的版本与 revision/source。

- Given 合并一个提交到 `main`
  When `Release` workflow 结束
  Then 不存在“半发布”：若镜像未 push 成功，则不会创建/更新 GitHub Release（含 assets）；若 GitHub Release 已创建/更新，则对应版本镜像必已发布成功。

- Given 有一个 PR 指向 `main`
  When `CI (PR)` 运行
  Then 会执行 Docker 镜像构建校验（`push: false`），且构建成功。

- Given 自动发布链路在“创建/更新 GitHub Release 或上传 assets”阶段失败
  When workflow 退出
  Then workflow 必须失败并输出清晰错误（指明失败步骤/原因/如何重试），且不执行任何镜像清理/回滚动作（已推送镜像可以保留）。

## 实现前置条件（Definition of Ready / Preconditions）

- “同步发布镜像”的同步语义已由主人确认（不允许半发布；镜像成功是 Release 的前置条件）。
- GHCR tag 策略已冻结（`<semver>`、`latest` 的触发条件与兼容性口径）。
- 契约文档已定稿，实现与测试可以直接按契约落地。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- CI 必须包含：
  - PR 阶段 Docker build 校验（不 push）
  - main/release 阶段 Docker build + push（按同步语义约束 fail-fast）

### Quality checks

- 复用仓库既有 CI 质量检查（lint/tests），并保证发布链路仅在其通过后触发。

## 文档更新（Docs to Update）

- `README.md`: 补充“镜像何时发布/从哪里拉取/有哪些 tags/`latest` 口径”。
- `docs/plan/0003:release-automation-alignment/PLAN.md`: 如与本计划的同步语义/标签策略有出入，需要在实现后同步口径（避免两份计划互相矛盾）。

## 实现里程碑（Milestones）

- [ ] M1: 移除 `release: published` 触发路径（只保留 `workflow_run` 自动发布）
- [ ] M2: 调整发布顺序：先 build+push GHCR 镜像，再创建/更新 GitHub Release + 上传 assets
- [ ] M3: 明确失败语义与错误输出（镜像失败不得创建 Release；Release/asset 失败不清理镜像但 workflow 必须失败）
- [ ] M4: PR 阶段继续保留 docker build 校验（不 push），确保 Dockerfile/构建不会在发布时才失败
- [ ] M5: 文档对齐：`README.md` 补充镜像 tags 与发布触发说明（仅 `workflow_run`）

## 方案概述（Approach, high-level）

- 以“失败策略可预测”为核心：把同步语义固化为工作流顺序/条件（例如 fail-fast 或延后创建 Release 等），避免出现难以追踪的半发布状态。
- 以“契约优先”为原则：镜像命名、tag、labels 等对外口径全部写入契约文档，并作为后续变更的基线。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 如果同步语义未冻结，后续实现很容易在“发布顺序/失败时残留”上反复修改，导致产物不一致。
  - 若要求严格原子性，可能需要调整工作流步骤顺序或引入更强的清理/回滚策略（需要权衡复杂度）。
- 需要决策的问题：见本计划对应的对话“开放问题”。
- 假设（需主人确认）：
  - 当前默认镜像名为 `dockrev`，发布到 `ghcr.io`（GHCR）。

## 变更记录（Change log）

- 2026-01-21: 创建计划，冻结“同步发布镜像”的口径与待决策项。

## 参考（References）

- `.github/workflows/ci-main.yml`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/release.yml`
- `.github/scripts/compute-version.sh`
