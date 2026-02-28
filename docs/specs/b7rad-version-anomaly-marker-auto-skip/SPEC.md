# Dockrev：版本异常标记 + 自动路径跳过（#b7rad）

## 状态

- Status: 已完成
- Created: 2026-02-28
- Last: 2026-02-28

## 背景 / 问题陈述

- 线上已出现 `latest` 回退导致候选版本低于当前版本（例如 `v0.3.1 -> v0.2.53`）但仍显示“可更新”。
- 当前 UI 对该场景缺少直观风险标记；批量更新弹窗也没有专门提示。
- 自动触发更新（webhook/schedule）没有降级保护，存在无人值守降级风险。

## 目标 / 非目标

### Goals

- UI 保持现有状态分类（可更新/需确认/架构不匹配/被阻止），但补充“版本异常”图标/备注。
- all/stack 批量更新确认弹窗增加轻提示，不阻断、不二次确认。
- 自动触发更新（`reason != ui`）遇到“严格 semver 降级异常”时自动跳过该服务。
- 手动更新能力保持不变。

### Non-goals

- 不新增筛选类别与状态计数。
- 不修改对外 API 字段结构。
- 不引入强制二次确认。

## 范围（Scope）

### In scope

- Frontend:
  - `web/src/updateStatus.ts`
  - `web/src/pages/OverviewPage.tsx`
  - `web/src/pages/ServicesPage.tsx`
  - `web/src/pages/ServiceDetailPage.tsx`
  - `web/tests/updateStatus.test.ts`
- Backend:
  - `crates/dockrev-api/src/updater.rs`
  - `crates/dockrev-api/src/api/mod.rs`
  - `crates/dockrev-api/src/api/tests.rs`

### Out of scope

- 新增 API endpoint 或 schema 字段。
- 变更手动 service 更新权限策略。

## 需求（Requirements）

### MUST

- 版本异常判定口径固定为：当前与候选都能解析为严格 semver，且候选 `<` 当前。
- `reason != ui` 的更新任务中，命中版本异常的服务必须从实际更新集合中剔除。
- Web 列表与详情在“状态/备注”或同级信息位展示 `⚠ 版本异常`。

### SHOULD

- 更新任务 summary 输出被跳过的版本异常服务，便于排查。
- 批量更新弹窗提示异常服务数量与“仍可继续更新”语义。

### COULD

- 后续在设置页增加“自动更新遇到降级时策略”开关（本期不做）。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 手动更新（UI）：
  - 版本异常仅提示，不阻断“执行更新”。
- 批量更新（all/stack）：
  - 确认弹窗显示轻提示（异常数量），用户可直接继续。
- 自动更新（webhook/schedule）：
  - 命中异常服务不执行 pull/up。

### Edge cases / errors

- 任一侧无法得到严格 semver（如 `latest` 且无 resolvedTag）：不标记版本异常。
- 仅 pre-release/build 变化按 semver 规则比较（例如 `1.0.0-rc.1 < 1.0.0`）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/updates` | HTTP API | external | Keep | None | dockrev-api | Web UI / webhook caller | 字段不变，仅执行行为变化 |
| `updater::run_update_job` | Rust internal | internal | Modify | None | dockrev-api | api/mod.rs | 新增自动路径跳过逻辑 |
| `isSemverDowngradeAnomaly` | TS helper | internal | New | None | web | Overview/Services/ServiceDetail | 统一前端异常判定 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 当前 `resolvedTag=v0.3.1`、候选 `resolvedTag=v0.2.53`，When 渲染列表/详情，Then 显示 `⚠ 版本异常`，且手动更新按钮可用。
- Given stack/all 批量更新弹窗包含异常候选，When 打开弹窗，Then 仅显示轻提示，不增加二次确认。
- Given webhook 触发更新且服务命中异常，When 任务执行，Then 该服务不进入实际更新集合。
- Given 候选或当前无法解析严格 semver，When 判定异常，Then 结果为“不异常”。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`
- `bun test web/tests/versionDisplay.test.ts web/tests/updateStatus.test.ts`
- `bun run --cwd web build`

### Quality checks

- Rust/TS 编译通过，无新增 lint/type 错误。

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增条目并维护状态。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 前端新增严格 semver 降级异常判定 helper 并接入状态备注。
- [x] M2: Overview/Services/ServiceDetail 增加异常可视化与批量轻提示。
- [x] M3: 后端自动路径跳过异常候选并输出 summary。
- [x] M4: 补齐回归测试并通过验证矩阵。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若 registry tag 回退频繁，自动路径会出现“更新完成但部分服务未动”的感知差异。
- 开放问题：是否需要把“自动跳过数量”做成首页可观测指标（本期不做）。
- 假设：自动触发语义可由 `reason != ui` 稳定识别。

## 变更记录（Change log）

- 2026-02-28: 新建规格并冻结口径（严格 semver 降级异常 + 自动路径跳过）。
- 2026-02-28: 完成 M1/M2/M3/M4 并通过测试矩阵。
