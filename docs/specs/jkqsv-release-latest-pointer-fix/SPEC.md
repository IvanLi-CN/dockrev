# Dockrev：GitHub Release latest 指针对齐 publication ledger（#jkqsv）

## 状态

- Status: 已完成
- Created: 2026-04-03
- Last: 2026-04-03

## 背景 / 问题陈述

- 当前 `Release` workflow 已经用 `refs/notes/release-publications` 正确决定 GHCR `latest` 是否应该随本次 stable 发布前进。
- 但 GitHub Releases 页面的 “Latest release” 指针仍然停留在旧版本，因为 `ncipollo/release-action` 默认按 GitHub 的 legacy 规则决定 latest，而不是复用 workflow 已经算出的 `publish_latest`。
- 这会让 backfill 之后的较新 stable release 虽然已经发布成功、PR comment 也已回写，却仍然无法成为 GitHub API / Releases 页面上的 `latest`。

## 目标 / 非目标

### Goals

- 将 GitHub Release 页面的 latest 指针与 `publish_latest` / publication ledger 真相源对齐。
- 保证较新的 stable backfill 可以显式把 GitHub latest 推进到正确版本。
- 保证较旧 stable rerun / backfill 不会借此把 GitHub latest 回拨到旧版本。

### Non-goals

- 不改变 release queue、tag 生成、GHCR 推送或 PR release comment 契约。
- 不新增新的 release notes ref、workflow 输入或管理员操作入口。

## 范围（Scope）

### In scope

- `.github/workflows/release.yml`
- `.github/scripts/release-channel-contract-check.sh`
- `README.md`
- `docs/specs/README.md`

### Out of scope

- `crates/**`
- `web/**`
- `release_snapshot.py` 的 publication ledger 计算逻辑

## 需求（Requirements）

### MUST

- GitHub Release 创建/更新步骤必须显式把 `needs.prepare.outputs.publish_latest` 传给 `ncipollo/release-action` 的 `makeLatest`。
- 当 `publish_latest=false` 时，older stable rerun / backfill 不得把 GitHub Release latest 指针回拨到旧版本。
- 当 `publish_latest=true` 时，较新的 stable backfill 必须能够把 GitHub Release latest 指针推进到正确版本。
- 离线 contract check 必须覆盖 `makeLatest` 与 `publish_latest` 的绑定，避免未来回归成默认 legacy 行为。

### SHOULD

- README 中的 release 说明应明确 GitHub Release page latest pointer 也受同一 `publish_latest` 决策控制。

## 验收标准（Acceptance Criteria）

- Given 一个 stable target 且 `publish_latest=true`，When `Release` workflow 创建/更新 GitHub Release，Then GitHub Release page / latest-release API 会把该版本视为 latest。
- Given 一个 stable target 且 `publish_latest=false`，When rerun / backfill 旧版本，Then GitHub Release 会继续更新版本自身 assets/notes，但不会夺回 latest。
- Given `channel:rc` target，When 发布 prerelease，Then `publish_latest=false` 且 GitHub latest 不变。
- Given 开发者运行 `bash ./.github/scripts/release-channel-contract-check.sh`，When workflow 漏掉 `makeLatest` 绑定，Then 脚本失败并报出 contract drift。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/release-channel-contract-check.sh`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'`

## 文档更新（Docs to Update）

- `README.md`
- `docs/specs/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `release.yml` 显式把 `publish_latest` 绑定到 `makeLatest`
- [x] M2: contract check 覆盖该绑定
- [x] M3: README / specs index 同步完成

## 方案概述（Approach, high-level）

- 保持 publication ledger 作为唯一 latest 判定真相源。
- 不再依赖 GitHub / release-action 的 legacy latest 决策，而是把 workflow 已经算好的 `publish_latest` 明确传入 GitHub Release step。
- 用离线 contract check 把这条约束冻结下来，防止未来再次出现“GHCR latest 正确但 GitHub latest 指针漂移”的分裂。

## 风险 / 假设（Risks, Assumptions）

- 假设 `ncipollo/release-action` 在 `allowUpdates=true` 路径下会尊重 `makeLatest` 并更新既有 release 的 latest 状态。
- 风险：GitHub latest-release API 的最终可见状态可能存在短暂延迟，因此线上验证需要允许短时间传播窗口。

## 参考（References）

- `docs/specs/qnq3w-release-publication-latest-pr-comment/SPEC.md`
- `docs/specs/taauj-release-api-tag-publish-contract/SPEC.md`
