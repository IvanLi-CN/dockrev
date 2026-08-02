# Dockrev：更新进行中按钮可点击直达任务详情 实现状态（#7ruev）

## Current Status

- Implementation: 已实现
- Lifecycle: active

## Coverage

- 服务详情页从 `lifecycle-status.activeJob.type` 解析服务级操作 owner，更新任务在候选消失前后都保持更新进度优先。
- 非 owner 动作组在桌面端由 split button 组级禁用并显示原因 Tooltip，在移动端保持菜单可发现但禁用具体项并沿用 Toast。
- 组件、服务详情页和 mock-only public demo 均有稳定 Storybook/play 覆盖。

## References

- `./SPEC.md`
- `./HISTORY.md`
- `../9cq2a-service-lifecycle-actions/SPEC.md`
