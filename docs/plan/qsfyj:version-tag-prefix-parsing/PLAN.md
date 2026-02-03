# Dockrev API: 解析带后缀的数字 tag（15-alpine 等）（#qsfyj）

## 状态

- Status: 已完成
- Created: 2026-02-02
- Last: 2026-02-02

## 背景 / 问题陈述

- 线上存在大量 “数字版本 + 后缀” 的 tag（例如 `15-alpine`、`7-alpine`、`15.6-alpine`、`5.1.4_v2.0.11`）。
- 目前后端 `parse_version()` 仅支持纯 semver（或 `major`/`major.minor` 的 coercion），导致上述 tag 无法解析为版本号。
- 当 `current_tag` 无法解析时，候选选择逻辑会回退到“字典序最大值”，从而出现明显不合理的候选（例如 `trixie` / `windowsservercore` 被选为版本候选），并在 UI 侧被标记为“标签关系不确定”。

## 目标 / 非目标

### Goals

- `parse_version()` 支持“前缀数字版本”：
  - 允许在 `v` 前缀后，读取 `major(.minor)?(.patch)?` 的**数字前缀**，忽略其后的后缀（如 `-alpine`、`_v2...`）。
  - 仍保持对严格 semver（含 prerelease/build）的优先解析。
- `select_candidate_tag()` 在上述 tag 场景下不再走字典序兜底，能稳定选出数值更高的候选 tag（并自然跳过非数字 tag）。
- 回归测试覆盖：至少包含 `15-alpine`/`7-alpine` 这类场景，避免再次退化。

### Non-goals

- 不在本计划内处理 `/api/services/:id/candidates` 的请求稳定性（另立计划处理）。
- 不在本计划内改动 Web 的状态判定逻辑与交互（仅依赖更正确的候选输出改善展示）。
- 不尝试“完全理解”复杂 tag（例如 `5.1.4_v2.0.11` 的双版本语义）；仅解析其前缀数字版本用于排序与推测。

## 范围（Scope）

### In scope

- 后端：
  - 扩展 `crates/dockrev-api/src/ignore.rs:parse_version()` 的解析能力（前缀数字版本）。
  - 为 `crates/dockrev-api/src/candidates.rs:select_candidate_tag()` 增加/更新单测，覆盖前缀数字版本与非数字 tag 混合的情况。
- 测试：
  - `cargo test -p dockrev-api`

### Out of scope

- UI/Storybook 变更与视觉调整。

## 验收标准（Acceptance Criteria）

- Given `current_tag=15-alpine` 且 tags 中包含 `15.6-alpine` 与 `trixie`，
  When 计算候选 tag，
  Then 候选应为 `15.6-alpine`（或更高的数字 tag），且不会选择非数字 tag（如 `trixie`）。

- Given `current_tag=7-alpine` 且 tags 中包含 `7.1-alpine` 与 `windowsservercore`，
  When 计算候选 tag，
  Then 候选应为 `7.1-alpine`（或更高的数字 tag），且不会选择 `windowsservercore`。

## 风险 / 开放问题（Risks & Open Questions）

- 解析能力变强后，更多候选会被判定为“可比较版本”，UI 可能从 “需确认” 变为 “跨标签版本/按当前标签序列”——这是预期改善，但可能改变列表分布。

## 变更记录（Change log）

- 2026-02-02: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-02: 实现并合并（PR #47）。
