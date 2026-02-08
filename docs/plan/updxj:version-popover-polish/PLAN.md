# Dockrev Web: 版本气泡可用性修复（#updxj）

## 状态

- Status: 待实现
- Created: 2026-02-08
- Last: 2026-02-08

## 背景 / 问题陈述

- 在版本气泡拆分（#k7hsm）之后，仍存在一些“体验与可追溯性”的尾项问题：
  - 更新确认 modal 中仍有残留的原生 tooltip/title 或缺少“当前版本”气泡，导致无法查看“为什么当前版本显示 `?`”。
  - digest tags 在标签较多时可读性差，用户无法确认是否“完整列出”。
  - 一些冗余提示文案（例如“将拉取镜像并重启容器；失败可能触发回滚。”）对齐后不再需要。

## 目标 / 非目标

### Goals

- 移除更新确认 modal / Storybook 中的冗余提示文案（不影响确认信息密度）。
- 更新确认 modal（OverviewPage / ServicesPage / ServiceDetailPage）中的“目标版本”行：
  - 当前版本 display 与 raw tag 均可点开 CurrentVersionPopover（独立气泡）。
  - 不再依赖浏览器原生 tooltip（不使用 `title` 拼多行信息）。
- CurrentVersionPopover / VersionTagsPopover：
  - digest tags 默认展示完整列表（可滚动）并支持过滤/复制。
  - “推测/说明”改为简洁、事实导向，避免无价值文案。

### Non-goals

- 不改后端 resolvedTag / resolvedTags 的推测逻辑。
- 不改变 UpdateTargetSelect 的候选选择策略。

## 验收标准（Acceptance Criteria）

- Given 更新确认 modal 的“目标版本”行，
  When hover/click 当前版本或原 tag，
  Then 出现对应气泡，并且不出现浏览器原生 tooltip（不依赖 `title`）。

- Given digest tags 数量很多，
  When 打开 popover，
  Then 可以看到完整列表，并可复制全部；过滤后可复制匹配。

- Given 当前版本显示为 `?`，
  When 打开当前版本气泡，
  Then 可以看到无法推测的关键原因（resolvedTag 缺失 / raw tag 非 semver / digest 未知或存在多 patch 等）。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cd web && bun run lint` 通过
- `cd web && bun run build` 通过
- 手工验收：Storybook/页面交互符合预期

## 实现里程碑（Milestones）

- [ ] M1: 移除冗余提示文案 + 更新确认 modal 的版本展示为 popovers
- [ ] M2: popovers 改为完整列表 + 过滤/复制 + 文案精简
- [ ] M3: 最小验证（lint/build）+ 手工检查（Storybook）

## 风险 / 开放问题（Risks & Open Questions）

- tags 列表可能较长，需要确保 popover 不溢出 viewport（max-height + scroll）。

## 变更记录（Change log）

- 2026-02-08: 创建计划并冻结范围与验收标准（Status=待实现）。
