# Dockrev：详情页双侧栏与 Stack→Service 树导航实现状态（#c2r2u）

## Implementation

- 已在 `AppShell` 新增详情页专用服务树侧栏与移动端抽屉插槽，桌面端改为 `主导航 / 服务树侧栏 / 主内容` 三列结构。
- 已新增 `DetailRouteServiceTree`，基于现有 `listStacks()` / `getStack()` 组合得到 `Stack -> Service` 树，并支持当前节点高亮、默认展开当前 Stack、点击 Stack 进入 Stack 详情、点击 Service 保留当前 section。
- 已让 `StackDetailPage` 与 `ServiceDetailPage` 统一采用新的 detail workspace hero / meta / tabs 壳层，并保留主线已存在的离线只读与 snapshot 语义。
- 已为移动端详情页接入底部主导航与“服务导航”抽屉入口。
- 已补齐 `ServiceDetailPage` 与 `StackDetailPage` 的 Storybook stories / interaction coverage，覆盖桌面与移动端导航行为。
- 已修正 `StackDetailPage` 路由下主导航 / 移动端底部导航的 active 映射，让 `stack` 详情与 `service` 详情统一归到 “服务” 主模块，并由 `StackDetailPage` stories 回归断言覆盖。
- 已补齐 `961px - 1160px` 窄桌面断点下的详情页 grid override，避免三列详情壳层误回退到 overview 使用的两列媒体规则。
- 已把归档 Stack 也纳入详情页服务树数据源，并补上归档服务详情的 Storybook 回归 story，保证 archived detail route 仍能显示当前 Stack / Service 高亮。
- 已把服务树详情读取改为“先加载 Stack 列表，再按当前/展开的 Stack 懒加载 detail”，避免每次进入详情页都并发请求全部 `getStack()`，并修正懒加载 effect 的自取消问题，确保展开 Stack 后能稳定落盘服务列表结果。
- 已让移动端抽屉内的服务树按打开状态再挂载，避免桌面详情页同时挂出桌面树与隐藏的移动树，从而重复触发导航请求。
- 已把详情页页头与主导航、服务树、主内容收敛到 AppShell 共享列变量：桌面品牌区止于服务树右边缘，右侧操作区与主内容共用起始边界，不新增可见竖分隔线。
- 窄桌面详情页把顶部操作条限制在主内容工作区；按钮保留完整文字，容器负责横向约束，不会向品牌区溢出。
- Stack 详情响应已在 `services[]` 上附带只读 `lifecycleState`；后端沿用 Compose 生命周期判定，以固定并发上限 7 批量读取，单服务失败降级为 `unknown`。
- 服务树已将运行态点与可更新版本信号分离：运行中 / 部分运行 / 已停止或未知分别使用绿色 / 琥珀 / 中性灰，只有 `updatable` 服务在版本 chip 右上显示 Signal Cyan dot；链接标题与无障碍名称同步包含运行态、版本和更新状态。
- 服务树已压缩桌面侧栏和移动抽屉的叶子缩进，并保持移动端至少 40px 行高与版本截断。
- 移动抽屉复用抽屉头的“服务导航”标题，移除树内重复标题；树列表参与抽屉剩余高度分配并在内容过长时独立滚动，最近扫描信息固定在底部。
- 应用内更新、回滚、生命周期和 Compose 标签保存结算后会发布 Stack 定向刷新事件；已展开 Stack 的可见页面每 30 秒轮询，隐藏时暂停且恢复可见立即补刷，重复刷新按 Stack 合并。

## Outstanding

- 无新的功能性未决项；后续只保留 PR / merge 收口与主线 review 反馈。

## Validation

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`
- `python3 ./.github/scripts/check-file-budgets.py`
- `cargo test -p dockrev-api`
