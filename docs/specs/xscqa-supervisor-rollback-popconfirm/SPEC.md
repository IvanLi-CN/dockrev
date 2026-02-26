# Dockrev：Supervisor 回滚按钮二次确认（气泡确认）（#xscqa）

## 状态

- Status: 已完成
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- 当前 supervisor 独立页中，“回滚”按钮点击后会直接调用 `POST /supervisor/self-upgrade/rollback`。
- 手动回滚属于高影响操作（可能触发容器重启），需要防止误触。

## 目标 / 非目标

### Goals

- 为 supervisor 页面“回滚”按钮增加气泡式二次确认层。
- 首次点击“回滚”仅打开确认气泡，不发送 rollback 请求。
- 仅在点击“确认回滚”后，才调用 rollback API。
- 支持基础可关闭路径：取消按钮、点击气泡外区域、按 `Esc`。
- 在轮询刷新后，如果回滚条件不再满足，自动关闭确认气泡并保持按钮禁用。

### Non-goals

- 不修改 `POST /supervisor/self-upgrade/rollback` 的请求/响应契约。
- 不改 `dry-run` / `apply` 的交互。
- 不把 supervisor 页面迁移为 React。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/src/app.rs`（内嵌 HTML/CSS/JS 交互）。
- `crates/dockrev-supervisor/src/app.rs`（render_ui 相关单测）。
- `docs/specs/README.md`（索引新增本规格）。

### Out of scope

- Rust 后端 rollback 执行逻辑。
- Dockrev 主 Web 应用页面。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /supervisor/self-upgrade/rollback` | HTTP API | external | No change | None | supervisor | supervisor UI | 仅前端触发条件改为二次确认 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given supervisor 状态允许回滚，When 用户首次点击“回滚”，Then 仅展示确认气泡，不发送 rollback 请求。
- Given 确认气泡已打开，When 用户点击“取消”/气泡外区域/按 `Esc`，Then 气泡关闭且不发送 rollback 请求。
- Given 确认气泡已打开，When 用户点击“确认回滚”，Then 发送 `POST /supervisor/self-upgrade/rollback` 并按既有逻辑刷新状态。
- Given 轮询刷新后回滚条件失效，When UI 同步状态，Then 回滚按钮禁用且确认气泡自动关闭。
- Given 用户使用 `dry-run`/`apply`，When 操作触发，Then 行为与本改动前一致。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-supervisor`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 增加回滚按钮气泡确认 UI（含确认/取消）。
- [x] M2: 补齐关闭路径（外部点击、Esc）与刷新态同步关闭。
- [x] M3: 完成 render_ui 相关单测并通过 `cargo test -p dockrev-supervisor`。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：内嵌 JS 交互增加后，需避免与现有轮询状态更新冲突。
- 假设：基础无障碍支持（Esc 关闭 + 按钮语义）满足本次需求，不引入完整焦点陷阱。

## 变更记录（Change log）

- 2026-02-27: 创建规格，冻结范围与验收标准。
- 2026-02-27: 完成 supervisor 回滚二次确认交互、关闭路径与 render_ui 测试覆盖。
