# Dockrev：新版本通知等待解析收敛 + 单服务正文瘦身（#qh4zx）

## 状态

- Status: 已完成
- Created: 2026-03-09
- Last: 2026-03-09
- Notes: fast-track, PR #159

## 背景 / 问题陈述

- 当前 `new_version_discovered` 在 check job 刚写入 summary 后就立即发送通知，版本推测仍在异步进行时，通知会直接回退到 raw tag，出现 `latest -> latest` 这类低价值内容。
- 单服务通知沿用了聚合模板，标题尾部、正文链接与“服务清单”重复表达同一目标，信息密度低且废话偏多。
- 现有 `dockrev.notification.new_version_discovered.v2` 已包含 raw/display tag 与服务链接，本次应在保持兼容的前提下收敛发送时机与文案。

## 目标 / 非目标

### Goals

- `new_version_discovered` 在发送前等待版本推测收敛；只延迟通知 dispatch，不延迟 check job 终态落库。
- 对仍依赖 floating tag 的服务，按 digest snapshot + snapshot worker 在途状态重算 display tag，避免把 `latest -> latest` 直接发给用户。
- 单服务通知正文改为直接陈述“某服务有新版本（旧版 -> 新版）”，并收敛 Telegram/Email/Web Push 的重复结构。
- 当只解析出一侧版本时允许 resolved + raw 混排；当双侧都不可读时退化为“某服务有新版本”。

### Non-goals

- 不改 `job_finished`、`ghcr_webhook_anomaly` 或其他通知类型。
- 不升级 `dockrev.notification.new_version_discovered.v2` schema 版本。
- 不新增通知模板编辑能力、用户配置项或通知历史 UI。

## 范围（Scope）

### In scope

- Backend:
  - `crates/dockrev-api/src/api/operations.rs`：在 check 完成后通知阶段加入等待收敛门禁。
  - `crates/dockrev-api/src/notify.rs`：补充等待/重算 helper，收敛摘要与各渠道单服务文案。
  - `crates/dockrev-api/src/api/tests.rs`、`crates/dockrev-api/src/notify.rs` tests：新增等待解析、部分解析、双侧未解析与渲染回归用例。
- Docs:
  - `docs-site/docs/notifications.md`
  - `docs-site/docs/en/notifications.md`
  - `docs/specs/README.md`

### Out of scope

- 版本推测 worker 并发、TTL、事件流或 API 结构调整。
- `links.serviceUrls[]` raw 字段删除或重命名。

## 需求（Requirements）

### MUST

- `finish_job(..., success, ...)` 先完成；其后才允许等待版本推测并发送 `new_version_discovered`。
- 仅对仍需版本推测的服务等待收敛：基于当前 `image_tag/candidate_tag`、digest snapshot 缓存与 snapshot worker in-flight 状态判断；若已有可用 snapshot，则直接重算 display tag，不做额外等待。
- 等待结束后必须按与 UI 一致的 semver 推断规则重算 display tag；不得把 `latest -> latest` 直接暴露到 `human.summary`、Telegram、Email 或 Web Push body。
- 单服务摘要固定为 `<stack> / <service> 服务有新版本（<from> -> <to>）`；若只有一侧可读，则允许 resolved + raw 混排；若双侧都不可读，则改为 `<stack> / <service> 服务有新版本。`
- 单服务 Telegram/Email 正文去掉聚合态 `服务清单` 区块与重复列表，直接从正文首句开始，只保留一个 `服务详情` 动作；不再显示泛化标题。
- 多服务通知仍按聚合发送，正文中每个服务必须单独一行；其中某条服务若双侧都不可读，则该条只显示服务名，不附迁移括号。
- `dockrev.notification.new_version_discovered.v2` 的 raw/display tag 字段、`links.primaryUrl`/`jobUrl`/`serviceUrls[]` 与跳转目标保持兼容。
- 必须设置固定等待上限；达到上限后若双侧仍不可读，按“无版本号正文”发送，不新增用户配置项。

### SHOULD

- 当通知因等待版本推测而延后时，保持现有 dedupe reserve/finalize 与 skip log 语义不回归。
- Web Push body 同步收敛为精简正文，不再拼接冗余说明。

## 验收标准（Acceptance Criteria）

- Given 单服务 `latest`/floating tag 发现新版本，When check job 已经 success 且对应 snapshot task 仍在运行，Then 通知不会立刻发出，而是在 snapshot ready 或等待上限到达后才发送。
- Given 单服务最终解析出 `1.0.0 -> 1.1.0`，When 发送 `new_version_discovered`，Then 摘要与人类可读渠道显示 `服务有新版本（1.0.0 -> 1.1.0）`，且不出现 `latest -> latest`。
- Given 单服务只解析出一侧版本，When 发送通知，Then 正文允许 resolved + raw 混排。
- Given 单服务等待到上限后双侧都仍不可读，When 发送通知，Then 正文只显示“某服务有新版本”，但 `serviceUrls[]` raw/display 字段与 `primaryUrl` 仍保持兼容。
- Given 单服务 Telegram/Email 渲染，When 通知生成，Then 不再出现标题尾重复“详情”+“服务清单”+重复服务详情列表。
- Given 多服务聚合通知，When 摘要/body 生成，Then 每条服务单独占一行。
- Given 多服务聚合通知中某条服务双侧都不可读，When 摘要/body 生成，Then 该条只显示服务名，不附迁移括号；其他可读条目保持原样。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增发送前 settle helper，与 snapshot worker/digest snapshot 对齐版本重算逻辑
- [x] M2: 接入 check 完成后的等待门禁，保持 job 终态与 dedupe 链路不回归
- [x] M3: 重写单服务 Telegram/Email/Web Push 文案并保留多服务聚合语义
- [x] M4: 更新中英文通知文档与 spec 索引
- [x] M5: 补齐 notify/API 回归测试与验证记录

## 验证记录

- `cargo test -p dockrev-api notify::tests -- --nocapture`
- `cargo test -p dockrev-api new_version_notification -- --nocapture`
- `bun run --cwd docs-site build`

## Change log

- 2026-03-09：创建规格，冻结“等待版本推测收敛后再发新版本通知 + 单服务正文瘦身”的实现边界与验收口径。
- 2026-03-09：完成新版本通知 settle 门禁、单服务精简文案、Webhook/Web Push 摘要收敛、通知文档更新与回归测试。
