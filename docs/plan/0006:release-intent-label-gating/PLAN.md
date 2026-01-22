# CI/CD: 自动发版意图标签与发布限制（防止 docs-only 发版）（#0006）

## 状态

- Status: 部分完成（1/2）
- Created: 2026-01-21
- Last: 2026-01-22

## 背景 / 问题陈述

仓库当前自动发布链路为：`CI (main)` 成功后触发 `Release`（`workflow_run`）。

关键事实（来自仓库静态勘察）：

- `Release` workflow：`.github/workflows/release.yml`
  - 总是在 `CI (main)` 成功后执行版本号计算与发版：创建 tag、推送 GHCR 镜像、创建/更新 GitHub Release + assets。
- 当前版本号计算脚本：`.github/scripts/compute-version.sh`
  - 以 `Cargo.toml` 的版本号作为 base（当前为 `0.1.0`），并在 tag 已存在时自动递增 patch，得到 `APP_EFFECTIVE_VERSION`。
- 仓库当前 tag 同时存在 `vX.Y.Z` 与 `X.Y.Z` 两种形态（`v*` 为历史 legacy 形态），脚本目前会将二者都视为“占用”。

现状问题：

- 仅包含文档/设计类变更的合并也会触发自动发版，产生“无产物变化/无功能变化”的版本噪音（新 tag、新 Release、版本镜像 tag）。
- 版本 bump 目前只能自动 patch（major/minor 固定为 `Cargo.toml` 的 base），无法在不改 `Cargo.toml` 的情况下表达 minor/major 语义。

本计划参考 `IvanLi-CN/catnap` 的实践（PR #9）：用 PR 标签显式声明“是否允许发版 + bump 等级”，并在 CI 中强制执行，以避免 docs-only 误发版。

## 已确认决策（主人已拍板）

- 意图标签集合固定为：`type:docs|type:skip|type:patch|type:minor|type:major`。
- 所有目标为 `main` 的 PR 必须且只能包含 1 个意图标签；缺失/冲突/未知一律在 PR 阶段失败。
- 新 tag 命名继续使用 `<semver>`（例如 `0.1.13`）；历史 `v<semver>` 只作为“占用判断”。
- 当 GitHub API 临时失败/超时，或无法唯一判定关联 PR：采取**保守策略**，跳过自动发版（`should_release=false`）。
- 版本 bump 语义完全由 PR 标签决定（major/minor/patch）；`Cargo.toml` 版本仅作为“无任何语义 tag 时”的 base fallback。

## 目标 / 非目标

### Goals

- 合并到 `main` 的提交必须可判定“是否允许自动发版”，且该判定对维护者可见、对 CI 可强制。
- 文档/设计类变更不得触发自动发版（不得创建新 tag / GitHub Release，不得推送新的版本镜像 tag）。
- 实现类变更通过 PR 标签显式声明 bump 等级（major|minor|patch），CI 负责计算并发布新版本。
- 无法关联到 PR 的 `push main`（direct push / 异常合并）默认跳过自动发版（只保留 CI 检查与构建）。

### Non-goals

- 不重做既有 Release 产物形态（GHCR 镜像名/tag、Release assets、smoke test、web embed 等沿用已冻结契约）。
- 不引入新的版本管理工具链（changesets / semantic-release 等）。
- 不在本计划内讨论“一个 merge commit 关联多个 PR”的复杂仲裁逻辑；如仓库实践确有该场景，另立计划冻结规则。

## 用户与场景

- 维护者：合并 PR 时通过标签明确“是否发版 + bump 等级”，并由 CI 强制执行，减少误发版/漏发版。
- 部署者：只在真正发布的变更上看到新版本，版本序列更干净。
- CI：在 PR 阶段即可发现缺失/冲突标签，避免合并后才发现发布不符合预期。

## 需求（Requirements）

### MUST

- 标签契约（互斥且必须 1 个）
  - PR 必须且只能包含一个“发版意图标签”（mutually exclusive, exactly one required）。
  - 缺失/冲突/未知标签必须在 PR 阶段被 CI 拦截并失败。
