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
| af32v | Dockrev：UI 主题重构（UI UX Pro Max，稳健增强） | 已完成 | `af32v-ui-retheme-uipro-max/SPEC.md` | 2026-02-27 | fast-track |
| dc2gg | Dockrev：Settings GHCR「解析并添加」加载反馈优化 | 已完成 | `dc2gg-settings-ghcr-resolve-loading/SPEC.md` | 2026-02-27 | fast-track，PR #101 |
| xscqa | Dockrev：Supervisor 回滚按钮二次确认（气泡确认） | 已完成 | `xscqa-supervisor-rollback-popconfirm/SPEC.md` | 2026-02-27 | fast-track |
| 4ae3k | Dockrev Supervisor：日志 operation 分组 Tabs | 已完成 | `4ae3k-supervisor-log-ops-tabs/SPEC.md` | 2026-02-27 | Fast-track |
| hryg9 | Dockrev：Rspress 完整文档站（中英双语 + Pages） | 已完成 | `hryg9-rspress-docs-site/SPEC.md` | 2026-02-26 | normal flow |
| s9w2h | Dockrev：Settings 自动保存串行化 + GHCR 错误归因 + PAT 预校验 | 已完成 | `s9w2h-settings-autosave-ghcr-error-alignment/SPEC.md` | 2026-02-26 | Storybook 已验证 PAT 预校验 |
| xqqkh | Dockrev：缓存未命中时列表版本显示加载态 | 已完成 | `xqqkh-snapshot-pending-loading-state/SPEC.md` | 2026-02-26 | Fast-track |
| e8kzr | Dockrev：版本推测可观测性与缓存状态补齐 | 已完成 | `e8kzr-version-inference-observability/SPEC.md` | 2026-02-25 | Normal flow |
| yhngp | Dockrev：固定 5 并行检查 + 双层任务进度 | 已完成 | `yhngp-check-parallel-dual-progress/SPEC.md` | 2026-02-24 | PR #90（checks 通过，review-loop 无 P0/P1 阻塞） |
| kdapc | Dockrev：版本推测采集解耦 + 缓存门控 + 前端就绪等待 | 已实现 | `kdapc-version-inference-decouple/SPEC.md` | 2026-02-24 | Fast-track |
| jjnz5 | Dockrev: 接入 UI UX Pro Max（Codex 团队共享） | 已完成 | `jjnz5-uipro-codex-integration/SPEC.md` | 2026-02-24 | PR #88 |
