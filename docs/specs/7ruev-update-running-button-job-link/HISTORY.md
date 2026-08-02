# Dockrev：更新进行中按钮可点击直达任务详情 演进记录（#7ruev）

## Decisions

- 服务级活动任务由 lifecycle status 的 `activeJob.type` 统一判定 owner；rollback-target 的活动任务字段不再单独驱动回滚文案。
- 更新活动态优先于候选存在性，避免更新结算期间候选刷新消失造成错误的回滚主动作。
- 桌面端以动作组为禁用边界，移动端保留统一菜单并只禁用非 owner 项，分别维持 Tooltip 与 Toast 的既有反馈模式。
