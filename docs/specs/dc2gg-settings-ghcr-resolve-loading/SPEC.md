# Dockrev：Settings GHCR「解析并添加」加载反馈优化（#dc2gg）

## 状态

- Status: 已完成
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- 在 Settings 页 GHCR 区域点击 `解析并添加` 后，当前交互仅禁用按钮，无明确“正在处理”可视反馈。
- 用户在 `flushAutoSave(['ghcr']) -> resolve -> add/select -> refresh` 链路等待时，容易误判为页面卡住并重复点击。

## 目标 / 非目标

### Goals

- 仅为 `解析并添加` 按钮提供即时加载反馈（Spinner + 文案切换）。
- 按钮在 pending 期间禁用，避免重复提交。
- 异常/取消/成功后都能可靠恢复默认态，不残留 loading。
- 提供 Storybook 可复现场景（resolve 慢响应）用于交互回归。

### Non-goals

- 不改后端 API 与 GHCR 业务语义。
- 不扩展到 GHCR 其他按钮。
- 不重构全局 Button 组件 API。

## 范围（Scope）

### In scope

- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/pages/SettingsPage.stories.tsx`

### Out of scope

- `web/src/ui.tsx` `Button` 公共接口变更。
- GHCR 同步 webhook、删除冲突等操作的加载态统一。

## 需求（Requirements）

### MUST

- 新增 `ghcrResolvePending`（命名可等价）仅服务于 `解析并添加` 链路。
- 按钮 pending 文案固定为 `解析中…`，并显示视觉 spinner。
- pending 条件下按钮禁用，且与现有 `busy` 逻辑并存。
- 通过 `finally` 兜底清理 pending 状态。
- spinner 设置 `aria-hidden="true"`，避免冗余朗读。
- 为 `prefers-reduced-motion: reduce` 关闭旋转动画。

### SHOULD

- Storybook 慢响应场景在 800~1200ms 量级稳定触发，便于人工观察加载态。

## 验收标准（Acceptance Criteria）

- Given 输入 repo 或 owner，When 点击 `解析并添加`，Then 1 帧内出现 `解析中…` + spinner。
- Given pending 中连续点击，When 请求尚未完成，Then 不会触发重复 resolve 请求。
- Given resolve/add 任一环节报错，When 流程结束，Then 按钮恢复为 `解析并添加` 且错误提示沿用现有映射。
- Given 系统启用 reduced motion，When pending，Then spinner 不旋转但文案状态仍可识别。

## 里程碑（Milestones / checklist）

- [x] M1: SettingsPage 加入局部 pending 状态并接入 onClick 链路。
- [x] M2: App.css 增加按钮 spinner 样式与 reduced-motion 兼容。
- [x] M3: Storybook 新增 settings resolve 慢响应场景。
- [x] M4: lint/build 验证通过。

## 风险 / 假设

- 风险：仅按钮级反馈无法覆盖弹窗确认阶段的“用户感知耗时”；本次按范围不新增阶段条。
- 假设：`解析中…` 文案可满足当前产品语气要求。

## 变更记录（Change log）

- 2026-02-27: 新建规格，锁定“仅解析并添加按钮”的加载反馈改造范围。
- 2026-02-27: 完成 `解析并添加` 按钮 loading 反馈改造（`ghcrResolvePending`、按钮 spinner + `解析中…`、pending 禁用 + `finally` 复位）。
- 2026-02-27: 补充 Storybook 慢响应场景 `settings-configured-resolve-slow` 与 `ResolveLoading` story，支持稳定复现交互反馈。
- 2026-02-27: 完成 reduced-motion 兼容修正并通过 `bun run lint`、`bun run build` 与 `codex review --base origin/main`（无 P0/P1 阻塞）。
