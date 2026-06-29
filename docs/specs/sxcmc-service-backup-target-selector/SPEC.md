# Dockrev：服务保护设置补全备份目标直选与备份说明（#sxcmc）

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- 现有“服务保护设置”抽屉无法直接表达单个 target 的真实备份策略；`selected + inherit|force|skip` 三态把“是否备份”和“如何备份”揉在了一起，既不直观，也难以承载共享卷的停机协调语义。
- 备份项与普通服务设置此前混在同一条保存链路里，语义边界模糊，前端也缺少“候选发现结果”和“存储说明”的专用接口。
- 用户在服务页内看不到“保存在哪 / 保存多久 / 是否压缩”的当前行为，只能靠代码或系统设置猜测，容易误解备份语义。

## 目标 / 非目标

### Goals

- 在现有“服务保护设置”抽屉内补齐 `Volumes` 与 `Bind paths` 的直接选择能力，不新增新的 stack 级备份编辑页。
- 为服务详情页新增专用 `GET/PUT /api/services/{service_id}/backup-targets`，返回 compose 派生候选、关联服务信息与只读存储说明。
- 后端基于 compose 声明解析当前服务可备份 mounts，并在保存时原子更新 `stack.backup.targets` 与当前服务级 backup-target policy 关系。
- 服务页内每个 target 使用三选一策略：`不备份`、`停机备份`、`在线备份`。
- 在抽屉内清楚展示备份目录、产物模式、压缩格式与默认保留摘要，但不扩展 retention 编辑能力。

### Non-goals

- 不把 `PUT /api/services/{id}/settings` 改造成隐式跨层级写 `stack.backup.targets` 或服务级 backup-target policy 的副作用接口。
- 不新增 stack 级或全局级的完整备份目标管理器。
- 不修改备份执行格式、压缩方式、cleanup 调度逻辑或默认保留策略。
- 不从 Docker 运行时挂载扫描候选，本次只以 compose 声明为准。

## 范围（Scope）

### In scope

