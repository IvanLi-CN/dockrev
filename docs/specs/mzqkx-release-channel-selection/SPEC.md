# Dockrev：发布流程补齐 Channel 显式选择（PR + Label）（#mzqkx）

## 状态

- Status: 已完成
- Created: 2026-03-04
- Last: 2026-03-04

## 背景 / 问题陈述

Dockrev 的发布流程原本只支持可选 `channel:prerelease`：

- PR Label Gate 允许 `channel:*` 缺失（仅限制“最多一个”）
- Release intent 缺省时自动按 stable 处理
- README 文档将 channel 描述为可选

这与既有 style-playbook 的 PR + Label 发布契约不一致（`type:*` 与 `channel:*` 都应“必须且仅一个”），容易导致发布通道决策不显式、审计信息不完整。

## 目标 / 非目标

### Goals

- 对齐 PR + Label 契约：每个面向 `main` 的 PR 必须且仅能有一个 channel 标签。
- Channel 标签集合固定为 `channel:stable | channel:rc`。
- 保持现有发布语义：
  - `stable` 更新 `latest`
  - `rc` 走 prerelease（`<semver>-rc.<shortsha>`）且不更新 `latest`
- 同步更新仓库文档与远端 label 定义，避免实现与使用侧脱节。

### Non-goals

- 不改 `type:*` 语义（`docs/skip/patch/minor/major`）。
- 不引入 changesets/semantic-release 等新版本工具链。
- 不改变 release 触发模型（仍由 `CI (main)` 的 `workflow_run` 触发）。

## 范围（Scope）

### In scope

- `.github/workflows/label-gate.yml`
- `.github/scripts/label-gate.sh`
- `.github/scripts/release-intent.sh`
- `.github/workflows/release.yml`
- `README.md`
- GitHub 仓库 labels（`IvanLi-CN/dockrev`）

### Out of scope

- 业务运行时代码（`crates/**`, `web/**`）
- 非 release 相关工作流行为

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PR release labels | File format / process contract | external | Modify (breaking for old channel label) | None | maintainers | contributors/maintainers | channel 从可选改为必选，并更名为 stable/rc |
| `release_channel` output (`label-gate.sh` / `release-intent.sh`) | CLI output | internal | Modify | None | CI maintainers | `release.yml` | 输出值收敛为 `stable|rc` |
| `Release` workflow prerelease/latest 分支逻辑 | CI workflow | internal | Modify | None | CI maintainers | release pipeline | 由 `rc` 驱动 prerelease 与 latest 抑制 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given PR 带 `type:patch` 且 `channel:stable`，When `PR Label Gate` 运行，Then gate 通过。
- Given PR 带 `type:patch` 且 `channel:rc`，When `PR Label Gate` 运行，Then gate 通过。
- Given PR 缺失 channel 或同时存在多个 channel，When `PR Label Gate` 运行，Then gate 失败并给出明确错误。
- Given PR 带未知 channel（例如 `channel:prerelease`），When `PR Label Gate` 运行，Then gate 失败并提示未知 channel。
- Given `release-intent.sh` 读取到 `type:patch + channel:stable`，Then 输出 `should_release=true` 且 `release_channel=stable`。
- Given `release-intent.sh` 读取到 `type:patch + channel:rc`，Then 输出 `should_release=true` 且 `release_channel=rc`。
- Given `release-intent.sh` 读取到 channel 缺失/冲突/未知，Then 输出 `should_release=false` 且含 `invalid_channel_label_count(...)` 或 `unknown_channel_label(...)`。
- Given Release workflow 处于 `release_channel=rc`，Then 生成 `-rc.<shortsha>` 版本，GitHub Release 为 prerelease，且不推 `latest`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash .github/scripts/release-channel-contract-check.sh`
- `bash -n .github/scripts/label-gate.sh .github/scripts/release-intent.sh .github/scripts/compute-version.sh`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/label-gate.yml"); YAML.load_file(".github/workflows/release.yml")'`
- 针对 `label-gate`/`release-intent` 的最小样例逻辑检查（stable/rc/缺失/冲突/未知）。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: PR Label Gate 强制 `channel:stable|channel:rc` 且“必须且仅一个”。
- [x] M2: `label-gate.sh` 与 `release-intent.sh` 输出 channel 统一为 `stable|rc`。
- [x] M3: `release.yml` 按 `release_channel == rc` 控制 prerelease 与 `latest` 发布。
- [x] M4: README 文档改为 required channel 契约。
- [x] M5: 远端标签迁移：新增 `channel:stable`，将 `channel:prerelease` 重命名为 `channel:rc`。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：历史自动化或贡献者模板若仍写 `channel:prerelease`，会在 gate 阶段失败。
- 假设：发布流程消费 `release_channel` 的唯一入口是当前仓库内 `release.yml`，不存在外部依赖旧值 `prerelease`。

## 变更记录（Change log）

- 2026-03-04: 创建规格并完成实现：发布 channel 改为显式必选（`stable|rc`），CI gate / release-intent / release workflow / README 同步更新，远端 labels 完成迁移。
- 2026-03-04: 增加 `release-channel-contract-check.sh` 并接入 `CI (PR)` / `CI (main)` 的 Lint & Checks，覆盖 stable/rc/缺失/冲突/未知通道矩阵与 release.yml 关键分支不变量。
