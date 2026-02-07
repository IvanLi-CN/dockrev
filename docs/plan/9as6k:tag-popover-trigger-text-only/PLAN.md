# Dockrev Web: 版本 tags 气泡触发区域仅文本生效（#9as6k）

## 状态

- Status: 已完成
- Created: 2026-02-07
- Last: 2026-02-07

## 背景 / 问题陈述

- 现状：版本候选 tags 气泡（VersionTagsPopover）在列表中用于展示 `? → <candidate>` 对应的 tags。
- 问题：当前触发区域覆盖了整行/整格的空白区域，指针划过空白也会触发气泡，造成误触与干扰阅读。

## 目标 / 非目标

### Goals

- 触发区域（hover/click）仅覆盖“可见文本”的包围区域（例如 `? → unstable` 这一行文字本身）。
- 空白区域不触发气泡；行点击进入 Service 详情的行为保持不变。
- 保持当前交互：hover 打开、click 固定、ESC/点外部关闭。

### Non-goals

- 不调整气泡的内容渲染、候选列表加载策略、digest→tags 聚合逻辑。

## 范围（Scope）

### In scope

- Web UI：调整 VersionTagsPopover trigger 的布局/命中区域，使其不再横向拉伸覆盖整格。
- OverviewPage / ServicesPage 的展示不需改动逻辑，仅受 trigger 命中范围变化影响。

### Out of scope

- 更改 hover close delay、重新设计交互（例如改成纯 click）。

## 验收标准（Acceptance Criteria）

- Given Overview/Services 列表中存在候选版本触发器，
  When 指针悬浮在同一单元格内的空白区域（不在文字上），
  Then 气泡不会打开。

- Given 指针悬浮在候选版本文字上，
  When 指针进入文字区域，
  Then 气泡会打开；点击可固定；关闭行为符合现有逻辑。

## 非功能性验收 / 质量门槛（Quality Gates）

- `web` 的 `lint` 与 `build` 通过。
- 手工检查：触发区域与视觉一致，不出现“空白误触”。

## 实现里程碑（Milestones）

- [x] M1: 调整 trigger 命中区域（仅文本生效）
- [x] M2: 最小验证（lint/build）+ 手工检查

## 风险 / 开放问题（Risks & Open Questions）

- 命中区域变小可能影响 hover 打开气泡的可达性；可通过 click 固定作为补充。

## 变更记录（Change log）

- 2026-02-07: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-07: 完成实现与最小验证；PR #62；Status=已完成。
