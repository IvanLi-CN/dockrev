# Dockrev：标签页恢复后自动补拉更新候选（#745rw）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-14
- Last: 2026-03-14

## 背景 / 问题陈述

- `OverviewPage`、`ServicesPage` 与 `ServiceDetailPage` 当前只会在首屏挂载、局部事件、手动操作或既有轮询链路里刷新数据。
- 当用户把页面挂在后台一段时间后再回到标签页，浏览器后台节流会让旧页面停留在过时快照，尤其 `/?updates=updatable` 这种候选过滤入口更容易暴露 stale 列表。
- 现有实现没有在页面重新可见、窗口重新聚焦或 `pageshow` 恢复时主动补一次 refresh，因此用户需要手动点击“刷新”才能追上最新状态。

## 目标 / 非目标

### Goals

- 在页面从 hidden 回到 visible、窗口重新获得焦点、以及页面发生 `pageshow` 恢复时，自动补一次合并后的 refresh。
- 对连续恢复事件做去重：同一恢复 burst 只触发一次 refresh；若 refresh 已在进行中，最多排队补跑一轮。
- 复用 Overview / Services / ServiceDetail 现有 refresh 路径，不改变 query filter、搜索词、折叠状态或局部 patch 职责。

### Non-goals

- 不修改 Rust 后端 API、SSE 契约、缓存策略或 `/api/**` 响应语义。
- 不在标签页 hidden 期间增加长期后台轮询。
- 不扩散到 Queue / GHCR / Supervisor 页面。

## 范围（Scope）

### In scope

- `web/src/usePageResumeRefresh.ts`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/tests/usePageResumeRefresh.test.ts`

### Out of scope

- `crates/**`
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/GhcrWebhook*.tsx`
- `web/src/components/ServiceResourcePanel.tsx`

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `createPageResumeRefreshController` | TS helper | internal | New | None | web | `usePageResumeRefresh` tests | 提供恢复补拉时序控制，可脱离 React 单测 |
| `usePageResumeRefresh` | React hook | internal | New | None | web | Overview / Services / ServiceDetail | 监听 `visibilitychange` / `focus` / `pageshow` |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `/?updates=updatable` 已打开并在后台停留一段时间，When 标签页重新可见或窗口重新 focus，Then 页面会在一次刷新周期内自动补拉最新列表，而不是依赖手动点击“刷新”。
- Given 连续触发 `visibilitychange` + `focus` + `pageshow`，When 页面处理恢复事件，Then 同一 burst 只触发一次 refresh；若已有 refresh in flight，最多只补跑一轮。
- Given 页面当前带有 `updates=updatable`、搜索词或折叠状态，When 恢复补拉完成后，Then 这些前端状态保持不变，只更新 stack/service 数据。
- Given ServiceDetail 正在显示单服务详情，When 页面恢复可见，Then 详情数据会 catch up，且既有 pending inference polling、digest snapshot 局部 patch 与手动刷新按钮行为不退化。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cd web && bun test`
- `cd web && bun run lint`
- `cd web && bun run build`
- Browser smoke：`chrome-devtools` 打开本地 preview（`VITE_API_BASE_URL=https://dockrev.ivanli.cc`），验证 Overview / Services / ServiceDetail 恢复补拉；当前仍待补齐。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增共享恢复补拉控制层，并对恢复 burst / in-flight queue 做去重约束。
- [x] M2: Overview / Services / ServiceDetail 接入共享 hook，复用现有 refresh 路径。
- [x] M3: `bun test` / `lint` / `build` 通过，覆盖恢复补拉关键时序。
- [ ] M4: `chrome-devtools` 浏览器 smoke 完成，确认列表与详情页恢复补拉的真实交互证据。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：浏览器在前后台切换时可能同时派发多个恢复事件；若不做 burst 合并，会出现重复 refresh 或多余排队。
- 风险：ServiceDetail 直接复用全量 `refresh` 会额外刷新 settings / ignore rules；本次改为复用 `refreshStackOnly`，仅收敛详情数据面。
- 风险：重叠 refresh 若简单按“最新 requestId 赢”直接丢弃旧成功结果，会在“新请求失败、旧请求成功”时把页面留在过时错误态；实现需要只阻止旧成功覆盖新成功，而不是无差别取消旧请求。
- 假设：250ms 的恢复 burst 窗口足以覆盖常见的 `visibilitychange` / `focus` / `pageshow` 连发，不影响用户真正隔一段时间后的再次恢复。
- 开放问题：当前 Codex `chrome-devtools` 会话在本机超时，真实浏览器 smoke 证据需在工具恢复后补齐。

## 变更记录（Change log）

- 2026-03-14: 创建规格，冻结恢复补拉范围、接口与验收标准。
- 2026-03-14: 完成共享 resume-refresh hook 与页面接入，并补充控制层时序测试。
- 2026-03-14: `cd web && bun test`、`cd web && bun run lint`、`cd web && bun run build` 通过；浏览器 smoke 仍待补证。
- 2026-03-14: 按 review 反馈补上 `pageshow.persisted` 过滤，并为 Overview / Services / ServiceDetail 的 refresh 增加 request-id 保护，避免旧请求回写覆盖新结果。
- 2026-03-14: 继续按 review 反馈收敛重叠 refresh 语义：允许旧成功结果在新请求失败时兜底落盘，并让 ServiceDetail 的 resume refresh 走完整 `refresh()`，同步 settings / rules 与详情卡片数据。
