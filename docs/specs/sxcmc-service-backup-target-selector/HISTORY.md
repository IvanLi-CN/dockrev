# Dockrev：服务保护设置补全备份目标直选与备份说明 演进历史（#sxcmc）

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-08-15: 备份存储改为部署派生并加入 Docker mount 解析；归档改为原子流式 zstd，更新编排以最小停机为优先，并加入实时备份进度。
- 2026-06-28: 创建 topic-level spec，锁定备份设置抽屉内直接选择 `Volumes` / `Bind paths` 与只读备份说明的范围。
- 2026-06-28: 新增专用 `GET/PUT /api/services/{service_id}/backup-targets`，把 compose 候选发现与服务级选择语义从通用 settings API 中拆开。
- 2026-06-28: 服务端收敛为“共享 target 取消只记当前服务 `skip`，独占 target 取消才真正移除 stack target”的保守引用语义。
- 2026-06-28: 备份设置抽屉完成 volumes/bind paths 直选、共享提示、空态文案、只读备份说明和 Storybook mock 覆盖。
- 2026-06-28: 服务级备份选择从 `selected + inherit|force|skip` 三态收敛为面向操作者的三策略：`不备份`、`停机备份`、`在线备份`。
- 2026-06-28: 备份设置抽屉的策略控件收敛为单轨 segmented button group，用水平滑块表达当前策略，解决长文案与独立按钮高亮的歧义。
- 2026-06-29: 新增 `GET /api/services/{service_id}/backup-records`，并把服务级备份摘要/记录从 `设置` 页迁到独立 `备份` 子页。
- 2026-07-08: 收紧 `GET /api/services/{service_id}/backup-records` 契约，只保留真正产生过备份产物的记录，并把备份页文案改为“实际备份记录”语义；无产物的 `skipped` / `failed` 尝试与 skipped target 明细都不再展示。

## Key Reasons / Replacements

- 备份目标选择改为专用服务级接口，原因是 `PUT /api/services/{id}/settings` 不应隐式承担 stack 级 `backup.targets` 的跨层级写入副作用。
- 候选来源固定为 compose 声明，原因是 compose 是当前系统里最稳定、可重复、无需依赖运行时状态的挂载真相源。
- 共享 target 采用保守不删语义，原因是一个服务取消选择不应误删其他服务仍依赖的 stack 级备份目标。
- 本轮只做只读备份说明，不扩 retention 编辑，原因是用户眼下的核心缺口是“能否直选 + 现在到底怎么保存”，不是新增一整套备份策略编辑器。
- UI 文案与交互从工程内部的 `inherit/force/skip` 抽象改写为操作者可直接理解的三策略，原因是用户关心的是“备不备份、要不要协调停机”，不是配置层的投影细节。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
