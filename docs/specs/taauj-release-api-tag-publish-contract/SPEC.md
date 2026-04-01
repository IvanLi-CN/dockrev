# Dockrev：Release Tag 预创建与发布完成态合同（#taauj）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-31
- Last: 2026-04-01

## 背景 / 问题陈述

- `#yt22e` 已经把 release queue 的误标 target、skip override、PR release comment contract 收口好了。
- 但 `#taauj` 把 release tag 创建改成了 `ncipollo/release-action` 的 `commit/TARGET_SHA` 路径后，线上真实发布又卡在 GitHub Release API `403 Resource not accessible by integration`。
- 现有证据表明：workflow-file 触碰的 target 会在 `git push tag` 时撞上 `workflows` 权限边界；但 release-enabled 产品 PR 现在已经被 label gate 禁止触碰 `.github/workflows/**`，因此“先显式创建 tag，再创建 GitHub Release”才是与当前权限模型一致的稳定路径。
- 目标不是增加额外凭据，而是在现有 GitHub 原生权限模型内，让 release 继续发布 GitHub Release，并保证 publication ledger 只在 release comment 合同满足后才记为完成。

## 目标 / 非目标

### Goals

- 恢复 `Release` workflow 对显式 tag 预创建/校验的支持，但仅用于已通过 label gate 的 release-enabled target。
- GitHub Release 步骤只对现有 tag 做 create/update + asset upload，不再依赖 `commit`/`target sha` 让 Release API 代建 tag。
- 让 publication ledger 只在 GitHub Release 成功且 source PR comment 合同满足后才记录，避免 comment 失败时 queue 错误前进。
- 不引入任何额外凭据。

### Non-goals

- 不修改 release intent labels / snapshot / override ledger 模型。
- 不修改 GHCR 镜像命名、用户可见 HTTP API、数据库 schema。
- 不新增前端功能。

## 范围（Scope）

### In scope

- `.github/workflows/release.yml`
- `.github/scripts/release-channel-contract-check.sh`
- `README.md`
- `docs/specs/README.md`

### Out of scope

- `crates/**`
- `web/**`
- git notes snapshot/publication/override payload schema

## 需求（Requirements）

### MUST

- `Release` workflow 必须先显式创建或校验 `RELEASE_TAG -> TARGET_SHA`，若同名 tag 已存在但指向其它 commit 则立即失败。
- GitHub Release 创建/更新步骤不得再使用 `commit: ${{ env.TARGET_SHA }}` 这类“让 Release API 代建 tag”的路径。
- source PR release-version comment 仍然必须在 successful publish 后存在且唯一。
- publication ledger 只能在 GitHub Release 成功且 PR comment 合同满足后记录。
- 整个方案必须继续只依赖 `GITHUB_TOKEN`，不得引入 PAT / GitHub App / 新 secrets。

### SHOULD

- contract check 明确防回归：既要校验 workflow 中存在 `Create and push tag`，也要校验 release action 不再带 `commit: ${{ env.TARGET_SHA }}`。
- README 应补一句 release-enabled PR 不能触碰 workflow 文件，因此显式 tag push 仍符合默认 `GITHUB_TOKEN` 权限模型。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `Release` workflow 继续先构建二进制与 GHCR 镜像。
- Publish 阶段先 `git fetch --tags`，若 `RELEASE_TAG` 不存在则创建 annotated tag 并 `git push origin refs/tags/...`；若已存在，则要求它解析到 `TARGET_SHA`。
- GitHub Release 步骤直接使用现有 release action，但只负责 create/update release 与上传 assets，不再承担缺失 tag 的创建职责。
- PR release-version comment 在 GitHub Release 成功后执行并验证。
- publication ledger 在 comment 合同满足后记录；只有此时 target 才算 published，可继续 queue。

### Edge cases / errors

- 若显式 tag 创建/校验失败，workflow fail，GitHub Release 与 publication ledger 都不得继续。
- 若 GitHub Release API 创建/更新失败，workflow fail，publication ledger 不得写入。
- 若 PR comment 合同失败，workflow fail，publication ledger 不得写入。
- 若 release 已存在且 tag 已正确指向目标 SHA，workflow 应允许 update 路径继续工作。

## 验收标准（Acceptance Criteria）

- Given 一个正常的 release-enabled target，When `Release` workflow 执行 publish，Then workflow 会先显式创建/校验 `RELEASE_TAG -> TARGET_SHA`，再调用 GitHub Release API create/update release。
- Given GitHub Release 成功但 PR comment 合同失败，When workflow 收尾，Then publication ledger 仍未记录该 target，queue 不会把它视为已发布。
- Given publish 全部成功，When workflow 完成，Then source PR 上存在唯一 bot-owned marker comment，且 publication ledger 已记录。
- Given 当前卡住的 release queue，When 修复上线并重新恢复 queue，Then 包含 `repoUrl auto-backfill` 的版本可以真正发布并进入 101。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`
- `bash ./.github/scripts/release-channel-contract-check.sh`

## 文档更新（Docs to Update）

- `README.md`
- `docs/specs/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 恢复显式 tag 预创建/校验，并移除 release-action 的 `commit/TARGET_SHA` 代建 tag 依赖
- [x] M2: 将 publication ledger 后移到 GitHub Release + PR comment 之后
- [x] M3: 补 contract check 防回归
- [ ] M4: 恢复 release queue 并确认产出真实新版本

## 参考（References）

- `docs/specs/yt22e-release-queue-override-comment-hardening/SPEC.md`
- `docs/specs/qnq3w-release-publication-latest-pr-comment/SPEC.md`
