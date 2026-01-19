# Storybook：组件覆盖与主题切换（#0002）

## 状态

- Status: 待实现
- Created: 2026-01-19
- Last: 2026-01-19

## 背景 / 问题陈述

当前仓库已包含前端工程 `web/`（Vite + React + TypeScript），但缺少用于组件/页面可视化回归与状态覆盖的 Storybook。

我们需要补齐 Storybook，并在 Storybook UI 内提供主题切换能力，以便在 Light/Dark 等主题下快速检查布局、页面与组件的不同状态呈现。

## 目标 / 非目标

### Goals

- 在 `web/` 内引入 Storybook（React + Vite），提供稳定的本地开发入口。
- Storybook 能覆盖：
  - layout（布局）
  - pages（页面）
  - 其他 UI 组件（含其关键状态）
- Storybook 提供主题切换（toolbar），可在不同主题下预览同一组 stories（`light` / `dark`）。
- 应用本身支持显式主题（不依赖仅 `prefers-color-scheme` 的系统模式），并与 Storybook 共用同一套主题机制。
- 建立可持续的 stories 组织与新增规范，让后续新增组件/页面能自然接入。

### Non-goals

- 不在本计划内“发明/实现新的 UI 组件库或设计系统”（除非为 Storybook/theme 切换最小必要的适配）。
- 不在本计划内引入视觉回归平台（Chromatic 等）或截图基线管理（可作为后续计划）。
- 不在本计划内发布/托管 Storybook（仅保证本地可用；如需部署另开计划）。

## 用户与场景

- 前端开发：开发组件/页面时快速预览与调参（Controls），覆盖 loading/empty/error 等状态。
- 评审者（你/未来的协作者）：在 PR 里基于 Storybook 快速验收 UI 变化与主题一致性。

## 需求（Requirements）

### MUST

- Storybook 必须作为 `web/` 的 dev tool 引入（不影响生产构建与运行）。
- 必须提供 Storybook 脚本入口（见契约）：
  - 本地启动 Storybook
  - 构建静态 Storybook（用于离线预览/未来部署）
  - 运行 Storybook 自动化测试（用于 CI）
- Storybook 必须加载 `web/` 前端的全局样式（例如 `web/src/index.css`）以保持视觉口径一致。
- Storybook 必须提供主题切换（toolbar）：
  - 至少支持 `light` / `dark`
  - 切换后对所有 stories 生效，且切换 stories 时保持不变
- 应用主题必须可被显式设置（不依赖系统主题）：
  - 主题值与 Storybook 一致：`light` / `dark`
  - 通过稳定的 DOM hook 应用（见契约），Storybook 与 App 共用
- 应用必须提供主题切换入口（UI）：
  - 可在任意页面/主要入口处切换 `light` / `dark`
  - 切换结果需持久化（刷新后仍保持）
- Storybook 的内容覆盖范围必须包含（以实现当下仓库实际存在者为准）：
  - layout / pages / components
  - 并为每个对象提供其“代表性状态”的 stories（口径见文末）
- 引入 Storybook 后，仓库既有质量门槛不得退化：
  - `web/` 的 `npm run lint` 仍可通过
  - `web/` 的 `npm run build` 仍可通过
- CI 必须包含 Storybook 的自动化校验：
  - `web/` 的 `npm run build-storybook` 通过
  - `web/` 的 `npm run test-storybook` 通过

## 接口清单与契约（Interface Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `web` Storybook scripts | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | frontend | devs | `npm run storybook` / `npm run build-storybook` |
| `web` Storybook test script | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | frontend | CI / devs | `npm run test-storybook` |
| Storybook config files | Config | internal | New | [./contracts/file-formats.md](./contracts/file-formats.md) | frontend | devs | `web/.storybook/*` |
| Story files convention | File format | internal | New | [./contracts/file-formats.md](./contracts/file-formats.md) | frontend | devs | `*.stories.tsx` 的路径与命名 |
| Theme application hook | Config | internal | New | [./contracts/file-formats.md](./contracts/file-formats.md) | frontend | app / storybook | `data-theme` + `color-scheme` |

### 契约文档（按 Kind 拆分）

- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 验收标准（Acceptance Criteria）