- 自动发版门槛（Release gating）
  - `type:docs` 与 `type:skip` 合并到 `main` 后不得触发自动发版（tag / GitHub Release / 版本镜像 tag）。
  - `type:patch|type:minor|type:major` 合并到 `main` 后允许自动发版，并按标签计算新版本号。
  - 无关联 PR 的 `push main` 必须跳过自动发版（可继续跑 lint/tests）。
  - GitHub API 临时失败/超时必须跳过自动发版（避免误发版），并输出可排障日志。
  - 若 `GITHUB_SHA` 关联到多个 PR（无法唯一仲裁），必须跳过自动发版（避免误发版），并输出可排障日志。
- 版本号策略（CI 计算）
  - base version：语义版本最大 tag（识别 `vX.Y.Z` 与 `X.Y.Z`；忽略非语义 tag）；无 tag fallback `Cargo.toml` 的 version。
  - bump：major → (X+1).0.0，minor → X.(Y+1).0，patch → X.Y.(Z+1)。
  - tag 命名与现有发布口径保持一致：目标 tag 为 `<semver>`（例如 `0.1.12`）。
  - 唯一性：若目标 tag 已存在（含 legacy `v` 前缀视为占用），继续递增 patch 直到未占用版本。
- 可观测与可排障
  - CI 日志必须输出：识别到的 PR、意图标签、should_release、bump_level、base version、目标版本与目标 tag。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PR intent labels (release intent) | File format | external | New | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | contributors/maintainers | 互斥且必须 1 个 |
| `.github/scripts/label-gate.sh` | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI (PR) | PR 阶段强制标签 |
| `.github/scripts/release-intent.sh` | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | Release | push main 映射意图 |
| `.github/scripts/compute-version.sh` | CLI | internal | Modify | [./contracts/cli.md](./contracts/cli.md) | maintainer | Release | 基于 bump 计算版本 |
| Git tag naming & occupancy | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | users/deployers | `<semver>`；legacy `v<semver>` 视为占用 |
| Release workflow gating semantics | File format | internal | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | CI | `should_release` gate |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 约束与风险（Constraints & Risks）

- 依赖 GitHub API 将 commit 映射到 PR/labels：需要明确 token 的最小权限（只读）与错误处理策略（超时/找不到 PR 的默认行为必须可预测）。
- 标签策略改变维护流程：需要约定“谁来打标签、何时打、缺失标签怎么处理”，否则会造成 PR 合并阻塞。
- `workflow_run` 事件上下文与权限边界需要验证：读取 PR 信息所需的 `permissions` 不应过度授权。
- tag 历史存在双形态（`vX.Y.Z` 与 `X.Y.Z`）：版本解析、占用判断与输出 tag 必须统一，否则可能导致冲突或重复发版。
- **安全性（必须修复）**：PR Label Gate 不得执行来自 PR checkout 的仓库脚本。
  - 当前实现为 `pull_request` + `actions/checkout` 默认检出 `refs/pull/<n>/merge`（`GITHUB_SHA` 为 PR merge commit），随后执行 `bash ./.github/scripts/label-gate.sh`；PR 可通过修改该脚本使 gate 恒通过，从而绕过“exactly one intent label”约束。
  - 推荐修复方向（二选一，优先 2）：
    1) 仍用 `pull_request`：在执行脚本前检出 base SHA（例如 `${{ github.event.pull_request.base.sha }}`）以确保脚本来自可信基线；
    2) 避免 checkout：直接基于 event payload / GitHub API 在 workflow 中完成 label 校验，或使用固定版本的外部 action（避免运行 PR 提交引入的脚本）。

## 实现入口点（Repo reconnaissance）

计划实现阶段预计触及/新增的关键位置：

- workflows：
  - `.github/workflows/ci-pr.yml`：新增 label gate（PR 阶段必须且只能 1 个意图标签）。
  - `.github/workflows/release.yml`：新增 release gating（`should_release` 为 false 时整条发版链路跳过）。
