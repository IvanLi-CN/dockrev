# Dockrev Web: 版本候选 tags 气泡（#8fjbt）

## 状态

- Status: 待实现
- Created: 2026-02-01
- Last: 2026-02-01

## 背景 / 问题陈述

- 现状：Overview/Services 列表的版本展示会出现 `? → v0.8.8-arm64` 等候选版本提示，同时 registry 上存在多种形态的 tag（semver、arch 后缀、渠道/分支、sha 等）。
- 痛点：目前依赖单行显示与 title tooltip，信息密度不足，难以快速确认“候选版本到底对应哪些 tag”，从而不利于后续定位“版本号解析不力”的根因。

## 目标 / 非目标

### Goals

- 在列表的候选版本区域（`? → <candidate>`）支持 **悬浮或点击** 打开气泡，展示“该版本”对应的 tag 列表。
- “该版本”定义：与候选 tag **digest 相同** 的全部 tag（以 Dockrev `/api/services/:id/candidates` 返回的候选集合为准）。
- 支持可解释与可降级：
  - digest 缺失：至少展示当前可见的 candidate tag，并提示无法聚合更多标签。
  - candidates 接口不可用：至少展示 candidate tag，并提示失败原因。
- 交互不干扰行级跳转：操作版本气泡不会误触进入 Service 详情。

### Non-goals

- 不在本计划内修复“版本号没正确解析/识别不力”的问题（待部署后基于气泡观测再决策）。
- 不新增后端“全量 tag 列表/按 digest 反查所有 tag”的重型接口（如需全量能力，另立计划并评估成本/缓存策略）。

## 范围（Scope）

### In scope

- Web UI：
  - OverviewPage 与 ServicesPage：版本顶行替换为可交互触发器（hover/click）。
  - 气泡组件：按 digest 聚合同 digest 的 tags，以 chip 形式展示；点击固定；`ESC`/点空白关闭；避免被列表裁切（portal 到 `document.body`）。
- 数据源：
  - 懒加载调用 `/api/services/:id/candidates`（仅在气泡打开且 digest 已知时请求），并在前端做 digest→tags 聚合。

### Out of scope

- ServiceDetailPage 的版本展示改造（如有需要可在后续小改动补齐）。

## 验收标准（Acceptance Criteria）

- Given 某行存在候选版本（`candidateTag`）且具备 `candidateDigest`，
  When 指针悬浮或点击候选版本区域，
  Then 气泡展示“同 digest 的 tags 列表”，且至少包含 `candidateTag`。

- Given `candidateDigest` 缺失或 candidates 接口失败，
  When 打开气泡，
  Then 仍展示 `candidateTag`，并以可读提示说明无法聚合更多标签。

- Given 用户在版本触发区域操作（hover/click/关闭），
  Then 不会触发行点击进入 Service 详情（除非用户点击行的其他区域）。

## 非功能性验收 / 质量门槛（Quality Gates）

- `web` 的 `lint` 与 `build` 通过。
- UI 不出现明显遮挡/裁切（在窄屏与列表滚动情况下可用）。

## 实现里程碑（Milestones）

- [ ] M1: 气泡组件与数据聚合（digest→tags）
- [ ] M2: Overview/Services 接入 + 样式与可用性（hover/click/关闭）
- [ ] M3: 最小验证（lint/build）与截图/手工检查

## 风险 / 开放问题（Risks & Open Questions）

- `/api/services/:id/candidates` 目前存在数量上限（候选集合不一定覆盖 registry 的“全部 tag”）；本计划以“Dockrev 可见候选集合”定义“所有标签”。

## 变更记录（Change log）

- 2026-02-01: 创建计划并冻结范围与验收标准（Status=待实现）。

