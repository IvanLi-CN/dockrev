# Dockrev Web: 概览页 Stack services 计数修复（#0009）

## 状态

- Status: 已完成
- Created: 2026-01-23
- Last: 2026-01-23

## Change log

- 2026-01-23: 创建计划
- 2026-01-23: 完成：概览页 services 计数口径修复 + Storybook 回归覆盖 + 避免 number/array 误用

## 背景 / 问题陈述

Dockrev Web 概览页（`/`）按 stack 成组展示“更新候选”，每个 stack 组头会展示一段 summary，其中包含 “`N services`”。

当前实现里 `N` 使用的是“本 stack 下具备更新候选（candidate）的 services 数量”（而非“该 stack 的总 services 数量”），当某个 stack 没有任何 candidate 时会显示 `0 services`，从而与用户对 “services=总服务数（与 `GET /api/stacks[].services` 一致）” 的认知产生冲突。

## 目标 / 非目标

### Goals

- 概览页每个 stack 组头展示的 “services 数量” 与 `GET /api/stacks` 返回的 `StackListItem.services` 一致（数值与语义一致）。
- 概览页展示逻辑不依赖“同名字段（`services`）在不同 endpoint 中类型不同”的隐式假设：`GET /api/stacks[].services` 为 number，`GET /api/stacks/{id}.services` 为 array。

### Non-goals

- 不改动后端 API 行为与字段命名（例如不在本计划内将 `services` 重命名为 `serviceCount`）。
- 不做概览页 UI 大改版，仅修复“services 计数”语义与展示。

## 用户与场景

- 运维/维护者：通过概览页快速判断每个 stack 的规模（services 数量）与更新态势。

## 需求（Requirements）

### MUST

- 概览页 stack 组头 summary 中的 “`N services`” 使用 `StackListItem.services`（`GET /api/stacks`）渲染。
- 在 `updates=0` 且 stack 仍有 services 的情况下，概览页仍应展示正确的 services 数量（不再出现 “0 services” 的误导）。
- 增加自动化回归校验，覆盖“stack 有 services 但无 candidate”的场景，确保不会再把 candidate 数量误当成 services 总数。
- 冻结并记录相关 HTTP API 的数据形状（尤其是 `services` 字段在 list/detail 两个 endpoint 中的类型差异），避免前端误用导致回归。

## 接口契约（Interfaces & Contracts）

本计划不改变服务端 API 行为；仅对“前端消费的现有契约”做补齐与冻结（docs-only）。因此本节的 `Change=Modify` 指“契约文档增量补齐”，不代表需要后端 rollout。

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/stacks` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 契约补齐（services:number 的语义与用法）；服务端不改行为 |
| `GET /api/stacks/{stack_id}` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | web/ui | 契约补齐（services:Service[]）；服务端不改行为 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)

## 约束与风险（Constraints & Risks）

- `GET /api/stacks[].services` 当前语义为“该 stack 下 services 总数（包含 archived）”；`archivedServices` 表示 archived 子集，前端需避免把 “candidate 数量/可更新数量” 误称为 services 总数。
- 概览页当前存在 N+1 请求（list + per-stack detail）；本计划不改变该策略，仅修复展示口径。

## 验收标准（Acceptance Criteria）

- Given Dockrev 中存在至少一个 stack，且其 services 总数 > 0
  When 打开概览页（`/`）
  Then 该 stack 组头显示的 “`N services`” 与 `GET /api/stacks` 对应 stack 的 `services` 字段一致

- Given 某 stack 的 `updates=0` 且 `services>0`
  When 打开概览页（`/`）
  Then 该 stack 组头不应显示 “0 services”

- Given 打开服务页（`/services`）
  When 服务列表加载完成
  Then 仍能正常展示与检索服务列表（不受本计划影响）

- Given 前端代码审查
  When 检查概览页对 `GET /api/stacks` 的消费
  Then 不存在对 `StackListItem.services` 取 `.length` 或等价的错误用法

## 实现前置条件（Definition of Ready / Preconditions）

- “services 数量”的口径冻结为：与 `GET /api/stacks[].services` 一致（总数，包含 archived）
- 回归校验的落点冻结（使用仓库既有 Storybook test-runner，不引入新测试框架）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Storybook test-runner：新增/更新一条针对 OverviewPage 的自动化校验，覆盖 “services>0 且无 candidate” 场景。

### Quality checks

- `web`: `npm run build`（`tsc -b` + `vite build`）
- `web`: `npm run lint`
- `web`: `npm run test-storybook`

## 文档更新（Docs to Update）

- None

## 实现里程碑（Milestones）

- [x] M1: 修复概览页 stack 组头的 services 计数口径（使用 `StackListItem.services`）
- [x] M2: 增加回归校验：覆盖 “services>0 且无 candidate” 的场景（Storybook + test-runner）
- [x] M3: 静态/类型约束：避免对 `StackListItem.services`（number）进行 `.length` 等数组用法

## 方案概述（Approach, high-level）

- 概览页的 stack 组头 summary 中，“services 总数”使用 `GET /api/stacks` 的聚合字段（`StackListItem.services`）；“更新候选/状态分布”继续使用 detail 数据（`GET /api/stacks/{id}`）计算，但避免混淆命名与文案。
- 回归校验优先走 Storybook 侧：新增一个“无 candidate 但 services>0”的 mock 场景，并在对应 story 中断言组头文案包含正确的 services 数量。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若未来后端调整 `services` 语义（例如改为不含 archived），需同步更新契约文档与 UI 文案。
- 假设（需主人确认）：本计划的 “services 数量” 指 `GET /api/stacks[].services`（总数，包含 archived），不改为 “未归档 services 数量”。

## 参考（References）

- 相关代码位置（用于实现阶段定位）：
  - `web/src/pages/OverviewPage.tsx`（stack 组头 summary 文案）
  - `web/src/api.ts`（`StackListItem`/`StackDetail` 类型）
  - `crates/dockrev-api/src/api/types.rs`（`StackListItem.services: u32`；`StackResponse.services: Vec<Service>`）
