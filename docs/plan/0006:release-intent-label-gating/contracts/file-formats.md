# File / Config Contracts（0006）

本文件定义“发版意图标签（release intent labels）”与版本号策略的可持续约定，用于让 CI/CD 行为稳定且可验证。

## PR intent labels (required)

PR 必须且只能包含一个“意图标签”（mutually exclusive, exactly one required）：

- `type:docs`：文档/设计类变更；不允许自动发版
- `type:skip`：显式跳过自动发版（不论变更内容为何）；不允许自动发版
- `type:patch`：允许自动发版；CI 做 patch bump 并发布
- `type:minor`：允许自动发版；CI 做 minor bump 并发布
- `type:major`：允许自动发版；CI 做 major bump 并发布

## Push-to-main release gating (required)

对 `main` 上每次进入 `Release` workflow 的 `GITHUB_SHA`，CI 必须输出如下 gate 信号：

- `should_release=true|false`
- `bump_level=major|minor|patch|none`

### Label mapping (required)

- `type:major` → `should_release=true`, `bump_level=major`
- `type:minor` → `should_release=true`, `bump_level=minor`
- `type:patch` → `should_release=true`, `bump_level=patch`
- `type:docs` / `type:skip` → `should_release=false`, `bump_level=none`

### No PR / direct push policy (required)

当 `GITHUB_SHA` 无法关联到 PR 时，必须跳过自动发版：

- `should_release=false`
- `bump_level=none`

### API failure / ambiguous PR policy (required)

当 GitHub API 临时失败/超时，或返回多个关联 PR 且无法唯一仲裁时，必须跳过自动发版：

- `should_release=false`
- `bump_level=none`

## Versioning (required)

### Tag format

- New tag format: `<semver>`
  - Example: `0.1.12`
- Legacy tags（仅用于占用判断）：允许存在 `v<semver>`
  - Example: `v0.1.7`

### Base version selection (required)

- 从仓库现存 tags 中选取语义版本最大值作为 base（允许 `v` 前缀；忽略非语义 tag）。
- Fallback：若仓库尚无任何语义 tags，则使用 `Cargo.toml` 的 version 作为 base。

### Bump math (required)

对 base `X.Y.Z` 应用 bump：

- `major`: `(X+1).0.0`
- `minor`: `X.(Y+1).0`
- `patch`: `X.Y.(Z+1)`

### Uniqueness & retry (required)

- 目标 tag 为 `<semver>`。
- 若目标 tag 已存在：继续递增 patch 直到找到未占用版本。
- 占用判断必须同时考虑：
  - `refs/tags/<semver>`
  - `refs/tags/v<semver>`（legacy 占用）
