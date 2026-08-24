## Shared types

新增共享类型：

- `CleanupPreset = "conservative" | "balanced" | "project_deep_clean" | "aggressive"`
- `CleanupScope = "all" | "stack" | "service"`
- `CleanupResourceKind = "image" | "container" | "network" | "volume" | "builder_cache"`
- `CleanupScanResponse`
- `CleanupScanRunStartResponse`
- `CleanupScanRunEvent`
- `CleanupScanRunPhase = "scan_started" | "scan_partial" | "scan_ready" | "scan_failed"`
- `CleanupServerDiskUsage`
- `CleanupStackGroup`
- `CleanupServiceGroup`
- `CleanupApplyRequest`
- `CleanupFingerprintMismatchError`

`CleanupScanResponse` 的分组 contract 固定为：

```json
{
  "reason": "page | confirm",
  "preset": "aggressive",
  "scope": "all | stack | service",
  "scannedAt": "RFC3339",
  "estimatedReclaimableBytes": 123456,
  "hasUnknownSize": false,
  "serverDiskUsage": {
    "usedBytes": 37800000000,
    "totalBytes": 80000000000
  },
  "stackGroups": [
    {
      "stackId": "stack_123",
      "stackName": "alpha",
      "estimatedReclaimableBytes": 12345,
      "hasUnknownSize": false,
      "stackOrphans": [
        {
          "resourceId": "network_alpha_default",
          "kind": "volume",
          "label": "alpha_cache",
          "minPreset": "conservative",
          "estimateUnknown": false,
          "estimatedReclaimableBytes": 4096
        }
      ],
      "services": [
        {
          "serviceId": "svc_123",
          "serviceName": "web",
          "estimatedReclaimableBytes": 8192,
          "hasUnknownSize": false,
          "resources": [
            {
              "resourceId": "img_web_old",
              "kind": "image",
              "label": "ghcr.io/acme/web:old",
              "minPreset": "balanced",
              "estimateUnknown": false,
              "estimatedReclaimableBytes": 8192
            }
          ]
        }
      ]
    }
  ],
  "unownedGroup": {
    "title": "未归属资源",
    "estimatedReclaimableBytes": 20480,
    "hasUnknownSize": false,
    "resources": [
      {
        "resourceId": "builder_cache",
        "kind": "builder_cache",
        "label": "global builder cache",
        "minPreset": "balanced",
        "estimateUnknown": false,
        "estimatedReclaimableBytes": 20480
      }
    ]
  },
  "confirmationFingerprint": "sha256-..."
}
```

说明：

- `reason=page` 的实现约定是：前端固定以 `preset=aggressive, scope=all` 读取一份完整 inventory snapshot，再用每个资源项的 `minPreset` 做本地 tab 投影；页面默认展示 `balanced` 投影，但不重复全量扫描。
- `minPreset` 表示该资源最早在哪个 preset 开始出现，前端据此决定 tabs 是否显示该资源。
- `estimatedReclaimableBytes` 对资源项来说允许为 `null`；这时 `estimateUnknown=true`，group/response 级 `hasUnknownSize=true`。
- `serverDiskUsage` 表示 Dockrev 运行环境看到的服务器根文件系统用量；字段可省略，省略时前端必须展示“未获取”而不是把它混入可回收候选估算。
- `reason=confirm` 只有在最新 cleanup snapshot 年龄 `<=300s`（5 分钟）且无 refresh in-flight 时才返回 ready；否则返回 pending，前端必须 poll 到 ready 后再允许确认。
- cleanup confirm/page 的首次请求可以使用 `refresh=true` 触发后台刷新；后续 poll 必须改用 `refresh=false`，避免重复 re-enqueue 同一轮扫描。
- confirm worker 已停止且记录失败终态时，接口返回明确的 5xx API 错误；客户端应展示可重试状态，不得把该错误继续包装为 pending。
- `unownedGroup` 仅允许在 `scope=all` 时出现。
- `service` 作用域的 payload 仍沿用 `stackGroups[] -> services[]` 结构，只是只包含目标 service 所属 stack 与该 service。

