# Dockrev：shadcn/ui 全量对齐与 Storybook Docs/Stories 补齐（#appaf）

## 状态

- Status: 已实现
- Created: 2026-03-07
- Last: 2026-03-07

## 背景 / 问题陈述

- `web` 当前存在一套自维护的基础组件层（button / switch / popover / confirm / chip 等），与页面内大量原生表单控件并存，交互模式和可维护性分散。
- 仓库尚未接入 shadcn/ui、Tailwind v4 与 `components.json`，共享 UI 没有统一来源，复用成本偏高。
- Storybook 虽然已覆盖部分组件与页面，但 reusable component 的 stories/docs 覆盖不完整，且当前未启用 autodocs / attached MDX 约定。

## 目标 / 非目标

### Goals

- 在 `web` 中按 Vite + Tailwind v4 官方路线接入 shadcn/ui，并将共享基础组件统一收敛到 `@/components/ui/*`。
- 保持当前 dark/light 主题语义与业务交互不变，仅替换共享 UI 骨架与实现来源。
- 为每个 reusable component 补齐 Storybook 入口；simple primitives 使用 autodocs，复杂复合组件补 attached MDX Docs。
- 推进到 fast-track 收口：本地验证、推送分支、创建/复用 PR、等待 checks 明确并执行 review-loop 收敛。

### Non-goals

- 不改动 Rust / 后端 API / 数据库契约。
- 不把 `AppShell` / sidebar / nav 壳层整体迁移为 shadcn `Sidebar` / `Sheet`。
- 不为 icon helper 与 typography helper 单独拆成逐个 story 页面；统一归入组合文档。

## 范围（Scope）

### In scope

- `web/package.json`、`web/vite.config.ts`、`web/tsconfig*.json`：接入 Tailwind v4、alias、Storybook docs 支撑。
- `web/components.json`、`web/src/lib/utils.ts`、`web/src/components/ui/*`：落地 shadcn primitives。
- `web/src/index.css`、必要时 `web/src/App.css`：把现有主题 token 映射到 shadcn CSS variables，并保留现有页面样式体系。
- `web/src/ui.tsx`、`web/src/ConfirmProvider.tsx`、`web/src/components/*.tsx`、相关页面：迁移共享交互实现。
- `web/src/stories/components/*`、`web/.storybook/*`：补齐 stories、autodocs 与 attached MDX docs。

### Out of scope

- 页面结构性重设计。
- 新增业务功能或 API 字段。
- PR 视觉证据截图（除非后续 review 明确需要）。

## 共享组件清单（Inventory）

### Primitives（必须来自 `@/components/ui/*`）

- Button
- Input
- Select
- Switch
- Badge
- Tabs
- ToggleGroup
- Tooltip
- Popover
- AlertDialog
- Label

### Wrappers / Composites（必须在 Storybook 中有 Docs + Stories）

- IconButton
- ResponsiveActionButton
- ConfirmDialog（通过 `ConfirmProvider` 暴露）
- UpdateCandidateFilters
- NotificationChannelCard
- ServiceResourcePanel
- CurrentVersionPopover
- VersionTagsPopover

### Combined docs（不单列每个 helper）

- Typography：`Mono`、`SectionTitle`
- Iconography：`ArrowRightIcon`、`RefreshIcon`、`TrashIcon`、`GitHubIcon`

## 设计决策 / 组件映射

- `Button` / `IconButton`：基于 shadcn `Button`，带 hint 的场景统一使用 `Tooltip` 包装，不再保留 Floating UI 基础按钮实现。
- `Pill`：迁移为基于 shadcn `Badge` 的语义 wrapper，保留 `ok / warn / bad / muted / info` tone 映射。
- `Switch`：统一使用 shadcn `Switch`；如需拖拽/点击涂抹等特殊语义，可由业务组件外层补交互逻辑，但视觉与可访问性基座必须复用 shadcn primitive。
- `FilterChips` 与 ServiceResourcePanel 时间窗口切换：迁移到 `ToggleGroup`。
- ServiceResourcePanel 指标切换：迁移到 `Tabs`。
- Confirm 流程：`useConfirm()` 契约保持不变，内部实现切到 shadcn `AlertDialog`。
- 版本浮层：`CurrentVersionPopover` 与 `VersionTagsPopover` 共享基于 shadcn `Popover` primitives 的 hover-open / click-pin 基座；保留 hover、pin、outside click、Esc、refresh/copy 逻辑。
- 页面内共享表单控件统一改为 shadcn `Input` / `Select` / `Label`；业务特定按钮允许保留原生 `<button>`，但不得再承担 reusable primitive 角色。

## Storybook Docs 矩阵

