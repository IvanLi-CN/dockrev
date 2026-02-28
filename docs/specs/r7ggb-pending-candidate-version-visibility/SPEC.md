# Dockrev：pending 时候选版本可见性修复（#r7ggb）

## 状态

- Status: 重新设计（#c6j2k）
- Created: 2026-02-27
- Last: 2026-03-01

## 背景 / 问题陈述

- 本规格最初用于修复：`versionInference.status=pending` 时候选版本在列表中不可见的问题。
- 该问题的最终产品决策与实现已被 **#c6j2k** 覆盖（pending 统一为 `加载中…` + 弱化样式，且不再依赖“等待中…”文案语义）。

## 目标 / 非目标

### Goals

- None（已由 #c6j2k 重新设计并接管交付）。

### Non-goals

- 不修改后端 API、候选生成、版本推测和 pending 语义。
- 不修改 `VersionTagsPopover` 查询参数和数据来源。
- 不引入新的 UI 文案或额外交互状态。

## 范围（Scope）

### In scope

- None

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

- None

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
