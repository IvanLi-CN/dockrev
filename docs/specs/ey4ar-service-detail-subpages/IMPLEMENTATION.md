# Dockrev：服务详情页五子页信息架构升级 实现状态（#ey4ar）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现，待 review / PR 收口
- Lifecycle: active
- Catalog note: fast-track（service detail route-backed overview/monitoring/backup/logs/settings subpages）

## Coverage / rollout summary

- 已扩展前端服务详情路由，支持 `overview / monitoring / backup / logs / settings` 五子页 section 语义，且旧 canonical URL 继续指向概览。
- 已将服务详情页重构为共享 shell + section 视图，保留统一的 hero、banner、异常提示、全局反馈与高频顶部动作。
- 已将 `ServiceResourcePanel` 迁移到 `监控` 子页，并将自动更新、Compose、服务保护、忽略规则、Webhook 与维护动作集中到 `设置` 子页。
- 已将服务级备份摘要、备份设置入口与当前服务相关备份记录迁移到 `备份` 子页，并从 `设置` 子页移除重复备份入口。
- 已新增服务级日志 snapshot + SSE 合同、`ServiceLogHub` 共享缓冲、`service_log_reset` 断线补偿语义，以及前端 `ServiceLogsPanel` 的虚拟滚动、搜索、自动换行开关与吸底交互。
- 日志实现语义已收敛为“单服务日志流”，不再在产品接口或界面上暴露容器聚合模型。
- 日志解析已按 Dozzle-like grouped log 语义保留 Docker timestamp 元信息，并将应用输出中的空行、缩进行、`Caused by:` 等 continuation 合并进同一逻辑日志记录；正文自带 tracing 时间与等级时，前端等级列不再重复显示等价级别文本。
- 服务日志采集同时消费 `docker logs` 的 stdout 与 stderr stream；snapshot 与 SSE live tail 均覆盖仅向 stderr 写日志的容器。
- 已更新 `PageHarness` 与服务详情 Storybook stories，补齐旧链接默认概览、tabs route 切换、备份页状态、日志深链与搜索交互、设置抽屉入口与监控页稳定渲染。
- 已产出 owner-facing mock-only 视觉证据并写回 `SPEC.md`。

## Remaining Gaps

- 待推进 PR / CI / merge 收口。

## Related Changes

- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/useServiceDetailPageState.tsx`
- `web/src/pages/useServiceLogsState.ts`
- `web/src/components/ServiceLogsPanel.tsx`
- `web/src/stories/mocks/PageHarness.tsx`
- `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- `web/src/App.css`
- `crates/dockrev-api/src/api/services.rs`
- `crates/dockrev-api/src/api/types/service_logs.rs`
- `crates/dockrev-api/src/service_logs.rs`

## References

- `./SPEC.md`
- `./HISTORY.md`
