# Dockrev Web: 版本气泡 debug 信息补齐（#dxdvu）

## 状态

- Status: 待实现
- Created: 2026-02-08
- Last: 2026-02-08

## 背景 / 问题陈述

- 版本气泡（Current/Candidate）已能展示 digest tags，但仍有两个“可追溯性/可读性”痛点：
  - ServiceDetail 页 banner 的 `当前 → 候选` 仍是纯文本：无法分别打开“当前版本”与“候选版本”的气泡，导致排查“当前版本为什么无法推测”不顺手。
  - digest tags 的聚合依赖逐个 tag 拉 manifest：当部分请求 timeout/error 时，服务端会静默跳过，UI 也无法判断 tags 列表是否完整。
- 另有一些“占位/提示”信息（例如显示 `?`、`标签=?`）对用户没有价值，且会污染视觉与理解。

## 目标 / 非目标

### Goals

- ServiceDetail banner：
  - 当前版本与候选版本的 tag 都是独立的 popover trigger（各自展示自己的气泡）。
  - 移除无价值提示文案（例如 `标签=?`）。
- CurrentVersionPopover / VersionTagsPopover：
  - trigger 默认显示有意义的信息（优先 resolvedTag，其次 raw tag；不展示 `?`）。
  - digest tags 列表展示“扫描摘要”（repo tags 总数、manifest ok/timeout/error），当可能不完整时明确提示原因。

### Non-goals

- 不修改后端 resolvedTag / resolvedTags 的推测逻辑与候选选择策略。
- 不改变 updateStatus 的判定口径（updatable/hint/crossTag 等）。

## 验收标准（Acceptance Criteria）

- Given ServiceDetail 页 banner 的 `当前 → 候选`，
  When hover/click 当前 tag 或候选 tag，
  Then 分别出现对应 popover（当前版本为 CurrentVersionPopover；候选为 VersionTagsPopover）。

- Given resolvedTag 缺失且 raw tag 非 semver，
  When 展示版本 trigger，
  Then 不出现 `?` 这类占位符；UI 仍可通过 popover 查看“无法推测”的关键原因。

- Given digest-tags 接口在部分 manifest lookup timeout/error，
  When 打开 popover，
  Then UI 明确显示扫描摘要并提示“列表可能不完整”的原因；且仍可复制全部/匹配列表。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api` 通过
- `cd web && bun run lint` 通过
- 手工验收：Storybook 中相关 stories 交互符合预期

## 实现里程碑（Milestones）

- [ ] M1: ServiceDetail banner 改为可交互的 current/candidate popovers
- [ ] M2: digest-tags 增加 scan summary；UI 展示摘要与“可能不完整”提示
- [ ] M3: 最小验证（cargo test + web lint）+ Storybook 手工检查

## 风险 / 开放问题（Risks & Open Questions）

- 对 tags 很多的镜像，digest-tags 扫描可能较慢；需要在 UI 中用摘要与提示避免误解为“少了”。

## 变更记录（Change log）

- 2026-02-08: 创建计划并冻结范围与验收标准（Status=待实现）。

