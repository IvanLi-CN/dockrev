# Dockrev Web: 表格左侧引导线对齐修复（#9m2ra）

## 状态

- Status: 已完成
- Created: 2026-02-03
- Last: 2026-02-03
- Notes: PR #49

## 背景 / 问题陈述

- 概览页（Overview）与服务页（Services）的列表使用“左侧引导线 + 圆点”来表达 stack 内 services 的分组结构。
- 现状：引导线的分段位置由前端用硬编码常量计算（row height / gap 等），当表格行高因样式调整（例如字体/line-height/padding）或文本换行而变化时，会出现“引导线与圆点错位”。

## 目标 / 非目标

### Goals

- 修复错位：引导线应与每行圆点在视觉上对齐。
- 保持表格记录行高的设计约束：每行按“两行内容高度”布局，不引入基于 DOM 尺寸的逐行动态测量。
- 提升可维护性：避免在 TS/TSX 中重复硬编码行高常量，减少字体/间距调整导致的漂移风险。

### Non-goals

- 不改变表格信息结构（列/字段/交互）。
- 不引入复杂的视觉回归系统（如全量 screenshot diff gating）。

## 范围（Scope）

### In scope

- OverviewPage / ServicesPage：
  - 将引导线渲染从“JS 逐段计算 top/height”改为“CSS 驱动（token 化 + 可重复背景）”；
  - 对 service name 进行最多两行的 clamping，避免文本导致行高突破既定约束。
- 最小验证：使用 Playwright（Storybook story）确认对齐不再漂移。

### Out of scope

- 为每个 stack/service 进行运行时 DOM 测量并实时重算引导线。

## 需求（Requirements）

### MUST

- Overview / Services 两处列表在常见数据与长 service 名场景下，引导线与圆点保持对齐。
- 行高保持“两行内容高度”的设计约束；超长文本最多展示两行（第二行截断）。
- 对齐策略应由样式 token 控制（单点调整），避免“改 CSS 后忘了同步 TS 常量”。

## 验收标准（Acceptance Criteria）

- Given 进入 Overview 与 Services 列表
  When 存在多条 service 且 service 名较长（需要换行）
  Then 左侧引导线分段与每行圆点保持对齐，不出现上下偏移
- Given 仅调整字体/line-height/padding（仍维持两行高度约束）
  When 页面重新渲染
  Then 引导线仍能正确对齐（无需修改 TS 常量）

## 测试 / 验证（Testing）

- Playwright：打开 Storybook 的 Overview/Services 相关 story，做基础截图或几何断言，确保 guide 与圆点不再错位。

## 里程碑（Milestones）

- [x] 以 CSS token 驱动引导线渲染，移除 JS 分段计算
- [x] service name 最多两行展示（clamp），避免行高突破
- [x] 最小 Playwright 验证（Storybook）

## 风险与开放问题（Risks & Open Questions）

- 需要明确表格行高 token（当前实现依赖多个子组件的 padding/line-height 组合）；应以单点 token 固定并被多个 CSS 规则复用。
