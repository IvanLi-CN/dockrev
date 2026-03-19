# Dockrev：更新候选跨版本发现次数标记（#2hnkx）

## 状态

- Status: 已完成
- Created: 2026-03-19
- Last: 2026-03-20

## 背景 / 问题陈述

- 更新候选列表原本只显示“当前版本 -> 最新候选”，无法告诉操作者这次候选期间其实已经跨过了多少次不同的新版本发现。
- 现有 `new_version_notifications` 只覆盖通知链路，不是完整发现历史，直接拿它计数会漏掉通知关闭或未触发通知的发现事件。
- 用户要求在列表行与聚合预览中明确标记“我们程序发现了几次版本更新”，并且计数必须来自线性的成功 `check` 历史。

## 目标 / 非目标

### Goals

- 为服务持久化“新版本发现历史”，来源仅限成功完成的 `check` 任务。
- 基于当前版本基线，按“稳定可见版本优先、浮动 alias 回退 `candidateDigest`”统计发现次数，并包含当前最新候选。
- 在更新候选列表 `StatusRemark` 和 `AggregateUpdatePreviewList` 中显示中性计数 pill：`发现 N 次`。
- 对外通过 `GET /api/stacks` 与 `GET /api/stacks/{id}` 返回 `newVersionDiscoveryCount`。

### Non-goals

- 不把 `new_version_notifications` 当作计数真相源。
- 不为服务详情页单独新增发现次数 banner。
- 不回放失败或中断的 `check` 任务。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/db/new_version_discoveries.rs`
- `crates/dockrev-api/src/discovery.rs`
- `crates/dockrev-api/src/db/jobs.rs`
- `crates/dockrev-api/src/db/stacks.rs`
- `crates/dockrev-api/src/api/types/core.rs`
- `web/src/api.ts`
- `web/src/ui.tsx`
- `web/src/components/AggregateUpdatePreviewList.tsx`
- `web/src/stories/components/StatusRemark.stories.tsx`
- `web/src/stories/components/AggregateUpdatePreviewList.stories.tsx`
- `web/scripts/capture-storybook-screenshots.mjs`
- `docs/specs/README.md`

### Out of scope

- 服务详情页新增独立视觉提示。
- 对 discovery 次数做通知级别重放或追溯修正。

## 接口契约（Interfaces & Contracts）

- `Service` 响应新增可选字段：`newVersionDiscoveryCount?: number | null`。
- 计数规则固定为“同一当前版本基线下，按稳定 `candidateDisplayTag` 去重；若候选仍是浮动 alias 或无稳定展示值，则回退按 `candidateDigest` 去重”。
- 当前版本基线匹配优先级：
  - `currentDigest`
  - `currentDisplayTag`
  - `currentTag`

## 验收标准（Acceptance Criteria）

- Given 同一当前版本基线下先后发现 `v1.16.1(digest A)`、`v1.16.1(digest B)`、`v1.16.2(digest C)`，When 当前候选为 `v1.16.2`，Then `newVersionDiscoveryCount=2`。
- Given 同一 `candidateDisplayTag` 被多次成功 `check` 重复发现，When 统计当前基线次数，Then 只计一次。
- Given 候选仍是 `latest` 这类浮动 alias，When 没有稳定 `candidateDisplayTag` 可用，Then 回退按不同 `candidateDigest` 计数。
- Given 通知事件关闭或通知渠道全部关闭，When 成功 `check` 仍发现新版本，Then 计数仍可正确显示。
- Given 服务当前版本已经从基线 `X` 升级到 `Y`，When 查询 `Y` 的候选计数，Then `X` 基线历史不会混入。
- Given 历史上同一 `candidateDigest` 先以浮动 alias 出现、后又解析出稳定 `candidateDisplayTag`，When 统计当前基线次数，Then 不会因为这两条历史记录重复累计。
- Given 更新候选列表与聚合预览同时展示同一服务，When `newVersionDiscoveryCount` 存在，Then 两处都显示 `发现 N 次` 且不覆盖原备注。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `bun test --cwd web tests/updateStatus.test.ts tests/statusRemark.test.tsx`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web storybook:screenshots`

## Visual Evidence (PR)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: element
- sensitive_exclusion: N/A
- submission_gate: pending-owner-approval
- story_id_or_title: Components/StatusRemark/AllStatuses
- state: multi-status matrix
- evidence_note: 验证更新候选列表状态标签右侧新增 `发现 N 次` 计数，并且第二行备注仍然保留。

![StatusRemark 发现次数矩阵](./assets/status-remark-all-statuses.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: element
- sensitive_exclusion: N/A
- submission_gate: pending-owner-approval
- story_id_or_title: Components/AggregateUpdatePreviewList/AllStates
- state: aggregate preview modal list
- evidence_note: 验证聚合更新预览条目使用本地化状态标签，并在其后追加同款 `发现 N 次` 计数。

![AggregateUpdatePreviewList 发现次数矩阵](./assets/aggregate-update-preview-all-states.png)

## 变更记录（Change log）

- 2026-03-19: 新建规格，冻结“发现次数”来自成功 `check` 历史的持久化与 UI 展示范围。
- 2026-03-19: 完成后端 discovery 历史表、历史回填、API 字段透出与前端状态/聚合预览展示。
- 2026-03-19: 补充 Storybook 证据故事与截图，作为 PR 可视完工证据来源。
- 2026-03-20: 修正计数口径为“稳定可见版本优先、浮动 alias 回退 `candidateDigest`”，并通过 migration 自动重建历史 discovery 数据。
