# Dockrev Web: 左下角版本号链接指向 GitHub Release（#fc8ua）

## 状态

- Status: 待实现
- Created: 2026-02-18
- Last: 2026-02-18
- Notes: -

## 背景 / 问题陈述

- Dockrev Web 在侧边栏左下角展示当前运行版本（来自 `GET /api/version`）。
- 现状：点击版本号会跳转到仓库源码树（`/tree/<ref>`），对运维排障来说不够直接。
- 期望：版本号链接应跳转到 GitHub Release（按 tag），便于快速查看 release notes / assets。

## 目标 / 非目标

### Goals

- 当 `/api/version` 返回非空字符串 `X` 时，版本号可点击并打开：
  - `https://github.com/IvanLi-CN/dockrev/releases/tag/X`
- 同步调整可访问性文案（aria-label / title），避免误导。

### Non-goals

- 不修改后端 `/api/version` 行为。
- 不调整发布流程/CI 口径。

## 范围（Scope）

### In scope

- Web：调整 AppShell（sidebar meta 区域）版本号链接生成逻辑（从 `/tree/…` 改为 `/releases/tag/…`）。
- Web：更新版本号链接的 aria-label / title 文案（指向 Release）。

### Out of scope

- 任何 Rust 端逻辑变更（API、版本计算、release tagging 等）。

## 验收标准（Acceptance Criteria）

- Given `/api/version` 返回非空字符串 `X`
  When UI 渲染完成并点击左下角版本号
  Then 浏览器打开 `https://github.com/IvanLi-CN/dockrev/releases/tag/X`（新标签页）
- Given `/api/version` 请求失败（网络/401/500 等）
  Then 版本号展示占位符（例如 `-`），且不影响 UI 其他功能（保持现有行为）

## 测试 / 验证（Testing）

- Web：
  - `cd web && bun install --frozen-lockfile`
  - `bun run lint`
  - `bun run build`

## 里程碑（Milestones）

- [ ] Web：版本号链接从 repo tree 改为 release tag
- [ ] Web：同步更新 aria-label / title 文案
- [ ] 验证：本地通过 web lint + build

## 风险与开放问题（Risks & Open Questions）

- `/api/version` 若返回非标准 tag（例如包含空格/特殊字符），链接可能不可用；当前保持“按返回值直连”的策略不做额外纠错。

