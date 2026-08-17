## `GET /api/homepage/nav`

Homepage single-read-model response used only by `/`.

```json
{
  "generatedAt": "2026-06-27T12:00:00Z",
  "lastCheckAt": "2026-06-27T11:58:10Z",
  "resourceSummary": {
    "enabled": true,
    "window": "1h",
    "generatedAt": "2026-06-27T12:00:00Z",
    "staleAfterSeconds": 60,
    "services": [
      {
        "serviceId": "svc_api",
        "sampledAt": "2026-06-27T11:59:40Z",
        "cpuPercent": 12.5,
        "memUsedBytes": 268435456,
        "memLimitBytes": 1073741824,
        "netRxRateBps": 100.0,
        "netTxRateBps": 50.0,
        "stale": false,
        "sampleCount": 2
      }
    ]
  },
  "items": [
    {
      "stackId": "stack_prod",
      "stackName": "prod",
      "serviceId": "svc_api",
      "serviceName": "api",
      "imageRef": "ghcr.io/acme/api:5.2.1",
      "imageTag": "5.2.1",
      "imageDigest": "sha256:...",
      "imageResolvedTag": "5.2.1",
      "imageResolvedTags": ["5.2.1"],
      "isDockrev": false,
      "homepage": {
        "group": "Brain",
        "name": "Acme API",
        "icon": "si-github",
        "href": "https://api.example.com",
        "description": "API gateway"
      },
      "candidate": {
        "tag": "5.2.3",
        "resolvedTag": "5.2.3",
        "digest": "sha256:...",
        "archMatch": "match",
        "arch": ["linux/amd64"]
      },
      "ignore": null,
      "versionInference": {
        "status": "ready",
        "reason": null,
        "checkedAt": null
      },
      "newVersionDiscoveryCount": 1,
      "settings": {
        "autoRollback": true,
        "backupTargets": {
          "bindPaths": {},
          "volumeNames": {}
        },
        "repoUrl": null
      },
      "archived": false,
      "resource": {
        "serviceId": "svc_api",
        "sampledAt": "2026-06-27T11:59:40Z",
        "cpuPercent": 12.5,
        "memUsedBytes": 268435456,
        "memLimitBytes": 1073741824,
        "netRxRateBps": 100.0,
        "netTxRateBps": 50.0,
        "stale": false,
        "sampleCount": 2
      }
    }
  ]
}
```

Rules:

- Requires the same app authorization as other private APIs.
- `items` contains only active, non-archived services with a non-empty valid `homepage.href`.
- Ordering is stable by `stackName`, then `serviceName`.
- `resourceSummary` and each item `resource` are built from the latest-sample read model, while `sampleCount` remains the number of historical samples for that service inside the endpoint's `1h` summary window.
- `resourceSummary.services` still lists active services that never exposed a homepage card; the top strip is a global summary, not card-only summary.
- `candidate`, `ignore`, `versionInference`, `newVersionDiscoveryCount`, `settings`, and `archived` preserve the same semantics used by existing service detail and update status logic.
- This endpoint is additive. Existing `/api/stacks*` consumers remain compatible.

## `GET /api/services/resource-usage/overview?window=3m|1h|24h|7d|30d`

Returns the latest resource summary for active services.

```json
{
  "enabled": true,
  "window": "1h",
  "generatedAt": "2026-06-27T12:00:00Z",
  "staleAfterSeconds": 60,
  "services": [
    {
      "serviceId": "svc_api",
      "sampledAt": "2026-06-27T11:59:40Z",
      "cpuPercent": 12.5,
      "memUsedBytes": 268435456,
      "memLimitBytes": 1073741824,
      "netRxRateBps": 100.0,
      "netTxRateBps": 50.0,
      "stale": false,
      "sampleCount": 2
    }
  ]
}
```

Rules:

- Current metric values (`sampledAt`, `cpuPercent`, `memUsedBytes`, `memLimitBytes`, `netRxRateBps`, `netTxRateBps`) come from the latest-per-service read model instead of rebuilding from a historical request-time scan.
- `window` remains semantically active for compatibility:
- For short windows, `sampleCount` is the number of raw samples inside the requested window. For `7d`/`30d`, it is the number of samples represented by retained buckets overlapping the requested window; boundary buckets are intentionally counted in full because their raw members may already be outside the 24-hour raw retention.
  - network rates still come from the latest/latest-previous counters persisted in the read model
- `stale` is true when the latest sample is older than `max(sample_interval_seconds * 2, 60)`.
- When resource monitoring is disabled, the endpoint returns `200` with `enabled=false` and an empty `services` array.

## `GET /api/services/{service_id}/resource-usage/history?window=3m|1h|24h|7d|30d`

Short windows preserve the existing `samples` response. `7d` returns one-minute buckets and `30d` returns five-minute buckets. Long-window responses add `resolutionSeconds` and a time-aligned `peaks` array; `samples` contain CPU、内存、PIDs、容器数与速率的桶均值，累计计数为桶末值，`peaks` 保留对应的桶峰值。

## `GET /api/jobs?view=compact`

This additive view is paginated with the existing cursor and limit parameters. It returns only job identity, status, timestamps, derived progress/result reason, display label and target version. It never serializes raw `summary`. Requests without `view=compact` preserve the existing response shape.

## `GET /api/stacks` / `GET /api/stacks/{id}` / related `Service` payloads

`Service` objects keep the optional `homepage` object introduced by the earlier homepage work:

```json
{
  "id": "svc_123",
  "name": "gitea",
  "homepage": {
    "group": "Developer",
    "name": "Gitea",
    "icon": "si-gitea",
    "href": "https://git.example.com",
    "description": "Git forge"
  }
}
```

Rules:

- Existing stack/service endpoints stay compatible.
- Homepage cache and homepage page rendering no longer rely on these endpoints for `/`, but other consumers may still do so.

## `GET /api/homepage-icons/{provider}/{path}`

Homepage icon proxy contract remains unchanged.