- Given 在 `web/` 中完成 Storybook 接入
  When 运行 `npm run storybook`
  Then Storybook dev server 正常启动，且 Sidebar 能看到至少一个来自 `web/src` 的 story

- Given Storybook toolbar 提供主题切换
  When 从 `light` 切到 `dark`（或相反）
  Then 预览区主题随之改变（背景/前景/控件等），且切换到其它 stories 后主题设置保持不变

- Given 仓库中存在某个 in-scope 的 UI 对象（layout/page/component）
  When 打开 Storybook Sidebar
  Then 该对象在 Storybook 中可被定位（有对应 story 入口），并包含其“关键状态”的 story（状态口径见下文）

- Given `web/` 引入 Storybook 的依赖与配置
  When 运行 `npm run lint` 与 `npm run build`
  Then 两者均通过

- Given 在 CI 环境中执行前端校验
  When 运行 `npm run build-storybook` 与 `npm run test-storybook`
  Then 两者均通过

- Given 用户打开 Web App
  When 通过应用内主题切换入口从 `light` 切到 `dark`（或相反）
  Then 页面主题立即变化，且刷新页面后仍保持该主题

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing / Quality

- 本计划不强制引入新的测试框架；以仓库既有约定为准。
- 交付门槛（实现阶段必须满足）：
  - `web/`：`npm run lint` 通过
  - `web/`：`npm run build` 通过
  - Storybook：`npm run storybook` 可用，且 `npm run build-storybook` 可用
  - Storybook tests：`npm run test-storybook` 通过（CI）

### Docs

- 需要补齐/更新 `web/README.md`：新增 Storybook 的使用方式与新增 stories 的约定。
- 如有其它文档明确写了“当前仓库未引入 Storybook”，在实现后应同步更新以避免信息过期（例如 `docs/plan/0001:dockrev-compose-updater/PLAN.md` 的相关段落）。

## 里程碑（Milestones）

- [ ] 在 `web/` 集成 Storybook（React + Vite）并可启动
- [ ] Storybook toolbar 主题切换（`light` / `dark`）并对所有 stories 生效
- [ ] 应用显式主题支持（`light` / `dark`），提供 UI 切换入口并持久化，与 Storybook 共用主题机制
- [ ] 落地 stories 组织规范与示例（目录/命名/新增方式）
- [ ] 为实现当下 in-scope 的 layout/pages/components 补齐 stories 与“代表性状态”覆盖
- [ ] CI 增加 `build-storybook` + `test-storybook` 校验
- [ ] 更新 `web/README.md` 与受影响的计划/文档说明

## 约束与风险

- 当前 `web/src` 仍处于 Vite 模板状态，尚未体现 layout/pages/components 的实际目录结构；若在实现阶段同时引入新的目录约定，需要避免“为 Storybook 而重构”造成范围膨胀。
- 若后续采用 DaisyUI 的主题体系（`data-theme=...`），需要确保本计划的 `light` / `dark` 命名与 DaisyUI 的主题命名/映射策略不冲突（必要时在 Storybook 侧做映射层，而不回退改口径）。
- 将 Storybook tests 纳入 CI 可能引入 Playwright 依赖与额外耗时；需要控制并行度与超时，避免 CI 不稳定。

## 开放问题（需要主人决策）

- None

## 假设（Assumptions，需要主人确认）

- Storybook 仅落在 `web/`（`web/package.json`）内，不改动仓库根目录的 Node 工具链。
- 以 Vite + React 为基线接入 Storybook（不引入 Next.js 等额外框架）。
- 主题选项为 `light` / `dark`，并通过统一的 DOM hook（`data-theme`）应用于 Storybook 与 App。

## “代表性状态”覆盖口径（用于实现阶段）

为避免“穷举所有 props 组合”导致范围失控，本计划将按“代表性状态”进行覆盖，并在实现阶段按实际组件类型取舍，但需满足以下最小口径：

- Base：默认态（default）
- Disabled：不可用态（若组件支持交互/输入）
- Loading / Skeleton：加载态（若组件/页面涉及异步数据）
- Empty：空数据态（若组件/页面展示集合数据）
- Error：错误态（若组件/页面展示错误信息）
- Variants：对用户可见且常用的 variant（例如 size/intent），每类组件至少覆盖 1–2 个最常用 variant
