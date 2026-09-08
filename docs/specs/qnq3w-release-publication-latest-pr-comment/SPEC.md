# Dockrev：Release Publication 对齐 latest 与 publication ledger（#qnq3w）

## 状态

- Status: 已完成
- Created: 2026-03-28
- Last: 2026-03-28

## 背景 / 问题陈述

- 当前 `Release` workflow 的 `latest` 决策仍基于“`main` 上是否存在更晚 stable snapshot”，而不是“是否已经存在更晚 stable 发布记录”。
- 这会让 `0.35.8` / `0.35.9` 这种队列场景在发布时都跳过 `latest`，最终使 `ghcr.io/ivanli-cn/dockrev:latest` 与 `ghcr.io/ivanli-cn/dockrev-supervisor:latest` 停留在更旧版本。
- 当前 release 链路的发布事实应记录在 `refs/notes/release-publications`，而不是写入 source PR。
- 本 spec 中曾规划的 source-PR release comment 子合同已由 `taauj-release-api-tag-publish-contract` 废止；release-owning agent 负责向 owner 报告 successful publication。

## 目标 / 非目标

### Goals

- 将 `latest` 的定义冻结为“最新已发布 stable 镜像”，而不是“最新 stable snapshot”。
- 引入可审计的发布账本 `refs/notes/release-publications`，让 rerun / backfill 能基于“已发布事实”重新计算 `latest`。
- 保持现有 semver/tag 契约不变：stable 仍为 `<semver>`，rc 仍为 `<semver>-rc.<sha7>`。

### Non-goals

- 不改变 `type:*` / `channel:*` label 契约。
- 不改变 release 触发拓扑、镜像名、GHCR 仓库结构或 release assets 格式。
- 不向 source PR 写入发布结果或状态评论。

## 范围（Scope）

### In scope

- `.github/scripts/release_snapshot.py`
- `.github/workflows/release.yml`
- `.github/scripts/test-release-snapshot.sh`
- `README.md`
- `docs/specs/README.md`

### Out of scope

- 业务代码（`crates/**`, `web/**`）
- release label gate 与 release channel 定义
- GitHub Release 文案模板与 asset packing 策略

## 需求（Requirements）

### MUST

- `refs/notes/release-publications` 必须记录 `target_sha`、`pr_number`、`release_tag`、`release_channel`、`published_at`、`dockrev_digest`、`dockrev_supervisor_digest`。
- stable release 的 `latest` 判定必须只依赖“当前 `main` 一阶父链上是否已存在更晚 stable publication”，不得再被“更晚但尚未发布的 stable snapshot”压制。
- rc release 永远不得发布 `latest`。
- `Release` workflow 在 GitHub Release 创建/更新成功后，必须写入并推送 `release-publications` note；publication ledger 不得在 release 失败时前进。

### SHOULD

- older rerun / manual backfill 在不存在更晚 published stable note 时仍应带 `latest`，以修复漏发后的当前 stable 头部。
- 一旦更晚 stable publication 已存在，older rerun / backfill 应继续发布版本 tag 与 Release，但不得把 `latest` 回拨到旧版本。
- `dockrev` 与 `dockrev-supervisor` 应共用同一 published-stable 判定，避免两个镜像的 `latest` 漂移。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `prepare/export` 读取 immutable snapshot 后，若启用 `--resolve-publication-tags`，则根据 `refs/notes/release-publications` 重新计算 `publish_latest` / `tags_csv` / `supervisor_tags_csv`。
- `publish` job 在两个镜像 push 成功后，读取 push 结果中的 digest，写入当前 `target_sha` 对应的 publication note，并 push `refs/notes/release-publications`。
- rerun / manual backfill 重新执行时，只要当前 target 对应 release tag 允许幂等发布，publication ledger 就负责阻止旧 stable 重新夺回 `latest`。

### Edge cases / errors

- 若 publication note push 失败，workflow 必须失败，避免 GHCR 与 ledger 事实分裂。
- source PR 不参与 publication ledger 完成判定；发布结果由 release-owning agent 报告给 owner。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `refs/notes/release-publications` | Git notes JSON payload | internal | New | None | CI maintainers | `release_snapshot.py`, `release.yml` | mutable publication ledger |
| `.github/scripts/release_snapshot.py export` | CLI output | internal | Modify | None | CI maintainers | `release.yml` | `latest` 改为按 published stable 判定 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `0.35.8` 已有 stable snapshot 且 `0.35.9` 仅有 snapshot 尚未发布，When `0.35.8` export publication tags，Then `DOCKREV_TAGS_CSV` 与 `SUPERVISOR_TAGS_CSV` 仍包含 `:latest`。
- Given `0.35.9` 已写入 stable publication note，When rerun/backfill `0.35.8`，Then 仍发布 `0.35.8` version tag，但不再带 `:latest`。
- Given `channel:rc` target，When release 完成，Then 只发布 `*-rc.<sha7>`，GitHub Release 为 prerelease，且不写入 source PR 发布结果评论。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-snapshot.sh`
- `python3 -m py_compile .github/scripts/release_snapshot.py`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`

### UI / Storybook (if applicable)

- None

## 文档更新（Docs to Update）

- `README.md`: 更新 `latest` 定义与 publication ledger 行为。
- `docs/specs/README.md`: 新增本 spec 索引，并注明它是当前 release publication 语义来源。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 publication ledger schema / CLI，并将 `latest` 判定切换到已发布 stable。
- [x] M2: `Release` workflow 在镜像 push 后写入 publication ledger。
- [x] M3: 回归测试、README 与 specs index 同步完成。
- [x] M4: source-PR release comment 子合同已废止，successful publication 改由 release-owning agent 报告给 owner。

## 方案概述（Approach, high-level）

- 保留 immutable snapshot 作为“发布意图与版本号”真相源，再增加 mutable publication ledger 记录“哪些版本已真正发布出去”。
- `latest` 不再通过“看后面是否还有 stable snapshot”推断，而是通过“看当前主线是否已有更晚 stable publication”判定。
- publication ledger 直接记录 release 事实，release-owning agent 向 owner 报告 successful publication；source PR 不作为发布结果承载面。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：镜像 digest 记录依赖 build-push-action 输出；workflow 需要明确从 step outputs 读取而不是再做 registry 查询。
- 风险：publication ledger 属于 mutable notes ref，rerun 并发下仍需保证 last-write-wins 的幂等性和 note push 失败时的显式报错。
- 假设：release snapshot 中保留的 `pr_number` 仍属于发布事实关联数据，但不用于 source-PR comment API。

## 变更记录（Change log）

- 2026-03-28: 创建规格，冻结 `latest = newest published stable` 与 publication ledger 契约。
- source-PR release comment 子合同已废止；publication ledger 与 latest 判定继续有效。
- 2026-03-28: PR #187 收敛到 merge-ready；GitHub required checks 全部通过，取消态的 `PR Label Gate / Release intent label gate` 已通过 rerun 转正。

## 参考（References）

- `~/.style-playbook-skills/skills/style-playbook/references/tags/pr-label-release.md`
- `docs/specs/48mh8-release-snapshot-queue-alignment/SPEC.md`
- `docs/specs/mzqkx-release-channel-selection/SPEC.md`
