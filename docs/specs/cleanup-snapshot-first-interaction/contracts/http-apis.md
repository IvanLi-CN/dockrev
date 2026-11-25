# Cleanup 快照交互 HTTP 合同

本文件只记录 `cleanup-snapshot-first-interaction` 对既有 cleanup API 的增量；未列出的字段、错误码与清理语义保持不变。

## `POST /api/cleanups/scan`

### Request

```json
{
  "reason": "page | confirm",
  "refresh": false,
  "preset": "conservative | balanced | project_deep_clean | aggressive",
  "scope": "all | stack | service",
  "stackId": "optional",
  "serviceId": "optional"
}
```

- `refresh=true` 只用于用户显式重扫；页面加载、动作打开与自动刷新 MUST 使用 `refresh=false`。
- `reason=page` 且快照新鲜时，服务端 MUST 直接返回 ready 快照，不启动扫描。
- 快照过期或缺失时，服务端 MUST 入队至多一次扫描，并返回 `pending` 或 `ready + refreshing=true`。
- 扫描运行期间，服务端 MUST 继续返回现有快照，而不是空响应。

### Response

在现有 `CleanupScanResponse` 上新增：

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `snapshotAgeMs` | integer \| null | ready/partial 必填 | 服务端计算的非负快照年龄；pending 为 `null` |

- `status=ready` 且 `refreshing=false` 时，`snapshotAgeMs < 300000` 才可作为执行依据。
- `status=ready` 且 `refreshing=true` 时，快照仍可能在窗口内；apply 是否可以执行由 apply 时的服务端新鲜度校验决定。
- `confirmationFingerprint` 只在 ready 响应中非空；partial 与 pending MUST 为 `null`。

## `POST /api/cleanups/scan-runs` 与 SSE

- Request 与现有约束不变：只接受 `reason=page`、`preset=aggressive`、`scope=all`。
- 响应字段不变：`scanId`、`previousSnapshot`、`retryAfterMs`。
- SSE 事件名不变：`scan_partial`、`scan_ready`、`scan_failed`。
- `scan_partial` 与 `scan_ready` 的 `response` MUST 包含 `snapshotAgeMs`。
- partial 推送节奏：
  - 距上次推送至少 300 毫秒，或累计新增 25 个候选，二者先到先推；
  - 每个资源阶段（容器、镜像、卷、网络、构建缓存）结束时 MUST 至少推送一次；
  - MUST NOT 因单个候选而无限高频推送。
- `scan_failed` 语义不变；客户端在发现态失败时必须丢弃已展示的 partial 候选。

## `POST /api/cleanups/apply`

- Request 字段不变。
- 准入条件改为：
  - `estimateVersion` 受支持；
  - `snapshotAgeMs` 仍在 300 秒窗口内；
  - 提交的 `confirmationFingerprint` 与服务端重算值一致；
  - volume 实例身份仍有效。
- 后台清理扫描运行中 MUST NOT 单独构成拒绝理由。
- apply MUST NOT 等待整轮扫描完成。
- apply 的快照校验与 job 注册 MUST 继续与 Stack 记录删除互斥；扫描的最终写入 MUST 与同一删除路径互斥。
- 不满足条件时沿用 `409 cleanup_snapshot_stale`，payload 为最新 ready 或 pending 响应。
- 之前一轮扫描失败 MUST NOT 阻止基于仍新鲜快照的 apply。

## 错误与兼容

- 既有 `409 cleanup_snapshot_stale`、`500 cleanup snapshot refresh failed` 语义不变。
- 新增字段为向后兼容增量；客户端在 `snapshotAgeMs` 缺失时必须按“不可确认”处理，不得回退为本地时钟推断。
- 单飞扫描由服务端 hub/worker 保证；客户端重复触发只会加入现有扫描。
