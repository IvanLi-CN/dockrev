# 规格（Spec）总览

本目录用于管理工作项的**规格与追踪**：记录范围、验收标准、任务清单与状态，作为交付依据；实现与验证应以对应 `SPEC.md` 为准。

> Legacy compatibility: historical entries in `docs/plan/**/PLAN.md` are kept as references. New entries must be created under `docs/specs/**/SPEC.md`.

## 快速新增一个规格

1. 生成新的规格 `ID`（推荐 5 个字符 nanoId 风格）。
2. 新建目录：`docs/specs/<id>-<title>/`。
3. 在目录下创建 `SPEC.md`。
4. 在下方 Index 表新增一行，并更新 `Status` 与 `Last`。

## 状态（Status）说明

仅允许以下状态值：

- `待设计`
- `待实现`
- `跳过`
- `部分完成（x/y）`
- `已完成`
- `作废`
- `重新设计（#<id>）`

## Index（固定表格）

| ID   | Title | Status | Spec | Last | Notes |
|-----:|-------|--------|------|------|-------|
| 2hnkx | Dockrev：更新候选跨版本发现次数标记 | 已完成 | `2hnkx-new-version-discovery-count/SPEC.md` | 2026-03-20 | fast-track（PR #170 merged；2026-03-20 follow-up fixes visible-version counting、backfill semantics、unresolved-history normalization、and DB/API stack-path parity） |
| 745rw | Dockrev：标签页恢复后自动补拉更新候选 | 已完成 | `745rw-resume-refresh-update-candidates/SPEC.md` | 2026-03-15 | fast-track（resume refresh hook + review fixes + local browser smoke passed on Overview / Services / ServiceDetail） |
| kv9pg | Dockrev：quality-gates 最终版对齐（merge queue + 条件 review + required checks） | 进行中 | `kv9pg-quality-gates-final-alignment/SPEC.md` | 2026-03-11 | fast-track |
| s4fqf | Dockrev：新版本通知事件驱动收敛 + 显式版本结果兜底 | 已完成 | `s4fqf-new-version-notify-event-settle-explicit-version/SPEC.md` | 2026-03-12 | fast-track |
| np5fm | Dockrev：Supervisor 自我升级页低拥挤度重构 | 已完成 | `np5fm-supervisor-roomier-layout/SPEC.md` | 2026-03-12 | fast-track（PR #160；visual evidence + copy UX + review fixes） |
| 99egq | Dockrev：显式 tag 驱动的 Update 契约 | 已完成 | `99egq-explicit-update-tag-contract/SPEC.md` | 2026-03-11 | fast-track（撤销 #162 的 semver raw fallback；统一 explicit targetTag + pullTags） |
| xyma9 | Dockrev：非 service update 的 semver pull 保留 OCI 原始 tag 并回退无 v 变体 | 重新设计（#99egq） | `xyma9-semver-pull-raw-tag-fallback/SPEC.md` | 2026-03-11 | superseded by explicit target contract |
| ufxaq | Dockrev：Update Job Docker 凭据透传桥接 | 已完成 | `ufxaq-update-job-docker-auth-bridge/SPEC.md` | 2026-03-10 | fast-track（update job Docker auth bridge + private registry docs sync） |
| 6epk6 | Dockrev：Popover 局部刷新边界收敛 + Snapshot 并发调整 | 已完成 | `6epk6-popover-local-refresh-snapshot-concurrency/SPEC.md` | 2026-03-10 | fast-track |
| bvxtm | Dockrev：Supervisor 暗色主题修复 + 同源主题偏好共享 | 已完成 | `bvxtm-supervisor-dark-theme-same-origin-share/SPEC.md` | 2026-03-09 | fast-track（PR #156；cargo test + browser smoke） |
| qh4zx | Dockrev：新版本通知等待解析收敛 + 单服务正文瘦身 | 已完成 | `qh4zx-new-version-notify-settle-and-copy/SPEC.md` | 2026-03-09 | fast-track（PR #159） |
| 4n5vr | Dockrev：新版本通知记录表去重 + 通知内版本号展示 | 已完成 | `4n5vr-new-version-notification-records/SPEC.md` | 2026-03-09 | fast-track（notification record table + display tag version copy + review fixes + main sync） |
| mmffn | Dockrev：聚合更新自升级保护 | 已完成 | `mmffn-dockrev-aggregate-self-upgrade-guard/SPEC.md` | 2026-03-09 | fast-track（aggregate guard: dockrev 改走 supervisor，自身不参与 all/stack update；cargo/web/storybook 回归通过） |
| e3f83 | Dockrev：GHCR Webhook 收件箱 SSE 实时更新 | 已完成 | `e3f83-ghcr-webhook-inbox-sse/SPEC.md` | 2026-03-09 | fast-track（delivery SSE + inbox auto refresh） |
| upjqw | Dockrev：更新后本地 Compose Tag 稳定化，消除手动 `docker compose up -d` 回退 | 已完成 | `upjqw-compose-tag-stability/SPEC.md` | 2026-03-08 | fast-track（普通更新 + supervisor 自升级稳定 tag，避免手工 up 回退） |
| uupfm | Dockrev：更新完成后状态自动收敛 | 已验证 | `uupfm-update-status-settle-after-finish/SPEC.md` | 2026-03-08 | fast-track（后端状态收敛 + 前端终态定向刷新） |
| mvjy8 | Dockrev：服务端模块拆分 + GitHub-hosted 官方环境验收 | 已实现 | `mvjy8-server-module-decomposition-testbox-gate/SPEC.md` | 2026-03-08 | fast-track（server decomposition + hosted deploy smoke + official CI regressions） |
| myy3w | Dockrev：Settings 空 Public Base URL 当前地址建议气泡 | 已完成 | `myy3w-settings-public-base-url-suggest/SPEC.md` | 2026-03-07 | fast-track（当前页面根地址建议气泡 + localStorage 拒绝偏好 + storybook 覆盖，PR #145） |
| qh1dx | Dockrev：Forward Auth 项目鉴权 + 部署文档补齐 | 已完成 | `qh1dx-forward-auth-project-authz-and-deploy-docs/SPEC.md` | 2026-03-08 | fast-track（review-loop 收敛 + 鉴权/部署文档补齐） |
| 6rpkt | Dockrev：Forward Auth 透明透传边界修补 | 已完成 | `6rpkt-forward-auth-transparent-boundary-repair/SPEC.md` | 2026-03-08 | fast-track（follow-up to #qh1dx；PR #149；checks 明确） |
| appaf | Dockrev：shadcn/ui 全量对齐与 Storybook Docs/Stories 补齐 | 已完成 | `appaf-shadcn-ui-alignment-storybook/SPEC.md` | 2026-03-07 | fast-track（PR #147；checks 通过；review-loop 无阻塞） |
| b67fg | Dockrev：GitHub Pages 合并发布文档站与 Storybook | 已完成 | `b67fg-pages-storybook-entry/SPEC.md` | 2026-03-07 | fast-track（Pages 并行构建 + docs Storybook 入口 + redirect bridge） |
| z3mw5 | Dockrev：GHCR Webhook 命中服务检查优先，零命中回退 Discovery | 已完成 | `z3mw5-ghcr-webhook-service-check/SPEC.md` | 2026-03-07 | fast-track（webhook check-first + fallback discovery + webhook notify + review-loop hardening） |
| pv9vc | Dockrev：更新按钮跨路由返回后保留运行态 | 已完成 | `pv9vc-update-button-route-return-spin/SPEC.md` | 2026-03-07 | fast-track（follow-up to #q6x2g：AppShell 级共享 tracker + browser back 回归 + storybook tests 通过） |
| p2n8k | Dockrev：通知事件开关 + 新版本发现通知 + GHCR Webhook 异常通知 | 已完成 | `p2n8k-notification-event-switches-and-new-alerts/SPEC.md` | 2026-03-06 | fast-track（spec-sync: clippy refactor + 用户文案去 jobId + 新版本/巡检摘要直出服务与仓库名 + 巡检文案移除打开设置 + Telegram 标题后详情超链接） |
| keynr | Dockrev：设置页新增 Cron 定时任务（定期检查更新 + Webhook 巡查）+ /queue 模块名修正 | 已完成 | `keynr-settings-scheduled-cron-tasks/SPEC.md` | 2026-03-05 | fast-track |
| tqeph | Dockrev：GHCR 维护页项目标题链接与 Webhook 快捷入口 | 已完成 | `tqeph-ghcr-webhook-registry-links/SPEC.md` | 2026-03-05 | fast-track（标题外链 + webhook 快捷入口 + story 覆盖） |
| 7cbvf | Dockrev：任务类型/作用域标签间距修复（全站统一 6px） | 已完成 | `7cbvf-job-tag-spacing/SPEC.md` | 2026-03-05 | fast-track（Queue/JobDetail 标签容器统一 + 6px gap） |
| gr3cs | Dockrev：通知渠道独立测试按钮与气泡结果可视化 | 已完成 | `gr3cs-notification-channel-test-bubbles/SPEC.md` | 2026-03-04 | fast-track（分渠道测试 + 常驻步骤气泡） |
| 7ruev | Dockrev：更新进行中按钮可点击直达任务详情 | 已完成 | `7ruev-update-running-button-job-link/SPEC.md` | 2026-03-04 | fast-track（活跃态按钮可点击直达 /queue/:jobId） |
| mzqkx | Dockrev：发布流程补齐 Channel 显式选择（PR + Label） | 已完成 | `mzqkx-release-channel-selection/SPEC.md` | 2026-03-04 | fast-track（channel 契约改为 required stable/rc） |
| dvxvx | Dockrev：检查更新并行提升到 7（registry per-host 维持 5） | 已完成 | `dvxvx-check-parallelism-7-registry-5/SPEC.md` | 2026-03-04 | fast-track（check=7, registry=5） |
| fmcxc | Dockrev：版本推测扫描提速（保守过滤） | 已完成 | `fmcxc-snapshot-scan-conservative-filter/SPEC.md` | 2026-03-04 | fast-track |
| b5tcx | Dockrev：概览卡片运行任务整行背景进度条（慢速流光） | 已完成 | `b5tcx-overview-running-row-progress-bg/SPEC.md` | 2026-03-03 | fast-track（overview running 行 subtle 背景进度 + 慢速流光） |
| ttq9u | Dockrev：Supervisor 自升级 dry-run/apply 按钮运行态修复（禁用 + spin） | 已完成 | `ttq9u-supervisor-self-upgrade-running-buttons/SPEC.md` | 2026-03-03 | fast-track（running 禁用 dry/apply，按 request.mode 显示 spinner） |
| rxcb6 | Dockrev：Telegram 群组支持与 Bot Token 脱敏改造 | 已完成 | `rxcb6-telegram-group-token-mask/SPEC.md` | 2026-03-03 | fast-track（群组 chatId 明文 + token 不回传 + 圆点掩码 + review-loop 收敛） |
| xg3dj | Dockrev：概览卡片任务列表增强（运行中/排队展示 + 补齐 + 直达详情） | 已完成 | `xg3dj-overview-card-job-list/SPEC.md` | 2026-03-03 | fast-track（5/10 动态上限 + jobs SSE 实时刷新 + 点击直达详情） |
| kbz3z | Dockrev：服务资源监控（SSE 实时推送 + 图表 + 历史持久化） | 已完成 | `kbz3z-service-resource-monitoring-sse/SPEC.md` | 2026-03-11 | fast-track（settings 开关/频率 + history + service detail SSE+SVG 图表；修复 SSE guard 提前释放导致的 10s 断流；2026-03-11 面板视觉层级升级） |
| q6x2g | Dockrev：执行更新按钮绑定任务运行态（Spin） | 已完成 | `q6x2g-update-action-button-spin/SPEC.md` | 2026-03-02 | fast-track（all/stack/service 按钮按任务态显示 spinner） |
| gh58m | Dockrev：全站任务展示优先人可读名称 | 已完成 | `gh58m-queue-readable-task-name/SPEC.md` | 2026-03-02 | fast-track（scope/type tag 与 type 颜色区分，PR #116 补图 spec-sync） |
| x2n6v | Dockrev：GHCR 状态同步（全量 + 单仓库）与队列并发可视 | 已完成 | `x2n6v-ghcr-sync-all-and-repo/SPEC.md` | 2026-03-03 | fast-track（sync-all/sync-repo + 并发与去重） |
| dk4dd | Dockrev：GHCR Webhook 注册维护专页（Settings 预览 + 专页维护） | 待实现 | `dk4dd-ghcr-webhook-registry-maintenance/SPEC.md` | 2026-03-02 | fast-track |
| m3tq9 | Dockrev：Service Update 禁止版本号反推 tag，改为显式 targetTag | 已完成 | `m3tq9-service-update-explicit-target-tag/SPEC.md` | 2026-03-02 | fast-track |
| ffgt4 | Dockrev：Supervisor 自我升级页补齐版本 / 开源仓库 / 开发者信息 | 已完成 | `ffgt4-supervisor-self-upgrade-meta/SPEC.md` | 2026-03-02 | fast-track |
| 69hb2 | Dockrev：修复 Update Job 失败误报成功 + 幂等步骤重试 | 已完成 | `69hb2-update-job-idempotent-retry/SPEC.md` | 2026-03-01 | fast-track |
| stt9n | Dockrev：状态点升级为 Iconify（全站） | 已完成 | `stt9n-status-dot-iconify/SPEC.md` | 2026-03-01 | fast-track |
| b7rad | Dockrev：版本异常标记 + 自动路径跳过（latest 回退场景） | 已完成 | `b7rad-version-anomaly-marker-auto-skip/SPEC.md` | 2026-03-01 | fast-track |
| p7k2m | Dockrev：GHCR Repos 区域 Inbox 入口 + Webhook Delivery 记录页 | 已完成 | `p7k2m-ghcr-webhook-inbox/SPEC.md` | 2026-03-02 | fast-track（状态/响应码筛选重构） |
| xc4az | Dockrev：版本推测任务内进度修复 + running/queued 标签语义区分 | 已完成 | `xc4az-version-inference-task-progress-and-running-pill/SPEC.md` | 2026-02-28 | fast-track |
| c6j2k | Dockrev：版本列推测 pending 统一为加载中（弱化样式对齐候选） | 已完成 | `c6j2k-inference-pending-loading/SPEC.md` | 2026-03-01 | fast-track |
| 83jm7 | Dockrev：候选版本首屏预取加载态（修复 latest 悬浮后才加载） | 已完成 | `83jm7-prefetch-candidate-loading-before-hover/SPEC.md` | 2026-02-28 | fast-track（lint/build/storybook 回归通过） |
| g5m9c | Dockrev：GHCR Webhook 自动任务化 + 队列可见 + SSE 进度 + 24h 巡检 | 已完成 | `g5m9c-ghcr-webhook-jobization/SPEC.md` | 2026-02-28 | backend+web+tests |
| xu4ew | Dockrev：GHCR 仓库选择器可用性增强（排序/搜索/筛选/拖动批量切换） | 已完成 | `xu4ew-settings-ghcr-picker-ux/SPEC.md` | 2026-02-27 | fast-track |
| r7ggb | Dockrev：pending 时候选版本可见性修复 | 重新设计（#c6j2k） | `r7ggb-pending-candidate-version-visibility/SPEC.md` | 2026-03-01 | superseded by #c6j2k |
| af32v | Dockrev：UI 主题重构（UI UX Pro Max，稳健增强） | 已完成 | `af32v-ui-retheme-uipro-max/SPEC.md` | 2026-02-27 | fast-track |
| t9x88 | Dockrev：移除侧栏 Compose 区块并迁移到服务详情页 | 已完成 | `t9x88-remove-sidebar-compose-move-to-detail/SPEC.md` | 2026-02-27 | Fast-track，PR #97 |
| dc2gg | Dockrev：Settings GHCR「解析并添加」加载反馈优化 | 已完成 | `dc2gg-settings-ghcr-resolve-loading/SPEC.md` | 2026-02-27 | fast-track，PR #101 |
| xscqa | Dockrev：Supervisor 回滚按钮二次确认（气泡确认） | 已完成 | `xscqa-supervisor-rollback-popconfirm/SPEC.md` | 2026-02-27 | fast-track |
| 4ae3k | Dockrev Supervisor：日志 operation 分组 Tabs | 已完成 | `4ae3k-supervisor-log-ops-tabs/SPEC.md` | 2026-02-27 | Fast-track |
| wczjc | Dockrev：版本推测收敛（digest snapshot 单数据源，功能不减） | 已完成 | `wczjc-version-inference-convergence/SPEC.md` | 2026-02-26 | Fast-track |
| hryg9 | Dockrev：Rspress 完整文档站（中英双语 + Pages） | 已完成 | `hryg9-rspress-docs-site/SPEC.md` | 2026-02-26 | normal flow |
| s9w2h | Dockrev：Settings 自动保存串行化 + GHCR 错误归因 + PAT 预校验 | 已完成 | `s9w2h-settings-autosave-ghcr-error-alignment/SPEC.md` | 2026-02-26 | Storybook 已验证 PAT 预校验 |
| xqqkh | Dockrev：缓存未命中时列表版本显示加载态 | 已完成 | `xqqkh-snapshot-pending-loading-state/SPEC.md` | 2026-02-26 | Fast-track |
| n2pw5 | Dockrev：进度条全局平滑（420ms） | 已完成 | `n2pw5-progress-bar-smoothing/SPEC.md` | 2026-03-04 | PR #94（review-loop Round 2 修复完成；lint/build/build-storybook/test-storybook 通过） |
| e8kzr | Dockrev：版本推测可观测性与缓存状态补齐 | 已完成 | `e8kzr-version-inference-observability/SPEC.md` | 2026-02-25 | Normal flow |
| yhngp | Dockrev：固定 5 并行检查 + 双层任务进度 | 已完成 | `yhngp-check-parallel-dual-progress/SPEC.md` | 2026-02-24 | PR #90（checks 通过，review-loop 无 P0/P1 阻塞） |
| kdapc | Dockrev：版本推测采集解耦 + 缓存门控 + 前端就绪等待 | 已实现 | `kdapc-version-inference-decouple/SPEC.md` | 2026-02-24 | Fast-track |
| jjnz5 | Dockrev: 接入 UI UX Pro Max（Codex 团队共享） | 已完成 | `jjnz5-uipro-codex-integration/SPEC.md` | 2026-02-24 | PR #88 |
| saqkf | Dockrev：GHCR 添加 Repo 弹窗加宽 + 镜像/部署筛选 | 已完成 | `saqkf-ghcr-picker-width-linked-filters/SPEC.md` | 2026-03-19 | fast-track（GHCR linked/deployed metadata + picker scope filter + review-loop fixes） |
| wpnmt | Dockrev：digest-only 镜像引用解析与 Discovery 临时 override 回退 | 已完成 | `wpnmt-digest-only-image-ref-discovery-fallback/SPEC.md` | 2026-03-21 | fast-track |
