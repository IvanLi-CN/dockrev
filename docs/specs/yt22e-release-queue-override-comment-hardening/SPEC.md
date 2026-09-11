# Dockrev：Release Queue Override 收口硬化（#yt22e）

## 状态

- Status: 作废（被 ADR 0008 的 PR label release identity 合同替代）
- Created: 2026-03-30
- Last: 2026-03-30

## 背景 / 问题陈述

- 现有 release topology 已采用 `PR label -> immutable release snapshot -> oldest pending queue -> publication ledger`，整体方向与 style-playbook 一致。
- 近期线上未发布 `#185` 的根因不是产品代码错误，而是 `PR #187 / 1913d2dc9e5308c78a301f8174492bbb4f553269` 只改了 release 基础设施，却被打成了 `type:patch + channel:stable` 并进入 release queue。
- 当前 queue 一旦冻结到这种误标 target，就会在 tag/publish 路径上持续失败，后续真实产品发布全部被挡住。
- source-PR release result comment 不属于当前 release contract；successful publication 由 release-owning agent 报告给 owner。

## 目标 / 非目标

### Goals

- 阻止 release-infra-only PR 以 `type:patch|minor|major` 进入 release queue。
- 在不改写 immutable snapshot 的前提下，为已冻结的误标 target 提供独立 `skip override` 恢复路径。
- 在不引入任何额外凭据的前提下恢复 release queue，并让后续真实产品版本继续发布。

### Non-goals

- 不改变 `type:*` / `channel:*` label 枚举本身。
- 不引入 PAT、GitHub App 或任何额外 secrets 作为 tag/publish 凭据。
- 不修改业务代码、数据库 schema 或用户可见 HTTP API。

## 范围（Scope）

### In scope

- `.github/workflows/label-gate.yml`
- `.github/workflows/release.yml`
- `.github/scripts/release_snapshot.py`
- `.github/scripts/test-release-snapshot.sh`
- `README.md`
- `docs/specs/README.md`

### Out of scope

- `crates/**`
- `web/**`
- GHCR 镜像命名、release asset 格式、产品业务功能

## 需求（Requirements）

### MUST

- PR Label Gate 必须拒绝“带 `type:patch|minor|major` 且触碰 release-infra 范围”的 PR。
- release-infra 范围至少包括：`.github/workflows/**`、`.github/scripts/release_*.py`、`.github/scripts/test-release-snapshot.sh`。
- immutable `refs/notes/release-snapshots` 不得被事后改写来修正误标 target。
- 新增独立 override ledger（`refs/notes/release-overrides`）记录管理员决策，至少支持 `{ target_sha, status, reason, created_at }`。
- `next-pending` 必须跳过被标记为 `status=skip` 的 frozen target。
- `Release` workflow 的 manual/admin 路径必须支持对指定 `head_sha` 记录 skip override，并能继续 queue。
- release workflow 不得通过 source PR comment API 承载 successful publication 结果。

### SHOULD

- release summary / exported metadata 应显式暴露当前 target 的 `published / skipped / pending` 状态，便于运维核对。
- gate 失败文案应明确指出 remediation：release-infra PR 改用 `type:skip` / `type:docs`，或拆出产品改动后再发布。
- 本次恢复路径应固定覆盖 `1913d2dc9e5308c78a301f8174492bbb4f553269`，理由明确写成“release-infra mislabel under no-extra-credential model”。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `PR Label Gate` 继续校验 `type:*` / `channel:*` 唯一性，同时增加 diff-aware 检查：若 PR 改动全部落在 release-infra 范围内，则只允许 `type:skip` 或 `type:docs`；若任意 release-enabled PR 触碰 `.github/workflows/**`，直接 fail。
- `release_snapshot.py` 增加 override 读写与状态判断：
  - `record-override`：为指定 `target_sha` 写入 skip override。
  - `next-pending`：返回最老且未发布、未 skip 的 pending target。
  - `export`：在保留 snapshot 原始事实的同时，额外导出 queue state，供 workflow summary / admin run 读取。
- `release.yml` 的 `workflow_dispatch` 增加 admin 模式，可对指定 SHA 写 skip override；写入成功后重新解析 pending queue 并继续 dispatch 下一条真实目标。
- publish 成功后记录 publication ledger；successful publication 由 release-owning agent 报告给 owner。

### Edge cases / errors

