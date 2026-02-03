# Dockrev Web: 增加版本 / 开源仓库 / 开发者信息（#838ry）

## 状态

- Status: 已完成
- Created: 2026-02-03
- Last: 2026-02-03
- Notes: PR #52

## 背景 / 问题陈述

- 作为运维/管理类工具，排查问题时经常需要快速确认当前运行版本，并能一键跳转到开源仓库查看变更/提交/发布说明。
- 现状：Dockrev Web UI 页面内缺少可见的“版本 + 仓库链接 + 开发者”信息，导致需要通过命令行或额外文档查找。

## 目标 / 非目标

### Goals

- 在 UI 的固定位置（截图标记处）展示：
  - 版本信息（来自 `GET /api/version`）
  - 开源仓库地址（可点击）
  - 开发者信息（可点击）
- 信息展示应尽量克制：不抢夺主操作视线，但在需要时可快速找到。
- 版本接口不可用时要降级（仍能正常使用 UI）。

### Non-goals

- 不新增单独的 About 页面。
- 不引入复杂的构建时版本注入/commit hash 展示。

## 范围（Scope）

### In scope

- Web：在 AppShell（侧边栏/页脚区域）增加 “App meta” 展示区。
- Web：新增 `GET /api/version` 的 API 调用封装，并在 UI 中显示其结果。
- Storybook：补齐 mock API 对 `/api/version` 的响应，避免 story 运行时报错。

### Out of scope

- 后端 API 行为变更（只消费现有 `/api/version`）。
- 发布流程/CI 口径调整。

## 需求（Requirements）

### MUST

- UI 上可见版本号（`/api/version` 返回的字符串）。
- UI 上可见开源仓库 URL 与开发者信息，且可点击打开（新标签页）。
- `/api/version` 失败时：展示占位符（例如 `-`），并且不影响其他页面与功能。

## 验收标准（Acceptance Criteria）

- Given 打开任意 Dockrev Web 页面
  When 页面渲染完成
  Then 截图标记位置出现“版本 / 开源仓库 / 开发者”信息，布局不遮挡主内容
- Given `/api/version` 正常返回
  Then “版本”展示为该返回值
- Given `/api/version` 返回失败（网络/401/500）
  Then “版本”展示为 `-`（或等价占位），且 UI 其他功能不受影响

## 测试 / 验证（Testing）

- Web：运行仓库约定的 lint/typecheck/test（最小集合即可覆盖改动）。
- 手动：打开 Storybook / 本地 web，确认 sidebar/footer 的 meta 信息显示与链接可用。

## 里程碑（Milestones）

- [x] UI：新增 app meta 展示区（版本/仓库/开发者）
- [x] API：新增 `/api/version` 调用并接入 UI
- [x] Storybook：mock `/api/version`，保证 stories 可运行

## 风险与开放问题（Risks & Open Questions）

- UI 展示位置需要确保在不同分辨率与滚动场景下仍可发现（建议放在 sidebar 底部并保持克制）。
