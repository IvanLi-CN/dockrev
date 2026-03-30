# Dockrev：Release API 建 Tag 与发布完成态合同（#taauj）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-31
- Last: 2026-03-31

## 背景 / 问题陈述

- `#yt22e` 已经把 release queue 的误标 target、skip override、PR release comment contract 收口好了。
- 但线上恢复时，release queue 在 `Create and push tag` 这一步继续失败，报错为：`refusing to allow a GitHub App to create or update workflow ... without workflows permission`。
- 这说明当前 `Release` workflow 仍然依赖 `git push origin <tag>` 创建 Git tag，而这条路径在现有 `GITHUB_TOKEN` 权限模型下已经不可靠。
- 目标不是增加额外凭据，而是在现有 GitHub 原生权限模型内，让 release 可以继续创建 tag / GitHub Release，并保证 publication ledger 只在 release comment 合同满足后才记为完成。

## 目标 / 非目标

### Goals

- 去掉 `Release` workflow 对 `git push tag` 的依赖。
- 改为通过 GitHub Release API 路径创建/更新 release，并让缺失 tag 由该路径以 `target_commitish`/`commit` 语义创建。
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

- `Release` workflow 不得再执行 `git push origin <tag>` 来创建 release tag。
- GitHub Release 创建/更新步骤必须显式绑定 `TARGET_SHA`，在 tag 不存在时由 GitHub Release API 路径创建该 tag。
- source PR release-version comment 仍然必须在 successful publish 后存在且唯一。
- publication ledger 只能在 GitHub Release 成功且 PR comment 合同满足后记录。
- 整个方案必须继续只依赖 `GITHUB_TOKEN`，不得引入 PAT / GitHub App / 新 secrets。

### SHOULD

- contract check 明确防回归：既要校验 release action 带 `commit: ${{ env.TARGET_SHA }}`，也要校验 workflow 中不存在旧的 `Create and push tag` 步骤。
- README 应补一句 release tag 由 GitHub Release API 路径创建，而不是 workflow 自己 `git push tag`。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `Release` workflow 继续先构建二进制与 GHCR 镜像。
- GitHub Release 步骤直接使用现有 release action，新增 `commit`/`target sha` 输入，使缺失 tag 在创建 release 时由 GitHub API 生成。
- PR release-version comment 在 GitHub Release 成功后执行并验证。
- publication ledger 在 comment 合同满足后记录；只有此时 target 才算 published，可继续 queue。

### Edge cases / errors

- 若 GitHub Release API 创建/更新失败，workflow fail，publication ledger 不得写入。
- 若 PR comment 合同失败，workflow fail，publication ledger 不得写入。
- 若 release 已存在且 tag 已正确指向目标 SHA，workflow 应允许 update 路径继续工作。

## 验收标准（Acceptance Criteria）

- Given 一个正常的 release-enabled target，When `Release` workflow 执行 publish，Then workflow 不再执行 `git push tag`，而是通过 GitHub Release API 路径创建/更新 tag + release。
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

- [x] M1: 移除 `git push tag` 路径，改为 release-action 绑定 `TARGET_SHA`
- [x] M2: 将 publication ledger 后移到 GitHub Release + PR comment 之后
- [x] M3: 补 contract check 防回归
- [ ] M4: 恢复 release queue 并确认产出真实新版本

## 参考（References）

- `docs/specs/yt22e-release-queue-override-comment-hardening/SPEC.md`
- `docs/specs/qnq3w-release-publication-latest-pr-comment/SPEC.md`