## `POST /api/cleanups/scan`

作用：读取 cleanup snapshot，并按页面 / 确认语义返回 `ready` 或 `pending`。该接口不再在 owner-facing 请求链路里同步执行全量 Docker 扫描。

Request body:

```json
{
  "reason": "page | confirm",
  "refresh": true,
  "preset": "balanced",
  "scope": "all | stack | service",
  "stackId": "optional for scope=stack|service",
  "serviceId": "optional for scope=service"
}
```

约束：

- `reason=page`：
  - `scope` 固定为 `all`
  - 不接受 `stackId` / `serviceId`
- `reason=confirm`：
  - `refresh=true` 表示“必要时触发/续接一次后台 refresh”；confirm 流程的首个请求固定使用此值
  - `refresh=false` 表示“只读当前 snapshot / in-flight 状态，不重复 enqueue”
  - 必须带 `scope`
  - `scope=stack` 时必须带 `stackId`
  - `scope=service` 时必须同时带 `stackId` 与 `serviceId`

Response:

- `200 OK` with `status=ready`
- `200 OK` with `status=pending`

```json
{
  "status": "ready",
  "reason": "confirm",
  "preset": "project_deep_clean",
  "scope": "stack",
  "scannedAt": "2026-03-29T13:40:00Z",
  "refreshing": false,
  "estimatedReclaimableBytes": 53248,
  "hasUnknownSize": false,
  "serverDiskUsage": {
    "usedBytes": 37800000000,
    "totalBytes": 80000000000
  },
  "stackGroups": [],
  "confirmationFingerprint": "sha256-abc"
}
```

pending 示例：

```json
{
  "status": "pending",
  "reason": "confirm",
  "preset": "project_deep_clean",
  "scope": "stack",
  "refreshing": true,
  "retryAfterMs": 800,
  "stackGroups": []
}
```

`confirmationFingerprint` 语义说明：

- fingerprint 代表“当前 confirm-scan 同意执行的 cleanup 语义快照”，必须在 confirm/apply 之间保持稳定。
- 仅当以下语义变化时才允许 fingerprint 变化：`preset`、`scope`、`stackId/serviceId`、候选 identity/ownership/category、候选估算值、候选 `estimateUnknown`、聚合 `estimatedReclaimableBytes`、聚合 `hasUnknownSize`。
- 对名字可复用的资源（例如 named volume、builder cache），服务端必须把底层实例 freshness identity 一并纳入 fingerprint（例如 volume `CreatedAt` / mountpoint、builder cache inventory hash），避免旧确认误删后续重建的同名目标。
- `scannedAt` 只用于 UI 展示“最新扫描时间”，不得单独导致 fingerprint 变化。

页面首次加载时的推荐调用：

```json
{
  "reason": "page",
  "preset": "aggressive",
  "scope": "all"
}
```

## `POST /api/cleanups/scan-runs`

作用：启动一次 owner-facing cleanup page 重扫会话。该接口用于页面重扫的流式刷新，不替代 confirm-scan / apply 的安全路径。

Request body:

```json
{
  "reason": "page",
  "refresh": true,
  "preset": "aggressive",
  "scope": "all"
}
```

约束：

- 仅接受 `reason=page + preset=aggressive + scope=all`；`stackId` / `serviceId` 不允许出现。
- 返回值可以带上一份可显示 snapshot，前端必须把它视为 stale baseline，仅用于重扫期间保持页面可读。
- 会话最终 `scan_ready.response` 必须是完整 `CleanupScanResponse`，并写回现有 cleanup snapshot cache。

Response: `202 Accepted`

```json
{
  "scanId": "clnscan_01J...",
  "previousSnapshot": {
    "status": "ready",
    "reason": "page",
    "preset": "aggressive",
    "scope": "all",
    "refreshing": true,
    "scannedAt": "2026-07-07T00:15:24Z",
    "estimatedReclaimableBytes": 5089538048,
    "stackGroups": []
  },
  "retryAfterMs": 800
}
```

