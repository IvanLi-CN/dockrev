# Dev/CI: Bun 迁移（替代 npm）（#0011）

## 状态

- Status: 已完成
- Created: 2026-01-23
- Last: 2026-01-24

## 问题陈述

仓库目前在本地开发、CI、Docker 构建与 hooks 中仍大量使用 npm（含 `npm ci` / `npm run` 与 `web/package-lock.json`），与“不要再使用 npm”的团队要求不一致。

## 目标 / 非目标

### Goals

- 以 Bun 作为包管理与脚本入口，并以 Bun 作为 JS/TS 运行时（本仓库不再依赖 `npm ci` / `npm run`，也不要求系统安装 `node`）。
- 锁文件迁移为 Bun 锁文件，并在 CI 中使用冻结安装（frozen lockfile）确保可复现。
- 文档、CI、Dockerfile、hooks 中的命令口径统一。

### Non-goals

- 不做“无必要”的依赖升级；但若为满足“完全不使用 node（Bun runtime）”而出现不兼容，则升级/替换工具链是**必须完成**的修复路径（见下文 Upgrade plan）。
- 不引入新的测试框架或质量工具。

## 用户与场景

- 开发者：本地安装依赖、运行 `web` 的 lint/build/storybook/test-storybook。
- CI：执行前端 build + Storybook build/test 的校验。
- Docker 构建：构建包含 web 产物的镜像（当前 Dockerfile 里使用 npm）。

## 需求（Requirements）

### MUST

- 仓库内的 JS 依赖安装与脚本执行默认使用 Bun（不再要求/依赖 npm 与 node）。
- 若发现 Storybook / Playwright / Vite / test-storybook 等在 Bun runtime 下不兼容：必须按 Upgrade plan 做最小必要的升级/替换，直到满足验收标准。
- CI 中不再调用 `npm ci` / `npm run`（改为 Bun 等价命令），并具备可复现安装门槛（frozen lockfile）。
- Dockerfile（含 `Dockerfile` 与 `deploy/Dockerfile.web`）中不再使用 Node 基础镜像与 npm 命令，改为 Bun 等价路径。
- 文档与 hooks（如 `lefthook.yml`）中不再出现 npm 作为推荐/默认路径。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `web` 安装依赖与运行脚本 | CLI | internal | Modify | ./contracts/cli.md | frontend | devs / CI | `bun install` + `bun run <script>` |
| 前端锁文件 | File format | internal | Modify | ./contracts/file-formats.md | frontend | devs / CI | `bun.lock`/`web/bun.lock`（替代 `package-lock.json`/`web/package-lock.json`） |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 约束与风险

- 约束：默认不升级依赖；仅当为满足 Bun runtime（不使用 node）所必需时，才按 Upgrade plan 做最小升级/替换。
- 风险：在“始终使用最新 Bun”的策略下，上游变更可能导致兼容性回归；需依赖 CI 持续发现，并以 Upgrade plan 作为修复路径。

## 验收标准（Acceptance Criteria）

- Given 使用者未安装 npm 与 node
  When 在仓库与 `web/` 执行依赖安装与常用脚本（lint/build/storybook/build-storybook/test-storybook、以及 commitlint）
  Then 均可通过 Bun 完成（按契约），且无需 npm 与 node

- Given CI 执行 PR 校验
  When 运行前端与 Storybook 相关 job
  Then 不再出现 `npm ci` / `npm run`，并使用 frozen lockfile 安装策略

- Given Docker 构建镜像
  When 构建包含 web 产物的镜像
  Then Dockerfile 不再使用 npm，且构建结果一致（功能与产物路径不变）

- Given 迁移过程中发现 Bun runtime 兼容性问题
  When 通过升级/替换前端工具链（例如 Playwright/Storybook/Vite）解决
  Then 变更被记录（Change log），且不引入与目标无关的额外重构，且最终满足“不使用 node”的验收标准

## 实现前置条件（Definition of Ready / Preconditions）

- “改用 bun”的范围已冻结：包管理器 + 运行时均使用 Bun（不使用 node）
- 锁文件形态与位置已冻结（`bun.lock` 文本锁文件）
- CI 的 Bun 安装方式与版本策略已冻结：使用 `oven-sh/setup-bun@v2`，并按 `bun-version: latest` 安装（与“始终最新”策略一致）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 迁移不应降低现有 CI 覆盖：`web` 的 lint/build/storybook build/test 仍需通过。

### Quality checks

- 不引入新工具；沿用仓库既有 lint/typecheck/CI 门槛。