- scripts：
  - 新增 `.github/scripts/label-gate.sh`
  - 新增 `.github/scripts/release-intent.sh`
  - 修改 `.github/scripts/compute-version.sh`（支持 `BUMP_LEVEL` + base=max tag）
- permissions（最小化）：
  - 读取 commit 关联 PR：需要 `pull-requests: read`。
  - 读取 PR labels：需要 `issues: read` 或 `pull-requests: read`（PR labels 属于 issues labels）。

## 验收标准（Acceptance Criteria）

- Given 一个 PR 合并到 `main` 且带 `type:docs`
  When `CI (main)` 成功触发 `Release`
  Then `Release` 不得创建 tag、不得创建/更新 GitHub Release、不得推送任何版本镜像 tag，并输出 `should_release=false` 的判定依据。

- Given 一个 PR 合并到 `main` 且带 `type:patch`
  When `CI (main)` 成功触发 `Release`
  Then `Release` 必须创建一个新的 `<semver>` tag（唯一且不冲突）、推送对应版本镜像 tag，并创建/更新 GitHub Release（保持既有产物契约）。

- Given 一个 PR 同时存在多个意图标签（例如 `type:docs` + `type:patch`）
  When `CI (PR)` 运行
  Then label gate 必须失败并提示“必须且只能有一个意图标签”。

- Given 一个 PR 试图绕过 gate（例如修改 `.github/scripts/label-gate.sh`，或通过 checkout 侧的可执行文件改变其行为）
  When `PR Label Gate` 运行
  Then gate 必须仍然使用可信代码路径执行校验（不运行 PR checkout 内的脚本），且不能被绕过。

- Given 一个提交 `push` 到 `main` 且无法关联到任何 PR
  When `CI (main)` 成功触发 `Release`
  Then `Release` 必须跳过自动发版（不创建 tag/Release/版本镜像 tag），并输出 “no PR” 判定。

- Given `Release` 在判定发布意图时遇到 GitHub API 超时/失败，或返回多个关联 PR
  When `CI (main)` 成功触发 `Release`
  Then `Release` 必须跳过自动发版（不创建 tag/Release/版本镜像 tag），并输出清晰可排障日志（包含错误原因/返回 PR 数量等）。

## 实现前置条件（Definition of Ready / Preconditions）

- GitHub 仓库已创建并维护意图标签集合（见契约），并明确维护流程（谁/何时/如何打标签）。
- 已确认 tag 命名口径：继续使用 `<semver>` 作为新 tag 名；legacy `vX.Y.Z` 仅作为“占用判断”。
- 已确认 GitHub API 失败/超时的默认策略（推荐：保守跳过自动发版，而不是误发版）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 对脚本进行最小可验证测试（在 CI 内可运行）：
  - `label-gate`：覆盖缺失/冲突/未知/合法四类输入。
  - `release-intent`：覆盖“有关联 PR 且标签合法”与“无 PR”两类场景。
  - `compute-version`：覆盖 base selection、bump math、tag 冲突重试。

### Quality checks

- 不引入新工具链；复用现有 CI 环境（bash + git + curl）。

## 文档更新（Docs to Update）

- `README.md`: 补充“什么时候会自动发版”“需要哪些 PR 标签”“skip 口径与示例”。
- `docs/plan/0003:release-automation-alignment/PLAN.md`: 若与本计划的新治理口径冲突，需要在实现落地后同步更新口径（避免计划互相矛盾）。

## 开放问题（Open Questions）

- None

## 假设（Assumptions）

- 主分支受到保护，`push main` 直接提交是异常路径；即便如此，仍按“无 PR → 跳过自动发版”处理以防误发版。
- 若 GitHub API 返回多个关联 PR：视为无法唯一仲裁，按“保守跳过自动发版”处理。

## 参考（References）

- 参考实现：`IvanLi-CN/catnap` PR #9（release intent label gating）
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `.github/workflows/release.yml`
- `.github/scripts/compute-version.sh`

## Change log

- 2026-01-22：落地 PR 标签 gate + release intent gating（保守跳过策略）+ label-driven 版本 bump（major/minor/patch）。
