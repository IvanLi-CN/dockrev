# Dockrev：Release Snapshot Queue 对齐与 #176 补发（#48mh8）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-03-23
- Last: 2026-03-23

## 背景 / 问题陈述

- 当前 `Release` workflow 直接基于 `workflow_run.head_sha` 读取关联 PR label，再即时计算 `intent/version` 并发版。
- 当 `main` 连续合并多个 PR 时，GitHub Actions 并发与前序 run 失败/跳过会让较早 commit 的 release 机会直接丢失，`#176` 就是该问题的已发生实例。
- `release-channel-contract-check.sh` 还内嵌了未显式注入 `GITHUB_TOKEN` 的 live branch-rules 校验，已在 `2026-03-22 05:39:30 UTC` 因 unauthenticated 403 rate limit 误伤 `CI (main)`，进一步导致 Release skipped。

## 目标 / 非目标

### Goals

- 将发版拓扑改为 immutable release snapshot + oldest-pending queue，确保 `main` 上 burst merge 不漏发。
- 保持既有 PR label 契约不变：`type:*` 仍为 `docs|skip|patch|minor|major`，`channel:*` 仍为必选且仅允许 `stable|rc`。
- 保持 Dockrev 当前稳定版 tag/version 形态不变：stable 使用无前缀 `<semver>`，RC 使用 `<semver>-rc.<sha7>`。
- 将 live quality-gates 校验从离线 contract check 中拆出，改为 CI 显式、带 `GITHUB_TOKEN` 的 authenticated step。
- 在拓扑合并进 `main` 后，手动补发 `PR #176` 的 merge commit `ea596780c9f6f2fff8148d2d103d625f22179369`，对外版本为 `0.34.1`。

### Non-goals

- 不改动 `crates/**`、`web/**` 的业务逻辑。
- 不迁移到 `vX.Y.Z` tag 前缀，也不引入 changesets / semantic-release。
- 不实现自动 PR release-version comment upsert。

## 范围（Scope）

### In scope

- `refs/notes/release-snapshots` 下的 immutable release snapshot 存储契约。
- `.github/scripts/release_snapshot.py` 的 `ensure` / `export` / `next-pending`。
- `CI (main)` 中的 snapshot materialization job。
- `Release` workflow 对 oldest-pending snapshot 的消费、backfill 与 queue continuation。
- `CI (PR)` / `CI (main)` 的 authenticated live quality-gates step。
- `README.md` 与相关 specs 的口径同步。

### Out of scope

- 远端 label 名称迁移。
- 发布资产格式、GHCR 镜像命名与二进制打包策略的业务性变更。
- 变更 required-check 名称或 quality-gates 声明中的对外 check 名称。

## 需求（Requirements）

### MUST

- snapshot 必须记录 `target_sha`、`pr_number`、`type_label`、`channel_label`、`release_enabled`、`release_bump`、`release_channel`、`app_effective_version`、`release_tag`、`tags_csv`。
- `release_snapshot.py` 的 stable base version 计算必须基于 `main` 一阶父链中“最近已发布 tag / 已存在 snapshot”的前序锚点，不能依赖仓库当前全局最大 tag。
- `CI (main)` 必须能为自最近发布/快照锚点之后缺失的 release-enabled commits 一次性 materialize snapshot，避免只处理当前 `HEAD`。
- `Release` workflow 自动路径及其由 `github-actions[bot]` 触发的内部 queue-continuation dispatch 必须按 oldest pending snapshot 逐个发布，且成功后继续 dispatch 下一个 pending target 直到队列清空。
- 手工 `workflow_dispatch(admin_action=release)` 必须只发布指定 target；只有由 `github-actions[bot]` 派发的内部 queue-continuation dispatch 才可继续处理后续 pending target。
- manual backfill 必须只要求目标 SHA 已在 `origin/main`，不依赖该 commit 当时一定有成功的 `CI (main)` 历史记录。
- `release-channel-contract-check.sh` 必须只保留离线 contract / self-test / mock API 检查，不得再直接访问真实 GitHub branch-rules API。

### SHOULD

