# Dockrev: digest-tags snapshot persistence（no live scan）（#fknrb）

## 状态

- Status: 待实现
- Created: 2026-02-18
- Last: 2026-02-18

## 背景 / 问题陈述

- 当前版本气泡（CurrentVersionPopover）与候选版本气泡（VersionTagsPopover）会在打开时调用
  `/api/services/:id/digest-tags` 做 live 扫描：
  - 后端需要 fan-out 拉 registry manifests，会出现“加载中…”
  - 数据不是“上次扫描的结果”，与 check/runtime-scan 的推测链路存在时间漂移，参考价值下降
- 需求：同 digest 的 tags 必须在扫描时获取并落库，UI 只读快照，避免 UI 行为影响数据口径。

## 目标 / 非目标

### Goals

- Popover 不再触发 live registry 扫描；只展示“最后一次扫描快照”。
- 快照存储满足硬要求：
  - 独立表
  - 单行 JSON（compact JSON string）
  - 只保留最新（每 digest 一行；每 service 仅保留 current/candidate 相关 digest 行）
- 深度限制：稳定顺序选取最近 X 个 tags（默认 X=100）做 digest 比对，并在 UI 展示
  `repoTagsConsidered/repoTagsTotal` 与 manifest 扫描摘要。

### Non-goals

- 不做“按 tag 实际更新时间排序”的最近 X 个（标准 Registry v2 tags/list 不提供该元数据；避免引入 registry 适配工程）。
- 不移除现有 `/api/services/:id/digest-tags`（保留为调试/可观测性用途），但 Web UI 不再依赖它。

## 验收标准（Acceptance Criteria）

- Given 打开 CurrentVersionPopover / VersionTagsPopover，
  When popover 渲染“同 digest 的 tags”，
  Then 不会触发 live registry 扫描（不再调用 `/api/services/:id/digest-tags`），只读取 snapshot endpoint。

- Given 最近一次 check/runtime-scan 已执行，
  When 打开 popover，
  Then 展示 tags + `checkedAt`，并在深度限制/timeout/error 时展示清晰 scan summary。

- Given 没有执行过 check/runtime-scan 或快照被 prune，
  When 打开 popover，
  Then 明确提示“快照缺失：请先执行一次 check”，不出现长时间卡住的“加载中…”。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api` 通过
- `cd web && bun run lint` 通过
- `cd web && bun run build` 通过

## 实现里程碑（Milestones）

- [ ] M1: 扫描时生成并 upsert digest-tags 快照；按 service prune 仅保留 current/candidate digest
- [ ] M2: 新增 snapshot API；Web popovers 改为只读快照并展示 checkedAt/considered summary
- [ ] M3: API tests + web mocks 补齐；最小验证通过（cargo test + web lint/build）

## 风险 / 开放问题（Risks & Open Questions）

- 对 tags 很多且 registry 较慢的镜像，扫描会增加 check/runtime-scan 的耗时；需要通过深度限制（X=100）+ 并发与预算控制来约束。

## 变更记录（Change log）

- 2026-02-18: 创建计划并冻结范围与验收标准（Status=待实现）。

