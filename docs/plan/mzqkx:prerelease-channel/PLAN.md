# CI/CD: Release prerelease channel (label-driven)（#mzqkx）

## 状态

- Status: 待实现
- Created: 2026-02-05
- Last: 2026-02-05

## 背景 / 问题陈述

Dockrev 当前的自动发布链路（PR intent label 驱动）只会生成稳定版语义版本（`X.Y.Z`）并更新 `latest`。

有时需要一个“预发布（prerelease/RC）”通道，用于在不产出稳定 tag（`X.Y.Z`）且不更新 `latest` 的前提下，发布可验证的：

- GHCR 镜像 tag
- GitHub Release（带 assets）

## 目标 / 非目标

### Goals

- 在保持现有 `type:*` 意图标签契约不变的前提下，增加一个可选的“预发布通道”开关。
- 预发布通道下：
  - Git tag 使用 semver prerelease 后缀（避免占用稳定 tag）。
  - GitHub Release 标记为 prerelease。
  - GHCR 镜像不更新 `latest`（只推送版本化 tag）。

### Non-goals

- 不改变现有 stable 发布口径（`type:patch|minor|major` → `X.Y.Z` + `latest`）。
- 不引入新的版本管理工具链（changesets/semantic-release）。
- 不为 PR（未合并）提供“预发布镜像”（仅扩展 main 的 release automation）。

## 需求（Requirements）

### MUST

- 标签契约（PR 阶段强制）：
  - 仍要求且仅允许 1 个 `type:*` 意图标签（沿用既有规则）。
  - 允许可选的通道标签：`channel:prerelease`（最多 1 个）。
  - 出现未知的 `channel:*` 标签必须失败（防止拼写错误导致误用）。
- Release intent 判定（main/release 阶段）：
  - 读取 PR labels 时，输出 `release_channel=stable|prerelease`（默认 stable）。
  - 若检测到未知/冲突的 `channel:*`，采取保守策略跳过自动发版（`should_release=false`），并输出可排障 reason。
- 版本/tag 策略：
  - Stable：`APP_EFFECTIVE_VERSION=<semver>`，tag 为 `<semver>`（不带 `v`）。
  - Prerelease：在 stable 版本基础上追加 prerelease 后缀：
    - 形式：`<semver>-rc.<shortsha>`
    - 示例：`0.3.6-rc.a1b2c3d`
  - Prerelease 不得更新 `latest` 镜像 tag。
- GitHub Release：
  - Stable：正常 release（`prerelease=false`）。
  - Prerelease：标记为 prerelease（`prerelease=true`）。

## 验收标准（Acceptance Criteria）

- Given 一个 PR 目标为 `main`，带 `type:patch` 且不带 `channel:prerelease`
  When 合并到 `main` 且 `CI (main)` 成功触发 `Release`
  Then `Release` 创建/推送 `<semver>` tag，推送 GHCR 镜像 `<semver>` 与 `latest`，并创建/更新 GitHub Release（非 prerelease）。

- Given 一个 PR 目标为 `main`，带 `type:patch` 且带 `channel:prerelease`
  When 合并到 `main` 且 `CI (main)` 成功触发 `Release`
  Then `Release` 创建/推送 `<semver>-rc.<shortsha>` tag，仅推送 GHCR 镜像 `<semver>-rc.<shortsha>`（不更新 `latest`），并创建/更新 GitHub Release（prerelease=true）。

- Given 一个 PR 带未知 `channel:*`（例如 `channel:prelease` 拼写错误）
  When `PR Label Gate` 运行
  Then gate 必须失败并提示未知标签。

## 实现入口点（Repo reconnaissance）

- `.github/workflows/label-gate.yml`：允许/校验 `channel:prerelease`
- `.github/scripts/release-intent.sh`：输出 release_channel
- `.github/scripts/label-gate.sh`：与 workflow 保持一致（便于复用/本地排障）
- `.github/workflows/release.yml`：按 channel 决定 tag/version、GitHub Release prerelease 标记、以及是否推送 `latest`

## 最小验证（Test Plan）

- 本地：
  - `bash -n .github/scripts/release-intent.sh .github/scripts/label-gate.sh .github/scripts/compute-version.sh`
  - 对 `release-intent`/`label-gate` 的决策逻辑做最小样例验证（JSON labels 输入覆盖 stable/prerelease/unknown channel）。

