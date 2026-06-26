## Shared types

新增共享类型：

- `CleanupPreset = "conservative" | "balanced" | "project_deep_clean" | "aggressive"`
- `CleanupScope = "all" | "stack" | "service"`
- `CleanupResourceKind = "image" | "container" | "network" | "volume" | "builder_cache"`
- `CleanupScanResponse`
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
- `reason=confirm` 只有在最新 cleanup snapshot 年龄 `<=30s` 且无 refresh in-flight 时才返回 ready；否则返回 pending，前端必须 poll 到 ready 后再允许确认。
- cleanup confirm/page 的首次请求可以使用 `refresh=true` 触发后台刷新；后续 poll 必须改用 `refresh=false`，避免重复 re-enqueue 同一轮扫描。
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
  - `refresh=true` 表示“必要时触发/续接一次后台 refresh”
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
