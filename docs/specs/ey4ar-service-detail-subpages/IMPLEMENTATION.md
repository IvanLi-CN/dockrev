# Dockrev：服务详情页七子页信息架构升级 实现状态（#ey4ar）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现，待 review / PR 收口
- Lifecycle: active
- Catalog note: fast-track（service detail route-backed overview/versions/history/monitoring/logs/backup/settings subpages）

## Coverage / rollout summary

- 已扩展前端服务详情路由，支持 `overview / versions / history / monitoring / logs / backup / settings` 七子页 section 语义，且旧 canonical URL 继续指向概览。
- 已将服务详情页重构为共享 shell + section 视图，移除页面内容区内冗余的标题说明块，改为保留一条 `服务名 + 监控指标` 行、单行状态信息带、异常提示、全局反馈与高频顶部动作。
- 已新增 `版本` 子页：首屏改为 locate-first，先调用统一 `release-notes/locate` 拿到当前部署版本附近的锚点窗口；命中时把当前卡片滚动到视口中心，未命中时回到最新窗口首屏并显示 warning banner，不再通过前端线性翻页扫描整段历史。
- 已将 `ServiceResourcePanel` 迁移到 `监控` 子页，并将自动更新、Compose、服务保护、忽略规则、Webhook 与维护动作集中到 `设置` 子页。
- 已将服务级备份摘要、备份设置入口与当前服务相关备份记录迁移到 `备份` 子页，并从 `设置` 子页移除重复备份入口。
- 七个服务详情子页现已共用 tabs 上方的双层页头：第一行展示服务名与 `CPU / 内存 / 磁盘读 / 磁盘写 / 下载 / 上传` 六项监控指标，并将这些指标的可见 label 收敛为图标前缀；独立的样本时间 / 状态 chip 已删除。窄屏时该指标区会切换为 `2 x 3` 网格，并按 `CPU / 内存`、`磁盘读 / 磁盘写`、`下载 / 上传` 成列配对。第二行保留镜像简述、状态、当前版本、目标版本与版本跨度；digest、raw tag、架构、规则与原因明细已从共享摘要中移除，绿色状态信息带里也不再重复服务名、Stack pill 或解释性副标题，旧的右侧状态卡与 header meta cards 已删除。
- `Image Ref / Service ID / Stack ID` 已从共享页头下沉到 `概览` 子页的一张 `服务标识` 卡，其余子页不再重复暴露这些技术标识字段。
- `版本` 子页已重构为桌面双虚拟列表：宽屏显示 `220px` 左目录和右侧版本卡流，目录时间标签按 7 天阈值在相对时间与绝对日期之间切换，目录与正文共享分页数据并互相联动定位。
- `版本` 子页页头已移除仓库/来源/版本 chips，改为仓库级 GitHub 与 OctoRill Releases 图标入口；OctoRill 入口只在后端/Mock 返回可信外链时展示。
- `版本` 子页桌面可执行卡片已固定 `19rem` 右侧状态/动作轨道，移动端 `≤1100px` 隐藏版本目录并保持单列卡片流与无横向溢出。
- 已新增 `更新记录` 子页：复用全量 jobs 读模型，基于 serviceId 与 summary targets 关联当前服务的 update/rollback 任务；stack-scope 任务也必须在 targets 中明确命中当前服务，避免混入同 Stack 的其他服务。记录按完成、开始、创建时间倒序显示，并支持 click、Enter、Space 直达任务详情。活动 Tab 已表达当前 section，内容区不重复标题、说明或记录数量。记录列严格收敛为“操作与补充结果摘要”及 Job ID 两行，摘要单行截断，且抑制“更新完成”“回滚完成”“任务执行失败”等已由类型或状态表达的泛化文案。可可靠解析目标 tag 的记录提供更新日志图标，复用 App 级右侧 release drawer、URL 状态与版本定位；无可靠 tag 时不显示入口。匹配记录超过 20 条时只挂载当前页行，并提供带页码状态的上一页/下一页箭头；切换服务或数据刷新导致页码越界时回退到有效页。更新记录仅保留外层 section card，表格通过表头和行分隔线组织，不嵌套圆角容器。当前可回滚目标的来源成功更新行显示受控回滚入口，复用服务级确认与后端并发保护；已回滚状态以独立琥珀色表达，失败任务弱化非状态列，但失败 Badge 保持完整对比度且仍可聚焦与跳转。
- 已固定更新记录桌面端的六列共享列模板，避免带更新日志/回滚按钮的行单独撑宽 `操作` 列并连带压缩其它列；Storybook `play` 断言与 Playwright 交互校验现在都会检查各行列框保持一致。
- 已把 Dockrev 服务详情顶部 `升级 Dockrev` 与 `版本` 子页 candidate 卡收敛到同一份 supervisor 自我升级动作描述：candidate 卡点击只进入 `/supervisor/`，不再误发普通 service update；更高但非 candidate 的 Dockrev release 卡保留禁用动作位和 candidate-only 解释，supervisor offline 时顶部与 candidate 卡同步禁用，重试仅保留在顶部。
- `更新记录` 仅在激活且在线时订阅全局 jobs SSE；事件按 250ms 去抖刷新，连续三次错误后按队列同款 10 秒轮询与 3 秒重连降级，切离 section 或卸载时清理资源。
- 只读快照继续缓存 jobs；离线 history 只回放 60 秒 fresh snapshot，日志和设置仍要求联网。
- 已新增服务级日志 snapshot + SSE 合同、`ServiceLogHub` 共享缓冲、`service_log_reset` 断线补偿语义，以及前端 `ServiceLogsPanel` 的虚拟滚动、搜索、自动换行开关与吸底交互。
- 日志自动跟随现以列表长度与末条日志 ID 共同驱动，并在虚拟列表完成新尾行测量后贴底；普通追加和 2,000 行满缓冲淘汰最旧行后替换末条的场景都保持跟随。Storybook mock 与交互校验会在用户上滚、点击“跳到最新”后延迟注入多行 SSE 尾日志，验证两条路径的真实滚动位置。
- 日志实现语义已收敛为“单服务日志流”，不再在产品接口或界面上暴露容器聚合模型。
- 日志解析已按 Dozzle-like grouped log 语义保留 Docker timestamp 元信息，并将应用输出中的空行、缩进行、`Caused by:` 等 continuation 合并进同一逻辑日志记录；未结构化的 inline tracing 行仍由前端避免重复渲染等级文本。
- 服务日志 API 已在 `ServiceLogLine` 上提供可选结构化 `meta`，支持 `json / logfmt / text` 归一化；其中 Rust `tracing` 默认文本输出会在 text meta 中提取应用级 `level/message/timestamp` 与 `key=value` attributes，前端默认 Human 视图渲染结构化摘要与 metadata chips，并保留 Raw 视图用于查看原始输出。
- 服务日志终端已收敛为局部主题令牌：亮色模式使用完整浅色终端，并为表头、正文、时间、元数据、等级、悬浮与 ANSI 前景色提供独立值；暗色模式维持既有终端外观。Storybook 的亮色场景与交互测试会在 Human / Raw 两种模式下校验计算后的文字对比度。
- 日志时间块已固定为时间第一行、日期第二行；桌面表头与正文行通过日志终端共享的 `18px` 水平边距令牌对齐，不再依赖 OverlayScrollbars 内部 viewport 节点，时间轨道为 `128px`。窄屏使用 `14px` 水平边距并隐藏终端表头，日志行改为时间块/等级首行与正文跨列次行，不再使用固定时间列轨道。桌面、`393x852` 移动端、Human/Raw 与 UTC 切换均由 Storybook 与浏览器断言覆盖。
- 时间列布局证据使用独立的桌面与移动端 render story，避免继承会改变筛选模式的异步 play；跟随最新回归等待可见跳转按钮后在页面轮询中触发点击，确保虚拟列表重排时仍验证真实吸底行为。
- 服务日志采集同时消费 `docker logs` 的 stdout 与 stderr stream；snapshot 与 SSE live tail 均覆盖仅向 stderr 写日志的容器。
- 已更新 `PageHarness`、服务树 section 标签与服务详情 Storybook stories，补齐旧链接默认概览、tabs route 切换、更新记录深链/混合列表/空态/click-Enter-Space 跳转/受控回滚入口、备份页状态、日志深链与搜索交互、Human/Raw 日志切换、设置抽屉入口与监控页稳定渲染。
- 已补齐版本页 Storybook `play` 覆盖：目录/正文双虚拟化、当前版本初始居中、目录点击联动、尾部分页、仓库级图标入口、固定动作栏与移动端无目录/无横向溢出。
- 服务更新与回滚的提交、排队、执行阶段已统一映射到共享状态信息带、桌面候选目录 chip 与候选卡动作；活动任务使用主题蓝色和 reduced-motion 兼容的加载图标，版本页不再渲染独立活动任务横幅，Job 建立后候选卡可直接进入任务详情。
- 更新任务跟踪会消费 jobs 管理事件中的 `queued / running` 状态变化，并在首次登记 Job 后立即读取一次任务快照，既恢复执行阶段实时同步，也关闭 POST 响应晚于 `running` 事件时的订阅时序窗口；REST 快照与 SSE 状态按 `queued -> running` 单调合并，延迟返回的旧快照不能把执行中任务降回排队态。
- 版本页的当前 rollback target 现会复用统一的备份摘要聚合：若其来源更新任务存在 included backup assets，则在版本卡右栏与服务级回滚确认中同时显示 `来源备份 = <目标数> · <总体积>`；缺失体积时回退为 `· --`，无实际纳入记录时不渲染该状态块。
- 已产出 owner-facing mock-only 视觉证据并写回 `SPEC.md`，其中版本页最终验收图来自 `ui_demo` 的桌面与 `390x900` 移动端截图。
- 已补齐 Dockrev 版本页自我升级回归 stories 与 mock：覆盖 candidate 卡跳 supervisor、非 candidate 卡禁用解释、supervisor offline 三态，并把新增 owner-facing 视觉证据写回 `SPEC.md`。
- 已产出 owner-facing mock-only 视觉证据并写回 `SPEC.md`。
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
- `web/src/updateActionTracking.ts`
- `web/tests/updateActionTracking.test.ts`
- `web/src/pages/useServiceLogsState.ts`
- `web/src/components/ServiceLogsPanel.tsx`
- `web/scripts/test-storybook.mjs`
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
