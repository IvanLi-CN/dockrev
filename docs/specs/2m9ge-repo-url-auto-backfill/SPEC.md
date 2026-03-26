# Dockrev：repoUrl 自动回填与历史空值修复（#2m9ge）

## 状态

- Status: 已完成
- Created: 2026-03-26
- Last: 2026-03-26

## 背景 / 问题陈述

- `repoUrl` 持久化、详情页手动推断和 registry/repo icon 已经上线，但线上历史服务数据全部是 `repo_url = NULL`。
- 线上真实验证表明：仓库推断接口可以成功返回 URL，但概览页和服务页没有 repo icon，因为前端只消费已持久化的 `repoUrl`。
- 当前系统缺少两条链路：一条用于给历史空值做一次性自动回填，另一条用于在后续 discovery 同步后自动补齐新产生的空值。

## 目标 / 非目标

### Goals

- 在不新增前端操作入口的前提下，自动补齐历史与未来服务的缺失 `repoUrl`。
- 自动回填复用现有单服务推断规则：`OCI source -> GHCR exact fallback -> none`。
- 明确区分“尚未回填”与“用户明确清空”，保证用户手动清空后不会被后台自动回填覆盖。
- 将自动回填纳入现有 jobs/queue 体系，保留进度、摘要与运维可观测性。

### Non-goals

- 不新增新的用户触发按钮或新的 HTTP API。
- 不改变现有单服务手动推断接口的对外契约。
- 不在只读接口或页面加载路径里偷偷写库。

## 范围（Scope）

### In scope

- `services` 表新增 `repo_url_auto_disabled` 持久化字段。
- 服务设置保存语义调整：显式清空会禁用后续自动补齐，显式保存非空 URL 会恢复自动补齐资格。
- 新增后台 job type `repo_link_backfill`，用于启动后历史补齐与 discovery follow-up 补齐。
- 启动时按需自动 enqueue 全局 backfill job。
- discovery 同步后按 stack 维度按需去重 enqueue backfill job。
- Jobs 页面与服务详情页文案的小幅前端对齐。

### Out of scope

- 不改 DB 之外的持久化存储形态。
- 不新增新的设置页开关或管理入口。
- 不做批量人工操作 UI。

## 需求（Requirements）

### MUST

- 新增 `repo_url_auto_disabled BOOLEAN NOT NULL DEFAULT 0`。
- 自动回填只能写 `repo_url IS NULL AND repo_url_auto_disabled = 0` 的服务，绝不覆盖已有非空 URL。
- `PUT /api/services/{id}/settings` 在 `repoUrl` 省略时必须保留 `repo_url` 和 `repo_url_auto_disabled`。
- `PUT /api/services/{id}/settings` 在 `repoUrl` 显式清空时必须写入 `repo_url = NULL` 且 `repo_url_auto_disabled = 1`。
- `PUT /api/services/{id}/settings` 在 `repoUrl` 为非空时必须写入 URL 且 `repo_url_auto_disabled = 0`。
- 启动后若存在 `repo_url IS NULL AND repo_url_auto_disabled = 0` 的服务，则自动 enqueue 一个 `scope=all` 的 `repo_link_backfill` job。
- discovery 在新建 stack 或同步已有 stack 后，若 stack 内存在可补齐缺失值，则按 stack 维度去重 enqueue `repo_link_backfill` job。
- 队列和任务详情必须能正确显示 `repo_link_backfill` 的人类可读名称。

### SHOULD

- 自动回填 job 输出 `updated / skippedDisabled / noMatch / error` 摘要计数。
- stack 级自动 enqueue 在存在 pending/running 的全局 backfill job 时应跳过，避免重复工作。
- 服务详情页应明确提示“清空并保存会禁用自动补齐；再次手动推断并保存可恢复”。

### COULD

- 自动回填 job 在日志或 progress 中带上当前处理的 `<stack>/<service>` 目标，便于定位。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 服务首次入库时，`repo_url = NULL` 且 `repo_url_auto_disabled = 0`，后续由后台 job 自动尝试补齐。
- 服务详情页保存设置时：
  - 省略 `repoUrl`：保持当前值与禁用标记不变。
  - 清空 `repoUrl`：清空 URL 并设置禁用标记。
  - 保存非空 `repoUrl`：写入 URL 并取消禁用标记。
- 应用启动完成 worker 初始化后，若数据库里仍有可补齐缺失值，则自动排入一条全局 backfill job。
- discovery 创建新 stack 或同步已有 stack 后，若该 stack 内存在可补齐缺失值，则自动排入一条 stack 级 backfill job；若已有同 scope pending job，则不重复排入。
- backfill worker 对每个目标服务复用现有 repo 推断逻辑；命中则回填 URL，推断不到则仅记 `noMatch`。

### Edge cases / errors

