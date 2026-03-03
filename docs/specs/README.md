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
| ttq9u | Dockrev：Supervisor 自升级 dry-run/apply 按钮运行态修复（禁用 + spin） | 已完成 | `ttq9u-supervisor-self-upgrade-running-buttons/SPEC.md` | 2026-03-03 | fast-track（running 禁用 dry/apply，按 request.mode 显示 spinner） |
| xg3dj | Dockrev：概览卡片任务列表增强（运行中/排队展示 + 补齐 + 直达详情） | 已完成 | `xg3dj-overview-card-job-list/SPEC.md` | 2026-03-03 | fast-track（5/10 动态上限 + jobs SSE 实时刷新 + 点击直达详情） |
| kbz3z | Dockrev：服务资源监控（SSE 实时推送 + 图表 + 历史持久化） | 已完成 | `kbz3z-service-resource-monitoring-sse/SPEC.md` | 2026-03-03 | fast-track（settings 开关/频率 + history + service detail SSE+SVG 图表） |
| q6x2g | Dockrev：执行更新按钮绑定任务运行态（Spin） | 已完成 | `q6x2g-update-action-button-spin/SPEC.md` | 2026-03-02 | fast-track（all/stack/service 按钮按任务态显示 spinner） |
| gh58m | Dockrev：全站任务展示优先人可读名称 | 已完成 | `gh58m-queue-readable-task-name/SPEC.md` | 2026-03-02 | fast-track（scope/type tag 与 type 颜色区分，PR #116 补图 spec-sync） |
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
| e8kzr | Dockrev：版本推测可观测性与缓存状态补齐 | 已完成 | `e8kzr-version-inference-observability/SPEC.md` | 2026-02-25 | Normal flow |
| yhngp | Dockrev：固定 5 并行检查 + 双层任务进度 | 已完成 | `yhngp-check-parallel-dual-progress/SPEC.md` | 2026-02-24 | PR #90（checks 通过，review-loop 无 P0/P1 阻塞） |
| kdapc | Dockrev：版本推测采集解耦 + 缓存门控 + 前端就绪等待 | 已实现 | `kdapc-version-inference-decouple/SPEC.md` | 2026-02-24 | Fast-track |
| jjnz5 | Dockrev: 接入 UI UX Pro Max（Codex 团队共享） | 已完成 | `jjnz5-uipro-codex-integration/SPEC.md` | 2026-02-24 | PR #88 |