## `GET /api/cleanups/scan-runs/{scanId}/events`

作用：以 SSE 推送 cleanup 重扫会话进度。事件名固定为：

- `scan_started`
- `scan_partial`
- `scan_ready`
- `scan_failed`

客户端可通过 `Last-Event-ID` 或 query `afterId=<event-id>` 续接事件流。服务端会对同一 `scanId` replay 已记录事件，再等待新事件。

SSE data payload:

```json
{
  "scanId": "clnscan_01J...",
  "phase": "scan_partial",
  "response": {
    "status": "pending",
    "reason": "page",
    "preset": "aggressive",
    "scope": "all",
    "refreshing": true,
    "confirmationFingerprint": null,
    "stackGroups": []
  },
  "message": null
}
```

语义约束：

- `scan_partial.response` 只能用于页面投影的渐进替换，不得用于 confirm dialog 或 apply fingerprint。
- `scan_ready.response` 是唯一可视为本轮重扫完整结果的 payload。
- `scan_failed` 不清空页面，前端保留上一份 snapshot 并展示非阻断错误。
- cleanup apply 仍只接受完整 confirm-scan fingerprint；partial 数据不得进入 apply。

## `POST /api/cleanups/apply`

作用：基于最近一次 ready confirm snapshot 的 fingerprint 发起异步 cleanup job；apply 路径不再内联全量重扫。

Request body:

```json
{
  "reason": "ui",
  "preset": "project_deep_clean",
  "scope": "service",
  "stackId": "stack_123",
  "serviceId": "svc_123",
  "confirmationFingerprint": "sha256-abc"
}
```

Response: `200 OK`

```json
{
  "jobId": "job_123"
}
```

### Stale fingerprint

当服务器在 apply 前重算 fingerprint 与请求值不一致时，返回：

- Status: `409 Conflict`
- Error code: `cleanup_snapshot_stale`

```json
{
  "error": {
    "code": "cleanup_snapshot_stale",
    "message": "Cleanup candidates changed since the last confirmation scan.",
    "details": {
      "latest": {
        "reason": "confirm",
        "preset": "project_deep_clean",
        "scope": "service",
        "scannedAt": "2026-03-29T13:41:22Z",
        "estimatedReclaimableBytes": 28672,
        "hasUnknownSize": false,
        "stackGroups": [],
        "confirmationFingerprint": "sha256-def"
      }
    }
  }
}
```

前端必须使用 `latest` 刷新当前确认弹窗，并要求用户再次确认；不得自动重试 apply。

confirm snapshot 的 freshness 边界由服务端固定为 300 秒，apply 使用同一边界。过期只会返回 pending/stale 并触发或等待新快照，不会自动创建 cleanup job；fingerprint 变化仍要求用户再次确认。

当命中 stale 分支时，服务端还应记录最小诊断日志（principal、preset、scope、stack/service 维度、submitted/latest fingerprint、target_count、estimate 摘要），用于线上排查。

## Job summary / log contract

`cleanup_apply` job 的 summary/log 至少包含：

```json
{
  "preset": "balanced",
  "scope": "stack",
  "reclaimedBytesEstimated": 53248,
  "deletedCountsByKind": {
    "image": 2,
    "container": 1,
    "volume": 1
  },
  "skippedInUse": [
    {
      "kind": "volume",
      "label": "alpha_data",
      "reason": "still_attached"
    }
  ],
  "groupedTargets": [
    {
      "stackId": "stack_123",
      "serviceIds": ["svc_123"]
    }
  ]
}
```

说明：

- `deletedCountsByKind` 统计实际删除的资源数量。
- `skippedInUse` 仅记录因仍在使用而跳过的资源，不把普通“未命中”混入其中。
- `groupedTargets` 用于队列/日志复盘本次 job 影响的 stack/service 范围。
