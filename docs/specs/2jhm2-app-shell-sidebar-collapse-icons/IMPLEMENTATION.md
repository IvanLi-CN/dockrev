# Dockrev：AppShell 左侧导航折叠与图标化实现状态（#2jhm2）

## Implementation

- `Shell.tsx` 将主导航配置扩展为 label / route / icon 单一来源，并增加桌面 sidebar 折叠状态。
- `App.css` 增加展开/折叠几何、真实图标样式、移动 drawer 图标布局、focus-visible 与 reduced-motion 覆盖。
- `AppShell.stories.tsx` 覆盖展开、折叠、切换交互与移动 drawer 图标场景；`AppShell.mdx` 提供可浏览的 docs 入口。

## Validation

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

## Current Coverage

- 桌面展开/折叠行为已由 Storybook play 覆盖。
- 移动 drawer 图标渲染已由 Storybook 场景覆盖。
- 视觉证据绑定到 Storybook mock-only AppShell 场景。