- 对不存在 snapshot 的 SHA 记录 override 必须失败，避免把 override ledger 当成旁路发布入口。
- 若 target 已发布，再次写 skip override 必须失败或无操作，防止 published/skipped 状态冲突。
- 若管理员手动指定的 `head_sha` 已被 skip override 标记，manual `admin_action=release` 必须在 prepare 阶段直接失败，不能继续构建/推送半程产物。
- 若 queue 因全是 skipped / docs / skip targets 而为空，workflow 应正常结束并给出 summary，而不是失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `refs/notes/release-overrides` | Git notes JSON payload | internal | New | None | CI maintainers | `release_snapshot.py`, `release.yml` | mutable admin override ledger |
| `.github/scripts/release_snapshot.py record-override` | CLI | internal | New | None | CI maintainers | `release.yml`, operators | skip frozen mislabel targets |
| `.github/workflows/label-gate.yml` | workflow policy | internal | Modify | None | CI maintainers | PR authors / merge queue | release-infra label hardening |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 一个只改 `.github/workflows/release.yml` 或 `.github/scripts/release_*.py` 的 PR，When 它被打上 `type:patch|minor|major`，Then `PR Label Gate` 失败，并提示改成 `type:skip|type:docs`。
- Given `1913d2dc9e5308c78a301f8174492bbb4f553269` 已存在 immutable snapshot 且尚未发布，When 记录 `skip override` 后再跑 `next-pending`，Then 该 SHA 不再出现在返回结果里。
- Given 成功发布某个真实 release-enabled target，When publish job 完成，Then publication ledger 已记录，且 release-owning agent 向 owner 报告 successful publication；source PR 不新增发布结果评论。
- Given 当前 release queue 中前一条 target 被 skip，When queue continuation 运行，Then 后一条真实产品 target 仍可继续发布。
- Given 带 `repoUrl auto-backfill` 的 commit 已在 pending queue 中，When 误标 target 被 skip 并恢复 release queue，Then 发布出包含该功能的版本后，101 上会出现 `repo_link_backfill` job，随后 repo icon 可见。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-snapshot.sh`
- `python3 -m py_compile .github/scripts/release_snapshot.py`
- `bash ./.github/scripts/release-channel-contract-check.sh`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml"); YAML.load_file(".github/workflows/label-gate.yml")'`

### UI / Storybook (if applicable)

- None

## 文档更新（Docs to Update）

- `README.md`: 增补 release-infra PR label policy 与 skip override 恢复方式。
- `docs/specs/README.md`: 新增本 spec 索引，并在完成后记录 fast-track 收口说明。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 release-infra label gate 规则，并冻结 remediation 文案。
- [x] M2: 引入 `refs/notes/release-overrides` 与 `record-override(skip)`，让 queue 可跳过 frozen mislabel target。
- [x] M3: `Release` workflow 支持 manual skip override，并完成 queue continuation 对接。
- [x] M4: source-PR release comment 子合同移除，publication ledger 继续作为发布事实。
- [ ] M5: 快车道收敛到 latest PR merge-ready，并完成 `PR #187` skip 恢复验证路径。

## 方案概述（Approach, high-level）

- 保持 `snapshot = immutable intent truth`、`publication ledger = published fact truth` 不变，再增加 `override ledger = admin recovery truth`，把误标恢复做成第三条显式轨道。
- 用 PR diff-aware label gate 在入口处挡掉 release-infra mislabel，避免再往 queue 里冻结错误 target。
- 将 publication ledger 作为发布事实记录，successful publication 由 release-owning agent 报告给 owner。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：label gate 的 release-infra 识别范围过窄会漏掉同类 PR；过宽会误伤夹带少量产品改动的 PR，因此需要在测试里覆盖 infra-only 与 mixed-diff 两类场景。
- 风险：skip override 是管理员恢复工具，必须保证只对已有 snapshot 生效，避免演变成旁路发版接口。
- 假设：当前不新增任何额外发布凭据；release 恢复完全依赖现有 `GITHUB_TOKEN`、git notes 与 workflow dispatch。

## 变更记录（Change log）

- 2026-03-30: 创建 follow-up spec，冻结“release-infra PR 不得进入发布队列 + frozen mislabel target 走 skip override”两件事的实施口径。
- source-PR release comment 子合同已废止；label gate、override ledger 与 workflow admin path 继续有效。

## 参考（References）

- `~/.style-playbook-skills/skills/style-playbook/references/tags/pr-label-release.md`
- `docs/specs/48mh8-release-snapshot-queue-alignment/SPEC.md`
- `docs/specs/qnq3w-release-publication-latest-pr-comment/SPEC.md`
