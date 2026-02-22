# 计划（Plan）总览

本目录用于管理“先计划、后实现”的工作项：每个计划在这里冻结范围与验收标准，进入实现前先把口径对齐，避免边做边改导致失控。

## 快速新增一个计划

1. 分配一个新的 `ID`（推荐 5 字符 nanoId 风格；兼容旧的四位数字 `0001`–`9999`）。
2. 新建目录：`docs/plan/<id>:<title>/`（`<title>` 用简短 slug，建议 kebab-case）。
3. 在该目录下创建 `PLAN.md`（模板见下方“PLAN.md 写法（简要）”）。
4. 在下方 Index 表新增一行，并把 `Status` 设为 `待设计` 或 `待实现`（取决于是否已冻结验收标准），并填入 `Last`（通常为当天）。

## 目录与命名规则

- 每个计划一个目录：`docs/plan/<id>:<title>/`
- `<id>`：
  - 推荐：5 字符 nanoId 风格（避免并行分支/worktree 下的 ID 冲突）。
  - 兼容：四位数字（`0001`–`9999`）的旧 ID（仓库历史计划仍沿用）。
- `<title>`：短标题 slug（建议 kebab-case，避免空格与特殊字符）；目录名尽量稳定。
- 人类可读标题写在 Index 的 `Title` 列；标题变更优先改 `Title`，不强制改目录名。

## 状态（Status）说明

仅允许使用以下状态值：

- `待设计`：范围/约束/验收标准尚未冻结，仍在补齐信息与决策。
- `待实现`：计划已冻结，允许进入实现阶段（或进入 PM/DEV 交付流程）。
- `部分完成（x/y）`：实现进行中；`y` 为该计划里定义的里程碑数，`x` 为已完成里程碑数（见该计划 `PLAN.md` 的 Milestones）。
- `已完成`：该计划已完成（实现已落地或将随某个 PR 落地）；如需关联 PR 号，写在 Index 的 `Notes`（例如 `PR #123`）。
- `作废`：不再推进（取消/价值不足/外部条件变化）。
- `重新设计（#<id>）`：该计划被另一个计划取代；`#<id>` 指向新的计划编号。

## `Last` 字段约定（推进时间）

- `Last` 表示该计划**上一次“推进进度/口径”**的日期，用于快速发现长期未推进的计划。
- 仅在以下情况更新 `Last`（不要因为改措辞/排版就更新）：
  - `Status` 变化（例如 `待设计` → `待实现`，或 `部分完成（x/y）` → `已完成`）
  - `Notes` 中写入/更新 PR 号（例如 `PR #123`）
  - `PLAN.md` 的里程碑勾选变化
  - 范围/验收标准冻结或发生实质变更

## PLAN.md 写法（简要）

每个计划的 `PLAN.md` 至少应包含：

- 背景/问题陈述（为什么要做）
- 目标 / 非目标（做什么、不做什么）
- 范围（in/out）
- 需求列表（MUST/SHOULD/COULD）
- 验收标准（Given/When/Then + 边界/异常）
- 非功能性验收/质量门槛（测试策略、质量检查、Storybook/视觉回归等按仓库已有约定）
- 文档更新（需要同步更新的项目设计文档/架构说明/README/ADR）
- 里程碑（Milestones，用于驱动 `部分完成（x/y）`）
- 风险与开放问题（需要决策的点）

## Index（固定表格）

