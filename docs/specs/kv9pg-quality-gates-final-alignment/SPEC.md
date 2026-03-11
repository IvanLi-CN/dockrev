# Dockrev：quality-gates 最终版对齐（merge queue + 条件 review + required checks）（#kv9pg）

## 状态

- Status: 进行中
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 当前仓库已经声明了 `quality-gates` 基线，但 repo 内实现仍停留在旧版口径。
- `PR Label Gate` 与 `CI (PR)` 已逐步补齐 merge queue，但条件 review 仍依赖 workflow-backed `Review Policy Gate`，这与最终 topic 契约不一致。
- GitHub 官方事件语义表明 `pull_request_review` 会使用 PR merge ref 对应的 workflow 版本，因此不能把 review approval enforcement 继续放在 required Actions check 里，否则 PR 可以自改 review gate 逻辑。
- 最终对齐目标必须改成：repo 内声明把 review policy 标记为 GitHub native enforcement，workflow-backed required checks 只负责 lint / test / build / label 这类可安全执行的门禁。

## 目标 / 非目标

### Goals

- 让 `.github/quality-gates.json`、`PR Label Gate`、`CI (PR)` 与当前 `quality-gates` 最终契约一致。
- 把条件 review enforcement 从 repo workflow 移到 GitHub native required-review rule，并保留“只有非 owner / 非 maintainer 作者需要 review”的语义。
- 保留默认分支 PR-only、禁止直接推送、所有提交必须签名的基线。
- 清除旧的 `Review Policy` workflow / `Review Policy Gate` required check，避免继续分发不安全实现。

### Non-goals

- 不调整其余 required checks / informational checks 的对外命名。
- 不修改 release 主流程、应用逻辑、数据库 schema 或 Web UI 行为。
- 不把 `PR Label Gate` 降级为 informational。

## 范围（Scope）

### In scope

- `.github/quality-gates.json`
- `.github/workflows/label-gate.yml`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `.github/scripts/release-channel-contract-check.sh`
- GitHub default-branch ruleset / branch protection 配置
- `docs/specs/README.md`
- 本规格文档

### Out of scope

- `crates/**`
- `web/**`
- `deploy/**`
- release 工作流的业务语义变更

## 需求（Requirements）

### MUST

- `review_policy` 必须在仓库声明中使用 GitHub native enforcement：`mode=github-native`，`bypass_mode=pull-request-only`。
- repo 内不得再保留 `Review Policy` workflow，也不得再把 `Review Policy Gate` 列为 required / informational check。
- GitHub 侧必须配置 native required review rule，并通过 PR-only bypass 保留 owner / maintainer 免 review 语义；不得退化成“所有人都必须 review”。
- `PR Label Gate` 必须继续使用 trusted-source workflow-backed gate：PR 元数据路径使用 `pull_request_target`，并支持 `merge_group` 对入口 PR 验证 `type:*` 与 `channel:*` 标签契约。
- `CI (PR)` 必须在 `merge_group` 上触发，并保证声明为 required 的 job 在 merge queue 上仍会产生真实 check。
- 默认分支专用 workflow 若复用与 PR workflow 相同的 job 名，必须改名以避免 GitHub check context 冲突。
- `changes` gating 在 `merge_group` 上必须走保守策略，确保 required jobs 不会因为“无法判断变更范围”而被跳过。
- 默认分支必须保持 PR-only、禁止直接推送，并要求签名提交。

### SHOULD

- merge queue 路径与 PR 路径尽量共享同一套标签 / CI 校验逻辑，避免两套规则长期漂移。
- repo 内声明、topic validator、静态 contract-check 与 GitHub live 配置必须一致识别最终契约，不允许“仓库自认为完成、topic 仍判 drift”。
- 规格与索引应记录这次对齐是 `quality-gates` 最终版落地，而不是临时兼容旧 workflow gate。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `PR Label Gate`
  - `pull_request_target`：对当前 PR 校验 exactly-one `type:*` + exactly-one `channel:*`。
  - `merge_group`：解析入口 PR，并复用相同的标签验证语义；入口 PR 不满足即失败。
- `CI (PR)`
  - `pull_request`：维持现有按路径裁剪的重活门禁。
  - `merge_group`：保守视为 full sweep，确保 required jobs 真跑出来。
- `GitHub native review policy`
  - 非 owner / 非 maintainer 作者：必须达到声明要求的 review 数量。
  - owner / maintainer 作者：可通过 PR-only bypass 继续走 PR 流程，但不要求额外 review。
  - 该 gate 由 GitHub native review rule 承担，不再由 repo workflow 重新实现。

### Edge cases / errors

- 若 merge queue 无法从 `head_ref` 证明入口 PR，`PR Label Gate` 必须 fail closed，不允许猜测放行。
- 若 `changes` 无法在 merge queue 上做精确 diff，required jobs 必须继续运行，而不是跳过。
- 若 GitHub live required checks / branch protection 与声明不一致，视为 drift，不因 GitHub merge 按钮可点击而视为完成。

## 验收标准（Acceptance Criteria）

- Given 一个普通 PR，When 缺少或冲突 `type:*` / `channel:*` 标签，Then `Release intent label gate` 失败。
- Given 一个 merge queue 组，When 入口 PR 标签不合法，Then `Release intent label gate` 失败并在 summary 标出具体 PR。
- Given 一个 owner / maintainer 作者 PR，When 没有额外 review，Then GitHub native review rule 允许其通过 PR-only bypass 合并。
- Given 一个非 owner / 非 maintainer 作者 PR，When 未达到要求 review，Then GitHub native review rule 阻断合并。
- Given 仓库内 declaration、workflow、contract-check 与 topic validator，When 校验最终契约，Then 不再出现 workflow-backed review gate、自改 gate、validator drift 或 main/PR check context 冲突。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `python3 /Users/ivan/.style-playbook-skills/skills/style-topic-quality-gates/scripts/check_quality_gates.py --repo-root . --declaration .github/quality-gates.json --allow-unchecked-branch-protection`
- `go run github.com/rhysd/actionlint/cmd/actionlint@latest .github/workflows/*.yml`
- `bash ./.github/scripts/release-channel-contract-check.sh`
- `git diff --check`
- PR CI 全绿，且当前 `head_sha` 的 review-loop 收敛后才允许合并

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `PR Label Gate` 对齐 trusted-source topic 契约，并保留 merge queue fail-closed 入口 PR 证明。
- [x] M2: `CI (PR)` 补齐 `merge_group`，让 required jobs 在 merge queue 上稳定产出。
- [x] M3: 仓库声明与静态 contract-check 切到 GitHub native review-policy 契约，并移除 legacy `Review Policy` workflow。
- [ ] M4: 完成 PR 收敛、合并，以及 GitHub required checks / branch protection live 对齐与复核。

## 风险 / 假设

- 假设：GitHub ruleset / branch protection 可表达 required review + PR-only bypass，从而保留 owner / maintainer 免 review 语义。
- 风险：merge queue full sweep 会让 `CI (PR)` 在 queue 上比普通 PR 更保守、更重，但这是为了保证 required checks 不缺失。

## 变更记录（Change log）

- 2026-03-11: 新建规格，冻结 Dockrev `quality-gates` 最终版对齐目标与验收口径。
- 2026-03-11: 完成 repo 内 label gate / CI merge queue 对齐，并消除 main / PR check context 冲突。
- 2026-03-11: 根据最新 review 结果回修最终契约：条件 review 不再由 workflow-backed gate 承担，改为 GitHub native required review rule + PR-only bypass；仓库内移除 legacy `Review Policy` workflow，并同步 contract-check 与声明口径。
