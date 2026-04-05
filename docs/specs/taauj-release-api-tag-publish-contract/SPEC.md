# Dockrev：Release Tag 预创建与发布完成态合同（#taauj）

## 状态

- Status: 已完成
- Created: 2026-03-31
- Last: 2026-04-05

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
- 不包含 main 上历史 backlog 的实际恢复执行与结果留痕；该合并后运维收尾由 `#q3nyf` 跟踪。

## 范围（Scope）

### In scope

- `.github/scripts/release_snapshot.py`
- `.github/scripts/test-release-snapshot.sh`
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

- `Release` prepare 阶段在自动 queue / skip-continue 路径上必须先对 historical tag-backed pending targets 做 publication ledger reconcile：只有当同一 target 同时满足 `tag -> target_sha`、GitHub Release 存在、且 `dockrev` / `dockrev-supervisor` 的 tag digest 都可解析时，才允许回填 `refs/notes/release-publications`。
- 显式 `workflow_dispatch(..., admin_action=release)` 手动补发路径必须继续直达指定 `head_sha`，不得因为更早的 partial backlog 在 reconcile 阶段被拦住。
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

- `Release` workflow 在自动 queue / skip-continue 路径的 prepare 阶段先扫描 oldest-pending queue；对“已有 tag 且证据完整”的历史 target 自动回填 publication ledger，然后才继续选择真正需要发布的下一条 target。
- `workflow_dispatch(..., admin_action=release)` 仍按 target-only manual backfill 语义工作：先确保该 SHA 的 immutable snapshot 存在，再直接发布该 target，不扫描更老 backlog。
- `Release` workflow 继续先构建二进制与 GHCR 镜像。
- Publish 阶段先 `git fetch --tags`，若 `RELEASE_TAG` 不存在则创建 annotated tag 并 `git push origin refs/tags/...`；若已存在，则要求它解析到 `TARGET_SHA`。
- GitHub Release 步骤直接使用现有 release action，但只负责 create/update release 与上传 assets，不再承担缺失 tag 的创建职责。
- PR release-version comment 在 GitHub Release 成功后执行并验证。
- publication ledger 在 comment 合同满足后记录；只有此时 target 才算 published，可继续 queue。

### Edge cases / errors

- 若历史 pending target 已有 tag，但 GitHub Release 缺失、仍是 draft、或任一 GHCR digest 无法证明，workflow fail，queue 停在该 target，不得静默标记 published/skip。
- 若显式 tag 创建/校验失败，workflow fail，GitHub Release 与 publication ledger 都不得继续。
- 若 GitHub Release API 创建/更新失败，workflow fail，publication ledger 不得写入。
- 若 PR comment 合同失败，workflow fail，publication ledger 不得写入。
- 若 release 已存在且 tag 已正确指向目标 SHA，workflow 应允许 update 路径继续工作。

## 验收标准（Acceptance Criteria）

- Given 一个历史 pending target 已有正确 tag、GitHub Release 与双镜像 digest，When `Release` prepare 运行 reconcile，Then publication ledger 会被自动补齐，且 queue 会继续指向真正未发布的下一个 target。
- Given 一个历史 pending target 只有 tag、但没有 GitHub Release 或缺任一 digest，When reconcile 运行，Then workflow 明确失败并保留 queue 阻断，而不是把该 target 当成已发布。
- Given 运维手动触发 `workflow_dispatch(head_sha=<sha>, admin_action=release)`，When 更早的 backlog 里还存在 partial published target，Then workflow 仍直接发布请求的 `head_sha`，而不是先因 reconcile 扫描旧 backlog 失败。
- Given 一个正常的 release-enabled target，When `Release` workflow 执行 publish，Then workflow 会先显式创建/校验 `RELEASE_TAG -> TARGET_SHA`，再调用 GitHub Release API create/update release。
- Given GitHub Release 成功但 PR comment 合同失败，When workflow 收尾，Then publication ledger 仍未记录该 target，queue 不会把它视为已发布。
- Given publish 全部成功，When workflow 完成，Then source PR 上存在唯一 bot-owned marker comment，且 publication ledger 已记录。
- Given 当前卡住的 release queue，When 本修复合并到 `main`，Then workflow 已具备在不新增权限前提下自动 reconcile 历史已发布 backlog 并继续选择真实 pending target 的合同能力；main 上的实际恢复执行与结果留痕由 `#q3nyf` 跟踪。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-snapshot.sh`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`
- `bash ./.github/scripts/release-channel-contract-check.sh`

## 文档更新（Docs to Update）

- `README.md`
- `docs/specs/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 恢复显式 tag 预创建/校验，并移除 release-action 的 `commit/TARGET_SHA` 代建 tag 依赖
- [x] M2: 将 publication ledger 后移到 GitHub Release + PR comment 之后
- [x] M3: 补 contract check 防回归
- [x] M4: historical backlog reconcile/manual backfill bypass/workflow-source queue continuation 合同、回归测试与 README/spec 同步完成；main 上实际恢复执行转交 `#q3nyf`

## 参考（References）

- `docs/specs/yt22e-release-queue-override-comment-hardening/SPEC.md`
- `docs/specs/qnq3w-release-publication-latest-pr-comment/SPEC.md`
- `docs/specs/q3nyf-release-queue-live-recovery/SPEC.md`
