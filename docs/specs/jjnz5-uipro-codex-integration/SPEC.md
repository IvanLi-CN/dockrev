# Dockrev 接入 UI UX Pro Max（Codex 团队共享）（#jjnz5）

## 状态

- Status: 已完成
- Created: 2026-02-24
- Last: 2026-02-24

## 背景 / 问题陈述

- 当前仓库没有项目级 `.codex/skills/ui-ux-pro-max`，无法在团队内稳定复用 UI UX Pro Max 能力。
- 仓库当前只有 `docs/plan/**`，未建立 `docs/specs/**` 入口，不满足 specs-first workflow。
- 不做接入会导致每个工作树或成员都要重复手工安装，且命令路径不统一。

## 目标 / 非目标

### Goals

- 在仓库中接入可追踪的 Codex skill：`.codex/skills/ui-ux-pro-max/**`。
- 建立 `docs/specs/README.md` 与本任务 `SPEC.md`，完成最小 specs-first 迁移。
- 调整 `.gitignore`，仅跟踪 `ui-ux-pro-max`，其余 `.codex` 继续忽略。
- 校验 `search.py` 在仓库根目录可直接运行。

### Non-goals

- 不改动任何业务运行时 API（Rust HTTP / TS 类型）。
- 不做页面视觉改版、组件重构或样式迁移。

## 范围（Scope）

### In scope

- `docs/specs/**` 的最小迁移与本任务规格建档。
- `.gitignore` 规则改造（选择性跟踪 `.codex/skills/ui-ux-pro-max/**`）。
- 通过 `uipro-cli@2.2.3` 离线安装 UI UX Pro Max 到项目。
- 规范化 `SKILL.md` 示例命令路径。

### Out of scope

- Dockrev 前端 UI 主题、组件、页面任何行为修改。
- 运行时配置、数据库、部署脚本调整。

## 需求（Requirements）

### MUST

- 必须创建 `docs/specs/README.md` 与 `docs/specs/jjnz5-uipro-codex-integration/SPEC.md`。
- 必须在 `th/` 前缀分支实施。
- 必须以 `uipro-cli@2.2.3` + `--offline` 方式安装。
- 必须保证 `.codex/skills/ui-ux-pro-max/SKILL.md` 未被忽略且其余 `.codex` 默认忽略。
- 必须通过两条脚本验证命令（design-system/domain）。
- 必须使用 conventional commit（英文）+ `--signoff`。

### SHOULD

- 建议在仓库 `README.md` 增加简短使用说明。
- 建议清理并忽略 `scripts/__pycache__/` 与 `*.pyc`。

### COULD

- 若未来接入更多团队技能，可复用同一 `.gitignore` 模式。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

1. 在仓库根目录执行 `npx -y uipro-cli@2.2.3 init --ai codex --offline`。
2. 安装后修正 `SKILL.md` 中 `search.py` 示例路径为 `.codex/...`。
3. 通过 `git check-ignore` 验证跟踪策略。
4. 运行 `search.py` 两条命令，输出 style/palette/typography 推荐内容。

### Edge cases / errors

- 若 `npx` 网络不可达：保留 `--offline` 并重试；仍失败则阻断并上抛。
- 若 `uipro` 写入了缓存文件（`__pycache__`）：删除后补充忽略规则。
- 若当前分支是 detached HEAD：必须先创建 `th/` 分支再提交。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.codex/skills/ui-ux-pro-max/**` | File format | internal | New | None | dev workflow | Codex users | 团队共享 skill 资产 |
| `.gitignore` `.codex` 规则 | Config | internal | Modify | None | repo maintainers | all developers | 选择性纳入 skill |
| `docs/specs/**` index + spec | Docs | internal | New | None | maintainers | workflow tools | specs-first 门禁入口 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 仓库完成接入
  When 检查 `.codex/skills/ui-ux-pro-max/`
  Then 存在 `SKILL.md`、`data/**`、`scripts/**`。

- Given `.gitignore` 已更新
  When 执行 `git check-ignore -v .codex/skills/ui-ux-pro-max/SKILL.md`
  Then 该文件未被忽略。

- Given `.gitignore` 已更新
  When 执行 `git check-ignore -v .codex/other-local-file`
  Then 命中忽略规则。

- Given `search.py` 示例路径已修正
  When 运行 design-system 与 style domain 两条命令
  Then 返回退出码 0 且输出包含设计建议。

## 实现前置条件（Definition of Ready / Preconditions）

- `flow_type=fast-track` 已锁定。
- 当前工作区脏区状态为 `不存在`。
- owner 已授权实施与远端推进（push/PR/checks/review-loop）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- CLI validation: `npx -y uipro-cli@2.2.3 init --ai codex --offline`。
- Script validation: `search.py` design-system/domain 各一条。

### Quality checks

- `git status --short` 仅包含本任务预期文件。
- 提交需通过仓库 hook，不得使用 `--no-verify`。

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增 index 与 specs-first 入口。
- `README.md`: 增加 Codex UI UX Pro Max 使用说明（若实施）。

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 建立 `docs/specs` 最小迁移与本任务 `SPEC.md`。
- [x] M2: 完成 `.gitignore` 规则改造并接入 `ui-ux-pro-max` skill。
- [x] M3: 完成脚本验证、提交、推送与 PR 创建。

## 方案概述（Approach, high-level）

- 先满足 specs-first gate，再做工程接入。
- 采用固定版本 + 离线安装保证可复现。
- 使用选择性忽略策略降低 `.codex` 噪音并保留团队共享资产。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`uipro` 后续版本模板路径若变更，可能需要再次规范化 `SKILL.md` 示例。
- 开放问题：None。
- 假设：当前远端仓库与 GitHub MCP 权限可用于创建 PR 与跟踪 checks。

## 变更记录（Change log）

- 2026-02-24: 初始规格创建。
- 2026-02-24: 完成接入与快车道交付（PR #88）。

## 参考（References）

- https://ui-ux-pro-max-skill.nextlevelbuilder.io/#styles
- https://github.com/nextlevelbuilder/ui-ux-pro-max-skill