- snapshot export 时重新解析 stable manifest tags，确保被后续 stable snapshot 超越的旧 stable release 不再继续写 `latest`。
- 现有 `Release` workflow 的 `workflow_dispatch.inputs.head_sha` 接口继续保留，降低运维切换成本。
- existing tag/release 场景保持幂等：若 tag 已存在且指向目标 commit，则允许继续发布 / 对账；若指向错误 commit，则阻断。

### COULD

- 在 snapshot payload 中额外保留 PR title、PR head SHA、snapshot source 等调试字段。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `CI (main)` 成功后，snapshot job 针对当前 `main` commit 拉取/写入 `refs/notes/release-snapshots`；若当前 commit 之前存在未快照的连续 main commits，则按 first-parent 顺序补齐缺失 snapshots。
- `Release` workflow 被 `workflow_run` 触发时，不再把触发 SHA 直接当作唯一发布目标，而是以该 SHA 作为上界，从 snapshots 中选择 oldest pending release-enabled target 发布。
- `Release` workflow 被 `workflow_dispatch(head_sha=...)` 触发时，允许对指定 main commit 走 target-only materialization + export，再发布该 target；手工 dispatch 不会扩展到其他 pending target。由 `github-actions[bot]` 派发的内部 queue-continuation dispatch 会在每个 target 成功后继续收敛队列。
- stable snapshot 导出 `<semver>` tag，并仅在该 snapshot 仍是 main 上最新 stable snapshot 时发布 `latest`；rc snapshot 导出 `<semver>-rc.<sha7>`，标记 GitHub prerelease，且不得更新 `latest`。
- docs/skip snapshot 仍需记录到 notes，便于历史对账，但不得进入发布队列。

### Edge cases / errors

- 目标 commit 不在 `origin/main` first-parent 历史上时，snapshot ensure/export 与 Release prepare 必须失败。
- commit 关联不到唯一 PR 时，非 target commit 的 catch-up materialization 允许跳过；目标 commit 若无法关联唯一 PR，必须失败。
- PR labels 缺失、冲突或未知时，snapshot materialization 必须失败；CI gate 继续维持今天的失败口径。
- 目标 tag 已存在但不指向目标 commit 时，发布必须阻断，避免错 tag 覆盖。
- authenticated live quality-gates 校验失败时，只能反映真实 branch-rules drift，不得因匿名限流误报。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `refs/notes/release-snapshots` | Git notes JSON payload | internal | New | None | CI maintainers | `CI (main)`, `Release`, operators | immutable release snapshot store |
| `.github/scripts/release_snapshot.py` | CLI | internal | New | None | CI maintainers | `CI (main)`, `Release` | `ensure/export/next-pending` |
| `.github/workflows/release.yml` `workflow_dispatch.inputs.head_sha` | workflow input | internal | Modify | None | CI maintainers | operators, queue continuation | `head_sha` 保持兼容；内部续队由 GitHub Actions actor 区分，不暴露可伪造的 workflow input |
| `.github/scripts/release-channel-contract-check.sh` | shell contract check | internal | Modify | None | CI maintainers | `CI (PR)`, `CI (main)` | 仅保留离线 contract/self-test |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given `type:patch + channel:stable` 或 `type:patch + channel:rc`，When `PR Label Gate` 与 snapshot materialization 运行，Then label 结果与现状一致，未知/缺失/冲突 label 仍按既有契约失败。
- Given `main` 上连续合并多个 release-enabled PR，When GitHub concurrency 只实际跑了最新的 `CI (main)`，Then snapshot job 会为缺失的一阶父链 commits 补齐 snapshot，`Release` 按 oldest pending 顺序逐个发布，不漏掉中间版本。
- Given `Release` 内部 dispatch 下一个 pending target，When 后续 run 以 `workflow_dispatch(admin_action=release)` 且 `github.actor=github-actions[bot]` 启动，Then 它继续处理下一个 pending target；任意手工 dispatch 只处理指定 target。
- Given 较晚时间再手动 backfill 较早的 `main` commit，When `release_snapshot.py ensure/export` 运行，Then 版本号取自该 commit 在一阶父链上的前序发布锚点，而不是仓库当前最新 tag。
- Given `CI (PR)` / `CI (main)` 执行 live quality-gates，When GitHub API 被调用，Then 使用 `GITHUB_TOKEN` 的 authenticated request；失败时仅反映真实 branch-rules drift。
- Given `channel:rc` snapshot，When `Release` 发布，Then 只发布 `*-rc.<sha7>` 和 prerelease，且不更新 `latest`。
- Given `#176` 的 merge commit `ea596780c9f6f2fff8148d2d103d625f22179369`，When backfill 完成，Then 对外可见版本为 `0.34.1`，且对应 `Release` run 不再以 `skipped` 结束。

