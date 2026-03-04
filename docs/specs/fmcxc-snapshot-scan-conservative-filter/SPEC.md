# Dockrev：版本推测扫描提速（保守过滤）（#fmcxc）

## 状态

- Status: 已完成
- Created: 2026-03-04
- Last: 2026-03-04

## 背景 / 问题陈述

- 版本推测主链路（snapshot worker）在 tags 很多时会扫描较大的候选集合，导致 digest snapshot 就绪时间偏长。
- 现有策略会先按 semver+词典序排序再截取 `SNAPSHOT_DEPTH=100`，虽然有正确性保障，但在高 tag 密度镜像上仍偏慢。
- 需要在不改变候选判定语义的前提下减少 manifest 扫描数量，优先提升主链路响应。

## 目标 / 非目标

### Goals

- 主链路引入保守过滤：`anchors + parse_version 可解析 tags + non-parseable topk 兜底`。
- 将 snapshot 扫描的常态 `repoTagsConsidered` 收敛到 `<= 40`。
- 保持 anchors 命中能力与 digest 匹配语义不变。

### Non-goals

- 不修改 `/api/services/{id}/digest-tags` live 调试接口行为。
- 不修改 Web 调用路径、DB schema、路由契约。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/service_check.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `docs/specs/README.md`

### Out of scope

- `crates/dockrev-api/src/api/mod.rs` 中 `/digest-tags` live 扫描。
- Web 端数据请求链路。

## 需求（Requirements）

### MUST

- considered tags 上限固定为 `40`。
- anchors 先入队（按传入顺序、去重、仅保留仓库存在 tag）。
- parseable tags 按 `version desc + tag desc` 填充到上限。
- 若仍未达到上限，则按词典序补充 non-parseable tags，最多 `20` 个。

### SHOULD

- 保持 manifest 并发与预算策略不变，仅减少输入集合。
- 继续在 scan summary 中返回 `repoTagsTotal/repoTagsConsidered`，便于观测。

## 验收标准（Acceptance Criteria）

- `service_digest_tags_snapshot_uses_anchor_tag_outside_depth` 场景中，`legacy-1` 仍可命中。
- snapshot 响应里的 `scan.repoTagsConsidered` 在该场景从 `100` 变为 `40`。
- 不影响 `/digest-tags` live 接口既有行为与测试。
- `cargo test -p dockrev-api` 通过。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `service_check.rs` 新增保守过滤常量并重写 considered 选择算法。
- [x] M2: 新增 `service_check.rs` 单测覆盖 anchors/cap/fallback 三类场景。
- [x] M3: 更新 `api/tests.rs` 里 anchor outside depth 场景的 considered 断言。
- [x] M4: 完成后端测试回归并记录结果。

## 风险 / 假设（Risks, Assumptions）

- 风险：纯非解析 tag 仓库在更小 considered 集下可能出现可见性下降。
- 缓解：保留 anchors 与 non-parseable topk 兜底；调试 live 路径保留用于排障。
- 假设：当前业务链路仅依赖 snapshot endpoint，不依赖 live `/digest-tags`。

## 变更记录（Change log）

- 2026-03-04: 新建规格并冻结“保守过滤”口径（40 cap + non-parseable topk=20）。
- 2026-03-04: 完成实现与测试更新，主链路扫描集合收敛到目标范围。
