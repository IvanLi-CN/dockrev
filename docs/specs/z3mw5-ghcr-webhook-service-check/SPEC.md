# Dockrev：GHCR Webhook 命中服务检查优先，零命中回退 Discovery（#z3mw5）

## 状态

- Status: 已完成
- Created: 2026-03-07
- Last: 2026-03-07

## 背景 / 问题陈述

- 当前 `package.published` webhook 命中后只会触发 `discovery.all`，不会立刻检查对应服务的镜像候选版本。
- `discovery` 负责同步 Compose 项目清单，不负责查 registry 新 digest；因此 webhook 到达后仍需等待下一轮定时 `check` 才能识别新版本。
- 多个 package delivery 在短时间连续到达时，会生成多个 discovery job；虽然底层执行会串行，但 Queue 里仍会堆积多个相似任务，意图不清晰。
- 希望 webhook 到达后优先检查真正受影响的服务，让系统更快刷新 candidate，并在发现新版本时直接复用既有通知链路。

## 目标 / 非目标

### Goals

- 将 GHCR webhook 主动作改为：优先匹配受影响服务并触发 `check.service`。
- 当 payload 无法精确映射到受管服务时，保留 `discovery` 作为回退路径，继续承担 inventory 自愈职责。
- 让 `reason=webhook` 的 check 在发现新版本时也能发送 `new_version_discovered` 通知。
- 为 webhook 触发的 check / discovery 增加任务复用与明确审计字段，减少重复 job 与排障成本。

### Non-goals

- 不自动执行 update/apply。
- 不把 webhook 默认升级为 `check.all`。
- 不改变现有 check 对 runtime digest 的判定语义。
- 不修复 Telegram 通道偶发网络失败。

## 范围（Scope）

### In scope

- GHCR webhook receiver 的 repo 解析、服务匹配、job 调度与 delivery 审计。
- `check` 完成后的通知触发策略，覆盖 `schedule` 与 `webhook` 两种 reason。
- discovery fallback 的 job 级复用。
- 对应后端测试、规格与文档索引同步。

### Out of scope

- 新增 GHCR webhook 设置页交互。
- 非 GHCR registry webhook 抽象。
- 运行时扫描、更新执行、Supervisor 行为。

## 需求（Requirements）

### MUST

- `POST /api/webhooks/github-packages` 在命中已跟踪 repo 且匹配到未归档服务时，按服务逐个入队 `check` job：
  - `type=check`
  - `scope=service`
  - `reason=webhook`
  - `created_by=github`
- 服务匹配规则基于 webhook 的 `owner/repo`，与 service `image_ref` 归一化后的镜像仓库名做大小写无关匹配；仅匹配未归档 stack / service。
- 当 webhook repo 零命中，或 payload 只能定位到 owner 不能定位 repo 时，才允许回退触发 1 个全局 discovery job。
- 对同一 service：若已有 `queued/running` 的 `check.service`，新 webhook 不再重复入队，必须复用现有 job。
- 对 discovery fallback：若已有 `queued/running` 的 `discovery.all`，新 webhook 不再重复入队，必须复用现有 job。
- `schedule` 与 `webhook` 触发的 check 在 `newVersions.count > 0` 时都发送 `new_version_discovered` 通知；`ui` 触发的 check 不发送该通知。
- webhook 任务 summary/log 必须包含足够审计信息：`source=github_webhook`、`repo`、`deliveryId`、`matchedServiceIds`、`fallbackUsed`、`reusedJobIds`。
- delivery 历史/API 视图在多服务命中时必须保留全部关联 `jobIds`，同时保留主 `jobId` 兼容既有跳转。

### SHOULD

