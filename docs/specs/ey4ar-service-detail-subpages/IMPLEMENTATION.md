# Dockrev：服务详情页七子页信息架构升级 实现状态（#ey4ar）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现，待 review / PR 收口
- Lifecycle: active
- Catalog note: fast-track（service detail route-backed overview/history/monitoring/backup/logs/settings subpages）

## Coverage / rollout summary

- 已扩展前端服务详情路由，支持 `overview / history / monitoring / backup / logs / settings` 六子页 section 语义，且旧 canonical URL 继续指向概览。
- 已将服务详情页重构为共享 shell + section 视图，保留统一的 hero、banner、异常提示、全局反馈与高频顶部动作。
- 已将 `ServiceResourcePanel` 迁移到 `监控` 子页，并将自动更新、Compose、服务保护、忽略规则、Webhook 与维护动作集中到 `设置` 子页。
- 已将服务级备份摘要、备份设置入口与当前服务相关备份记录迁移到 `备份` 子页，并从 `设置` 子页移除重复备份入口。
- 七个服务详情子页现已共用精简状态摘要：只保留状态、当前版本、目标版本与版本跨度；digest、raw tag、架构、规则与原因明细已从共享 banner 摘要中移除。
- `版本` 子页已重构为桌面双虚拟列表：宽屏显示 `220px` 左目录和右侧版本卡流，目录时间标签按 7 天阈值在相对时间与绝对日期之间切换，目录与正文共享分页数据并互相联动定位。
- `版本` 子页页头已移除仓库/来源/版本 chips，改为仓库级 GitHub 与 OctoRill Releases 图标入口；OctoRill 入口只在后端/Mock 返回可信外链时展示。
- `版本` 子页桌面可执行卡片已固定 `19rem` 右侧状态/动作轨道，移动端 `≤1100px` 隐藏版本目录并保持单列卡片流与无横向溢出。
- 已新增 `更新记录` 子页：复用全量 jobs 读模型，基于 serviceId 与 summary targets 关联当前服务的 update/rollback 任务；stack-scope 任务也必须在 targets 中明确命中当前服务，避免混入同 Stack 的其他服务。记录按完成、开始、创建时间倒序显示，并支持 click、Enter、Space 直达任务详情。活动 Tab 已表达当前 section，内容区不重复标题、说明或记录数量。记录列严格收敛为“操作与补充结果摘要”及 Job ID 两行，摘要单行截断，且抑制“更新完成”“回滚完成”“任务执行失败”等已由类型或状态表达的泛化文案。可可靠解析目标 tag 的记录提供更新日志图标，复用 App 级右侧 release drawer、URL 状态与版本定位；无可靠 tag 时不显示入口。匹配记录超过 20 条时只挂载当前页行，并提供带页码状态的上一页/下一页箭头；切换服务或数据刷新导致页码越界时回退到有效页。更新记录仅保留外层 section card，表格通过表头和行分隔线组织，不嵌套圆角容器。当前可回滚目标的来源成功更新行显示受控回滚入口，复用服务级确认与后端并发保护；已回滚状态以独立琥珀色表达，失败任务弱化非状态列，但失败 Badge 保持完整对比度且仍可聚焦与跳转。
- 已固定更新记录桌面端的六列共享列模板，避免带更新日志/回滚按钮的行单独撑宽 `操作` 列并连带压缩其它列；Storybook `play` 断言与 Playwright 交互校验现在都会检查各行列框保持一致。
- `更新记录` 仅在激活且在线时订阅全局 jobs SSE；事件按 250ms 去抖刷新，连续三次错误后按队列同款 10 秒轮询与 3 秒重连降级，切离 section 或卸载时清理资源。
- 只读快照继续缓存 jobs；离线 history 只回放 60 秒 fresh snapshot，日志和设置仍要求联网。
- 已新增服务级日志 snapshot + SSE 合同、`ServiceLogHub` 共享缓冲、`service_log_reset` 断线补偿语义，以及前端 `ServiceLogsPanel` 的虚拟滚动、搜索、自动换行开关与吸底交互。
- 日志实现语义已收敛为“单服务日志流”，不再在产品接口或界面上暴露容器聚合模型。
- 日志解析已按 Dozzle-like grouped log 语义保留 Docker timestamp 元信息，并将应用输出中的空行、缩进行、`Caused by:` 等 continuation 合并进同一逻辑日志记录；未结构化的 inline tracing 行仍由前端避免重复渲染等级文本。
- 服务日志 API 已在 `ServiceLogLine` 上提供可选结构化 `meta`，支持 `json / logfmt / text` 归一化；其中 Rust `tracing` 默认文本输出会在 text meta 中提取应用级 `level/message/timestamp` 与 `key=value` attributes，前端默认 Human 视图渲染结构化摘要与 metadata chips，并保留 Raw 视图用于查看原始输出。
- 服务日志采集同时消费 `docker logs` 的 stdout 与 stderr stream；snapshot 与 SSE live tail 均覆盖仅向 stderr 写日志的容器。
- 已更新 `PageHarness`、服务树 section 标签与服务详情 Storybook stories，补齐旧链接默认概览、tabs route 切换、更新记录深链/混合列表/空态/click-Enter-Space 跳转/受控回滚入口、备份页状态、日志深链与搜索交互、Human/Raw 日志切换、设置抽屉入口与监控页稳定渲染。
- 已补齐版本页 Storybook `play` 覆盖：目录/正文双虚拟化、当前版本初始居中、目录点击联动、尾部分页、仓库级图标入口、固定动作栏与移动端无目录/无横向溢出。
- 已产出 owner-facing mock-only 视觉证据并写回 `SPEC.md`，其中版本页最终验收图来自 `ui_demo` 的桌面与 `390x900` 移动端截图。
- 服务详情页面与 stories 已收敛到仓库 1200 行文件预算以内，并通过 CI 同款预算检查。

## Remaining Gaps

- 待推进 PR / CI / merge 收口。

## Related Changes

- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/components/RecentUpdateRecords.tsx`
- `web/src/components/ServiceVersionCard.tsx`
- `web/src/components/ServiceVersionsSection.tsx`
- `web/src/components/serviceVersionsSectionUtils.ts`
- `web/src/pages/useServiceDetailPageState.tsx`
- `web/src/pages/useServiceLogsState.ts`
- `web/src/components/ServiceLogsPanel.tsx`
- `web/src/stories/mocks/PageHarness.tsx`
- `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- `web/src/stories/pages/serviceDetailVersionsStories.tsx`
- `web/src/App.css`
- `crates/dockrev-api/src/api/services.rs`
- `crates/dockrev-api/src/api/types/service_logs.rs`
- `crates/dockrev-api/src/service_logs.rs`

## References

- `./SPEC.md`
- `./HISTORY.md`