## 实现前置条件（Definition of Ready / Preconditions）

- snapshot JSON shape、queue 语义与 manual backfill 约束已明确。
- release workflow 继续保留无前缀 stable tag 与 `head_sha` dispatch 接口的兼容要求已冻结。
- live quality-gates 的鉴权来源已明确为 `secrets.GITHUB_TOKEN`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `release_snapshot.py` 通过本地脚本化自测，覆盖 stable/rc/docs/skip、target-only backfill、burst merge catch-up、existing tag 幂等。
- Integration tests: `bash ./.github/scripts/release-channel-contract-check.sh`
- E2E tests (if applicable): GitHub Actions workflow YAML 解析 + local dry-run export

## Related ADRs

- [0005-source-build-release-gate](../../adr/0005-source-build-release-gate.md)

## UI / Storybook (if applicable)

- None

## Quality checks

- Lint / typecheck / formatting: `bash -n`、`python3 -m py_compile`、`ruby -e 'require "yaml"; ...'`

## 文档更新（Docs to Update）

- `README.md`: 更新 release topology、snapshot queue、manual backfill 与 authenticated live quality-gates 说明。
- `docs/specs/README.md`: 增加本 spec 索引行。
- `docs/specs/kv9pg-quality-gates-final-alignment/SPEC.md`: 记录 authenticated live-gates 已落到 CI 显式步骤。

## 计划资产（Plan assets）

- Directory: `docs/specs/48mh8-release-snapshot-queue-alignment/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.
- If an asset must be used in impl (runtime/test/official docs), list it in `资产晋升（Asset promotion）` and promote it to a stable project path during implementation.

## Visual Evidence

- None

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 `release_snapshot.py` 与自测脚本，冻结 snapshot schema 和 first-parent 版本计算。
- [x] M2: `CI (main)` 增加 snapshot materialization，`CI (PR/main)` 拆分并接入 authenticated live quality-gates。
- [x] M3: `Release` workflow 改为消费 snapshot queue、支持 queue continuation 与 manual backfill。
- [x] M4: README / specs / contract checks 同步完成，并完成本地验证。
- [ ] M5: 远端 PR 合并后补发 `#176` 对应 `0.34.1`，验证 tag / GitHub Release / GHCR 到位。

## 方案概述（Approach, high-level）

- 以 git notes 存储不可变 release snapshot，把“label 解析 + 版本号决定”从即时 workflow_run 推断改为可重放的 main-history artifact。
- `CI (main)` 负责沿 first-parent 路径补齐 snapshot，`Release` 负责基于 notes + tags 选择 oldest pending target 并发布。
- live quality-gates 改成 workflow 显式 step，以 `GITHUB_TOKEN` 调 `check-live-quality-gates.py`；离线 contract 脚本只验证静态约束和 mock API 场景。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：git notes 写入/push 在并发场景下可能发生冲突，需要 retry 与幂等读取。
- 风险：旧 stable release 在补发历史版本后不应错误刷新 `latest`，因此 export 时需按当前 main 重新解析 publication tags。
- 假设：`PR #176` 是当前唯一明确漏发的 stable release，补发目标版本确定为 `0.34.1`。

## 变更记录（Change log）

- 2026-03-23: 创建规格，冻结 release snapshot queue、authenticated live quality-gates 与 `#176` backfill 验收口径。
- 2026-03-23: 完成本地实现与验证：snapshot queue、CI authenticated live-gates、release queue continuation、自测与 README/spec 同步已落地；剩余 M5 为 merge 后的远端补发操作。

## 参考（References）

- `~/.style-playbook-skills/skills/style-playbook/references/tags/pr-label-release.md`
- `docs/specs/mzqkx-release-channel-selection/SPEC.md`
- `docs/specs/kv9pg-quality-gates-final-alignment/SPEC.md`
