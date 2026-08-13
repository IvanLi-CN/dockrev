# Dockrev：GHCR Webhook 注册维护专页 实现状态（#dk4dd）

## Current Status

- Implementation: 已实现
- Lifecycle: active，待 PR 收敛

## Coverage

- 后端仓库列表将 `selected`、`webhookState` 与四字段搜索归入同一 SQL 条件构造，计数和分页查询复用该条件。API 测试覆盖状态筛选、兼容值、非法参数、搜索字段、分页与页大小上限。
- GHCR 维护页以当前页为唯一仓库行来源，默认每页 50 条，可选 25、50、100 条。筛选、搜索和页大小更新会回到第一页；响应显示的总数若使当前页失效，页面回退到最后有效页。
- 刷新同时读取 Webhook overview、当前仓库页以及有界活跃 GHCR 任务。队列与 SSE 刷新经单飞队列和请求代数协调，旧页响应无法覆盖新页。
- 维护页的 204 条 mock 数据覆盖分页、扩展搜索、状态筛选和分页请求边界；服务日志大缓冲 Storybook 场景断言所有测量行都有有效索引。
- `ServiceLogsPanel` 的测量元素使用 `data-index={item.index}`，满足 TanStack Virtual 的测量合同。

## References

- `./SPEC.md`
- `./HISTORY.md`
