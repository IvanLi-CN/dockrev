## Shared types

新增共享类型：

- `CleanupPreset = "conservative" | "balanced" | "project_deep_clean" | "aggressive"`
- `CleanupScope = "all" | "stack" | "service"`
- `CleanupResourceKind = "image" | "container" | "network" | "volume" | "builder_cache"`
- `CleanupScanResponse`
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

- `reason=page` 的实现约定是：前端固定以 `preset=aggressive, scope=all` 拉取一份完整 inventory，再用每个资源项的 `minPreset` 做本地 tab 投影；页面默认展示 `balanced` 投影，但不重复全量扫描。
- `minPreset` 表示该资源最早在哪个 preset 开始出现，前端据此决定 tabs 是否显示该资源。
- `estimatedReclaimableBytes` 对资源项来说允许为 `null`；这时 `estimateUnknown=true`，group/response 级 `hasUnknownSize=true`。
- `reason=confirm` 返回当前动作作用域的最新候选；`confirmationFingerprint` 在 page/confirm 两类响应里都可能出现，但前端只使用 confirm 返回值发起 apply。
- `unownedGroup` 仅允许在 `scope=all` 时出现。
- `service` 作用域的 payload 仍沿用 `stackGroups[] -> services[]` 结构，只是只包含目标 service 所属 stack 与该 service。

## `POST /api/cleanups/scan`

作用：执行同步 cleanup 扫描，返回页面展示或确认弹窗所需的最新候选分组。

Request body:

```json
{
  "reason": "page | confirm",
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
  - 必须带 `scope`
  - `scope=stack` 时必须带 `stackId`
  - `scope=service` 时必须同时带 `stackId` 与 `serviceId`

Response: `200 OK`

```json
{
  "reason": "confirm",
  "preset": "project_deep_clean",
  "scope": "stack",
  "scannedAt": "2026-03-29T13:40:00Z",
  "estimatedReclaimableBytes": 53248,
  "stackGroups": [],
  "confirmationFingerprint": "sha256-abc"
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

作用：基于最近一次 confirm-scan 的 fingerprint 发起异步 cleanup job。

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
