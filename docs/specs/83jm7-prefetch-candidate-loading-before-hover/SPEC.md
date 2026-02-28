# Dockrev：候选版本首屏预取加载态（修复 latest 悬浮后才加载）（#83jm7）

## 状态

- Status: 已完成
- Created: 2026-02-28
- Last: 2026-02-28

## 背景 / 问题陈述

- 当前 `VersionTagsPopover` 只有在悬浮或点击后才会触发 digest snapshot 请求。
- 当候选值是 floating tag（例如 `latest`）且 `candidate.resolvedTag` 尚未就绪时，列表首屏会先展示 `latest`，交互后才变成 `加载中…`。
- 这会让用户误以为候选版本已经确定，直到触发悬浮才看到真实状态切换。

## 目标 / 非目标

### Goals

- 在列表主行里，当候选版本属于“未解析的 floating tag”时，挂载后立即进入 `加载中…` 并后台请求 snapshot。
- snapshot 进入 ready 后，触发器自动恢复为候选展示文案（resolved/raw 回退逻辑保持不变）。
- 已知推测版本（`candidate.resolvedTag` 已是 strict semver）保持直出，不被首屏 loading 覆盖。

### Non-goals

- 不修改 Rust 后端 API/DB/worker。
- 不修改 `CurrentVersionPopover` 的请求触发策略。
- 不引入全局预取，仅在 `Services/Overview` 的候选主触发器做选择性预取。

## 范围（Scope）

### In scope

- `web/src/components/VersionTagsPopover.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/src/stories/components/VersionTagsPopover.stories.tsx`
- `web/scripts/test-storybook.mjs`

### Out of scope

- `crates/**`
- `web/src/components/CurrentVersionPopover.tsx`
- `web/src/pages/ServiceDetailPage.tsx`

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `VersionTagsPopover` | TS component props | internal | Modify | None | web | Services/Overview pages | 新增 `prefetchOnMount?: boolean`，默认 `false` |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 候选 tag 为 `latest`、`candidate.resolvedTag` 缺失且 digest 存在，When 列表首屏渲染且未悬浮，Then 候选触发器显示 `加载中…`。
- Given snapshot 从 `pending` 轮询到 ready，When 列表保持渲染，Then 候选触发器从 `加载中…` 恢复为候选版本文案。
- Given `candidate.resolvedTag` 已是 strict semver，When 列表首屏渲染，Then 候选触发器直接显示该版本号，不显示 `加载中…`。
- Given 候选 digest 缺失，When 列表首屏渲染，Then 维持当前展示文案，不进入 `加载中…`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun test web/tests/versionDisplay.test.ts`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `VersionTagsPopover` 支持可选挂载预取，并在预取阶段显示 `加载中…`。
- [x] M2: `Services/Overview` 仅对“未解析 floating 候选”传入预取开关。
- [x] M3: Storybook 场景与 `test-storybook` 增加“无交互首屏 loading”回归断言。
- [x] M4: lint/build/storybook 回归全部通过。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若误把预取开关传给已解析候选，会短暂遮挡已知版本号；需用条件门控与回归断言兜底。
- 假设：候选预取只发生在 digest 已知场景，额外请求量可接受。

## 变更记录（Change log）

- 2026-02-28: 创建规格，冻结范围、接口与验收标准。
- 2026-02-28: 完成实现与回归；`lint/build/build-storybook/test-storybook` 全部通过。
- 2026-02-28: 按 review 反馈修复预取计时器清理与 digest 切换首帧加载态一致性问题。