- 对 owner-only payload 的回退原因写出明确日志，避免用户误以为“命中了 repo 但没查服务”。
- webhook 到达后若只命中单服务，Queue 与通知应能直接指向该服务或对应 check job。
- 复用已有 job 时，delivery 记录结果能反映“processed_reused_check / processed_reused_discovery”之类的人可读状态。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/webhooks/github-packages` | HTTP API | external | Modify | `./contracts/http-apis.md` | dockrev-api | GitHub Webhooks | 调度从 discovery-only 改为 check-first |
| `dockrev.notification.new_version_discovered.v2` | event payload | external | Modify | `./contracts/events.md` | dockrev-api | Telegram/Webhook/Email/Web Push | 新增 webhook reason 触发来源 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)

## 验收标准（Acceptance Criteria）

- Given `ghcr.io/ivanli-cn/codex-vibe-monitor` 对应 1 个未归档服务，When 收到该 repo 的 `package.published` webhook，Then Queue 出现 1 个 `check.service`（`reason=webhook`），且该服务 candidate 在短时间内刷新。
- Given 同一 repo 被多个未归档服务引用，When 收到 webhook，Then 仅这些服务各自产生 1 个 `check.service`，不会产生 `check.all`。
- Given 同一 delivery 被 GitHub 重试，When 再次投递，Then 不会生成新的 check/discovery job。
- Given 同一 service 已有 `queued/running` 的 webhook check，When 新 delivery 到达，Then 复用现有 check job。
- Given webhook repo 在当前受管服务中零命中，When webhook 到达，Then 只创建或复用 1 个 discovery fallback job，并记录 `fallbackUsed=true`。
- Given webhook reason 的 check 发现了新版本，When job 完成，Then 发送 `new_version_discovered` 通知并留下 `notify:` 日志。
- Given UI 手动触发 check 发现了新版本，When job 完成，Then 不发送 `new_version_discovered` 通知。

## 实现前置条件（Definition of Ready / Preconditions）

- GHCR webhook 现有签名校验、delivery 去重与 repo 选择逻辑保持可用。
- check / discovery job 现有状态机无需新增状态值。
- `new_version_discovered` 现有 schema 与渠道渲染保持兼容，仅扩展触发来源。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit/integration tests:
  - webhook 命中单服务 / 多服务 / 零命中回退
  - 重复 delivery 不重复入队
  - 复用已有 service check 与 discovery fallback
  - webhook/schedule/ui 三种 check reason 的通知 gating
- Regression tests:
  - check 仍基于 runtime digest 判断 candidate

### Quality checks

- `cargo test -p dockrev-api api::tests -- --nocapture`
- `cargo test -p dockrev-api notify::tests -- --nocapture`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增规格索引并同步状态
- `docs-site/docs/zh/integrations.md`: webhook 行为说明从 discovery-only 改为 check-first + fallback discovery
- `docs-site/docs/en/integrations.md`: same as above
- `docs-site/docs/notifications.md` / `docs-site/docs/en/notifications.md`: 说明 webhook reason 的 check 也会触发新版本通知

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: webhook receiver 支持 repo -> service 精确匹配与 service check 入队
- [x] M2: webhook 路径支持 service check / discovery fallback 的 job 复用与审计字段
- [x] M3: check 完成后对 `schedule` + `webhook` reason 统一触发新版本通知，排除 `ui`
- [x] M4: 补齐 webhook / notify / reuse 回归测试
- [x] M5: spec-sync、文档更新与快车道验证收敛

## 方案概述（Approach, high-level）

- 复用现有 GHCR webhook 验签、selected repo 过滤与 delivery 去重，不改外部接入面。
- 在 webhook receiver 内新增“受影响服务解析器”，直接从数据库中的 stack/service 镜像引用反查匹配服务。
- job 复用优先在入队层完成，避免仅靠执行锁导致 Queue 堆积相似任务。
- 新版本通知继续复用既有 payload/render/send 实现，只调整 check 完成后的触发条件。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：repo -> service 匹配若只看镜像仓库名，可能命中同仓库多 tag 服务；本次接受这种结果，因为 webhook 发布语义本就以仓库为粒度。
- 风险：零命中回退 discovery 仍可能因 Compose override 文件缺失而失败，但不影响 webhook check 主路径。
- 假设：当前数据库查询性能足以支撑按 webhook 实时扫描现有 stacks/services；若后续服务量显著增大，再考虑 repo 索引化。

## 变更记录（Change log）

- 2026-03-07：新建规格，冻结“GHCR webhook 改为 check-first，零命中才回退 discovery，并让 webhook check 发送新版本通知”的行为边界与验收口径。
- 2026-03-07：实现 webhook 命中服务优先检查、零命中回退 discovery、webhook check 新版本通知，以及相关去重/审计/回归测试。
- 2026-03-07：根据 review-loop 收敛结果补强 webhook job 复用与审计落盘，明确只复用 `check.service` / `discovery.all`，并确保复用 job 的 summary 数组合并与 delivery 记录不会在异常路径丢失。

## 参考（References）

- `docs/specs/p2n8k-notification-event-switches-and-new-alerts/SPEC.md`
- `docs/specs/g5m9c-ghcr-webhook-jobization/SPEC.md`
- `docs/plan/hkr8b:github-package-webhook-registration/PLAN.md`
