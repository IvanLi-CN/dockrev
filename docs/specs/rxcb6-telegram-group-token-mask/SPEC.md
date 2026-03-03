# Dockrev：Telegram 群组支持与 Bot Token 脱敏改造（#rxcb6）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- Settings 页 Telegram 配置当前把 `botToken` 与 `chatId` 都按密文掩码返回（`******`），造成 `chatId` 不可见且 Bot token 的刷新后掩码长度不可控。
- Bot token 输入框当前不是密码输入，不符合敏感信息默认隐藏的交互预期。
- 用户希望保持单 `chatId` 模式下可直接填写群组/频道目标（包含负数群组 ID），并确保后端不回传 Bot token 明文。

## 目标 / 非目标

### Goals

- Telegram 保持单 `chatId` 模式，并支持群组/频道 ID（字符串，不限制负号前缀）。
- 后端 GET `/api/notifications` 不返回 Bot token 明文。
- 前端 Bot token 使用密码输入框，支持眼睛按钮切换显示/隐藏。
- 刷新后若已配置 Bot token，显示固定 16 位圆点掩码（`••••••••••••••••`），不再使用星号。
- `chatId` 明文显示并可编辑。

### Non-goals

- 不做多 `chatId` 批量通知。
- 不重构 Telegram 发送实现（仍使用单目标 `chat_id` 调用）。
- 不新增 Telegram topic/thread/channel 路由字段。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/stories/mocks/dockrevMockApi.ts`

### Out of scope

- `crates/dockrev-api/src/notify.rs` 的消息发送逻辑。
- 其他通知渠道（Email/Webhook/WebPush）的契约改动。

## 接口契约（Contracts）

### GET /api/notifications（Telegram）

- `telegram.botToken`: 不返回真实值（缺省或 `null`）。
- `telegram.botTokenConfigured: boolean`: 标记是否已配置 Bot token。
- `telegram.chatId`: 明文返回。

### PUT /api/notifications（Telegram）

- 保留 Bot token 的 secret merge 语义：未提交新 token 时沿用旧值。
- `chatId` 按普通字段处理（不走 secret merge）。

## 验收标准（Acceptance Criteria）

- Given Telegram 已保存 token，When 刷新设置页，Then Bot token 输入框默认隐藏且显示固定 16 位圆点掩码。
- Given Bot token 输入框，When 点击眼睛按钮，Then 可在明文/密文显示间切换。
- Given 后端 GET `/api/notifications`，When 读取 Telegram，Then `botToken` 不泄露明文且 `botTokenConfigured` 准确。
- Given `chatId` 已配置，When 刷新设置页，Then `chatId` 明文可见。
- Given 未输入新 token 仅改动其他 Telegram 字段，When 保存，Then 原 token 不被覆盖。

## 里程碑（Milestones / checklist）

- [x] M1: 新增规格并登记到 `docs/specs/README.md`。
- [x] M2: 后端契约改造（bot token 不回传，chatId 明文，botTokenConfigured）。
- [x] M3: 前端 Telegram 输入交互改造（密码框 + 眼睛按钮 + 固定圆点掩码）。
- [x] M4: 测试与构建验证通过。

## 风险 / 假设

- 风险：旧客户端若依赖 `telegram.botToken="******"` 可能需要兼容适配。
- 假设：固定长度掩码（16 位）不需要暴露真实 token 长度。

## 变更记录（Change log）

- 2026-03-03: 新建规格，冻结范围为“单 `chatId` 群组支持 + Bot token 脱敏 + Settings 交互改造”。
- 2026-03-03: 完成后端通知契约改造：`botToken` 不回传，新增 `botTokenConfigured`，`chatId` 明文返回；并为 legacy 掩码与字段缺失场景增加兼容合并逻辑。
- 2026-03-03: 完成前端 Settings Telegram 区块改造：密码输入框、眼睛切换、固定 16 位圆点掩码、`chatId` 明文编辑、保存前归一化。
- 2026-03-03: 完成回归验证：`cargo test -p dockrev-api`、`bun run --cwd web lint`、`bun run --cwd web build` 全部通过。
- 2026-03-03: 执行 codex review 闭环并修复审查问题（旧掩码兼容、字段缺失保留语义、文档契约与 mock 语义对齐）。
