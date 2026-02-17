# Dockrev: 运行态版本漂移自动发现（runtime diff scan + SSE）（#8xt2t）

## 状态

- Status: 已完成
- Created: 2026-02-17
- Last: 2026-02-17

## 背景 / 问题陈述

- 现象：外部操作（`docker compose pull/up`、supervisor 自升级、手工替换镜像等）导致运行态镜像 digest 变化后，Dockrev UI 仍显示旧的 `current_digest/resolvedTag`（DB 缓存未刷新）。
- 根因：`services.current_digest/current_resolved_tag/current_resolved_tags_json` 目前只在慢路径 `POST /api/checks` 的 check 任务中刷新；外部更新不会自动回写 DB。
- 目标：以“轻量对账”为主，及时发现并修正“运行态 vs DB”的漂移，而不是通过提高慢 check 的频率来掩盖问题。

## 目标 / 非目标

### Goals

- 后端每 X 分钟执行一次 runtime diff scan，对比“运行态 digest vs DB current_digest”，发现漂移时自动修正 DB 的 current/resolvedTag。
- 前端访问 Versions 相关页面时，强制触发一次 runtime diff scan，并通过 SSE 接收增量更新，逐步刷新页面数据。
- registry 推测逻辑必须与现有 `POST /api/checks` 保持一致（不重定义策略；通过抽取复用同一段逻辑实现）。

### Non-goals

- 不把 runtime diff scan 变成新的全量慢 check（避免频繁 `list_tags/get_manifest`）。
- 不引入“仅修某一个镜像”的特例逻辑：覆盖 scope=all/stack/service 的通用能力。
- 不重新定义 resolvedTag 推测规则与排序、topN、降级策略（runtime digest 多值/缺失仍保持降级）。

## 需求（Requirements）

### MUST

- 新增 JobType：`runtime_scan`（jobs.type = `runtime_scan`）。
- 新增 API：
  - `POST /api/runtime-scans`：触发 runtime scan job（reason: `ui|schedule`）。
  - `GET /api/jobs/{jobId}/events`：SSE（`text/event-stream`），按漂移服务增量推送事件。
- 定时任务：后端每 10 分钟触发一次 `scope=all` 的 runtime scan（可通过 env 覆盖）。
- 漂移发现后必须复用现有推测逻辑，更新：
  - `services.current_digest`
  - `services.current_resolved_tag`
  - `services.current_resolved_tags_json`
  - 并重新计算 candidate（含 “no update fast-path”）以避免漂移后候选失真。

### SHOULD

- SSE 支持 `Last-Event-ID` 续传（以 job_logs.id 作为 event id）。
- SSE 响应包含 `Cache-Control: no-cache` 与 `X-Accel-Buffering: no`（避免代理缓冲）。
- runtime scan 的 docker 采集使用批量命令，避免逐服务 `docker ps/inspect/image inspect`。

## 验收标准（Acceptance Criteria）

1) Given 某服务容器镜像被外部更新（runtime digest 变化），When 后端定时 runtime scan 运行一次，Then 该服务对应的 DB `current_digest` 与 `resolvedTag/resolvedTags` 会被更新为与运行态一致（不要求全量慢 check）。

2) Given 用户打开 Versions 相关页面，When 页面触发 runtime scan 且订阅 SSE，Then 页面能在 scan 过程中逐步收到增量事件并刷新到最新 current/resolvedTag（无需手工点“立即扫描”）。

3) Given 相同的 tags/runtimeDigest 输入，When 对同一服务分别执行一次 `check` 与一次 drift 触发的 `runtime_scan`，Then 推测出的 `resolvedTag/resolvedTags` 行为一致（策略/排序/topN/降级策略不变）。

## 测试与验证（Testing）

- `cargo test -p dockrev-api`
- 覆盖：
  - drift 更新：DB current_digest=old（或缺失），runtime digest=new -> runtime scan 更新 digest/resolvedTag。
  - 逻辑一致：check 与 runtime scan 输出一致（同输入）。
  - 性能护栏：无 drift 时 runtime scan 不调用 registry（`list_tags/get_manifest` 调用次数为 0）。
- Web 最小验证：
  - `bun --cwd web run build`

## 里程碑（Milestones）

- [x] M1: 后端 runtime scan job（定时 + API 触发 + drift 对账 + DB 回写）
- [x] M2: 抽取并复用既有 registry 推测逻辑（check 与 runtime scan 共用）
- [x] M3: SSE events（job_logs 事件队列 + `/api/jobs/{id}/events`）
- [x] M4: Web 页面 mount 触发 + SSE 订阅 + 最小刷新（Overview/Services/ServiceDetail）
- [x] M5: 测试补齐 + 最小验证通过

## 风险与开放问题（Risks / Open Questions）

- SSE 可能被反向代理缓冲：需要在部署侧确保禁用 buffering（本仓库的 deploy nginx 可作为参考）。
- runtime scan 触发频率：默认 10 分钟，且页面访问会主动触发；需要确保无 drift 时的路径足够轻量。

## 变更记录 / Change log

- 2026-02-17: 实现完成，待随 PR #67 合入。
