# Dockrev：新版本通知记录表去重 + 通知内版本号展示（#4n5vr）

## 状态

- Status: 已完成
- Created: 2026-03-09
- Last: 2026-03-09

## 背景 / 问题陈述

- 当前 `new_version_discovered` 是否发送，只看本次 check summary 里的 `newVersions`，缺少持久化去重状态。
- 相同 `service + candidate digest` 会在后续 schedule / webhook check 中重复提醒，操作者会反复收到已经看过的内容。
- 现有通知文案主要展示 raw tag；对于 `latest` 这类浮动 tag，通知里缺少更可读的新版本号。

## 目标 / 非目标

### Goals

- 为 `new_version_discovered` 增加专用通知记录表，承接去重与审计。
- 统一 schedule / webhook 新版本通知链路，保证相同 `service_id + candidate_digest` 的 active 记录只通知一次。
- 在新版本通知 payload 与人类可读文案中增加 display tag，统一展示 `旧版 -> 新版`，优先使用 resolved version/tag。

### Non-goals

- 不新增通知中心 UI 或通知历史查询 API。
- 不改造 `job_finished` / `ghcr_webhook_anomaly` 的去重模型。
- 不引入跨实例分布式消息总线或外部幂等存储。

## 范围（Scope）

### In scope

- Backend:
  - 新增 `new_version_notifications` 表与 migration；
  - 新增 reserve / finalize / reconcile DB helper；
  - 调整 check 持久化与新版本通知发送链路；
  - 扩展 `dockrev.notification.new_version_discovered.v2` payload 的 service link 字段。
- Docs:
  - 更新中英文 notifications 文档；
  - 记录本次通知去重与 display tag 行为。

### Out of scope

- 设置页新增开关；
- 通知模板编辑能力；
- 新的 webhook schema 版本。

## 需求（Requirements）

### MUST

- 新增 `new_version_notifications` 表，字段固定为：
  - `id`、`service_id`、`job_id`、`reason`、`image_ref`、`image_tag`
  - `current_tag`、`current_display_tag`
  - `candidate_tag`、`candidate_display_tag`、`candidate_digest`
  - `status`、`sent_channels_json`、`created_at`、`sent_at`、`superseded_at`、`last_error`
- 去重键为 `service_id + candidate_digest`，且仅对 active 记录（`pending` / `sent`）生效。
- 发送流程必须为：先 reserve `pending`，再发送，最后 finalize 为 `sent` 或 `failed`；若记录在发送过程中已被判定失效，finalize 仍需保留审计字段但不得重新占用 active 去重位。
- reserve 与实际发送前都必须按 service 当前持久化状态回读校验；若候选已清空、`image_ref/image_tag` 已变化、或 `candidate_digest` 已不再匹配，则本轮通知必须静默跳过。
- 当 service 候选清空、`image_ref/image_tag` 变化、或候选 digest 变化时，旧 active 记录必须自动转为 `superseded`。
- 若所有已启用渠道都发送失败，则该记录必须为 `failed`，不得继续占用 active 去重位。
- `dockrev.notification.new_version_discovered.v2` 保持兼容；保留 `currentTag` / `candidateTag`，新增可选 `currentDisplayTag` / `candidateDisplayTag`。
- `human.summary`、Telegram、Email、Web Push body、服务清单统一展示 `旧版 -> 新版`；display tag 优先使用 resolved version/tag，缺失时回退 raw tag。

### SHOULD

- 当整批服务都因已有 active 记录被跳过时，任务日志写入一条 explain log，明确说明本轮静默是由于重复通知去重。
- 保持现有渠道日志格式（`notify: <channel>=ok/failed`）不回归。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `dockrev.notification.new_version_discovered.v2` | webhook schema | external | Modify | None | dockrev-api | webhook consumers | `links.serviceUrls[]` 追加 display tag 字段 |
| `new_version_notifications` | DB | internal | New | None | dockrev-api | dockrev-api | 新版本通知去重与审计表 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 某个 service 已经对候选 digest A 发送过新版本通知，When 后续 schedule 或 webhook check 仍发现同一个 digest A，Then 不再发送重复 `new_version_discovered`。
- Given 两个并发 job 同时命中同一 `service_id + candidate_digest`，When 执行 reserve，Then 只有一个 job 成功创建 `pending` 记录，另一个 job 被跳过且不先发送通知。
- Given service 当前候选被清空、或候选 digest 从 A 变成 B，When 后续再次出现可通知候选，Then 旧记录转为 `superseded`，新候选可以重新通知。
- Given 本次所有启用渠道都发送失败，When finalize 记录，Then 记录状态为 `failed`，后续 job 仍可再次尝试发送。
- Given 当前/候选 raw tag 为浮动值且已存在 resolved tag，When 生成 `new_version_discovered` 通知，Then 通知正文与 payload display 字段优先展示 resolved version/tag，而不是仅展示 `latest -> latest`。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: DB schema + migration 增加 `new_version_notifications` 与索引
- [x] M2: DB helper 落地 reserve / finalize / reconcile
- [x] M3: check 持久化与新版本通知链路接入去重记录表
- [x] M4: `new_version_discovered` payload / render 增加 display tag 与版本展示
- [x] M5: DB/notify/API 回归测试补齐
- [x] M6: 中英文通知文档更新 + spec-sync
- [x] M7: 快车道验证、review-loop、PR 收敛

## 验证记录

- `cargo test -p dockrev-api webhook_notifications_filter_to_matched_service_ids -- --nocapture`
- `cargo test -p dockrev-api stale_new_version_notifications_are_skipped_when_candidate_was_cleared -- --nocapture`
- `cargo test -p dockrev-api new_version_notification -- --nocapture`
- `cargo test -p dockrev-api notify -- --nocapture`
- `cargo test -p dockrev-api`
- `bun install --cwd docs-site`
- `bun run docs:build`

## Change log

- 2026-03-09：创建规格，冻结“通知记录表去重 + display tag 版本展示”的实现边界与验收标准。
- 2026-03-09：完成通知记录表、去重 reserve/finalize/reconcile 链路、payload display tag 扩展、中英文文档与回归测试。
- 2026-03-09：补强失效候选的二次校验、`pending -> superseded` 审计保留、以及 compose sync / runtime fallback 的活跃记录释放语义。
