# Dockrev：Supervisor 自我升级页补齐版本 / 开源仓库 / 开发者信息（#ffgt4）

## 状态

- Status: 已完成
- Created: 2026-03-02
- Last: 2026-03-02
- Notes: fast-track

## 背景 / 问题陈述

- 当前自我升级页（`/supervisor/`）缺少版本、开源仓库与开发者信息，运维排障和来源确认需要跳出页面手工查询。
- `GET /supervisor/version` 仅返回 `version`，无法为页面或第三方工具提供仓库与开发者元信息。

## 目标 / 非目标

### Goals

- 在自我升级页标题区下方展示三项固定元信息：
  - `Supervisor 版本`
  - `开源仓库`
  - `开发者`
- 扩展 `GET /supervisor/version` 返回结构，新增 `repository` / `developerName` / `developerUrl`，并保持 `version` 兼容。
- 元信息优先使用构建元数据（`CARGO_PKG_*`），缺失时回退到官方默认值，保证页面始终可用。

### Non-goals

- 不改 `dry-run` / `apply` / `rollback` 的 API 与执行状态机。
- 不新增 Dockrev 主程序版本展示（仅覆盖 Supervisor 页）。
- 不新增环境变量用于自定义仓库/开发者信息。

## 范围（Scope）

### In scope

- `crates/dockrev-supervisor/Cargo.toml`：补齐 package 元数据（`repository`/`homepage`/`authors`）。
- `crates/dockrev-supervisor/src/app.rs`：
  - 元信息解析与回退策略；
  - `/supervisor/version` 响应扩展；
  - 自我升级页 HTML/CSS 增加元信息展示。
- `docs-site/docs/api-reference.md`
- `docs-site/docs/zh/api-reference.md`
- `docs-site/docs/en/api-reference.md`
- `docs/specs/README.md`

### Out of scope

- Dockrev Web React 应用（`web/src/**`）的侧栏元信息逻辑。
- Supervisor 的鉴权与路由策略。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /supervisor/version` | HTTP API | external | Modify (backward-compatible) | None | supervisor | supervisor UI / 外部调用方 | 保留 `version`，新增三字段 |
| `GET /supervisor/` | HTML UI | external | Modify | None | supervisor | 操作员 | 新增元信息展示，不改原操作区 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 打开 `/supervisor/`，When 页面渲染完成，Then 可见“Supervisor 版本 / 开源仓库 / 开发者”三项信息。
- Given `version` 与 `repository` 可用，When 查看元信息区，Then 版本可跳转到 GitHub release tag，仓库与开发者链接可在新标签打开。
- Given `CARGO_PKG_*` 元数据缺失或为空，When 访问页面与 `/supervisor/version`，Then 返回并展示官方回退值，不影响页面其余功能。
- Given 历史调用方只读取 `version`，When 请求 `/supervisor/version`，Then 行为兼容不受影响。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-supervisor`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 补齐 `dockrev-supervisor` 包元数据并实现元信息回退策略。
- [x] M2: 扩展 `/supervisor/version` 响应并保持字段兼容。
- [x] M3: 在 `/supervisor/` 页面增加版本/仓库/开发者展示与链接。
- [x] M4: 补齐相关单测并通过 `cargo test -p dockrev-supervisor`。
- [x] M5: 同步三语 API 文档对 `/supervisor/version` 的说明。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：构建元数据在 fork 场景可能不完整；通过回退值保证最差可用。
- 假设：`APP_EFFECTIVE_VERSION` 仍由现有发布流程注入；本改动不调整其来源。

## 变更记录（Change log）

- 2026-03-02: 创建规格并冻结范围与验收标准。
- 2026-03-02: 完成 `/supervisor/version` 扩展、自我升级页元信息展示、单测与文档更新。
