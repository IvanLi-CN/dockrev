# Dockrev Web: 版本气泡列表默认折叠（#zwsh7）

## 状态

- Status: 待实现
- Created: 2026-02-11
- Last: 2026-02-11

## 背景 / 问题陈述

- 版本气泡（Current/Candidate）为了可追溯性会展示 tags 列表，但在 tags 很多时会默认 dump 大段文本，体验差且干扰阅读。
- 当前版本的 display 口径若直接使用 `resolvedTag`，可能把 `latest` 这类非 semver tag 显示成“当前版本”，造成误导（当前版本无法推测时尤其明显）。

## 目标 / 非目标

### Goals

- CurrentVersionPopover / VersionTagsPopover：
  - tags 列表默认不直接展开，改为通过“显示列表/隐藏列表”按需展开。
  - 仍保留统计信息、扫描摘要提示、复制（全部/匹配）能力，保证排查与可追溯性不丢。
- 版本列 display 口径：
  - 仅当 `resolvedTag` 为严格 semver 时才用于 display；否则回退到 raw tag（严格 semver）或 `-`。
  - `latest` 等非 semver tag 不应出现在“当前版本 display”里（可在 raw tag 明细气泡中看到）。

### Non-goals

- 不修改后端 resolvedTag/resolvedTags 的推测逻辑与 candidates 选择策略。
- 不改变 tags 扫描/聚合接口的返回结构。

## 验收标准（Acceptance Criteria）

- Given popover 打开且 tags 很多，
  When 默认展示，
  Then 不直接渲染长列表（无默认大段 `<pre>`）；仅展示统计与操作按钮。

- Given 用户点击“显示列表”，
  When 列表展开，
  Then 可以看到完整 tags 列表；可过滤；可复制全部/匹配内容。

- Given 当前 resolvedTag=latest 且 raw tag 非严格 semver，
  When 渲染版本列 display，
  Then 不显示 `latest`；应显示 `-`（并可通过 popover 查看“无法推测”的原因）。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cd web && bun run lint` 通过
- `cd web && bun run build` 通过
- 手工验收：Storybook 中 CurrentVersionPopover / VersionTagsPopover 行为符合预期

## 实现里程碑（Milestones）

- [ ] M1: CurrentVersionPopover 的 digest tags 默认折叠（按需展开 + 过滤/复制不退化）
- [ ] M2: VersionTagsPopover 的 digest tags / repo tags 默认折叠（两个区块互不影响）
- [ ] M3: 版本列 display 仅显示严格 semver 的 resolvedTag（避免 `latest` 等误导）
- [ ] M4: 最小验证（lint/build）+ Storybook 手工检查

## 风险 / 开放问题（Risks & Open Questions）

- 默认折叠会降低“开箱即见”的信息量，但可以显著降低噪音；保留统计与复制作为平衡点。