### Autodocs（CSF + `tags: ['autodocs']`）

- Button
- Input
- Select
- Switch
- Badge（通过 `Pill` wrapper story 暴露语义 tone）
- Tabs
- ToggleGroup
- Tooltip
- Popover
- AlertDialog
- Label
- IconButton
- UpdateCandidateFilters

### Attached MDX Docs（复杂交互 / 组合行为）

- ResponsiveActionButton
- ConfirmDialog
- NotificationChannelCard
- ServiceResourcePanel
- CurrentVersionPopover
- VersionTagsPopover

### Combined docs（组合文档页）

- Typography
- Iconography

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 备注 |
| --- | --- | --- | --- | --- |
| `useConfirm()` | internal TS API | internal | None | 调用方式保持不变，内部改为 AlertDialog |
| `@/components/ui/*` | internal UI API | internal | Add | 作为共享 primitives 唯一来源 |
| `web/components.json` | shadcn config | internal | Add | 定义组件、别名与 Tailwind CSS 入口 |
| `web/.storybook/main.ts` stories globs | Storybook config | internal | Modify | 需纳入 `.mdx` docs |
| `/api/**` | HTTP API | external | None | 不变 |

## 验收标准（Acceptance Criteria）

- Given 任一 reusable component，When 在 Storybook 中访问，Then 存在对应 `Components/<Name>` entry，且可打开 docs 页面与至少一个运行 story。
- Given simple primitive（Button/Input/Select/Switch/Tabs/ToggleGroup/Tooltip/Popover/AlertDialog/Label），When 打开 Storybook docs，Then 通过 autodocs 渲染 props 与基本示例。
- Given complex composite（ResponsiveActionButton/ConfirmDialog/NotificationChannelCard/ServiceResourcePanel/CurrentVersionPopover/VersionTagsPopover），When 打开 Storybook docs，Then 存在 attached MDX，对交互语义、状态与使用方式有说明。
- Given 现有页面交互，When 完成迁移，Then `useConfirm()`、版本浮层 hover/pin 行为、通知测试气泡、资源面板 tabs/window 切换保持当前语义。
- Given 执行 `bun --cwd web lint`、`bun --cwd web build`、`bun --cwd web build-storybook -- --quiet`、`bun --cwd web test-storybook`，Then 全部通过。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1：创建 spec、索引与主题分支，冻结组件 inventory / docs 矩阵。
- [x] M2：接入 Tailwind v4 + shadcn/ui 基础设施并生成 primitives。
- [x] M3：完成 shared UI / overlay / composite 迁移。
- [x] M4：完成 Storybook stories + docs 补齐。
- [ ] M5：完成 lint/build/storybook/review/PR 收敛并同步 spec。

## 风险 / 假设

- 风险：现有 `App.css` 类名体系与 Tailwind utility 并存，若 token 映射不完整，可能导致 primitives 与页面视觉割裂。
- 风险：版本 popover 与 notification bubble 含较多时序逻辑，迁移时若过度“组件化”容易引入 hover/focus 回归。
- 假设：当前 repo 使用 bun 作为包管理器，且允许在 `web` 内引入新的前端依赖。
- 假设：Storybook 10 React Vite 可通过增加 docs 相关配置支持 autodocs 与 MDX。

## 变更记录（Change log）

- 2026-03-07：创建规格，冻结 shadcn/ui 迁移范围、共享组件 inventory 与 Storybook docs 策略。
- 2026-03-07：完成 Tailwind v4 + shadcn/ui 基础设施接入，新增 `web/components.json`、`web/src/lib/utils.ts`、`web/src/components/ui/*`、Vite/TS alias 与主题 token 映射，并保留现有 dark/light 语义。
- 2026-03-07：完成 shared UI 迁移：`web/src/ui.tsx` 收缩为兼容 barrel + app-specific wrappers，`ConfirmProvider` 切换到 shadcn `AlertDialog`，版本浮层收敛到共享 `HoverPinnedPopover` 基座，页面内共享输入/选择/切换统一改到 shadcn primitives。
- 2026-03-07：完成 Storybook 覆盖补齐：primitives 采用 autodocs，复杂复合组件补 attached MDX，`Typography` / `Iconography` 组合文档落地。
- 2026-03-07：本地验证通过：`bun --cwd web lint`、`bun --cwd web build`、`bun --cwd web build-storybook -- --quiet`、`bun --cwd web test-storybook`。
- 2026-03-07：review-loop 修复 hover-open → click-pin 固定气泡回归，并恢复 `DeployWelcomePage` 中 switch 与 label 的可点击关联；Storybook smoke 新增 hover-pin 留存校验。
