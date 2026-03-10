# Dockrev：Popover 局部刷新边界收敛 + Snapshot 并发调整（#6epk6）

## 状态

- Status: 已完成
- Created: 2026-03-10
- Last: 2026-03-10
- Notes: fast-track

## 背景 / 问题陈述

- 当前版本气泡与候选版本气泡都复用了 `POST /api/services/{service_id}/version-inference/refresh`，点击任一侧的“强制刷新”都会按 service 级语义同时刷新当前与候选 digest。
- 该联动会触发页面级 `versionInference.status=pending`，导致服务列表 / 概览 / 服务详情中的当前版本主展示进入 `加载中…`，即使本次操作只与候选版本气泡相关。
- 从交互边界看，popover 内的动作应只刷新当前气泡展示的相关数据；当前实现存在明显的 scope 泄漏。
- Snapshot 采集并发当前为“任务并发 4 + 单任务 manifest 并发 4 + registry host 总并发 5”，与本次目标不一致，且需要重新收敛为更保守的任务级并发与更高的 host 级吞吐。

## 目标 / 非目标

### Goals

- 将 `POST /api/services/{service_id}/version-inference/refresh` 收敛为 digest 级局部刷新接口，请求体显式携带 `digest`。
- 当前版本气泡只允许刷新当前 digest；候选版本气泡只允许刷新候选 digest；两者互不越界。
- popover 局部刷新期间只影响本组件的 trigger / notice / snapshot 状态，不再触发页面级 `versionInference.status=pending` 联动。
- 当局部刷新完成且 snapshot ready 后，popover 允许用最新 snapshot tags 在本地更新自身 trigger 展示。
- 固定并发策略调整为：任务并发 `2`、单任务 manifest 并发 `4`、registry host 总并发 `7`。

### Non-goals

