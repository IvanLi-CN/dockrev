# Dockrev：Supervisor 自升级按钮运行态修复（dry-run/apply disable + spin）（#ttq9u）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 现有 `/supervisor/` 页面在自升级进行中时，`预览（dry-run）` 与 `开始升级（apply）` 按钮仍可点击，容易造成“运行中重复触发”的误解。
- 页面没有明确标识当前是 dry-run 还是 apply 在执行，刷新后也无法稳定恢复“哪个按钮在工作”的视觉反馈。

## 目标 / 非目标

### Goals

- 自升级 `state=running` 时，`dry-run` 与 `apply` 按钮统一禁用。
- 根据后端返回的当前请求参数，准确给正在执行的按钮显示 spinner。
- 页面刷新后仍能恢复正确按钮运行态（不依赖前端内存点击记录）。
- 保持接口兼容：不破坏既有调用方。

### Non-goals

- 不修改 `POST /supervisor/self-upgrade` 与 `POST /supervisor/self-upgrade/rollback` 语义。
- 不改升级状态机与 Docker/Compose 执行流程。
- 不迁移 supervisor 页面到 React。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/src/app.rs`
  - `GET /supervisor/self-upgrade` 响应新增可选 `request` 字段（含 `mode` / `rollbackOnFailure`）。
  - 内嵌 `/supervisor/` CSS/JS 增加运行态按钮禁用 + mode 定向 spinner。
  - 补充响应序列化与 UI 片段回归测试。
- `docs/specs/README.md` 增加本规格索引。
- `docs-site/docs/api-reference.md`、`docs-site/docs/en/api-reference.md`、`docs-site/docs/zh/api-reference.md` 同步新增字段说明。

### Out of scope

- Dockrev 主 Web 应用（`web/src/**`）改动。
- 新增全局进度条或阶段型文案改版。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /supervisor/self-upgrade` | HTTP API | external | Modify (backward-compatible) | None | supervisor | supervisor UI / 外部调用方 | 新增可选 `request`，旧字段保持不变 |
| `/supervisor/` 按钮运行态 | HTML UI | external | Modify | None | supervisor | 操作员 | `running` 期间禁用 dry/apply，按 mode 显示 spinner |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `state=running` 且 `request.mode=apply`，When 页面刷新或轮询同步，Then `#dry/#apply` 均 disabled 且仅 `#apply` 显示 spinner。
- Given `state=running` 且 `request.mode=dry-run`，When 页面刷新或轮询同步，Then `#dry/#apply` 均 disabled 且仅 `#dry` 显示 spinner。
- Given `state!=running`，When 页面渲染，Then dry/apply 按钮无 spinner 且恢复既有可用性。
- Given 历史/空闲状态没有 `request`，When 调用 `GET /supervisor/self-upgrade`，Then 响应保持兼容（`request` 可缺省）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-supervisor`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `GET /supervisor/self-upgrade` 新增可选 `request` 字段并保持兼容序列化。
- [x] M2: `/supervisor/` 页面 dry/apply 按钮接入 running 禁用与 mode 定向 spinner。
- [x] M3: 增加 `app.rs` 回归测试覆盖响应字段与 UI 关键片段。
- [x] M4: 同步中英文 API 参考文档。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：UI 仅靠轮询更新，若后端状态短时抖动，按钮视觉可能在轮询间隔内延迟 1 个周期。
- 假设：`request.mode` 由后端状态文件持久化且可信，可作为“当前工作按钮”的唯一判据。

## 变更记录（Change log）

- 2026-03-03: 创建规格并完成实现：self-upgrade 响应新增 `request`，supervisor 页面在 running 时禁用 dry/apply 且按 mode 显示 spinner，补齐测试与 API 文档同步。
