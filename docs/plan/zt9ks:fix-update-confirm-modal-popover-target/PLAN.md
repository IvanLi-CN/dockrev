# Dockrev Web/API: 修复服务更新确认弹窗（popover 层级 + 目标版本错选）（#zt9ks）

## 状态

- Status: 已完成
- Created: 2026-02-18
- Last: 2026-02-18
- Notes: PR #70

## 背景 / 问题陈述

- 线上可复现：更新确认弹窗内，指针悬浮在版本号上时，版本信息 popover 会被弹窗遮罩层压在下面，导致不可见、不可交互。
- 严重风险：更新确认弹窗内“目标版本”可能与列表上展示的候选版本不一致（异步加载候选列表后被前端静默改选），存在误更新到错误版本的风险。
- 一致性缺口：当服务当前 tag 为 floating tag（例如 `latest`/`stable`），`/api/services/:id/candidates` 在该场景下对 tags 的排序可能走字典序，进一步放大“默认目标选择”的错选风险。

## 目标 / 非目标

### Goals

- 修复更新确认弹窗内 popover 的层级问题：popover 必须显示在遮罩层之上。
- 修复更新确认弹窗默认目标版本：默认目标必须锁定为列表候选 `candidate.tag`（即 `initialTag`），且 candidates 异步加载完成后不得静默改选。
- 后端 `/api/services/:id/candidates` 在 floating tag 场景下按 semver-first 排序（可解析版本优先按版本降序），避免字典序导致候选顺序异常。

### Non-goals

- 不改 UI 的结构/文案布局，不引入新的交互流程（仅修 bug）。
- 不改变 candidates 接口的返回结构，仅调整顺序以提升一致性与可预期性。

## 验收标准（Acceptance Criteria）

- Given 更新确认弹窗已打开，
  When hover/click 版本号触发 popover，
  Then `.versionTagsPopover` 可见且不被 `.modalOverlay` 遮挡。

- Given 该服务列表候选为 `svc.candidate.tag`，
  When 打开确认弹窗并等待候选加载完成，
  Then select 的值仍为 `svc.candidate.tag`；
  And 点击“执行更新”发送的 `targetTag` 等于该值。

- Given 服务当前 tag=latest 且 registry tags 含 `v0.2.9/v0.2.10/v0.2.11`，
  When 调用 `/api/services/:id/candidates`，
  Then 返回顺序以 `v0.2.11` 为首（semver-first，而非字典序）。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `cd web && bun run lint`
- `cd web && bun run build`
- `cd web && bun run build-storybook && bun run test-storybook`

## 实现里程碑（Milestones）

- [x] M1: Web z-index 修复 + UpdateTargetSelect 默认目标锁定 initialTag
- [x] M2: API candidates 在 floating tag 场景 semver-first 排序 + 回归测试
- [x] M3: Storybook Playwright 回归测试 + 最小验证通过

## 风险 / 开放问题（Risks & Open Questions）

- Popover 使用 `createPortal(..., document.body)`；本计划通过 z-index 修复遮挡，需要确保不会被其他 overlay 再次压住。
- candidates 顺序调整可能改变 UI 中候选的展示顺序，但不改变可选性判断与更新逻辑。

## 变更记录（Change log）

- 2026-02-18: 创建计划并冻结范围与验收标准（Status=待实现）。
- 2026-02-18: 完成实现与回归测试；本地验证通过（cargo test + web lint/build/storybook）；提交 PR #70。
