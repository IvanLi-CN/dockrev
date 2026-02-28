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

- None（已被 #c6j2k 覆盖）

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

- None（已被 #c6j2k 覆盖）

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- None（已被 #c6j2k 覆盖）

## 变更记录（Change log）

- 2026-02-27: 创建规格，冻结范围、验收标准与质量门槛。