- 用户曾显式清空的服务必须统计为 `skippedDisabled`，且不得被自动回填。
- 已有非空 `repoUrl` 的服务不进入回填目标集。
- registry 读取 OCI source 失败时，job 不应中断整个批次；该服务计入 `error` 并继续其它服务。
- 若存在 pending/running 的全局 backfill job，则 startup 和 discovery 都不得再追加 stack 级重复 job。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `services.repo_url_auto_disabled` | DB | internal | New | [./contracts/db.md](./contracts/db.md) | dockrev-api | dockrev-api | 区分“可自动回填”与“用户明确禁用” |
| `PUT /api/services/{service_id}/settings` `repoUrl` semantics | HTTP API | internal | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | dockrev-api | web | 省略/清空/非空三态语义调整 |
| `jobs.type=repo_link_backfill` | HTTP API | internal | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | dockrev-api | web | 通过既有 `/api/jobs*` 暴露 |

## 验收标准（Acceptance Criteria）

- Given 数据库里存在 `repo_url IS NULL AND repo_url_auto_disabled = 0` 的服务，When 应用启动完成，Then 自动排入一个全局 `repo_link_backfill` job。
- Given 某服务在详情页将 `repoUrl` 清空并保存，When 后续后台 backfill job 运行，Then 该服务仍保持 `repo_url = NULL`，且被计入 `skippedDisabled`。
- Given 某服务保存了非空 `repoUrl`，When 后续后台 backfill job 运行，Then 该 URL 不会被覆盖。
- Given GHCR 精确匹配且仓库已被跟踪，When backfill job 处理该服务，Then `repoUrl` 被回填为对应 GitHub 仓库地址。
- Given OCI source 可被识别为外部仓库 URL，When backfill job 处理该服务，Then `repoUrl` 被回填且策略保持 `oci_source`。
- Given 推断不到仓库或 registry 读取失败，When backfill job 结束，Then job 成功完成且摘要正确累计 `noMatch` 或 `error`。
- Given 队列页和任务详情页展示该 job，When `type = repo_link_backfill`，Then 显示人类可读标签“仓库链接补齐”而不是原始 machine name。

## 实现前置条件（Definition of Ready / Preconditions）

- 自动回填以后台 job 实现、而非 UI 按钮，已经锁定。
- 用户显式清空 `repoUrl` 视为永久禁用自动补齐，直到再次保存非空 URL，已经锁定。
- follow-up spec 独立于 `6uwgs`，不回写旧 spec 的完成口径，已经锁定。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: 覆盖 migration 默认值、服务设置三态保存语义、自动回填摘要计数、GHCR/OCI/no-match/disabled 场景。
- Integration tests: 覆盖 startup enqueue、discovery follow-up enqueue、自动回填后 `repoUrl` 真正写回数据库。
- E2E tests (if applicable): None

### UI / Storybook (if applicable)

- Stories to add/update: `web/src/stories/pages/ServiceDetailPage.stories.tsx`
- Docs pages / state galleries to add/update: none（沿用现有 page stories）
- `play` / interaction coverage to add/update: 服务详情页仓库字段说明与 repo-link editing flow 的交互断言

### Quality checks

- Rust: `cargo test -p dockrev-api ...`
- Web: `bun test --cwd web`、`bun run --cwd web lint`、`bun run --cwd web build`
- Storybook: `bun run --cwd web build-storybook`、`bun run --cwd web test-storybook`

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增 follow-up spec 索引。
- `docs/specs/6uwgs-service-image-links-and-repo-url/SPEC.md`: 在参考中引用 follow-up spec 即可，不回写完成态口径。

## 计划资产（Plan assets）

- Directory: `docs/specs/2m9ge-repo-url-auto-backfill/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- Visual evidence source: maintain `## Visual Evidence` in this spec when owner-facing screenshots are needed.

## Visual Evidence

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: DB schema / settings semantics 落地 `repo_url_auto_disabled`
- [x] M2: repo link backfill worker、startup enqueue 与 discovery follow-up enqueue 落地
- [x] M3: 前端任务类型映射与服务详情页说明文案补齐
- [x] M4: Rust/Web/Storybook 回归通过并完成 spec 同步

## 方案概述（Approach, high-level）

- 继续把 repo URL 识别真相源留在现有 inference 逻辑里，只新增一个后台消费层，不复制第二套规则。
- 用布尔禁用标记表达“用户明确不要自动补齐”，而不是依赖 `repo_url IS NULL` 单字段猜测意图。
- 通过 queued job + worker 模式保证启动补齐与 discovery 补齐都走同一条实现与同一份可观测摘要。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：registry 请求失败可能让部分服务暂时无法回填，需要在 job 摘要里清晰暴露 `error`。
- 风险：若启动时已有 stack 级 pending job，全局 job 与之存在部分重叠；实现需要做最小去重，避免明显重复。
- 假设：当前历史空值服务主要来自已上线前的数据，因此一次启动补齐足以覆盖现网存量。

## 变更记录（Change log）

- 2026-03-26: 创建 follow-up spec，冻结自动回填、显式清空禁用与 jobs 可观测性口径。
- 2026-03-26: 完成 `repo_url_auto_disabled` 持久化、`repo_link_backfill` 后台任务、startup/discovery 自动排队，以及前端 jobs/详情页文案对齐；本地 Rust/Web/Storybook 回归通过。

## 参考（References）

- [6uwgs-service-image-links-and-repo-url/SPEC.md](../6uwgs-service-image-links-and-repo-url/SPEC.md)
