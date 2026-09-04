# Dockrev：详情页双侧栏与 Stack→Service 树导航历史（#c2r2u）

## History

- 2026-07-09：将详情页服务树导航从 `#ey4ar` 中拆出为独立 spec；原因是 `#ey4ar` 已明确声明“不增加侧栏级第二套服务详情子导航”。
- 2026-07-09：完成 `AppShell` 详情页壳层扩展、`DetailRouteServiceTree` 树导航、`StackDetailPage` / `ServiceDetailPage` 新工作区布局，以及移动端底部主导航 + 服务树抽屉接入。
- 2026-07-09：在同步 `origin/main` 的 rebase 中保留主线离线只读 / snapshot 合同，并把详情页服务树与视觉层重放到最新主线之上。
- 2026-07-09：fresh merge-proof 指出 `StackDetailPage` 未把主导航 / 移动端底部导航映射为 “服务” active；已修正 `AppShell` active 路由映射、补上 `StackDetailPage` story 断言，并刷新受影响的桌面视觉证据。
- 2026-07-09：fresh merge-proof 继续指出 `961px - 1160px` 断点会让详情页三列壳层错误回退为两列；已为 `.appShellWithDetailSidebar` 补齐窄桌面媒体规则，并刷新最终桌面 / 移动端视觉证据。
- 2026-07-09：fresh merge-proof 继续指出归档 Stack / Service 详情路由不会出现在详情树里；已将 `listStacksArchived("only")` 并入服务树读模型，并补上归档详情 story 回归覆盖。
- 2026-07-09：fresh merge-proof 继续指出详情树会为全部 Stack 并发请求 `getStack()`；已改为按当前/展开 Stack 懒加载详情，并用 Storybook mock debug 断言不预取无关 Stack detail。
- 2026-07-09：fresh merge-proof 继续指出懒加载 effect 会因自身 `loading` 状态写回而自取消；已改为用 in-flight 集合与仅卸载失效的守卫收口请求，避免展开后长期停在“加载服务列表…”。
- 2026-07-09：fresh merge-proof 继续指出详情路由会同时挂载桌面树与隐藏的移动树；已把抽屉服务树改为仅在移动抽屉打开时挂载，消除重复导航请求。
- 2026-08-01：Stack 详情服务读模型增加 `lifecycleState`，服务树左侧运行态点与版本更新信号分离；定向刷新事件和可见页 30 秒轮询补齐外部变化，桌面与移动叶子缩进同步压缩。
- 2026-08-01：移动服务抽屉移除箭头所指的抽屉头重复标题，保留树内容区域标题，并让服务树列表填充剩余高度、独立承载溢出滚动，最近扫描保持在底部。
- 2026-09-04：修正移动端品牌规则误把 Stack 详情裁切为图标的问题；Shell 现在按详情路由类型隔离品牌样式，Stack 维持完整字标，Service 维持紧凑图标，并补齐 320px / 960px 真实视口回归。
