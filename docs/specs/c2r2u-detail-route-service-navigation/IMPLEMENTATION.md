# Dockrev：详情页双侧栏与 Stack→Service 树导航实现状态（#c2r2u）

## Implementation

- 已在 `AppShell` 新增详情页专用服务树侧栏与移动端抽屉插槽，桌面端改为 `主导航 / 服务树侧栏 / 主内容` 三列结构。
- 已新增 `DetailRouteServiceTree`，基于现有 `listStacks()` / `getStack()` 组合得到 `Stack -> Service` 树，并支持当前节点高亮、默认展开当前 Stack、点击 Stack 进入 Stack 详情、点击 Service 保留当前 section。
- 已让 `StackDetailPage` 与 `ServiceDetailPage` 统一采用新的 detail workspace hero / meta / tabs 壳层，并保留主线已存在的离线只读与 snapshot 语义。
- 已为移动端详情页接入底部主导航与“服务导航”抽屉入口。
- 已补齐 `ServiceDetailPage` 与 `StackDetailPage` 的 Storybook stories / interaction coverage，覆盖桌面与移动端导航行为。
- 已修正 `StackDetailPage` 路由下主导航 / 移动端底部导航的 active 映射，让 `stack` 详情与 `service` 详情统一归到 “服务” 主模块，并由 `StackDetailPage` stories 回归断言覆盖。

## Outstanding

- 无新的功能性未决项；后续只保留 PR / merge 收口与主线 review 反馈。

## Validation

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`
- `python3 ./.github/scripts/check-file-budgets.py`
