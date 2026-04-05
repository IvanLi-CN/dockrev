# Dockrev：Release Queue 线上恢复与历史补账执行（#q3nyf）

## 状态

- Status: 待实现
- Created: 2026-04-05
- Last: 2026-04-05

## 背景 / 问题陈述

- `#taauj` 已经把 historical tag-backed backlog reconcile、manual release bypass、workflow-source queue continuation 这套发布合同落地到代码与回归测试里。
- 但当前仓库的真实 release backlog 仍需要在 `main` 上执行一次恢复闭环：先补齐历史 publication ledger，再继续推动真实未发布版本产出 GitHub Release。
- 这一步依赖合并后的线上 workflow 事实，不能在合并前通过本地实现 PR 自证完成，因此需要独立跟踪。

## 目标 / 非目标

### Goals

- 在不新增任何权限、secret、PAT 或人工凭据的前提下，基于现有 `GITHUB_TOKEN` 恢复 `main` 上的 release queue。
- 观察并记录 historical published backlog 的 reconcile 结果，确认历史已发布条目不再永久阻塞新的 pending release。
- 观察并记录真实 pending 版本的发布结果，优先确认 `0.38.2`、`0.39.0`、`0.39.1` 是否按顺序恢复产出。

### Non-goals

- 不再修改 release queue 逻辑本身；若线上恢复暴露新的合同缺口，再另开 follow-up implementation spec。
- 不改变 release label taxonomy、版本号语义或 GHCR 命名。

## 范围（Scope）

### In scope

- `main` 分支上的 `Release` workflow 运行与结果
- GitHub Releases / tags / publication ledger 的线上恢复证据
- `docs/specs/README.md`
- 本 spec 的结果留痕

### Out of scope

- 新的工作流权限、token 或 secret
- 与当前 backlog 恢复无关的产品功能开发

## 需求（Requirements）

### MUST

- 恢复执行必须只使用现有 workflow 权限模型与 `GITHUB_TOKEN`。
- 线上 workflow 必须先执行 historical publication reconcile，再选择真实 `next-pending` target。
- 若某个历史 backlog 条目缺少 `tag -> target_sha`、GitHub Release、或任一 GHCR digest 证据，则必须显式阻断并记录原因，不得静默 `skip` 或误记为 `published`。
- 若历史 backlog 证据齐全，则必须自动回填 publication ledger，让队列继续推进到真实未发布版本。
- 结果记录必须包含对应 workflow run / release / version outcome，便于后续追责与复盘。

### SHOULD

- 若恢复执行顺利，应补充 backlog 被清空或缩减后的可见证据。
- 若恢复执行失败，应明确记录失败发生在 reconcile、publish、comment contract、还是 queue continuation。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 合并 `#taauj` 后，`main` 上的 `Release` workflow 先扫描 first-parent release queue，对已发布但缺 publication ledger 的历史 target 执行 reconcile。
- reconcile 成功的历史 target 会被回填 publication ledger，不再阻塞 `next-pending`。
- queue 随后继续选择真实未发布 target，并按现有 stable 发布路径创建/更新 GitHub Release、PR comment 与 publication ledger。

### Edge cases / errors

- 若历史条目只有 tag，没有 GitHub Release 或镜像 digest 证据，则 queue 必须停在该 target 并暴露明确错误。
- 若 `0.38.2`、`0.39.0`、`0.39.1` 中任一版本在 publish/comment/ledger 环节失败，必须记录具体 run 与失败位置，而不是笼统描述“没发出来”。

## 验收标准（Acceptance Criteria）

- Given `#taauj` 已合并到 `main`，When `Release` workflow 运行 prepare，Then historical published backlog 会先被 reconcile，且不需要新增权限。
- Given 当前 backlog 中存在真实未发布版本，When reconcile 后继续 queue，Then workflow 会继续指向 `0.38.2`，并随后推进 `0.39.0`、`0.39.1`，或在某个明确的阻断点停止并留下可见证据。
- Given 恢复执行结束，When 回看 GitHub Releases / workflow runs / publication ledger，Then 可以明确回答“哪些版本已恢复发布、哪些版本仍阻断、阻断原因是什么”。

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/specs/q3nyf-release-queue-live-recovery/SPEC.md`

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: 合并 `#taauj` 并确认 `main` 上的 release workflow 使用新合同运行
- [ ] M2: 历史 published backlog reconcile 结果可见并留痕
- [ ] M3: `0.38.2` / `0.39.0` / `0.39.1` 的恢复发布结果完成记录
- [ ] M4: specs index 与最终结论同步完成

## 参考（References）

- `docs/specs/taauj-release-api-tag-publish-contract/SPEC.md`
