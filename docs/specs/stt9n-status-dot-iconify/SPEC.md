# Dockrev：状态点升级为 Iconify（全站）（#stt9n）

## 状态

- Status: 已完成
- Created: 2026-03-01
- Last: 2026-03-01

## 背景 / 问题陈述

- 更新候选列表中的状态点在 light 主题下存在样式覆盖问题，`可更新` 与 `无更新` 会出现同形感知，降低可读性。
- 当前状态点只依赖颜色表达，在复杂表格和浅色背景中辨识成本偏高。
- 主人明确要求图标方案必须使用 Iconify，并覆盖更新候选与 Settings webhook 状态点。

## 目标 / 非目标

### Goals

- 引入 `@iconify/react` + `@iconify-icons/mdi` 并建立统一状态图标映射（本地 icon data，避免运行时远程拉取）。
- 修复 light 主题状态点颜色覆盖，确保 `ok/updatable/hint/archMismatch/blocked` 在 light/dark 下可稳定区分。
- 全站统一状态点样式语义：Overview/Services 的 `StatusRemark` 与 Settings webhook 列表保持一致风格。
- 补齐 Storybook 场景用于回归验证。

### Non-goals

- 不修改后端 API、数据库、状态计算规则或更新执行逻辑。
- 不引入 `unplugin-icons`、不改 Vite 构建链。
- 不做全局主题 redesign。

## 范围（Scope）

### In scope

- `web/package.json`
- `web/bun.lock`
- `web/src/updateStatus.ts`
- `web/src/ui.tsx`
- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/stories/components/StatusRemark.stories.tsx`
- `web/src/stories/pages/SettingsPage.stories.tsx`（如需补充状态故事）

### Out of scope

- `src/**` Rust runtime behavior
- API contracts under `web/src/api.ts`

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 备注（Notes） |
| --- | --- | --- | --- | --- |
| `statusIconName(st: RowStatus)` | TS helper | internal | Add | 统一 row 状态到 Iconify icon 名称的映射 |
| `webhookStateIconName(state: string)` | TS helper | internal | Add | 统一 webhook 状态到 Iconify icon 名称的映射 |
| `StatusRemark` | React component | internal | Modify | 在状态点内渲染 Iconify 图标，保持 label/note 可访问性语义 |

## 验收标准（Acceptance Criteria）

- Given 同一组服务中同时存在 `updatable` 与 `ok`，When 在 light 主题查看 Overview/Services，Then 状态点颜色不同且图标不同。
- Given `hint/archMismatch/blocked` 状态，When 在 light/dark 主题切换，Then 各状态点可稳定区分且不依赖 hover。
- Given Settings webhook 列表，When 出现 `ok/missing/queued/running/error/conflict/unknown`，Then 每个状态都有明确图标映射，文案保持原语义。
- Given 运行 lint/typecheck/build，When 构建完成，Then 无新增错误并通过。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cd web && bun --bun ./node_modules/.bin/eslint .`
- `cd web && bun --bun ./node_modules/.bin/tsc -b`
- `cd web && bun --bun ./node_modules/.bin/vite build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 接入 `@iconify/react` + `@iconify-icons/mdi` 并定义状态图标映射。
- [x] M2: `StatusRemark` 渲染 Iconify 图标并保留现有文本语义。
- [x] M3: Settings webhook 状态点切换到统一 Iconify 语义。
- [x] M4: 修复 light 主题状态点颜色覆盖，确保状态色不被主题规则吞掉。
- [x] M5: Storybook 场景补齐并完成 lint/typecheck/build 验证。

## 风险 / 假设

- 风险：状态点尺寸内嵌 icon 后可能影响行高或对齐，需要通过故事与页面实测回归。
- 假设：Iconify 使用 `mdi` 图标集即可满足状态语义，无需额外视觉资源。
