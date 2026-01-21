# CLI Contracts（0006）

本文件定义本计划涉及的 GitHub Actions 内部脚本接口契约（输入/输出/退出码），用于让 workflow 行为稳定且可测试。

## `.github/scripts/label-gate.sh`

- Scope: internal
- Change: New

### Purpose

在 PR 阶段强制“发版意图标签”契约：PR 必须且只能包含一个意图标签，否则 CI 失败并给出清晰提示。

### Inputs

- `GITHUB_TOKEN`（required）：用于查询 GitHub API（读取 PR labels）
- `GITHUB_REPOSITORY`（required）
- `PR_NUMBER`（required）
- Allowed labels（required）：见 `./file-formats.md` 的 `type:` 集合

### Outputs

通过 `$GITHUB_OUTPUT` 输出（供 workflow 打印日志/后续 step 使用）：

- `intent_label=type:docs|type:skip|type:patch|type:minor|type:major`
- `should_release=true|false`
- `bump_level=major|minor|patch|none`

### Exit codes

- `0`：校验通过
- `!= 0`：校验失败（缺少意图标签 / 同时存在多个意图标签 / 存在未知意图标签）

## `.github/scripts/release-intent.sh`

- Scope: internal
- Change: New

### Purpose

在 `Release`（push main / workflow_run）阶段，将“commit”映射为发布意图（`should_release + bump_level`），用于 gate 自动发版链路。

### Inputs

- `GITHUB_TOKEN`（required）：用于查询 GitHub API（读取 commit 关联 PR 与 labels）
- `GITHUB_REPOSITORY`（required）
- `GITHUB_SHA`（required）
- Label contract（required）：见 `./file-formats.md`

### Outputs

通过 `$GITHUB_OUTPUT` 输出：

- `should_release=true|false`
- `bump_level=major|minor|patch|none`
- `pr_number=<number|none>`（可选，但推荐输出以便排障）
- `intent_label=<label|none>`（可选，但推荐输出以便排障）

### Behavior (normative)

- 若能关联到 PR：按 `./file-formats.md` 的 label mapping 输出。
- 若无法关联到 PR：按 “No PR / direct push policy” 输出 `should_release=false`、`bump_level=none`。
- 若 GitHub API 失败/超时：必须输出 `should_release=false`、`bump_level=none`（保守跳过发版），并打印可排障日志；默认不应因此失败退出。
- 若关联到多个 PR（无法唯一仲裁）：必须输出 `should_release=false`、`bump_level=none`（保守跳过发版），并打印可排障日志。

### Exit codes

- `0`：判定成功（包含“保守跳过发版”的成功判定：no PR / API 失败 / 多 PR）
- `!= 0`：判定失败（仅用于明显输入不合法或脚本自身错误；不应因 API 瞬时失败而误发版）

## `.github/scripts/compute-version.sh`（planned change）

- Scope: internal
- Change: Modify

### Purpose

根据 `BUMP_LEVEL` 与仓库 tags 计算有效版本号，并导出到 `APP_EFFECTIVE_VERSION`。

### Inputs

- `BUMP_LEVEL=major|minor|patch`（required）
- Git tags（`vX.Y.Z` 或 `X.Y.Z`；用于选择 base version 与避免冲突）
- `Cargo.toml` version（用于无 tag fallback）

### Outputs

- `$GITHUB_ENV`:
  - `APP_EFFECTIVE_VERSION=<semver>`

### Algorithm (normative)

1. Resolve base version：从 tags 中选择语义版本最大值（忽略 `v` 前缀）；若无可用 tag，fallback `Cargo.toml` 的 version。
2. Apply bump math：按 `BUMP_LEVEL` 计算 next。
3. Ensure uniqueness：若目标 tag 已存在（含 legacy `v` 前缀视为占用）则继续递增 patch。
4. Export：写入 `APP_EFFECTIVE_VERSION=...`。
