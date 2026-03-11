# Dockrev：quality-gates 最终版对齐（merge queue + 条件 review + required checks）（#kv9pg）

## 状态

- Status: 进行中
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 当前仓库已经声明了 `quality-gates` 基线，但 repo 内实现与 GitHub live 配置仍存在漂移。
- `PR Label Gate` 与 `CI (PR)` 已逐步补齐 merge queue，但条件 review 的最终语义还没有稳定落到“owner / maintain 免 review，其他作者必须 review”的可执行实现上。
- 实际验证表明，GitHub native review ruleset 的 PR-only bypass 仍会把 PR 显示为 `Awaiting approval`，不能作为 dockrev 的最终条件 review 实现。
- 因此 dockrev 的最终对齐目标必须改成：条件 review 继续由 workflow-backed `Review Policy Gate` 精确表达；GitHub rules 只负责 PR-only、required checks、signed commits 与 direct-push 防护。

## 目标 / 非目标

### Goals

- 让 `.github/quality-gates.json`、`Review Policy`、`PR Label Gate`、`CI (PR)` 与 dockrev 最终质量门禁语义一致。
- 保留“只有非 owner / 非 maintainer 作者需要 review”的行为语义，并让 merge queue 与普通 PR 共用同一套 fail-closed 逻辑。
- 保留默认分支 PR-only、禁止直接推送、所有提交必须签名的基线。
- 让 GitHub live required checks / ruleset 与 repo 声明一致，不再残留旧的 native review ruleset 漂移。

### Non-goals

- 不调整其余 required checks / informational checks 的对外命名。
- 不修改 release 主流程、应用逻辑、数据库 schema 或 Web UI 行为。
- 不把 `Review Policy Gate` 或 `PR Label Gate` 降级为 informational。

## 范围（Scope）

### In scope

- `.github/quality-gates.json`
- `.github/workflows/review-policy.yml`
- `.github/workflows/label-gate.yml`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `.github/scripts/check-live-quality-gates.py`
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

- `review_policy` 必须在仓库声明中使用 workflow-backed enforcement：`mode=required-check`，`check_name=Review Policy Gate`。
- repo 内必须保留 `Review Policy` workflow，并把 `Review Policy Gate` 列为 required check。
- `Review Policy Gate` 必须表达以下语义：owner / `admin` / `maintain` 作者免 review；其余作者必须获得 1 个来自 `write|maintain|admin` reviewer 的批准。
- `Review Policy Gate` 必须以 trusted-source 方式运行：PR 侧使用 `pull_request_target`，并支持 `merge_group`；对 merge-group 关联到的全部 open PR 做 fail-closed 校验。
- `PR Label Gate` 必须以 trusted-source 方式运行：PR 侧使用 `pull_request_target`，并支持 `merge_group`；继续校验 exactly-one `type:*` + exactly-one `channel:*`。
- `CI (PR)` 必须在 `merge_group` 上触发，并保证声明为 required 的 job 在 merge queue 上仍会产生真实 check。
- 默认分支专用 workflow 若复用与 PR workflow 相同的 job 名，必须改名以避免 GitHub check context 冲突。
- GitHub live 规则必须保留：默认分支 PR-only、禁止直接推送、required checks、signed commits；不得额外保留 native required review 造成 owner PR 继续强制 review。

### SHOULD

- merge queue 路径与 PR 路径尽量共享同一套标签 / review / CI 校验逻辑，避免两套规则长期漂移。
- repo 内声明、topic validator、静态 contract-check 与 GitHub live 配置必须一致识别最终契约，不允许“仓库自认为完成、topic 仍判 drift”。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `Review Policy Gate`
  - `pull_request_target` / `pull_request_review`：对当前 PR 重新计算条件 review 结果。
  - `merge_group`：解析 merge queue 关联的全部 open PR，并逐个复用同一套 review 语义；任一 PR 不满足即失败。
- `PR Label Gate`
  - `pull_request_target`：对当前 PR 校验 exactly-one `type:*` + exactly-one `channel:*`。
  - `merge_group`：解析 merge-group 关联的全部 open PR，并复用相同的标签验证语义；任一 PR 不满足即失败。
- `CI (PR)`
  - `pull_request`：维持现有按路径裁剪的重活门禁。
  - `merge_group`：保守视为 full sweep，确保 required jobs 真跑出来。

### Edge cases / errors

- 若 merge queue 无法从 `head_ref` + `commits/{sha}/pulls` 证明完整且一致的关联 PR 集合，`Review Policy Gate` 与 `PR Label Gate` 都必须 fail closed。
- 若 `changes` 无法在 merge queue 上做精确 diff，required jobs 必须继续运行，而不是跳过。
- 若 GitHub live required checks / branch protection 与声明不一致，视为 drift，不因 GitHub merge 按钮可点击而视为完成。

## 验收标准（Acceptance Criteria）

- Given 一个普通 PR，When 缺少或冲突 `type:*` / `channel:*` 标签，Then `Release intent label gate` 失败。
- Given 一个 owner / maintainer 作者 PR，When 没有额外 review，Then `Review Policy Gate` 通过。
- Given 一个非 owner / 非 maintainer 作者 PR，When 未达到要求 review，Then `Review Policy Gate` 失败。
- Given 一个 merge queue 组，When 其中任一关联 PR 未满足标签或 review 语义，Then 对应 required gate 失败并标出具体 PR。
- Given 仓库内 declaration、workflow、contract-check 与 live rules，When 校验最终契约，Then 不再出现 review gate / required checks / signed-commit 规则漂移。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `python3 /Users/ivan/.style-playbook-skills/skills/style-topic-quality-gates/scripts/check_quality_gates.py --repo-root . --declaration .github/quality-gates.json --allow-unchecked-branch-protection`
- `go run github.com/rhysd/actionlint/cmd/actionlint@latest .github/workflows/*.yml`
- `bash ./.github/scripts/release-channel-contract-check.sh`
- `python3 ./.github/scripts/check-live-quality-gates.py --declaration .github/quality-gates.json --repo "${GITHUB_REPOSITORY}" --mode require`（GitHub Actions 上必须通过）
- `git diff --check`
- PR CI 全绿，且当前 `head_sha` 的 review-loop 收敛后才允许合并

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `PR Label Gate` 对齐 merge queue fail-closed 入口 PR / 关联 PR 证明。
- [x] M2: `CI (PR)` 补齐 `merge_group`，让 required jobs 在 merge queue 上稳定产出。
- [ ] M3: 条件 review 改回 workflow-backed `Review Policy Gate`，并清除与 GitHub native review ruleset 的语义冲突。
- [ ] M4: 完成 PR 收敛、合并，以及 GitHub required checks / branch protection live 对齐与复核。

## 风险 / 假设

- 假设：metadata-only review gate 继续由 repo workflow 表达，比 GitHub native PR-only bypass 更贴近 dockrev 的真实合并语义。
- 风险：merge queue full sweep 会让 `CI (PR)` 在 queue 上比普通 PR 更保守、更重，但这是为了保证 required checks 不缺失。

## 变更记录（Change log）

- 2026-03-11: 新建规格，冻结 Dockrev `quality-gates` 最终版对齐目标与验收口径。
- 2026-03-11: 完成 repo 内 label gate / CI merge queue 对齐，并消除 main / PR check context 冲突。
- 2026-03-11: 根据 live GitHub 行为复盘回修策略：dockrev 的条件 review 改回 workflow-backed `Review Policy Gate`，GitHub live 只保留 PR-only、required checks、signed commits 与 direct-push 防护。
