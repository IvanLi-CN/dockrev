# Dockrev：通知渠道独立测试按钮与气泡结果可视化（#gr3cs）

## 状态

- Status: 已完成
- Created: 2026-03-04
- Last: 2026-03-04

## 背景 / 问题陈述

- Settings 通知区当前只有“发送测试通知（全渠道）”按钮，无法按渠道单独验证。
- 测试反馈仅走全局错误文案，缺少渠道级进度、结果和具体报错定位。
- 用户要求交互对齐参考图：每个渠道都有带图标测试按钮，结果用气泡展示步骤与错误细节。

## 目标 / 非目标

### Goals

- 为 Email / Webhook / Telegram / Web Push 提供独立测试按钮（含图标）。
- 每个渠道测试时展示步骤气泡，覆盖“进行中 / 成功 / 失败”。
- 失败时展示具体错误原文（优先后端渠道级错误）。
- 测试结果常驻显示最近一次结果，下一次测试覆盖。
- 后端 `/api/notifications/test` 支持可选 `channel` 参数并保持旧调用兼容。
- 指定 `channel` 时，即使渠道未启用也要执行并返回可读报错。

### Non-goals

- 不新增通知渠道类型。
- 不重写通知发送 provider（SMTP/Telegram/Webhook/WebPush）。
- 不改动 GHCR、Queue、Service 页面行为。

## 范围（Scope）

### In scope

- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/api.ts`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/pages/SettingsPage.stories.tsx`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/notify.rs`
- `crates/dockrev-api/src/api/tests.rs`

### Out of scope

- 通知发送通道凭据存储模型变更。
- 非通知功能的 UI 结构重排。

## 接口契约

- `POST /api/notifications/test` 请求体新增可选字段：
  - `channel: "email" | "webhook" | "telegram" | "webPush"`
  - 保留 `message`（可选）。
- 行为：
  - 未指定 `channel`：保持现有语义，仅测试已启用渠道。
  - 指定 `channel`：仅测试该渠道，不受 enabled 开关限制。

## 验收标准（Acceptance Criteria）

- 通知区存在 4 个渠道级测试按钮（均含图标），且原“全渠道测试”按钮移除。
- 每个渠道按钮点击后会出现独立步骤气泡，显示进行中状态。
- 测试结束后气泡常驻显示最近结果（成功或失败）。
- 失败气泡包含具体错误信息（非泛化错误）。
- 渠道未启用或配置缺失时，渠道按钮可点击并返回具体缺失项错误。
- API 兼容旧调用（仅 message）且不回归。

## 里程碑（Milestones / checklist）

- [x] M1: 后端 `channel` 请求参数与单渠道发送逻辑落地（含兼容）。
- [x] M2: Settings 页面改为四个独立测试按钮 + 图标 + 气泡步骤展示。
- [x] M3: Storybook mock 与故事覆盖成功/失败/缺配置场景。
- [x] M4: API 回归测试补齐并通过。

## 风险 / 假设

- 假设：渠道错误字符串可直接作为面向用户可读消息展示。
- 风险：Web Push 在无订阅或缺私钥时多见失败，需明确展示错误并不影响其它渠道交互。

## 变更记录（Change log）

- 2026-03-04: 新建规格，冻结通知测试分渠道与气泡可视化验收口径。
- 2026-03-04: 完成后端 `channel` 扩展与单渠道测试覆盖，保持旧请求兼容。
- 2026-03-04: 完成 Settings 四渠道独立图标按钮与常驻步骤气泡，移除全渠道测试按钮。
- 2026-03-04: 完成 Storybook mock/故事与 API 回归测试；本地 lint/build/build-storybook 与 rust 相关测试通过。
