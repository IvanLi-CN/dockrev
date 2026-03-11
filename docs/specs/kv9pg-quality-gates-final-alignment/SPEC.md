# Dockrev：quality-gates 最终版对齐（merge queue + 条件 review + required checks）（#kv9pg）

## 状态

- Status: 已完成
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 当前仓库已经声明了 `quality-gates` 基线，但 repo 内 workflow 仍停留在旧实现。
- `PR Label Gate` 与 `Review Policy` 仍未对齐 merge queue；`Review Policy` 仍使用旧的 commit status 镜像路径，`pull_request_target` 也与最终 topic 契约不一致。
- 现状会导致“仓库声明看起来是最终版，实际 workflow 与 GitHub required checks 语义仍漂移”，无法满足统一的 merge gate 契约。

## 目标 / 非目标

### Goals

- 让 `PR Label Gate`、`CI (PR)`、`Review Policy` 与仓库内 `quality-gates` 声明完全对齐。
- 对所有 required workflow 补齐 `merge_group` 语义，避免 merge queue 卡死、漏跑或漂移。
- 保留条件 review 语义：只有非 owner / 非 maintainer 作者需要 review，不接受退化成“所有人都必须 review”。
- 清除旧的 review-policy commit status 镜像实现，改为最终 topic 定义的本地 required-check 口径。

### Non-goals

- 不调整 `required_checks` / `informational_checks` 的对外命名。
- 不修改 release 主流程、应用逻辑、数据库 schema 或 Web UI 行为。
- 不把 label gate / review policy 降级为 informational。

## 范围（Scope）

### In scope

- `.github/workflows/review-policy.yml`
- `.github/workflows/label-gate.yml`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `docs/specs/README.md`
- 本规格文档

### Out of scope

- `crates/**`
- `web/**`
- `deploy/**`
- release 工作流的业务语义变更

## 需求（Requirements）

### MUST

- `Review Policy` 必须切换到最终 topic 模板口径：`pull_request` + `pull_request_review` + `merge_group`，并以 dedicated local required check `Review Policy Gate` 直接表达结论。
- `Review Policy` 必须能在 `merge_group` 上下文安全还原关联 PR 集合；无法从 GitHub 披露数据证明成员集合时，必须 fail closed。
- `Review Policy` 不得继续依赖 `statuses: write` / `createCommitStatus` 充当最终契约。
- `PR Label Gate` 必须支持 `merge_group`，并对 merge queue 中的每个 PR 分别验证 `type:*` 与 `channel:*` 标签契约。
- `CI (PR)` 必须在 `merge_group` 上触发，并保证声明为 required 的 job 在 merge queue 上仍会产生真实 check。
- 默认分支专用 workflow 若复用与 PR workflow 相同的 job 名，必须改名以避免 GitHub check context 冲突。
- `changes` gating 在 `merge_group` 上必须走保守策略，确保 required jobs 不会因为“无法判断变更范围”而被跳过。

### SHOULD

- merge queue 路径与 PR 路径尽量共享同一套校验逻辑，避免两套规则长期漂移。
- 规格与索引应记录这次对齐是 `quality-gates` 最终版落地，而不是临时修补。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `PR Label Gate`
  - `pull_request`：对当前 PR 校验 exactly-one `type:*` + exactly-one `channel:*`。
  - `merge_group`：先解析关联 PR 集合，再逐个复用相同的标签验证语义；任一 PR 不满足即整体失败。
- `Review Policy`
  - `pull_request` / `pull_request_review`：按声明的条件 review 规则直接评估当前 PR。
  - `merge_group`：解析关联 PR 集合，逐个评估；任一 PR 不满足 review 契约则整体失败。
- `CI (PR)`
  - `pull_request`：维持现有按路径裁剪的重活门禁。
  - `merge_group`：保守视为 full sweep，确保 required jobs 真跑出来。

### Edge cases / errors

- 若 merge queue 无法从 commit-associated pulls 证明 PR 集合，`PR Label Gate` 与 `Review Policy` 都必须失败，不允许猜测放行。
- 若 merge queue 解析到的 PR 集合与 `head_ref` / `head_commit.message` 中可解析编号不一致，必须视为异常并失败。
- 若 `changes` 无法在 merge queue 上做精确 diff，必须默认 required jobs 继续运行，而不是跳过。

## 验收标准（Acceptance Criteria）

- Given 一个普通 PR，When 缺少或冲突 `type:*` / `channel:*` 标签，Then `Release intent label gate` 失败。
- Given 一个 merge queue 组，When 其中任一 PR 标签不合法，Then `Release intent label gate` 失败并在 summary 标出具体 PR。
- Given 一个 maintainer / owner 作者 PR，When 没有额外 review，Then `Review Policy Gate` 通过。
- Given 一个非 maintainer / 非 owner 作者 PR，When 未达到要求 review，Then `Review Policy Gate` 失败。
- Given 一个 merge queue 组，When 任一 PR 未满足条件 review，Then `Review Policy Gate` 失败并标出具体 PR。
- Given 运行本地 `quality-gates` validator，When 校验仓库声明与 workflow，Then 不再出现 `merge_group` drift、`pull_request_target` drift 或 review-policy 旧实现 drift。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `python3 /Users/ivan/.style-playbook-skills/skills/style-topic-quality-gates/scripts/check_quality_gates.py --repo-root . --declaration .github/quality-gates.json --allow-unchecked-branch-protection`
- 与 workflow 变更直接相关的静态检查 / shell 语法检查（若涉及脚本）
- PR CI 全绿，且当前 `head_sha` 的 review-loop 收敛后才允许合并

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `Review Policy` 替换为最终 topic 模板口径，补齐 merge queue fail-closed 评估。
- [x] M2: `PR Label Gate` 支持 merge queue 多 PR 校验，消除 `pull_request_target` 依赖。
- [x] M3: `CI (PR)` 补齐 `merge_group`，让 required jobs 在 merge queue 上稳定产出。
- [ ] M4: 完成规格同步、验证、PR 收敛与 GitHub required checks / branch protection 对齐。

## 风险 / 假设

- 假设：GitHub `merge_group` 事件可通过 commit-associated pulls 接口证明关联 PR 集合；若 GitHub 不披露该集合，按 fail-closed 阻断。
- 风险：merge queue full sweep 会让 `CI (PR)` 在 queue 上比普通 PR 更保守、更重，但这是为了保证 required checks 不缺失。

## 变更记录（Change log）

- 2026-03-11: 新建规格，冻结 Dockrev `quality-gates` 最终版对齐目标与验收口径。
- 2026-03-11: 完成 repo 内 workflow 对齐，补齐 merge queue required checks，并消除 main / PR check context 冲突。
