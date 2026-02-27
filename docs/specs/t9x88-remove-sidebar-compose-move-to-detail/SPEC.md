# Dockrev：移除侧栏 Compose 区块并迁移到服务详情页（#t9x88）

## 状态

- Status: 已完成
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- 侧栏当前展示 `Compose / path / profile`，但数据来自“首个可用 stack”，不是当前上下文，容易误导。
- 主人已明确要求：截图位置不要再显示该信息，并希望在服务详情页查看更准确的 Compose 信息。

## 目标 / 非目标

### Goals

- 删除侧栏中的 `Compose / path / profile` 区块，保留导航、最近一次扫描与底部 meta。
- 在服务详情页新增 Compose 信息展示块，字段包含：
  - `compose.type`
  - `compose.composeFiles`（全量）
  - `compose.envFile`
- 清理前端不再需要的 `composeHint / onComposeHint` 透传链路与 Storybook 调用签名。

### Non-goals

- 不修改后端 API、DB、扫描策略与 auto-discovery。
- 不改动“最近一次扫描”的业务语义。

## 范围（Scope）

### In scope

- `web/src/Shell.tsx`
- `web/src/App.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/VersionInferencePage.tsx`
- `web/src/stories/layouts/AppShell.stories.tsx`
- `web/src/stories/mocks/PageHarness.tsx`
- `web/src/stories/pages/OverviewPage.stories.tsx`
- `web/src/stories/pages/ServicesPage.stories.tsx`
- `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- `web/src/App.css`

### Out of scope

- Rust 服务端与数据模型变更。

## 验收标准（Acceptance Criteria）

- Given 任意页面（概览/服务/队列等），When 查看侧栏，Then 截图位置不再出现 `Compose / path / profile`。
- Given 服务详情页，When 页面加载完成，Then 可看到 Compose 信息块，且内容来自当前 `stack.compose`。
- Given `composeFiles` 为空或 `envFile` 缺失，When 渲染 Compose 信息块，Then 对应字段显示 `-`，页面不报错不崩溃。
- Given 前端校验执行，When 运行 `lint/build/build-storybook`，Then 全部通过且无 TS unused 回归。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook -- --quiet`
- `bun run --cwd web test-storybook`（可执行时）

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 侧栏下线 Compose 区块，保留最近一次扫描。
- [x] M2: 服务详情页新增 Compose 信息展示（含空值回退）。
- [x] M3: 清理 props/state 透传链与 Storybook 调用签名。
- [x] M4: 完成前端验证与回归。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若仅在部分路由提供 `lastScan`，切换路由时可能出现短暂空白；本次以“语义正确优先”处理。
- 假设：当前 UI 评审关注点为“不在侧栏展示误导信息”，不要求新增全局 stack 选择器。

## 变更记录（Change log）

- 2026-02-27: 创建规格，冻结范围、接口影响与验收标准。
- 2026-02-27: 完成实现；侧栏移除 Compose/path/profile，服务详情页新增 Compose 信息块（type/composeFiles/envFile）。
- 2026-02-27: 前端验证通过 `lint/build/build-storybook/test-storybook`。