| ID   | Title | Status | Plan | Last | Notes |
|-----:|-------|--------|------|------|-------|
| 0001 | Dockrev: Docker/Compose 更新管理器（MVP→生产） | 已完成 | `0001:dockrev-compose-updater/PLAN.md` | 2026-01-19 | UI 已对齐 mockups；Jobs 审计与 webhook trigger 已补齐；PR #2 |
| 0002 | Web: Storybook（组件覆盖 + 主题切换） | 已完成 | `0002:storybook-theme-switching/PLAN.md` | 2026-01-20 | - |
| 0003 | CI/CD: 自动发布（GHCR + GitHub Release Assets）对标与补齐 | 已完成 | `0003:release-automation-alignment/PLAN.md` | 2026-01-21 | 单镜像 `dockrev`；仅 workflow_run 触发；Release assets: linux/amd64+arm64（gnu+musl）；CI 运行测试；web 资源嵌入；docker.sock+socket-proxy；PR #5 |
| 0004 | CI/CD: 自动发布时“同步发布镜像”口径冻结与验收 | 已完成 | `0004:auto-release-publish-image/PLAN.md` | 2026-01-21 | 冻结：仅 workflow_run；镜像成功后才创建/更新 Release；允许“镜像已推送但 Release 失败”残留（不清理，需清晰报错） |
| 0005 | CI/CD: GitHub Actions 构建提速（策略与验收） | 已完成 | `0005:github-actions-performance/PLAN.md` | 2026-01-22 | 提速验证：`CI (PR)` run `21219660095` ~1m41s（gating 生效）；`Release` run `21220384586` ~3m03s（arm64 binaries ~27s，`ubuntu-24.04-arm`） |
| 0006 | CI/CD: 自动发版意图标签与发布限制（防止 docs-only 发版） | 已完成 | `0006:release-intent-label-gating/PLAN.md` | 2026-01-22 | 参考 catnap PR #9：PR label gate（type:docs|skip|patch|minor|major，缺失即失败）+ main release-intent；无关联 PR / API 失败 / 多 PR=保守跳过；bump 仅由标签决定（major|minor|patch）；安全：label gate 不得执行 PR checkout 的脚本（已修复） |
| 0007 | Dockrev: Compose 项目自动发现（Auto-Discovery） | 已完成 | `0007:dockrev-compose-auto-discovery/PLAN.md` | 2026-01-22 | 基于 compose labels 自动发现/注册；移除手动注册（迁移清理 stack-bound 数据）；无 Stack 项目聚合组；missing 重启自动归档；支持归档/恢复 stack/service/project；归档不发通知 |
| 0009 | Dockrev Web: 概览页 Stack services 计数修复 | 已完成 | `0009:fix-overview-services-count/PLAN.md` | 2026-01-23 | - |
| 0010 | Dev/CI: 禁用默认端口（统一高位端口） | 已完成 | `0010:no-default-ports/PLAN.md` | 2026-01-23 | PR #24 |
| 0011 | Dev/CI: Bun 迁移（替代 npm） | 已完成 | `0011:bun-migration/PLAN.md` | 2026-01-24 | - |
| 0012 | Dockrev Web: 一键执行更新（service/stack/all）+ 自我升级策略 | 已完成 | `0012:update-buttons-self-upgrade/PLAN.md` | 2026-01-25 | - |
| 0013 | CI/CD: GitHub Release 追加发布 `dockrev-supervisor` 二进制包 | 已完成 | `0013:release-supervisor-assets/PLAN.md` | 2026-01-26 | - |
| 0014 | CI/CD: Release workflow 成功后自动清理 Actions Artifacts | 已完成 | `0014:cleanup-actions-artifacts/PLAN.md` | 2026-01-27 | PR #32, #33, #34；验证：Release run `21397351262`（success→artifacts=0）+ `21396639515`（failure→保留关键 artifacts 且无 `*.dockerbuild`） |
| kcxtp | CI/CD: 修复 GHCR 镜像 `dockrev` 主程序不可执行（exec bit 丢失） | 已完成 | `kcxtp:fix-image-exec-permission/PLAN.md` | 2026-01-31 | PR #37 |
| zdg25 | Dockrev: floating tag 当前版本推测（latest≈semver）+ UI/交互对齐 | 已完成 | `zdg25:auto-version-inference-ui/PLAN.md` | 2026-01-31 | local: `feat/zdg25-auto-version-inference-ui-sync` |
| a2zdt | Dockrev: /supervisor 路由防呆（兜底页 + API 不可吞） | 已完成 | `a2zdt:supervisor-route-failsafe/PLAN.md` | 2026-01-31 | - |
| hkr8b | GitHub/GHCR: 自动注册 `package` webhook（新镜像发布通知） | 已完成 | `hkr8b:github-package-webhook-registration/PLAN.md` | 2026-01-31 | local: `feat/hkr8b-github-package-webhook-registration` |
| 9wyaj | Dockrev: /supervisor 日志区域换行修复 | 已完成 | `9wyaj:supervisor-log-newlines/PLAN.md` | 2026-02-01 | PR #44 |
| 8fjbt | Dockrev Web: 版本候选 tags 气泡 | 已完成 | `8fjbt:version-tags-popover/PLAN.md` | 2026-02-01 | PR #45 |
| 9m2ra | Dockrev Web: 表格左侧引导线对齐修复 | 已完成 | `9m2ra:table-guide-line-alignment/PLAN.md` | 2026-02-03 | PR #49 |
| qsfyj | Dockrev API: 解析带后缀的数字 tag（15-alpine 等） | 已完成 | `qsfyj:version-tag-prefix-parsing/PLAN.md` | 2026-02-02 | PR #47 |
| yh457 | Dockrev API: candidates 接口稳定性（超时/并发/降级） | 已完成 | `yh457:candidates-endpoint-stability/PLAN.md` | 2026-02-02 | PR #46 |
| fay4j | Dockrev API: 修复 check jobs 长时间卡在 running | 已完成 | `fay4j:check-jobs-stuck-running/PLAN.md` | 2026-02-03 | PR #48 |
| h6dwy | CI/CD: 拆分 Dockrev / dockrev-supervisor 镜像 | 待实现 | `h6dwy:split-images/PLAN.md` | 2026-02-04 | - |
| 838ry | Dockrev Web: 增加版本 / 开源仓库 / 开发者信息 | 已完成 | `838ry:app-meta-footer/PLAN.md` | 2026-02-03 | PR #52 |
| mzqkx | CI/CD: Release prerelease channel（label-driven） | 待实现 | `mzqkx:prerelease-channel/PLAN.md` | 2026-02-05 | - |
| 2dkvs | Dockrev API: 修复 multi-arch 镜像当前版本推测（resolvedTag / digest 对齐） | 已完成 | `2dkvs:fix-resolvedtag-multiarch-digest/PLAN.md` | 2026-02-05 | PR #57 |
| bgwkw | Dockrev Web: Queue 日志迁移到任务详情页（避免布局错位） | 已完成 | `bgwkw:fix-queue-logs-layout-shift/PLAN.md` | 2026-02-06 | PR #58 |
| p43u7 | Dockrev: Self-upgrade 后不应触发 config_files_conflict（归一 + warning） | 已完成 | `p43u7:dockrev-discovery-config-files-superset/PLAN.md` | 2026-02-06 | PR #60 |
| 43fyu | Dockrev Web: 更新候选页视图状态持久化（tag in URL + stack 折叠） | 已完成 | `43fyu:persist-candidates-view-state/PLAN.md` | 2026-02-06 | PR #59 |
| n2z72 | Dockrev API: resolvedTag 推测兼容 runtime platform digest | 已完成 | `n2z72:fix-resolvedtag-platform-digest-match/PLAN.md` | 2026-02-06 | PR #61 |
| 9as6k | Dockrev Web: 版本 tags 气泡触发区域仅文本生效 | 已完成 | `9as6k:tag-popover-trigger-text-only/PLAN.md` | 2026-02-07 | PR #62 |
| k7hsm | Dockrev Web: 当前/候选版本气泡拆分 + 移除原生 tooltip | 已完成 | `k7hsm:split-version-popovers/PLAN.md` | 2026-02-08 | PR #63 |
| updxj | Dockrev Web: 版本气泡可用性修复 | 已完成 | `updxj:version-popover-polish/PLAN.md` | 2026-02-08 | PR #63 |
| dxdvu | Dockrev Web: 版本气泡 debug 信息补齐（移除 ? + 扫描摘要） | 已完成 | `dxdvu:version-popover-debug-bubbles/PLAN.md` | 2026-02-08 | PR #63 |
| 6kvn2 | Dockrev API: floating tag 候选选择避免版本倒挂（latest/sha → semver） | 已完成 | `6kvn2:fix-floating-tag-candidate-selection/PLAN.md` | 2026-02-10 | PR #64 |
| zwsh7 | Dockrev Web: 版本气泡 tags 列表默认折叠（减少噪音） | 待实现 | `zwsh7:version-popover-list-toggle/PLAN.md` | 2026-02-11 | - |
| 832pb | Dockrev API: 更新任务使用 stale container id 导致误报失败 | 已完成 | `832pb:fix-update-job-stale-container-id/PLAN.md` | 2026-02-14 | PR #65 |
| wtb4a | Dockrev API/Web: 修复 check jobs 并发与退出未收尾 | 已完成 | `wtb4a:check-running-job-recovery/PLAN.md` | 2026-02-15 | PR #66 |
| 8xt2t | Dockrev: 运行态版本漂移自动发现（runtime diff scan + SSE） | 已完成 | `8xt2t:runtime-drift-scan-sse/PLAN.md` | 2026-02-17 | PR #67 |
| fc8ua | Dockrev Web: 左下角版本号链接指向 GitHub Release | 已完成 | `fc8ua:version-link-to-release/PLAN.md` | 2026-02-18 | PR #69 |
| zt9ks | Dockrev Web/API: 修复服务更新确认弹窗（popover 层级 + 目标版本错选） | 已完成 | `zt9ks:fix-update-confirm-modal-popover-target/PLAN.md` | 2026-02-18 | PR #70 |
| fknrb | Dockrev: digest-tags snapshot persistence (no live scan) | 已完成 | `fknrb:digest-tags-snapshot/PLAN.md` | 2026-02-18 | PR #71 |
| vah6t | Dockrev: 仅“同 tag 的新 digest”更新（移除版本候选/选择 + 更新锁定） | 已完成 | `vah6t:same-tag-digest-only-updates/PLAN.md` | 2026-02-20 | PR #74 |
| nxjyx | Dockrev: 修复 supervisor 自动识别（labels-first + inspect fallback）+ 更新时 best-effort 拉 semver tag（避免悬空） | 待实现 | `nxjyx:supervisor-auto-match-and-semver-pull/PLAN.md` | 2026-02-21 | - |
| rc9kk | Dockrev: 候选版本号推断修复（candidate resolved tag） | 已完成 | `rc9kk:fix-candidate-version-inference/PLAN.md` | 2026-02-21 | PR #80 |
| yd6wp | Dockrev: `check all` 提速 + 任务进度可观测性统一 | 已完成 | `yd6wp:job-progress-and-check-speed/PLAN.md` | 2026-02-22 | PR #81 |
| gnae4 | Dockrev API: check 429 限流与中档提速 | 已完成 | `gnae4:check-429-throttle-and-speed/PLAN.md` | 2026-02-22 | PR #82 |
