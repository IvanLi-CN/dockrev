# Dev/CI: 禁用默认端口（统一高位端口）（#0010）

## 状态

- Status: 待实现
- Created: 2026-01-23
- Last: 2026-01-23

## 问题陈述

当前仓库内存在会使用工具默认端口启动的开发/测试服务（例如 Vite `5173`、Storybook `6006`、Vite preview `4173`），在多项目/多 worktree 并行开发时易冲突，且不符合“服务必须使用高位端口且需显式指定”的团队要求。

## 目标 / 非目标

### Goals

- 本仓库内所有“会对本机开放监听”的开发/测试服务，默认都绑定到**显式指定的高位端口**（不再落到工具默认端口）。
- 端口分配与覆盖方式形成稳定契约（可实现、可测试），并在文档中成为单一事实来源（single source of truth）。

### Non-goals

- 不引入“自动找空闲端口/动态端口分配”（仍以稳定、可预期的固定端口为主）。
- 不变更现有线上/部署对外端口语义（例如 `DOCKREV_HTTP_ADDR` 的现有默认值如继续满足“高位端口”即可）。
- Dockrev API：`DOCKREV_HTTP_ADDR` 当前默认已为高位端口（`0.0.0.0:50883`），本计划不调整其默认值。

## 用户与场景

- 开发者在同一台机器同时跑多个项目/多个 worktree：需要端口不冲突，且默认行为不占用常见默认端口。
- CI 在执行 Storybook 相关校验时：不应因为默认端口冲突导致不稳定失败。

## 需求（Requirements）

### MUST

- `web/` 的 Vite dev server 不得使用默认端口 `5173`；必须默认使用高位端口且可覆盖。
- `web/` 的 Vite preview server 不得使用默认端口 `4173`；必须默认使用高位端口且可覆盖。
- `web/` 的 Storybook dev server 不得使用默认端口 `6006`；必须默认使用高位端口且可覆盖。
- `web/` 的 `test-storybook`（自起本地静态 server 的模式）不得使用 `6006`；必须默认使用高位端口且可覆盖。
- 覆盖方式需在契约中明确（环境变量/CLI 参数优先级、冲突处理、退出码）；并在文档中建议“遇到冲突时显式指定端口（优先 env vars）”。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `web: vite (dev)` | CLI | internal | Modify | ./contracts/cli.md | frontend | devs | Vite dev server 端口策略 |
| `web: vite preview` | CLI | internal | Modify | ./contracts/cli.md | frontend | devs | Vite preview 端口策略 |
| `web: storybook dev` | CLI | internal | Modify | ./contracts/cli.md | frontend | devs | Storybook dev 端口策略 |
| `web: test-storybook` | CLI | internal | Modify | ./contracts/cli.md | frontend | CI / devs | 本地静态 server 端口策略 |
| 端口相关环境变量 | Config | internal | New | ./contracts/config.md | frontend | devs / CI | 统一端口配置入口 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/config.md](./contracts/config.md)

## 约束与风险

- 约束：需要兼容常见脚本入口（例如 `bun run` / `npm run`）的参数透传；且默认行为仍必须是“高位端口 + 显式指定 + 冲突严格失败”。
- 风险：若冲突时允许自动换端口，易造成误判与串线；本计划明确为严格失败。

## 验收标准（Acceptance Criteria）

- Given 本机无相关服务占用计划内端口
  When 启动 `web` 的 Vite dev server
  Then dev server 监听端口为“约定的高位默认端口”，且不是 `5173`

- Given 本机无相关服务占用计划内端口
  When 启动 `web` 的 Vite preview server
  Then preview server 监听端口为“约定的高位默认端口”，且不是 `4173`

- Given 本机无相关服务占用计划内端口
  When 启动 `web` 的 Storybook dev server
  Then Storybook dev 监听端口为“约定的高位默认端口”，且不是 `6006`

- Given 本机无相关服务占用计划内端口
  When 运行 `web` 的 `test-storybook` 且未指定 `--url` / `TARGET_URL`
  Then 本地静态 server 监听端口为“约定的高位默认端口”，且不是 `6006`

- Given 端口被占用
  When 启动上述任一服务
  Then 严格失败退出（非 0），且输出明确指向“端口冲突 + 如何显式指定端口”

## 实现前置条件（Definition of Ready / Preconditions）

- 高位端口分配已确认（见 `./contracts/config.md`）
- 端口冲突策略已确认：严格失败
- 显式指定默认端口已确认：允许（但文档建议优先用 env vars 显式指定高位端口）
- 本计划的 CLI/Config 契约已定稿，实现与测试可以直接按契约落地

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 覆盖至少包括：默认端口行为、环境变量覆盖、参数透传覆盖、端口被占用时行为。

### UI / Storybook

- Storybook 相关命令在新端口策略下保持可用且稳定（含 `build-storybook` 与 `test-storybook`）。

### Quality checks

- 不引入新工具；沿用仓库既有 lint/typecheck/CI 流水线门槛。

## 文档更新（Docs to Update）

- `README.md`: “UI (dev server)” 示例端口不再使用 `5173`，同步更新为本计划约定的高位端口。
- `web/README.md`: Storybook 启动/测试说明同步端口策略与覆盖方式。
- `docs/plan/0002:storybook-theme-switching/contracts/cli.md`: Storybook 端口口径与默认行为补齐（如需与本计划对齐）。

## 实现里程碑（Milestones）

- [ ] M1: 统一端口分配与覆盖入口（按契约落地）
- [ ] M2: `web` 的 dev/preview/storybook/test-storybook 全部遵循高位端口策略并可被测试覆盖
- [ ] M3: 文档与 CI 口径对齐（README/相关计划契约更新 + CI 稳定性验证）

## 方案概述（Approach, high-level）

- 以“固定高位端口 + 显式指定 + 可覆盖”为主策略；端口值集中定义，避免散落在脚本/文档里。
- Storybook 侧优先使用其 CLI `--port` 与 `--exact-port` 语义；Vite 侧通过配置或 CLI `--port` 固定端口，并明确端口冲突策略。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 已确认：为 Dockrev 预留连续高位端口区间（`50884`–`50887`）用于 `web` dev/preview/storybook/test-storybook。

## 参考（References）

- Storybook CLI：`storybook dev -p/--port`、`--exact-port`
- Vite 默认端口：dev `5173`、preview `4173`；均可通过 `--port` 或配置项调整

## 代码库探索结论（Repo reconnaissance, minimal）

当前仓库内明确会对本机开放监听/占用端口的入口（可检索到的）为：

- Dockrev API：`DOCKREV_HTTP_ADDR`（默认 `0.0.0.0:50883`）
- `web`：Vite dev（默认 `5173`）、Vite preview（默认 `4173`）、Storybook dev（默认 `6006`）、`test-storybook` 本地静态 server（当前写死 `6006`）
