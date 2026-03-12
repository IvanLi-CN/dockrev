# Dockrev：新版本通知事件驱动收敛 + 显式版本结果兜底（#s4fqf）

## 状态

- Status: 已完成
- Created: 2026-03-12
- Last: 2026-03-12
- Notes: fast-track

## 背景 / 问题陈述

- 现有 `new_version_discovered` 通知在 check job 完成后只额外等待固定短窗口，并通过轮询 snapshot ready 状态重算 display tag。
- 当 digest snapshot 结果晚于窗口返回时，通知会提前退化成 `latest` 或 raw tag，出现 `v1.1.3 -> latest` 这类低价值内容。
- 即使 snapshot 已完成，若受 `repoTagsConsidered` 上限或 manifest timeout/error 影响，`tags[]` 仍可能只剩 `latest`；通知链路当前没有显式版本元数据兜底，因此会继续落成 `0.29.12 -> latest`。

## 目标 / 非目标

### Goals

- `new_version_discovered` 的发送前收敛主路径改为等待 `snapshot_worker task_finished` 明确事件，而不是固定 sleep 轮询。
- 保留固定 timeout 作为兜底，但事件齐备时只做一次最终重算并立即发送。
- 当 snapshot 已终态但仍无法从 `tags[]` 推出可读 semver 时，通知链路补充 OCI `org.opencontainers.image.version` 兜底。
- 统一通知与 supervisor 对 OCI version 的 semver 规整规则，避免 `v` 前缀 / build metadata 处理分叉。

### Non-goals

- 不修改 HTTP 路由、SSE 合同、数据库 schema 或 UI 展示合同。
- 不把 explicit version 回写到 `services`、`jobs.summary_json` 或 snapshot 持久层。
- 不扩展到 `job_finished`、`ghcr_webhook_anomaly`、update apply settle 或其他通知类型。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/snapshot_worker.rs`
- `crates/dockrev-api/src/notify.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `crates/dockrev-api/src/registry.rs` 及相关 fake registry 测试桩
- `crates/dockrev-supervisor/src/docker_exec.rs` 中 OCI version 规整 helper 的共享化
- `docs-site/docs/notifications.md`
- `docs-site/docs/en/notifications.md`
- `docs/specs/README.md`

### Out of scope

- snapshot 扫描策略、`repoTagsConsidered` 上限、manifest timeout 配置本身。
- 前端版本列/Popover 的展示文案或 badge 行为。
- notification record 历史数据回填。

## 需求（Requirements）

### MUST

- 通知发送前不得再以 `50ms` 轮询 snapshot ready 作为主路径；对相关 digest 的等待必须基于进程内明确终态事件。
- 等待 helper 必须避免订阅竞态：在判断 runtime 状态前就建立事件订阅，不能漏掉刚发出的 `task_finished`。
- 事件等待结果至少要区分 `success`、`error/all_failed`、`timed_out`，并允许通知层在全部 key 终态后统一重算一次 display tag。
- 对已有 ready snapshot 的 digest，即使 worker 仍有同 key in-flight 任务，通知也必须直接发送，不得无谓等待。
- 通知 display tag 的优先级固定为：`snapshot 推断 > frozen/live resolvedTag > OCI explicit version > raw/generic 文案`。
- OCI explicit version 仅使用 `org.opencontainers.image.version`；其 semver 规整规则必须与 supervisor 共享，支持前导 `v/V`，拒绝 build metadata。
- 当 snapshot 已终态且 `tags[]` 仅剩 `latest`，但 OCI explicit version 可解析时，通知必须展示明确版本（例如 `0.29.12 -> 0.30.0`），而不是回退成 `latest`。
- 当 snapshot 与 OCI explicit version 都无法给出可读版本时，通知继续沿用现有 mixed/raw/generic 文案，并保持 `latest -> latest` 压制语义。
- fallback timeout 固定为 `10s`，不新增用户配置项。

### SHOULD

- 事件驱动 barrier 只在必要 digest 上生效，避免影响已冻结 display tag 或非 floating tag 通知路径。
- 通知 payload / `links.serviceUrls[]` / human summary 在完成重算后保持彼此一致。

## 验收标准（Acceptance Criteria）

- Given digest snapshot 正在运行且尚未 ready，When `new_version_discovered` 进入发送阶段，Then 通知会等待对应 `task_finished` 事件，而不是在短轮询窗口结束前提前发送。
- Given snapshot 在事件后提供可读 semver，When 通知发送，Then `human.summary` 与 `links.serviceUrls[].*DisplayTag` 直接使用该结果。
- Given ready snapshot 已缓存，When 同 digest 又有一条 in-flight 刷新任务，Then 通知立即发送，不等待该任务。
- Given snapshot 终态仅有 `latest` 且 scan 记录包含 timeout/error，When OCI explicit version 可解析为 semver，Then 通知显示显式版本兜底结果。
- Given snapshot 已终态但 OCI explicit version 缺失或不可解析，When 通知发送，Then 继续压制 `latest -> latest` 并回退 generic/mixed copy。
- Given 事件在订阅与 runtime 检查之间极短时间内完成，When 通知等待 helper 运行，Then 不会丢失该终态并错误超时。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 为 snapshot worker 增加进程内终态事件等待原语，并覆盖订阅竞态场景
- [x] M2: 重写 `new_version_discovered` settle 流程为事件驱动 barrier + 10s fallback timeout
- [x] M3: 抽出共享 OCI version 规整 helper，并为通知链路加入 explicit version fallback
- [x] M4: 补齐后端回归测试，覆盖事件驱动、cached snapshot、timeout fallback、explicit version fallback 与 suppress 语义
- [x] M5: 同步通知文档与 spec 索引，完成 fast-track 验证与交付记录

## 验证记录

- `cargo test -p dockrev-api schedule_new_version_notification_waits_for_version_inference_settle -- --nocapture`
- `cargo test -p dockrev-api schedule_new_version_notification_uses_cached_snapshot_without_waiting_for_in_flight_task -- --nocapture`
- `cargo test -p dockrev-api schedule_new_version_notification_falls_back_to_oci_explicit_version_when_snapshot_tags_stay_latest -- --nocapture`
- `cargo test -p dockrev-api notify -- --nocapture`
- `cargo test -p dockrev-api`
- `cargo test -p dockrev-common`
- `cargo test -p dockrev-supervisor`

## Change log

- 2026-03-12：创建规格，冻结“通知事件驱动收敛 + OCI explicit version fallback”边界与验收口径。
- 2026-03-12：完成 snapshot worker 事件等待原语、`new_version_discovered` barrier settle、共享 OCI version 规整 helper、通知 explicit version fallback 与相关回归测试/文档更新。