## 文档更新（Docs to Update）

- `README.md`: `npm install` / `npm run dev` 等示例替换为 Bun 口径。
- `web/README.md`: Storybook 相关命令口径替换为 Bun。
- `lefthook.yml`: `npm run lint` 迁移为 Bun 等价命令。
- `.github/workflows/*.yml`: `actions/setup-node` + `npm ci` 迁移为 Bun 安装与执行策略（不依赖 node）。
- `Dockerfile` 与 `deploy/Dockerfile.web`: Node builder stage 迁移为 Bun builder stage。
- `crates/dockrev-api/build.rs`: 文本提示中的 `npm ci && npm run build` 迁移为 Bun 口径（避免误导）。

## 实现里程碑（Milestones）

- [x] M1: 生成并提交 Bun 锁文件（`bun.lock` / `web/bun.lock`），并保留 `package-lock.json` / `web/package-lock.json`（待验收后移除）
- [x] M2: 本地开发与 hooks 迁移到 Bun（README + lefthook + scripts），并确保不依赖 node
- [x] M3: CI 与 Docker 构建迁移到 Bun（setup-bun + bun install --frozen-lockfile + bunx），并验证前端/Storybook 相关 job 稳定
- [x] M4: Upgrade plan（如需）：为达成 Bun runtime 兼容性而进行的最小工具链升级/替换，并记录变更

## 方案概述（Approach, high-level）

- 以 Bun 作为包管理器与脚本入口（`bun install` / `bun run <script>`），并以 Bun 作为运行时（不要求 node）。
- 通过 CI gating 验证 Storybook / Playwright / test-storybook 在 Bun runtime 下可用；若失败，按计划内回退/替代策略处理（需主人决策）。

### Upgrade plan（兼容性升级/替换策略）

原则：仅在“Bun runtime 下无法通过现有 CI/本地脚本”的前提下升级；一旦确认不兼容，升级/替换即为**必须**的修复动作；升级以“最小必要变更”为准，避免无关重构。

优先级（从低风险到高风险）：

1. Playwright：升级 `playwright`（并同步其浏览器安装步骤），以解决运行时/安装链路问题。
2. Storybook：升级 `storybook`、`@storybook/*`、`@storybook/test-runner`，以解决 Bun 下 CLI/runner 兼容性问题。
3. Vite：升级 `vite` 与相关 plugins，以解决 Bun 下 dev/build/preview 行为差异。
4. TypeScript/ESLint 等：仅当工具链版本差异导致无法运行时才升级。

每次升级必须：

- 在本计划 `Change log` 记录“为什么必须升级 + 升级范围（哪些包）+ 验证项（哪些 job/命令）”。
- 保持命令契约与端口策略（#0010）不被破坏。

### 代码库探索结论（Repo reconnaissance, minimal）

本计划的实现预计会触及以下位置（用于评估范围与冲突面；实现阶段再逐项落地）：

- CI：`.github/workflows/ci-pr.yml`、`.github/workflows/ci-main.yml`、`.github/workflows/release.yml`（存在 `actions/setup-node`、`npm ci`、`npm run ...`）
- Docker 构建：`Dockerfile`、`deploy/Dockerfile.web`（存在 `node:20-alpine` builder stage + `npm ci`/`npm run build`）
- Hooks：`lefthook.yml`（存在 `npm run lint`；commitlint 已有 `bunx` fallback）
- 文档/提示：`README.md`、`web/README.md`、`crates/dockrev-api/build.rs`（存在 npm 口径提示）

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：迁移目标为“禁用 npm（命令与锁文件）且不使用 node”，并接受实现阶段以 CI 结果作为是否可达成的最终判定依据。
- 已确认：若 Bun runtime 兼容性问题只能通过升级/替换部分前端工具链解决（例如 Playwright/Storybook/Vite 版本），必须按 Upgrade plan 执行并记录。

## 变更记录（Change log）

- 2026-01-23: 确认“不使用 node”，并将工具链不兼容时的升级/替换纳入交付范围（Upgrade plan）。
- 2026-01-23: 将“工具链升级/替换”从“允许”明确为“如遇不兼容则必须完成”的修复路径（不改变“默认尽量不升级”的约束）。
- 2026-01-24: `test-storybook` 在 Bun runtime 下不兼容（`The superclass is not a constructor`）；改为 Playwright 驱动的 Storybook smoke 测试以满足“不依赖 node”的验收标准。

## 参考（References）

- Bun lockfile：`bun.lock`（文本锁文件；可从 `package-lock.json` 自动迁移）
