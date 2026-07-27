# Dockrev：AppShell 左侧导航折叠与图标化实现状态（#2jhm2）

## Implementation

- `Shell.tsx` 将主导航配置扩展为 label / route / icon 单一来源，并增加桌面 sidebar 折叠状态。
- `App.css` 增加展开/折叠几何、真实图标样式、移动 drawer 图标布局、focus-visible 与 reduced-motion 覆盖。
- 桌面身份入口现位于 sidebar footer 元信息区首位，并在折叠态保留圆形头像触发器；普通路由页头与主导航共用列宽变量，不新增视觉分隔线。
- `AppShell.stories.tsx` 覆盖展开、折叠、切换交互与移动 drawer 图标场景；`AppShell.mdx` 提供可浏览的 docs 入口。
- 审查收敛后，普通路由折叠态将页头品牌裁为图标宽度并保留品牌可访问名称；桌面身份入口的隐藏规则限定在 `AppShell`，不影响独立 `SupervisorMisroute` 页头。

## Validation

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

## Current Coverage

- 桌面展开/折叠行为已由 Storybook play 覆盖。
- 移动 drawer 图标渲染已由 Storybook 场景覆盖。
- 视觉证据绑定到 Storybook mock-only AppShell 场景。
