# Dockrev Supervisor：日志 operation 分组 Tabs（#4ae3k）

## 状态

- Status: 已完成
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- `GET /supervisor/self-upgrade` 当前返回单个扁平 `logs` 列表，页面直接整块渲染，历史 operation 混在一起，排障成本高。
- 接口按 1.5s 轮询且日志无组级上限，payload 会持续增长，带来前端渲染与网络开销风险。

## 目标 / 非目标

### Goals

- 为 `/supervisor` 页面增加 operation 级 tabs（最新在最左），可快速切换查看单次 operation 日志。
- 后端按 operation 聚合日志并限制为最近 30 组，稳定轮询 payload 大小。
- tabs 支持单行展示与“展开/收起”多行切换；tab dot 颜色表达运行态（running/succeeded/failed/rolled_back/unknown）。
- 保持向后兼容：保留现有 `logs` 字段，同时新增结构化 `operations` 字段。

### Non-goals

- 不改 Dockrev 主 Web（`web/`）页面。
- 不改 supervisor 鉴权、反代路由或升级执行流程语义。
- 不引入新的 supervisor endpoint。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/src/state_store.rs`
- `crates/dockrev-supervisor/src/app.rs`

### Out of scope

- `web/src/**`
- deploy / nginx / 运行环境配置

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /supervisor/self-upgrade` | HTTP API | external | Modify (backward-compatible) | None | supervisor | supervisor console | 新增 `operations`，保留 `logs` |
| `LogLine` in state file | File format | internal | Modify (backward-compatible) | None | supervisor | supervisor | 新增可选 `opId` |

### 响应新增字段（`operations`）

- `operations: OperationLogs[]`（按时间倒序，`operations[0]` 为最新）
  - `opId: string`
  - `state: "running" | "succeeded" | "failed" | "rolledBack" | "unknown"`
  - `startedAt: string`
  - `updatedAt: string`
  - `logs: LogLine[]`

## 验收标准（Acceptance Criteria）

- Given supervisor 累积多次升级日志，When 打开 `/supervisor`，Then 可见 tabs，最左为最新 operation，切换 tab 后日志区仅展示该 operation 的日志。
- Given tabs 超过一行，When 页面渲染，Then 显示“展开”按钮；点击后可多行展示，再次点击可收起。
- Given operation 状态分别为 `running/succeeded/failed/rolled_back/unknown`，When tabs 渲染，Then dot 颜色分别为蓝/绿/红/红/灰。
- Given 用户停留在历史 tab，When 出现新 operation，Then 页面不自动跳转到最新 tab（仅在当前为最新 tab 时自动跟随）。
- Given 新旧 mixed 日志（部分无 `opId`），When 后端分组，Then 仍可稳定得到 operation 组并限制为最近 30 组。
- Given 兼容性调用方仍读取 `logs`，When 请求 `GET /supervisor/self-upgrade`，Then `logs` 字段仍存在且为最近 30 组 operation 的扁平日志。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-supervisor`
- 浏览器手工验收 `https://dockrev.ivanli.cc/supervisor`（真实交互确认 tabs/展开/状态点）

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `LogLine` 增加可选 `opId`，新增统一日志追加 helper。
- [x] M2: 完成 operation 分组、状态推断、最近 30 组裁剪，并扩展 `self-upgrade` 响应。
- [x] M3: 完成 `/supervisor` tabs UI（单行/展开、彩色 dot、自动跟随策略）。
- [x] M4: 补齐后端单元测试与 UI 字符串回归断言，完成本地验证。
- [ ] M5: 快车道交付：提交、push、PR、checks、review-loop 收敛。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：历史日志无 `opId` 时只能通过边界启发式切段，可能存在极少量 legacy 误分组。
- 假设：`self-upgrade requested` 足够稳定，能作为 legacy operation 的主要边界信号。
- 假设：外部依赖方可接受 `operations` 增量字段，且仍可通过 `logs` 兼容旧逻辑。

## 变更记录（Change log）

- 2026-02-27: 创建规格，冻结范围、接口增量与验收口径。
- 2026-02-27: 完成后端 operation 分组/裁剪、`operations` 响应字段与 `/supervisor` tabs UI；通过 `cargo test -p dockrev-supervisor` 与本地浏览器手工验收。
