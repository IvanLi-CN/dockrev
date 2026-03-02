# Dockrev：执行更新按钮绑定任务运行态（Spin）（#q6x2g）

## 状态

- Status: 已完成
- Created: 2026-03-02
- Last: 2026-03-02

## 背景 / 问题陈述

- 当前 `执行更新 / 更新此 stack / 更新全部` 在点击后仅有全局“处理中”提示，按钮本身无法反映任务生命周期。
- `POST /api/updates` 返回后，UI 没有将对应 update job 的 `queued/running` 态绑定回触发按钮。
- 在多 stack/多 service 的列表中，操作者无法快速判断“哪个目标正在更新”。

## 目标 / 非目标

### Goals

- 点击更新后，触发按钮 1 帧内进入 spinner loading。
- spinner 覆盖“提交中 + 任务运行中（queued/running）”两个阶段。
- 仅对应目标按钮显示 loading，不影响其他行按钮的视觉状态。
- 任务结束（success/failed/rolled_back 等非活跃态）后自动恢复按钮默认态。

### Non-goals

- 不改后端 update job 协议与执行流程。
- 不引入进度百分比、phase 文案、全局进度条。
- 不扩展到 dry-run（预览更新）按钮。

## 范围（Scope）

### In scope

- `web/src/ui.tsx`：`Button` 增加可选 `loading` 能力（spinner + `aria-busy` + 自动 disabled）。
- `web/src/updateActionTracking.ts`：新增 update 按钮任务态跟踪模块（目标 key + 提交态 + getJob 轮询运行态）。
- `web/src/pages/OverviewPage.tsx`：all/stack/service 三类 apply 按钮接入 loading。
- `web/src/pages/ServicesPage.tsx`：stack/service apply 按钮接入 loading。
- `web/src/pages/ServiceDetailPage.tsx`：service apply 按钮接入 loading（不影响 preview）。
- `web/scripts/test-storybook.mjs` 与 `web/src/stories/mocks/dockrevMockApi.ts`：补充回归断言与 mock 终态收敛。

### Out of scope

- queue 页展示结构与 job model 字段设计。
- update 任务进度条展示策略。

## 需求（Requirements）

### MUST

- 目标 key 规范：`all` / `stack:<stackId>` / `service:<serviceId>`。
- 活跃态判定：`queued|running`，其余状态视为终态。
- 提交失败（401/409/网络异常）不能留下“卡死 spinner”。
- 按钮可访问名称保持不变（仍可通过 `name=执行更新` 等查询）。

### SHOULD

- 轮询错误具备容错重试，避免短暂抖动导致误退 loading。
- 同一目标重复触发时，仅保留最新 job 跟踪关系。

### COULD

- 后续可复用该跟踪器到其他“触发型任务按钮”。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户在 Overview/Services/ServiceDetail 点击 apply 按钮并确认。
- UI 立即将对应目标 key 标记为 submitting，按钮显示 spinner。
- 收到 `jobId` 后切换到 running 跟踪，轮询 `getJob(jobId)`。
- 当 job 状态不再是 `queued/running`，清理目标 key，按钮恢复默认。

### Edge cases / errors

- 提交请求异常：清理 submitting 态，按钮恢复默认，并沿用原有错误提示。
- 轮询偶发失败：重试；连续失败超过阈值后清理，避免无限 loading。
- 目标 key 不可解析：不进入 loading 跟踪（防御性处理）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Button.loading` | React component props | internal | Modify | None | web | Overview/Services/ServiceDetail/Settings | 统一按钮内 spinner 能力 |
| `useUpdateActionTracker` | React hook | internal | New | None | web | Overview/Services/ServiceDetail | 管理 submitting+running 任务态 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 在 Overview 点击 `更新全部`，When update 任务进入 `queued/running`，Then 仅该按钮显示 spinner，终态后恢复。
- Given 在 Services 点击某一行 `执行更新`，When 该行任务运行，Then 该行按钮 spinner 且同页其他 service 按钮不显示 spinner。
- Given 在 ServiceDetail 点击 `执行更新`，When 任务运行，Then `执行更新` 按钮 spinner 且 `预览更新` 行为不变。
- Given 提交请求返回 401/409 或抛错，When 错误分支完成，Then spinner 不残留。

## 实现前置条件（Definition of Ready / Preconditions）

- 目标按钮范围（all/stack/service）已锁定。
- 活跃态口径（queued/running）已锁定。
- 不改后端 API 的约束已锁定。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: None（本次以 Storybook 交互回归为主）。
- Integration tests: `bun run test-storybook` 覆盖更新按钮 loading 行为。
- E2E tests (if applicable): None。

### UI / Storybook (if applicable)

- 更新 `web/scripts/test-storybook.mjs`：新增“目标按钮 spinner + 非目标不 spinner + 终态恢复”断言。

### Quality checks

- `bun run lint`
- `bun run build`
- `bun run build-storybook`
- `bun run test-storybook`

## 文档更新（Docs to Update）

- `docs/specs/README.md`：新增本规格索引行并记录状态。

## 计划资产（Plan assets）

- Directory: `docs/specs/q6x2g-update-action-button-spin/assets/`

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 update 按钮任务态跟踪模块（目标 key + 提交态 + 运行态轮询）。
- [x] M2: `Button` 组件支持 loading spinner 与 `aria-busy`。
- [x] M3: Overview/Services/ServiceDetail 三页 apply 按钮接入目标级 loading。
- [x] M4: Storybook 回归覆盖“仅目标按钮 loading + 终态恢复”。

## 方案概述（Approach, high-level）

- 前端通过目标 key 解耦“哪个按钮应该转圈”与“全局 busy”状态。
- 提交态和运行态由同一 hook 合并，避免三页面各自复制轮询逻辑。
- 复用现有 `.btnInlineLoading/.btnInlineSpinner` 样式，降低 UI 改动面。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：轮询失败时可能提前退出 loading；通过有限重试降低误判。
- 需要决策的问题：None。
- 假设（需主人确认）：`queued/running` 之外状态均可视为“按钮结束 spin”。

## 变更记录（Change log）

- 2026-03-02: 新建规格并锁定“apply 按钮绑定 update 任务态”范围与验收。
- 2026-03-02: 完成实现与回归验证，状态更新为 `已完成`。

## 参考（References）

- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/updateActionTracking.ts`
