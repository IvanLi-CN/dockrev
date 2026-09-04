# Dockrev 统一页面内导航

> 当前有效规范以本文为准；实现覆盖与验证事实见 `./IMPLEMENTATION.md`，决策演进见 `./HISTORY.md`。

## Context and Scope

- Dockrev 将一级页面入口与详情服务树拆成多个桌面侧栏，造成宽度、折叠状态和移动抽屉语义不一致。
- In scope: AppShell 单栏壳层、Overview/Queue/Services/Stack/Service/Cleanup/Settings 的页面内导航、PageHarness、Storybook 和 mock-only 视觉证据。
- Out of scope: 后端 API、数据模型、一级页面路由、Docker/任务/鉴权/备份/日志/监控/GHCR 业务合同，以及真实登录态截图。

## Terms and Interfaces

- `page navigation`: 固定的五个一级页面入口；切换工作区，不依赖 Stack、Service、任务或设置区块。
- `context navigation`: 当前页面拥有的第二层目录，只消费该页面已经加载的读模型。
- `mobile context drawer`: 窄屏顶部临时抽屉；与底部一级导航互补，不合并为一个菜单。
- `service directory`: 可搜索、多展开的 Stack→Service 树；服务详情 section 仍由 URL 保持。
- `cleanup view filter`: 只改变清理页展示投影，不进入扫描或 `CleanupApplyRequest`。

## Requirements

### REQ-UPCN-001

- AppShell MUST 在桌面页头渲染 Logo，并只渲染一个侧栏；侧栏依次包含五个横向一级图标、页面内导航和固定底部元信息，不得重复放置 Logo。页面内导航 MUST 独立滚动；不得提供桌面折叠控件或折叠持久化。

### REQ-UPCN-002

- 窄屏 MUST 保留页头 Logo 和底部一级导航；顶部抽屉 MUST 在顶部额外显示一份 Logo，并只挂载当前页面内导航的一份实例，且不得与一级导航合并或同时挂载桌面侧栏实例。

### REQ-UPCN-003

- Overview 页面内导航 MUST 展示已加载分组并定位/高亮可见分组；Queue 及其子路由 MUST 展示版本推测、GHCR Webhook、活动任务和最近五条终态任务；Services、Stack、Service MUST 共用可搜索、多展开的 Stack→Service 树；Settings 桌面定位区块、移动端进入既有子路由。

### REQ-UPCN-004

- Cleanup 的范围和资源类型筛选 MUST 只过滤页面视图，不改变扫描请求、`CleanupApplyRequest`、确认语义或实际执行范围。

### REQ-UPCN-005

- 既有 URL、服务详情局部 tabs、离线快照、管理事件刷新和可见性轮询语义 MUST 保持不变；页面内导航只能组合已有读模型，不得新增后端导航接口。

### REQ-UPCN-006

- 页面内导航控件 MUST 暴露当前态、键盘焦点和窄屏可读标签；桌面和移动布局 MUST 不出现文本溢出或控件重叠。

### REQ-UPCN-007

- 同一页面的桌面和移动页面内导航 SHOULD 复用同一份数据和交互模型，只更换呈现容器。

## Verification

### VER-UPCN-001

- Method: `Layouts/AppShell` Storybook canvas 与交互测试。
- covers: `REQ-UPCN-001`, `REQ-UPCN-002`
- Pass condition: 桌面只有一个侧栏且顺序、独立滚动、固定底部和无折叠入口成立；移动只挂载底部一级导航和当前 context drawer。

### VER-UPCN-002

- Method: `page-context-navigation` 单测及 Overview、Queue、Services、Cleanup、Settings 页面 stories。
- covers: `REQ-UPCN-003`, `REQ-UPCN-004`, `REQ-UPCN-006`, `REQ-UPCN-007`
- Pass condition: 五类页面目录行为、队列五条终态上限、服务树搜索/多展开、清理纯视图筛选、设置双端行为和可访问当前态均通过。

### VER-UPCN-003

- Method: `routesContract`、`cleanup-page-model`、`management-polling-guard` 以及现有页面回归测试。
- covers: `REQ-UPCN-004`, `REQ-UPCN-005`
- Pass condition: 正式路由和服务 section 可解析，清理执行请求不接收视图筛选参数，快照/刷新/轮询语义不回退。

### VER-UPCN-004

- Method: mock-only `ui_demo` 桌面/移动场景与 Storybook `DetailRouteServiceTree` canvas，配合主人确认的视觉比较。
- covers: `REQ-UPCN-001`, `REQ-UPCN-002`, `REQ-UPCN-006`
- Pass condition: 无登录信息、无重叠或溢出，截图差异有明确确认记录。

## Related ADRs

None

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- `../../2jhm2-app-shell-sidebar-collapse-icons/SPEC.md`（superseded）
- `../../c2r2u-detail-route-service-navigation/SPEC.md`（superseded）
- `../../../CONTEXT.md` 的 Navigation Model