- 不改变自动后台版本推测的既有触发条件与 `GET /api/stacks/{id}` 的 `pending/ready` 语义。
- 不新增新的并发 env 开关，也不恢复 legacy 并发 env override。
- 不重构 version popover 的 hover/pin 基座与 snapshot 数据模型。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/services.rs` 与相关 API type/tests：digest 级 refresh 契约与 in-flight snapshot 行为。
- `crates/dockrev-api/src/snapshot_worker.rs` / `service_check.rs` / `config.rs` / `registry.rs`：并发常量收敛到 `2 / 4 / 7`。
- `web/src/api.ts`、`web/src/components/CurrentVersionPopover.tsx`、`web/src/components/VersionTagsPopover.tsx`：局部刷新改造与本地 trigger 收敛。
- `web/src/stories/**` 与 `web/scripts/test-storybook.mjs`：新增 / 调整局部刷新不越界回归。
- `docs/specs/README.md` 与本文档：索引、状态与验收口径。

### Out of scope

- 运行态 current/candidate 数据落库结构改造。
- 自动 background inference 的全局 loading UX 重新设计。
- 其它非 version popover 的 refresh 入口或 Queue / GHCR 并发策略调整。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/services/{service_id}/version-inference/refresh` | HTTP API | external | Modify | `./contracts/http-apis.md` | backend | web/operator | 改为 digest 级局部刷新 |
| `GET /api/services/{service_id}/digest-tags-snapshot` | HTTP API | external | Modify | `./contracts/http-apis.md` | backend | web | 当目标 digest 存在 in-flight task 时返回 `pending` |
| `CurrentVersionPopover` | React component | internal | Modify | None | web | Overview/Services/ServiceDetail | 当前气泡局部刷新只作用于当前 digest |
| `VersionTagsPopover` | React component | internal | Modify | None | web | Overview/Services/ServiceDetail | 候选气泡局部刷新只作用于候选 digest |

### 契约文档（按 Kind 拆分）

- [contracts/http-apis.md](./contracts/http-apis.md)

## 验收标准（Acceptance Criteria）

- Given `POST /api/services/{service_id}/version-inference/refresh` 未带 `digest`，When 请求到达后端，Then 返回 `400`。
- Given 请求中的 digest 不属于该 service 的 current/candidate digest，When 调用 refresh API，Then 返回 `404` 且不 enqueue snapshot task。
- Given 从候选版本气泡点击“强制刷新”，When 请求返回并等待本地 snapshot 刷新，Then 候选 trigger 允许进入局部 loading/ready，但当前版本主展示与整页 `versionInference` 状态不变。
- Given 从当前版本气泡点击“强制刷新”，When 请求返回并等待本地 snapshot 刷新，Then 当前 trigger 允许进入局部 loading/ready，但候选 trigger、候选 popover 与页面其它服务行不被影响。
- Given refresh API 成功 enqueue 或命中 in-flight 去重，When 前端接收响应，Then 仍返回 `status=pending` 与 `reason=force|running`，并带回本次目标 `digest`。
- Given snapshot worker 有多个 digest 待处理，When 运行采集，Then 同时运行的 snapshot task 不超过 `2`，单个 task 的 manifest fan-out 仍为 `4`，同 registry host 的 HTTP 并发不超过 `7`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Rust API / worker tests：覆盖 digest refresh 契约、unknown digest 拦截、单 digest enqueue、snapshot in-flight pending 与 worker concurrency cap。
- Web tests：覆盖 API client 变更、popover 局部刷新不越界、story 交互回归。
- 回归检查：现有自动 background inference 的 `pending -> ready`、snapshot miss/failure/retry/anchor 相关测试继续通过。

## 文档更新（Docs to Update）

- `docs/specs/README.md`：新增索引并在完成后同步状态。
- `docs-site/docs/api-reference.md`
- `docs-site/docs/zh/api-reference.md`
- `docs-site/docs/en/api-reference.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 digest 级 refresh 请求/响应契约，并让 refresh API 只处理单一 digest。
- [x] M2: `digest-tags-snapshot` 在目标 digest 有 in-flight task 时返回 `pending`，支撑 popover 本地刷新闭环。
- [x] M3: 前端 current/candidate popover 改为局部刷新，不再派发全局 refresh 事件。
- [x] M4: 并发常量调整为 `2 / 4 / 7`，并同步 worker / registry 相关测试。
- [x] M5: docs-site API reference、Storybook 回归与交互验证同步完成。

## 方案概述（Approach, high-level）

- 保持现有 refresh 路由路径不变，但收窄其语义为“指定 digest 的 snapshot 局部刷新”；前端必须显式声明目标 digest，后端只校验并 enqueue 该 digest。
- `GET /digest-tags-snapshot` 在发现目标 digest 已有 in-flight task 时优先返回 `pending`，即使库里已有旧 snapshot；popover 通过局部 polling 自行等待 ready，而不是依赖页面级 `versionInference` 轮询。
- popover 内部维护本地 display override：snapshot ready 后优先用返回 tags 的最佳 semver 刷新自身 trigger；raw-tag 辅助行继续保持 raw 文案。
- 并发值仍保持固定常量风格：任务层收敛、manifest fan-out 不变、registry host limiter 提升到 7。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`GET /digest-tags-snapshot` 对 in-flight 返回 `pending` 后，已有旧 snapshot 的 UI 会更多看到局部 loading；前端必须只在本组件范围内消费该 loading，避免回退到整页联动。
- 风险：refresh API 语义变更后，外部若存在旧调用方发送 `{}` 将收到 `400`；需同步 API reference 并在 PR 描述中注明 breaking semantic change。
- 假设：popover 的局部刷新只需要影响本组件 trigger，不要求同步回写页面父级 service 数据。

## 变更记录（Change log）

- 2026-03-10: 创建规格，冻结“popover 局部刷新边界收敛 + snapshot 并发调整”的范围、接口契约与验收口径。
- 2026-03-10: 完成实现与回归验证（backend tests + web build/lint + storybook build/test）。