- `docs/specs/README.md`
- `docs/specs/sxcmc-service-backup-target-selector/**`
- `crates/dockrev-api/src/api/**`
- `crates/dockrev-api/src/compose.rs`
- `crates/dockrev-api/src/db/**`
- `crates/dockrev-api/src/discovery.rs`
- `web/src/api.ts`
- `web/src/api/types.ts`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/useServiceDetailPageState.tsx`
- `web/src/App.css`
- `web/src/stories/**`

### Out of scope

- 备份执行器、压缩实现、清理 worker 或系统设置编辑界面
- 非 compose 来源的 backup target 发现
- 自动部署策略抽屉、更新状态语义或 repoUrl 行为改造

## 需求（Requirements）

### MUST

- 服务保护抽屉必须能直接选择 compose 中声明的 Docker named volumes 与 host bind mounts。
- `GET /api/services/{service_id}/backup-targets` 必须返回 `bindPaths[]`、`volumeNames[]` 与 `storage { baseDir, artifactPattern, compression, keepLast, deleteAfterStableSeconds }`。
- 每个候选项必须统一为 `{ key, policy, relatedServiceCount, relatedServiceIds }`，其中 `policy` 只允许 `disabled | stop_related_services | live_backup`。
- `PUT /api/services/{service_id}/backup-targets` 的输入必须表达“当前服务对候选项的策略选择结果”，而不是要求前端自行拼接 stack 级 diff。
- 共享 target 的信息必须以关联服务计数和 service id 列表形式可见，供更新前停机协调使用。
- compose 相对 bind path 必须按 compose 文件目录解析为绝对路径，即使路径当前不存在。
- 匿名 volume、`tmpfs`、`image` mounts 等非可恢复目标必须忽略，不得出现在候选列表里。
- 抽屉内必须展示只读备份说明，明确 `<baseDir>/<stackId>/<timestamp>.tar.gz`、`gzip`、`keepLast=1` 与 `deleteAfterStableSeconds=3600`。

### SHOULD

- 候选项应按 `Volumes` 再 `Bind paths` 分组显示，并保留 compose 顺序与稳定 key。
- 共享 target 应在行内明确提示“关联了多少个服务”，并保留 service id 列表供实现和调试使用。
- 没有任何可备份项时，应显示明确空态文案，而不是只显示“暂无”。

## 功能与行为规格（Functional/Behavior Spec）

### Candidate discovery

- 候选来源仅来自当前 stack 的 compose 文件解析结果。
- `named volume` 候选使用 volume 名称作为 `key`。
- `bind mount` 候选使用解析后的绝对主机路径作为 `key`。
- 服务端按 compose 文件顺序收集候选，并按规范化 identity 去重。
- 多 compose 文件 merge 后，服务级候选应与最终 merge 结果保持一致。

### Service backup-target API

- `GET /api/services/{service_id}/backup-targets`
  - 返回当前服务可选 `bindPaths[]` 与 `volumeNames[]`。
  - 每个条目带 `policy`、`relatedServiceCount` 与 `relatedServiceIds`。
  - `policy=disabled` 仍保留候选行，便于当前服务重新开启该 target。
  - 返回当前备份存储说明，用于抽屉只读展示。
- `PUT /api/services/{service_id}/backup-targets`
  - 输入为当前服务对候选项的完整策略选择结果。
  - `policy=disabled` 表示当前服务不为该 target 触发自动备份。
  - `policy=stop_related_services` 表示升级前备份此 target 时，需要协调停掉相关服务后备份，再恢复。
  - `policy=live_backup` 表示保持相关服务运行，直接备份。
  - 响应仅返回 `ok`；普通服务设置保存链路不再依赖该接口回传旧 `backupTargets`。

### Save semantics

- 保存服务保护设置时，前端先提交专用 backup-targets API，再沿用现有 settings API 保存 `repoUrl`、`autoRollback` 与 `autoUpdatePolicy` 草稿。
- 服务端必须原子更新 `stack.backup.targets` 与当前服务级 backup-target policy 关系，避免前端推断共享引用计数。
- 现有 `PUT /api/services/{service_id}/settings` 仍只负责服务设置本身，不承担 stack 级 backup target 管理职责，也不再覆盖专用 backup-target policy。

### UI behavior

- 服务保护抽屉中的“备份项”区域改为发现结果驱动：
  - 先显示 `Volumes`
  - 再显示 `Bind paths`
- 每行展示 technical key、关联服务计数、策略说明与三选一 button group。
- 每行的三选一策略为：
  - `不备份`
  - `停机备份`
  - `在线备份`
- 三选一控件采用单轨 segmented button group，选中态通过水平滑动高亮块表达当前策略。
- 即使当前策略为 `不备份`，候选行也必须保留可见。
- 无候选时显示“当前服务在 Compose 中未发现可备份 volume 或 bind path”。
- 只读说明区块展示目录、产物格式、压缩与保留摘要，不提供编辑控件。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/services/{service_id}/backup-targets` | HTTP | external | New | 本文 | dockrev-api | Web | 返回 compose 派生候选与存储说明 |
| `PUT /api/services/{service_id}/backup-targets` | HTTP | external | New | 本文 | dockrev-api | Web | 保存当前服务 backup target 选择结果 |
| `ComposeServiceSpec.backup_*` | backend type | internal | New | 本文 | dockrev-api | discovery / services API | compose 解析产物 |
| `ServiceDetailPage` 服务保护抽屉 | frontend UI | internal | Modify | 本文 | web | operators | volumes/bind paths 直选与只读说明 |

### JSON shape

```json
{
  "bindPaths": [
    {
      "key": "/srv/app/data",
      "policy": "live_backup",
      "relatedServiceCount": 2,
      "relatedServiceIds": ["svc-prod-api", "svc-prod-web"]
    }
  ],
  "volumeNames": [
    {
      "key": "api-cache",
      "policy": "stop_related_services",
      "relatedServiceCount": 1,
      "relatedServiceIds": ["svc-prod-api"]
    }
  ],
  "storage": {
    "baseDir": "/srv/dockrev/backups",
    "artifactPattern": "/srv/dockrev/backups/<stackId>/<timestamp>.tar.gz",
    "compression": "gzip",
    "keepLast": 1,
    "deleteAfterStableSeconds": 3600
  }
}
```

## 验收标准（Acceptance Criteria）

- Given 服务 compose 中同时存在 named volume、绝对 bind path 与相对 bind path
  When 调用 `GET /api/services/{id}/backup-targets`
  Then 返回稳定候选，且相对 bind path 已解析为绝对路径。

- Given 服务把一个当前独占的 target 设为 `disabled`
  When 调用 `PUT /api/services/{id}/backup-targets`
  Then 该 target 从 `stack.backup.targets` 中移除，且当前服务 policy 记录为 `disabled`。

- Given stack 中存在不再被任何服务策略引用的历史 backup target
  When 当前服务保存新的 backup-target policy
  Then 该孤儿 target 会从 `stack.backup.targets` 清理掉。

- Given 服务把一个仍被其他声明服务共享的 target 设为 `disabled`
  When 调用 `PUT /api/services/{id}/backup-targets`
  Then stack 级 target 继续保留，当前服务 policy 记录为 `disabled`。

- Given 服务保护抽屉存在候选项
  When 用户打开抽屉
  Then 可直接选择 `Volumes` 与 `Bind paths`，并能为每个 target 切换 `不备份 | 停机备份 | 在线备份`。

- Given 服务 compose 中没有任何可备份目标
  When 用户打开抽屉
  Then 显示明确空态文案，而不是模糊的“暂无”。

- Given 用户查看备份说明
  When 抽屉渲染只读说明区块
  Then 能直接看到备份目录、`.tar.gz` 产物模式、`gzip` 压缩与默认保留摘要，无需跳转系统设置页。

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 涉及的接口/契约已写清楚。
- [x] 相关验收条件已经可以用于实现与 review 对齐。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api put_service_backup_targets -- --nocapture`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

### UI / Storybook

- Stories to add/update: `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- Docs pages / state galleries to add/update: `none (reason: repo currently uses page-story canvas coverage for this surface)`
- `play` / interaction coverage to add/update: 有 volume + bind path、共享 target 关闭、无候选空态、只读备份说明
- Visual regression baseline changes (if any): 服务保护抽屉 backup targets 直选与说明区块 mock-only 视觉证据

## Visual Evidence

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x1600`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `approved`
  PR: include
  story_id_or_title: `Pages/ServiceDetailPage/Service Protection Backup Targets`
  state: `volume + bind path candidates`
  evidence_note: 验证服务保护抽屉直接展示 `Volumes` / `Bind paths` 两组候选、技术 key、关联服务计数、带水平滑块的三选一策略按钮组，以及只读备份说明卡片。

![服务保护抽屉：Volumes + Bind paths 直选](./assets/service-protection-backup-targets.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x1600`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `approved`
  story_id_or_title: `Pages/ServiceDetailPage/Service Protection Shared Target Off`
  state: `shared target deselected`
  evidence_note: 验证共享 bind path 设为“不备份”后仍保留可见行，并明确提示“关联 2 个服务”与禁用态说明。

![服务保护抽屉：共享 target 关闭态](./assets/service-protection-shared-target-off.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x1300`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `approved`
  story_id_or_title: `Pages/ServiceDetailPage/Service Protection Empty Backup Targets`
  state: `no compose backup candidates`
  evidence_note: 验证当前服务未声明任何可备份 volume 或 bind path 时，抽屉给出明确空态文案而不是“暂无”。

![服务保护抽屉：无可备份候选空态](./assets/service-protection-empty-backup-targets.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x1500`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `approved`
  story_id_or_title: `Pages/ServiceDetailPage/Service Protection Storage Summary Only`
  state: `read-only backup storage summary`
  evidence_note: 验证抽屉内的只读备份说明明确展示目录、`.tar.gz` 产物模式、`gzip` 压缩与“最近 1 份保留 / 稳定 1h 后清理”摘要。

![服务保护抽屉：只读备份说明](./assets/service-protection-storage-summary-only.png)

## Related PRs

- None

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：compose 短语法 volume/bind 解析若放宽过头，可能把匿名 volume 误当作可恢复目标；实现必须保守忽略不稳定语义。
- 风险：服务设置保存链路分成两个请求后，若第二个 settings 保存失败，需要确保 backup targets 的独立持久化语义是明确且可接受的。
- 假设：首版只要求 compose 声明可推导的 named volumes 与 bind paths，运行时临时挂载不进入候选。

## 参考（References）

- `docs/specs/xyy72-auto-deploy-policy-configurator/SPEC.md`
- `docs/specs/6uwgs-service-image-links-and-repo-url/SPEC.md`
- `crates/dockrev-api/src/backup.rs`
