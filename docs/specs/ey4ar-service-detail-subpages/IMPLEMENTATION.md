# Dockrev：服务详情页四子页信息架构升级 实现状态（#ey4ar）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现，待 review / PR 收口
- Lifecycle: active
- Catalog note: fast-track（service detail route-backed overview/monitoring/backup/settings subpages）

## Coverage / rollout summary

- 已扩展前端服务详情路由，支持 `overview / monitoring / backup / settings` 四子页 section 语义，且旧 canonical URL 继续指向概览。
- 已将服务详情页重构为共享 shell + section 视图，保留统一的 hero、banner、异常提示、全局反馈与高频顶部动作。
- 已将 `ServiceResourcePanel` 迁移到 `监控` 子页，并将自动更新、Compose、服务保护、忽略规则、Webhook 与维护动作集中到 `设置` 子页。
- 已将服务级备份摘要、备份设置入口与当前服务相关备份记录迁移到 `备份` 子页，并从 `设置` 子页移除重复备份入口。
- 已更新 `PageHarness` 与服务详情 Storybook stories，补齐旧链接默认概览、tabs route 切换、备份页状态、设置抽屉入口与监控页稳定渲染。
- 已产出 owner-facing mock-only 视觉证据并写回 `SPEC.md`。

## Remaining Gaps

- 待推进 PR / CI / merge 收口。

## Related Changes

- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/useServiceDetailPageState.tsx`
- `web/src/stories/mocks/PageHarness.tsx`
- `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- `web/src/App.css`

## References

- `./SPEC.md`
- `./HISTORY.md`
