# Dockrev：更新按钮跨路由返回后保留运行态

- ID: `pv9vc`
- Status: `已完成`
- Owner: `web`
- Last Updated: `2026-03-07`
- Related: `q6x2g-update-action-button-spin`

## 背景

`q6x2g` 已让 Overview / Services / ServiceDetail 的 update 按钮在 `queued/running` 时显示 spinner，并允许点击跳转到任务详情。

当前跟踪器在各页面内单独实例化；当用户从这些页面跳到 `/queue/:jobId` 后，原页面组件会卸载，导致按钮运行态丢失。浏览器后退回原页时，任务仍在运行，但按钮错误地恢复为默认态。

## 目标

- 在同一 SPA 会话内，Overview / Services / ServiceDetail 的 update 按钮运行态跨路由保持。
- 用户从 spinning 按钮进入任务详情，再浏览器后退返回原页时，按钮仍显示对应 spinner。
- 仍保留“点击运行中按钮跳转任务详情”的现有行为。

## 非目标

- 不处理整页刷新后的运行态恢复。
- 不修改后端 update job API / job model。
- 不扩展到非 update 类按钮。

## 范围

### In scope

- `web/src/updateActionTracking.ts`：将 page-local tracker 提升为 AppShell 级共享 provider。
- `web/src/Shell.tsx`：在不会随路由切换卸载的位置挂载 provider。
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/stories/pages/InteractiveApp.stories.tsx`
- `web/scripts/test-storybook.mjs`
- `web/src/stories/mocks/dockrevMockApi.ts`

### Out of scope

- 页面刷新/直达链接的 spinner 重建。
- 队列页或任务详情页的视觉改版。

## 接口与实现约束

- 保持 `useUpdateActionTracker()` 消费接口稳定，避免三页面按钮接线大改。
- 目标 key 仍使用 `all` / `stack:<stackId>` / `service:<serviceId>`。
- 活跃态判定仍为 `queued|running`。
- provider 卸载时必须清理轮询 timer，避免悬空轮询。

## 验收标准

- Given 在 Overview 触发某个 service 的 update，When 按钮进入 `queued/running` 且点击进入任务详情，再浏览器后退返回，Then 原按钮仍显示 spinner，且仍可点击进入同一 job。
- Given 在 Services 或 ServiceDetail 触发 update，When 发生相同路由跳转与返回，Then 对应按钮运行态同样保留。
- Given job 进入终态，When tracker 收到终态，Then 对应按钮恢复默认态，不残留 spinner。

## 验证

- `bun run lint`
- `bun run build`
- `bun run build-storybook`
- `bun run test-storybook`

## 里程碑

- [x] M1: 共享 tracker provider 落到 AppShell 层。
- [x] M2: 三个页面改为消费共享 tracker，跨路由保持 spinner。
- [x] M3: Storybook 交互回归覆盖“进入 job detail 后浏览器返回，spinner 仍存在”。

## 变更记录

- 2026-03-07: 新建 follow-up spec，聚焦修复 update 按钮跨路由返回时运行态丢失。

- 2026-03-07: 完成共享 tracker 实现，并通过 lint/build/build-storybook/test-storybook 回归。
