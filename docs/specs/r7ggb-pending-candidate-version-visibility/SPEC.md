# Dockrev：pending 时候选版本可见性修复（#r7ggb）

## 状态

- Status: 待实现
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- 当前 `versionInference.status=pending` 时，当前版本与候选版本都会被格式化成 `等待中…`。
- 列表页使用 `candidateDisplayTag !== currentDisplayTag` 决定是否渲染候选箭头，因此 pending 时会误判为“无候选展示”。
- 结果是用户看不到预期的 `等待中… -> 候选版本`，只能看到等待态与 raw tag 回显，语义不完整。

## 目标 / 非目标

### Goals

- 保持当前版本展示规则不变：pending 时仍显示 `等待中…`。
- 调整候选版本展示规则：pending 时仍按既有优先级展示候选值（`resolvedTag` 优先，缺失时回退 raw tag）。
- 全站统一行为：`Services`、`Overview`、`ServiceDetail` 同步生效。

### Non-goals

- 不修改后端 API、候选生成、版本推测和 pending 语义。
- 不修改 `VersionTagsPopover` 查询参数和数据来源。
- 不引入新的 UI 文案或额外交互状态。

## 范围（Scope）

### In scope

- `web/src/versionDisplay.ts`
- `web/tests/versionDisplay.test.ts`
- 调用点回归确认（无需额外业务分支逻辑）：
  - `web/src/pages/ServicesPage.tsx`
  - `web/src/pages/OverviewPage.tsx`
  - `web/src/pages/ServiceDetailPage.tsx`

### Out of scope

- `crates/**`（Rust API / DB / worker）
- 部署配置、任务执行逻辑、通知链路

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `formatCurrentTagDisplay` | TS helper | internal | Keep | None | web | Overview/Services/ServiceDetail | pending 仍返回 `等待中…` |
| `formatCandidateTagDisplay` | TS helper | internal | Modify | None | web | Overview/Services/ServiceDetail | 取消 pending 早退，保留 resolved/raw 回退 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `current.tag=latest`、`current.resolvedTag=v0.2.51`、`versionInference.status=pending`，When 渲染当前版本，Then 显示 `等待中…`。
- Given `candidate.tag=latest`、`candidate.resolvedTag=v0.2.51`、`versionInference.status=pending`，When 渲染候选版本，Then 显示 `v0.2.51`。
- Given `candidate.tag=latest`、`candidate.resolvedTag` 缺失、`versionInference.status=pending`，When 渲染候选版本，Then 回退显示 `latest`。
- Given pending 且存在候选，When 渲染 Versions 列，Then 展示 `等待中… -> <candidateDisplayTag>`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun test web/tests/versionDisplay.test.ts`
- `bun run --cwd web lint`
- `bun run --cwd web build`

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: `versionDisplay` helper 行为按新规则调整（仅 candidate 取消 pending 早退）。
- [ ] M2: 单测补齐 pending 场景并保持既有回归通过。
- [ ] M3: Web 侧 test/lint/build 全部通过。
- [ ] M4: 提交、推送、PR 创建并使 checks 状态明确。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若未来将 pending 文案从 `等待中…` 改为其他文本，列表显隐依赖展示字符串比较的逻辑可能再次引发误判。
- 假设：pending 期间保留候选可见性符合产品语义，且不会误导用户执行更新。

## 变更记录（Change log）

- 2026-02-27: 创建规格，冻结范围、验收标准与质量门槛。
