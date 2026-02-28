# Dockrev：版本列推测 pending 统一为加载中（#c6j2k）

## 状态

- Status: 已完成
- Created: 2026-03-01
- Last: 2026-03-01

## 背景 / 问题陈述

- 当前版本列的“当前版本”在 `versionInference.status=pending` 时会显示 `等待中…`（颜色更像主信息）。
- 同一行的“候选版本”在 digest snapshot `pending/loading` 时会显示 `加载中…` 并弱化颜色（候选效果符合预期）。
- 当两者同时存在时，列表出现“等待中… -> 加载中…”的混合中间态：语义不一致、视觉上不像同一种 loading。

## 目标 / 非目标

### Goals

- 文案统一：当 `versionInference.status=pending` 时，当前版本显示 `加载中…`（不再使用 `等待中…`）。
- 样式统一：当前版本触发器在上述状态下复用候选侧 `versionTagsTriggerLoading` 的弱化色 + pulse。
- 箭头弱化：当“当前推测 pending loading”或“候选首屏预取 loading”出现时，版本列中的箭头同样弱化（避免中间仍像确定态）。
- 影响范围：`Services` / `Overview` / `ServiceDetail` 的版本列一致生效。
- Storybook 可回归：增加稳定可复现的页面场景，覆盖“加载中… -> 加载中…”同屏。

### Non-goals

- 不修改后端 API、`versionInference` pending 的语义与状态机。
- 不修改 snapshot 的生成策略/重试策略。
- 不新增全局 loading 组件或 redesign。

## 范围（Scope）

### In scope

- `web/src/versionDisplay.ts`
- `web/src/components/CurrentVersionPopover.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/App.css`
- `web/tests/versionDisplay.test.ts`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/pages/ServicesPage.stories.tsx`
- `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- `web/scripts/test-storybook.mjs`

### Out of scope

- Rust API / DB / worker 行为修改。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `formatCurrentTagDisplay` | TS helper | internal | Modify | None | web | Overview/Services/ServiceDetail | pending 返回 `加载中…` |
| `CurrentVersionPopover` | React component | internal | Modify | None | web | Overview/Services/ServiceDetail | 支持 inference pending 的 loading 样式复用 |

## 验收标准（Acceptance Criteria）

- Given `service.versionInference.status = 'pending'`，When 渲染版本列当前版本，Then 当前侧显示 `加载中…`（不出现 `等待中…`）。
- Given 候选侧触发 digest snapshot pending/loading，When 同一行渲染，Then 看到 `加载中… -> 加载中…`（两端都弱化）。
- Given `preferSource="rawTag"` 的 raw tag 行，When 推测 pending，Then raw tag 行仍展示真实 raw tag 文案（不进入 loading 样式）。
- Given 运行 `test-storybook`，When 打开新增的 ServicesPage 场景，Then 自动化断言通过且可稳定复现上述同屏状态。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun test web/tests/versionDisplay.test.ts`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web test-storybook`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `versionDisplay` pending 文案从 `等待中…` 改为 `加载中…`。
- [x] M2: `CurrentVersionPopover` 在 inference pending 时复用 `versionTagsTriggerLoading` 样式（rawTag 行不受影响）。
- [x] M3: 版本列箭头在 loading 条件下弱化（与两端一致）。
- [x] M4: Storybook + `test-storybook` 增加“加载中… -> 加载中…”页面回归用例。
- [x] M5: Web 侧 test/lint/build/test-storybook 全部通过。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`等待中…` 文案被移除后，历史截图/说明文档可能需要同步（本次仅修正 spec 与 UI）。
- 假设：`versionInference.status=pending` 在产品语义上与“加载中”一致，用户预期把它视为 loading。
