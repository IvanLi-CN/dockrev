# Dockrev Web: 当前/候选版本气泡拆分 + 移除原生 tooltip（#k7hsm）

## 状态

- Status: 待实现
- Created: 2026-02-08
- Last: 2026-02-08

## 背景 / 问题陈述

- 现状：Versions 单元格使用 VersionTagsPopover 展示 `? → <candidate>` 的 tags；触发器上仍有浏览器原生 tooltip（`title`）。
- 问题：
  - 浏览器原生 tooltip 会遮挡阅读（截图标记①）。
  - 当前/候选共享同一个气泡（截图标记②），导致看不到当前版本为何显示 `?`（截图标记③/④）。

## 目标 / 非目标

### Goals

- 移除 Versions 行的浏览器原生 tooltip（不再使用 `title`）。
- 将 Versions 行拆分为两个独立气泡：
  - 当前版本（左侧，可能是 `?`）：展示 raw tag / resolvedTag / digest / resolvedTags，并解释为什么是 `?`。
  - 候选版本（右侧 tag）：继续展示该候选 digest 对应的所有 tags。
- 保持交互：hover 打开、click 固定、ESC/点外部关闭；行点击进入 Service 详情不受影响。

### Non-goals

- 不改后端推测逻辑（resolvedTag/resolvedTags 的生成规则不变）。
- 不重新设计 modal 预览中的 tooltip/title。

## 范围（Scope）

### In scope

- Web UI：ServicesPage / OverviewPage 版本列的交互拆分
- Web UI：新增 CurrentVersionPopover 组件；调整 VersionTagsPopover trigger 不再使用 `title`
- Storybook：更新/新增用例覆盖上述交互

### Out of scope

- Service 详情页的 banner/说明文案不做改动

## 验收标准（Acceptance Criteria）

- Given Services/Overview 列表存在 `? → vX.Y.Z` 的版本行，
  When 鼠标悬停/点击在版本行上，
  Then 不出现浏览器原生 tooltip（`title`）。

- Given 同一版本行，
  When hover/click 在左侧当前版本（`?` 或 resolved tag）上，
  Then 打开“当前版本”气泡，能看到 raw tag/resolvedTag/digest，并明确说明为什么显示 `?`（若为 `?`）。

- Given 同一版本行，
  When hover/click 在右侧候选 tag 上，
  Then 打开“候选版本 tags”气泡，内容与候选 digest 相关。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cd web && bun run lint` 通过
- `cd web && bun run build` 通过
- 手工验收：Services/Overview 两页交互符合预期

## 实现里程碑（Milestones）

- [ ] M1: 拆分 Versions 行为当前/候选两个 popover（Services/Overview）
- [ ] M2: 移除原生 tooltip + Storybook 回归/新增用例
- [ ] M3: 最小验证（lint/build）+ 手工检查

## 风险 / 开放问题（Risks & Open Questions）

- 当前版本气泡内容依赖服务端字段（digest/resolvedTag 等）；字段缺失时需要明确展示“未知/缺失”，避免误导。

## 变更记录（Change log）

- 2026-02-08: 创建计划并冻结范围与验收标准（Status=待实现）。

