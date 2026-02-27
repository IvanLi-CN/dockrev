# Dockrev：GHCR 仓库选择器可用性增强（排序 / 搜索 / 筛选 / 拖动批量切换）（#xu4ew）

## 状态

- Status: 已完成
- Created: 2026-02-27
- Last: 2026-02-27

## 背景 / 问题陈述

- 当前 GHCR 仓库选择弹窗仅支持逐条开关，仓库数量较多时操作成本高。
- 缺少默认的“最近活动优先”视图，难以优先处理最近变更仓库。
- 缺少搜索与筛选（已添加/公开私有）会放大大量仓库场景下的定位成本。
- 缺少拖动批量切换，批量开关时需要重复点击。

## 目标 / 非目标

### Goals

- 弹窗支持搜索、筛选（已添加/可见性）与排序（默认最近活动）。
- 默认按最近活动时间（新→旧）排序，支持切换到仓库名 A→Z。
- 支持鼠标/触屏从开关列按下并拖动，批量将触及项设为同一目标状态（双向刷选）。
- 保持“确认后仅提交变更项”的现有语义。

### Non-goals

- 不改动 GHCR 已跟踪仓库分页列表（弹窗外区域）的交互模型。
- 不引入新的后端查询接口；仅扩展 resolve 响应字段。
- 不改变 webhook 同步/删除仓库等业务流程。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/github.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/SettingsPage.tsx`
- `web/src/App.css`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/pages/SettingsPage.stories.tsx`

### Out of scope

- `GET /api/github-packages/repos` 接口契约与字段结构。
- 其他页面（Overview/Services）开关交互行为。

## 需求（Requirements）

### MUST

- `POST /api/github-packages/resolve` 响应 `repos[]` 新增可选字段：
  - `visibility?: "public" | "private" | "unknown"`
  - `lastActivityAt?: string | null`
- 前端默认排序为最近活动时间降序，空活动时间排末尾。
- 前端支持搜索 `owner/repo`（大小写不敏感）。
- 前端支持筛选：`all/selected/unselected` 与 `all/public/private`。
- 前端支持鼠标/触屏拖动开关列批量切换，起点决定目标状态。
- 保持确认后仅调用“状态发生变化”的仓库更新请求。

### SHOULD

- 弹窗内展示可见性与活动时间辅助信息，帮助理解筛选与排序结果。
- Storybook 提供大量仓库场景用于回归。

## 验收标准（Acceptance Criteria）

- Given owner 解析返回多个仓库，When 打开弹窗，Then 列表默认按最近活动新到旧展示。
- Given 输入搜索词并组合筛选，When 条件变化，Then 列表按 `筛选 -> 搜索 -> 排序` 的固定管道更新。
- Given 从“关”开关按下并拖动触及多个开关，When 抬起，Then 触及项均为“开”。
- Given 从“开”开关按下并拖动触及多个开关，When 抬起，Then 触及项均为“关”。
- Given 仅变更了部分仓库，When 点击确认，Then 仅对变更项调用 selected 更新接口。
- Given repo 直输解析（如 `acme/widgets`），When 调用 resolve，Then 返回 `visibility="unknown"` 且 `lastActivityAt=null`。

## 里程碑（Milestones / checklist）

- [x] M1: 扩展 resolve 响应元数据（visibility/lastActivityAt）并补齐后端测试。
- [x] M2: 实现弹窗搜索/筛选/排序与默认活动排序逻辑。
- [x] M3: 实现开关列拖动批量切换（双向刷选，鼠标/触屏）。
- [x] M4: 更新 Storybook 场景并完成 lint/build/关键路径验证。

## 风险 / 假设

- 风险：GitHub API 返回的 `pushed_at/updated_at` 可能缺失，需稳定处理为“未知活动时间”。
- 假设：仓库可见性在 owner resolve 场景可由 GitHub `private` 字段可靠映射。
- 假设：拖动批量切换以 Pointer Events 为统一实现，可覆盖桌面与移动端输入。

## 变更记录（Change log）

- 2026-02-27: 新建规格，冻结“GHCR 仓库选择弹窗排序/搜索/筛选/拖动批量切换”需求与验收口径。
- 2026-02-27: 后端 `resolve` 响应新增 `visibility/lastActivityAt`，owner 解析返回公开私有与活动时间，repo 直输返回 `unknown/null`。
- 2026-02-27: 前端 GHCR 仓库选择弹窗新增搜索、双筛选、默认活动排序与名称排序切换。
- 2026-02-27: 开关列支持 Pointer 拖动批量切换（双向刷选），并补充 mock/Storybook 场景。
- 2026-02-27: 通过 `cargo test -p dockrev-api github_packages_resolve_`、`web/bun run lint` 与 `web/bun run build`。
