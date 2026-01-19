# File / Config Contracts（#0002）

本文件定义 Storybook 配置文件与 stories 文件的“可持续约定”（路径、命名、主题切换口径），用于让后续新增组件/页面可稳定接入。

## Storybook config files

### Directory

- Path: `web/.storybook/`
- Files (expected):
  - `main.(ts|js)`：stories discovery、addons、builder 等配置
  - `preview.(ts|js)`：decorators、globalTypes、parameters（含主题切换）
  - （可选）`preview-head.html` / `manager-head.html`：注入额外样式/脚本

> 具体扩展名与内容以实现阶段选择的 Storybook 版本为准，但路径与职责应保持稳定。

## Stories file convention

### Path pattern

- 推荐：与组件同目录 colocate，使用 `*.stories.tsx`
  - 示例：`web/src/components/Button.stories.tsx`
  - 示例：`web/src/layouts/AppShell.stories.tsx`
  - 示例：`web/src/pages/Dashboard.stories.tsx`
- 若需要集中管理（例如 pages/layout 的组合场景），允许使用 `web/src/stories/**`：
  - 示例：`web/src/stories/pages/Dashboard.stories.tsx`

### Naming

- `title` 以分组表达类型层次（示例）：
  - `Components/Button`
  - `Layouts/AppShell`
  - `Pages/Dashboard`

## Theme switching contract (Storybook toolbar)

### Global type

- Key: `theme`
- Values（默认建议；如主人选择不同命名，以实现阶段冻结为准）：
  - `light`
  - `dark`

### Application rule

- Storybook 在预览区应用主题时，必须做到：
  - 对所有 stories 生效
  - 在 stories 之间切换时保持不变
- Web App 在应用主题时，必须做到：
  - 与 Storybook 使用同一套主题值（`light` / `dark`）
  - 在页面刷新后保持主题不变（持久化）

### DOM hook (stable)

- 将主题值写入预览 iframe 的根节点属性，作为稳定的样式挂载点：
  - `<html data-theme="<value>">`
  - 并同步 `color-scheme`（Light→`light`，Dark→`dark`）

> 若后续采用 DaisyUI（见 `docs/plan/0001:dockrev-compose-updater/ui/daisyui-theme.md`），该 `data-theme` 可直接复用；若暂未引入 DaisyUI，则由 `web/src/index.css` 定义基于 `data-theme` 的 CSS tokens/样式来体现主题差异（App 与 Storybook 共用）。

### Preference persistence (stable)

- Storage: `localStorage`
- Key: `dockrev:theme`
- Values: `light` | `dark`
