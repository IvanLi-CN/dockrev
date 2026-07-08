# Dockrev：详情页双侧栏与 Stack→Service 树导航历史（#c2r2u）

## History

- 2026-07-09：将详情页服务树导航从 `#ey4ar` 中拆出为独立 spec；原因是 `#ey4ar` 已明确声明“不增加侧栏级第二套服务详情子导航”。
- 2026-07-09：完成 `AppShell` 详情页壳层扩展、`DetailRouteServiceTree` 树导航、`StackDetailPage` / `ServiceDetailPage` 新工作区布局，以及移动端底部主导航 + 服务树抽屉接入。
- 2026-07-09：在同步 `origin/main` 的 rebase 中保留主线离线只读 / snapshot 合同，并把详情页服务树与视觉层重放到最新主线之上。
- 2026-07-09：fresh merge-proof 指出 `StackDetailPage` 未把主导航 / 移动端底部导航映射为 “服务” active；已修正 `AppShell` active 路由映射、补上 `StackDetailPage` story 断言，并刷新受影响的桌面视觉证据。
- 2026-07-09：fresh merge-proof 继续指出 `961px - 1160px` 断点会让详情页三列壳层错误回退为两列；已为 `.appShellWithDetailSidebar` 补齐窄桌面媒体规则，并刷新最终桌面 / 移动端视觉证据。
- 2026-07-09：fresh merge-proof 继续指出归档 Stack / Service 详情路由不会出现在详情树里；已将 `listStacksArchived("only")` 并入服务树读模型，并补上归档详情 story 回归覆盖。
- 2026-07-09：fresh merge-proof 继续指出详情树会为全部 Stack 并发请求 `getStack()`；已改为按当前/展开 Stack 懒加载详情，并用 Storybook mock debug 断言不预取无关 Stack detail。
