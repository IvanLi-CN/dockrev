# Dockrev：服务端自主自检与无阻塞网页入口主题历史

> 这里记录主题局部生命周期、兼容背景与决策关联；当前行为合同以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- 本主题承接既有部署检查报告、管理事件和诊断页，重新划分服务端调度、网页读取和操作影响三个边界。
- 既有 GET/POST 路径、`pending`/`ready` 等字段和旧报告 JSON 保持可读；新增启动归属、修订号、启动状态、故障键和影响投影为兼容扩展。
- 旧的 `neverAutoOpen` 数据与接口保留兼容，但不再作为全站门禁或新路由自动展示的决定因素。

## Decisions

- 服务端启动后主动执行首轮自检，网页访问只消费结果；服务端每 300 秒复查并合并相关变化。
- 入口首读没有可用当前启动结论时正常进入目标页面；晚到的启动故障、后台新故障和恢复均保留当前页面。
- 仅当前启动已确认的核心故障具有一次自动展示资格；可选功能故障局部提示，操作限制按能力和对象范围投影。
- 跨标签页自动展示使用按部署路径和启动故障键隔离的 IndexedDB 原子认领。

## Related Changes

- ADR 0016 records the server-owned lifecycle and no-blocking browser contract.
- The EdgeOne timeout topic retains cleanup snapshot, release drawer and SSE constraints, while its obsolete deploy-check gate wording now points here.
- Owner-approved Storybook mock-only evidence now records healthy entry rendering, scoped core failure diagnosis and the 393x852 mobile layout under this topic's canonical assets.
- Review hardening wires operation impacts into overview, batch-update, homepage, detail, context-menu, cleanup and version-history controls, preserves project/service object IDs in backend projections, and acknowledges recovery only after the report and event are durable.
- The interactive app stories include a concurrent IndexedDB auto-open claim check so same-origin tabs have executable one-winner evidence.
- Auto-open claims are keyed by origin and application base path rather than the current route, so root and nested pages share the same one-time claim.
- The final compatibility cleanup extracted deploy-check API types and service-detail snapshot helpers while preserving the existing public re-exports and runtime behavior; frontend file-budget validation is part of the delivery evidence.

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `../../adr/0016-server-owned-self-checks.md`
