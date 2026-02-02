# Dockrev API: candidates 接口稳定性（#yh457）

## 状态

- Status: 待实现
- Created: 2026-02-02
- Last: 2026-02-02

## 背景 / 问题陈述

- 线上 `/api/services/:id/candidates` 在浏览器侧可复现 `net::ERR_HTTP2_PROTOCOL_ERROR`，导致候选列表、版本 tags 气泡等功能不可用或长时间卡住。
- 推测根因：接口实现对 registry 的 tags + manifests 查询存在扇出与无超时控制，触发边缘网关/上游连接被中断。

## 目标 / 非目标

### Goals

- `/api/services/:id/candidates` 能稳定返回：
  - 成功：HTTP 200 + 合理的候选列表；
  - 失败：明确的 HTTP 错误响应（而不是浏览器级协议错误）。
- 即使部分 manifest 查询失败/超时，接口仍能快速返回：失败项允许 `digest=null`（前端按不可选/不可聚合降级）。

### Non-goals

- 不在本计划内改变“候选 tag 的挑选策略”与“版本号解析/推测”规则（另立计划处理）。

## 范围（Scope）

### In scope

- 后端：
  - 为 registry 请求增加超时与并发上限，避免单个请求长期阻塞；
  - 对 manifest fan-out 做降级：失败/超时返回 `digest=null`，其余项继续返回。
- 测试：补回归测试，覆盖“慢 registry 也不会无限等待”。

### Out of scope

- UI 交互调整与样式优化（已有功能按接口恢复即可）。

## 验收标准（Acceptance Criteria）

- Given 调用 `/api/services/:id/candidates`，
  Then 响应应在可接受时间内完成（目标：≤ 8s），且不再产生浏览器 `ERR_HTTP2_PROTOCOL_ERROR`。
- Given registry 的部分 tag manifest 查询超时/失败，
  Then 返回的 `candidates[]` 仍包含该 tag，但 `digest=null`（并标明 archMatch=unknown）。
- Given Web 侧版本气泡 / 目标版本选择器触发该接口，
  Then “加载中”会在合理时间内结束（成功展示或明确提示“候选列表不可用”）。

## 测试（Testing）

- `cargo test -p dockrev-api`

## 风险 / 开放问题（Risks & Open Questions）

- 并发提高可能触发 registry rate limit；需要并发上限（必要时再引入缓存）。

## 变更记录（Change log）

- 2026-02-02: 创建计划并冻结范围与验收标准（Status=待实现）。

