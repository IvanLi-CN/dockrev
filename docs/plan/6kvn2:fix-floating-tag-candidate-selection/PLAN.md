# Dockrev API: floating tag 候选选择避免版本倒挂（latest → semver）（#6kvn2）

## 状态

- Status: 已完成
- Created: 2026-02-10
- Last: 2026-02-10

## 背景 / 问题陈述

- 线上 `dockrev.ivanli.cc` 出现版本倒挂：某些服务使用 floating tag（如 `latest`），但候选 tag 却被选成更低版本（例如 `v0.2.11 → v0.2.9`）。
- 根因：后端 candidates 选择在 `current_tag` 无法解析为版本号时，会回退到“字典序最大值”，而字典序会把 `v0.2.9` 视为大于 `v0.2.11`。

## 目标 / 非目标

### Goals

- 当 `current_tag` 无法解析为版本号（例如 `latest` / `sha-xxxx`）且 tags 列表中存在可解析版本 tag（semver 或数字前缀版本）时，候选 tag 必须选择“版本最大”的那一个。
- 仅在 tags 列表中不存在任何可解析版本 tag 时，才允许继续使用现有“字典序最大值”兜底。
- 补充单测回归，避免再次出现版本倒挂。

### Non-goals

- 不改动 Web UI 展示与交互。
- 不改动 resolvedTag 推测逻辑。
- 不改动 HTTP API schema（字段保持不变）。

## 范围（Scope）

### In scope

- 后端：
  - 调整 `crates/dockrev-api/src/candidates.rs:select_candidate_tag()` 的兜底策略。
- 测试：
  - 新增回归单测覆盖 `current_tag=latest` 场景。
  - `cargo test -p dockrev-api`

### Out of scope

- UI/Storybook 变更与视觉调整。

## 验收标准（Acceptance Criteria）

- Given `current_tag=latest` 且 tags 中包含 `v0.2.9` 与 `v0.2.11`，
  When 计算候选 tag，
  Then 候选应为 `v0.2.11`（而不是 `v0.2.9`）。

- Given tags 中不存在任何可解析版本 tag（例如仅 `alpha` / `beta`），
  When 计算候选 tag，
  Then 行为保持现状（字典序兜底）。

## 测试与验证（Testing）

- `cargo test -p dockrev-api`

## 风险 / 开放问题（Risks & Open Questions）

- 解析/排序规则变强后，候选选择分布可能变化（从字典序兜底变为版本兜底）。这是预期修复方向。

## 变更记录（Change log）

- 2026-02-10: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-10: 实现并提交（PR #64）。
