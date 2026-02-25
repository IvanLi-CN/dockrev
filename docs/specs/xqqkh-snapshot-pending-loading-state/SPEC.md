# Dockrev：缓存未命中时列表版本显示加载态（#xqqkh）

## 状态

- Status: 已完成
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

- 版本列当前会直接显示解析出的版本文本；当 digest snapshot 命中 `pending`（缓存未命中）时，触发器仍显示版本号，和“正在读取快照”的真实状态不一致。
- 用户在列表中会看到“像是已确定版本”的文案，造成误导。

## 目标 / 非目标

### Goals

- 在 `CurrentVersionPopover` 与 `VersionTagsPopover` 中引入统一的 snapshot fetch phase：`idle/loading/ready/missing/error`。
- 当 snapshot endpoint 返回 `pending` 时，列表触发器文本强制显示 `加载中…`。
- 当 snapshot 变为 ready 后，触发器自动恢复原版本文本。
- 为 Storybook 增加稳定的 `pending -> ready` mock 场景与自动化回归。

### Non-goals

- 不修改后端 snapshot 生成和重试策略。
- 不修改 `versionInference.status=pending` 的既有业务语义。
- 不新增全局预取机制。

## 范围（Scope）

### In scope

- `web/src/components/CurrentVersionPopover.tsx`
- `web/src/components/VersionTagsPopover.tsx`
- `web/src/App.css`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `web/src/stories/components/VersionTagsPopover.stories.tsx`
- `web/scripts/test-storybook.mjs`

### Out of scope

- Rust API / DB / worker 行为修改。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/services/{id}/digest-tags-snapshot` | HTTP API | external | Modify (UI usage only) | None | backend | web | 仅调整前端对 `pending` 的展示策略，不改返回结构 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given snapshot API 返回 `pending`，When 版本触发器渲染，Then 触发器文本显示 `加载中…`，不显示版本号。
- Given snapshot 进入 ready，When 轮询返回 snapshot，Then 触发器文本恢复为原版本文本。
- Given snapshot 为 404 缺失，When 打开气泡，Then 仍显示“快照缺失”提示，不出现无限 loading。
- Given 回归测试执行，When 运行 `test-storybook`，Then 新增 pending 场景通过且未回退到 `/digest-tags` live 调用。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web test-storybook`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: Popover 组件接入 snapshot phase，并在 pending 时显示加载态触发器文本。
- [x] M2: Storybook mock 新增 pending->ready 场景并补充 story。
- [x] M3: test-storybook 回归用例覆盖“加载中 -> 恢复版本文本”。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若 pending 持续时间过长，用户可能需要额外超时提示（本次不处理）。
- 假设：只要触发过 snapshot 请求，就应在 pending 期间展示 `加载中…`。

## 变更记录（Change log）

- 2026-02-26: 创建规格，冻结范围与验收标准。
- 2026-02-26: 完成组件加载态、Storybook pending 场景与自动化回归；本地通过 `lint/build/test-storybook`。
